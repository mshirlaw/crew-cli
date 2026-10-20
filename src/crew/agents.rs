use super::context::Ctx;
use crate::util::{self, ui};
use std::fmt::Write;

pub fn kickoff_prompt(ctx: &Ctx, role: &str, revive: bool) -> String {
    let crew = ctx.crew.display();
    let mut msg = format!(
        "You are the {role} agent in a multi-agent crew. Read {crew}/PROTOCOL.md and {crew}/roles/{role}.md \
         now and follow them, then read {crew}/board.md and {crew}/tasks.md."
    );
    if revive {
        let _ = write!(
            msg,
            " You were restarted: also read {crew}/notes/{role}.md for your own handoff and continue from there."
        );
    } else if role == "lead" {
        msg.push_str(" Then briefly greet the human and ask what the crew should work on.");
    }
    msg
}

// Placeholders are filled in per word, so a path with spaces stays one argument.
// The prompt goes last unless the command places {prompt} itself.
pub fn agent_words(ctx: &Ctx, role: &str, revive: bool) -> Vec<String> {
    let prompt = kickoff_prompt(ctx, role, revive);
    let crew = ctx.crew.display().to_string();
    let repo = ctx.root.display().to_string();
    let template = ctx.config.agent_command(&ctx.config.role_agent(role));
    let uses_prompt = template.iter().any(|w| w.contains("{prompt}"));
    let mut words: Vec<String> = template
        .iter()
        .map(|w| {
            w.replace("{crew}", &crew).replace("{repo}", &repo).replace("{role}", role).replace("{prompt}", &prompt)
        })
        .collect();
    if !uses_prompt {
        if ctx.config.role_agent(role) == "claude" && !words.iter().any(|word| word == "--") {
            words.push("--".to_string());
        }
        words.push(prompt);
    }
    words
}

pub fn launch(ctx: &Ctx, role: &str, pane: &str, revive: bool) -> bool {
    let words = agent_words(ctx, role, revive);
    if !util::has(&words[0]) {
        ui::warn(format!("{role}: '{}' is not installed, pane left as a shell", words[0]));
        return false;
    }
    ctx.mux.start_agent(ctx, role, pane, &words)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crew::testutil::{fixture, ROLES};

    #[test]
    fn kickoff_names_the_role_and_files() {
        let f = fixture(ROLES);
        let prompt = kickoff_prompt(&f.ctx, "builder", false);
        let crew = f.ctx.crew.display();
        assert!(prompt.starts_with("You are the builder agent"));
        assert!(prompt.contains(&format!("{crew}/roles/builder.md")));
        assert!(!prompt.contains("restarted"));
        assert!(!prompt.contains("greet"));
    }

    #[test]
    fn restarted_agents_are_pointed_at_their_handoff() {
        let f = fixture(ROLES);
        let prompt = kickoff_prompt(&f.ctx, "builder", true);
        assert!(prompt.contains(&format!("{}/notes/builder.md", f.ctx.crew.display())));
    }

    #[test]
    fn only_a_fresh_lead_greets_the_human() {
        let f = fixture(ROLES);
        assert!(kickoff_prompt(&f.ctx, "lead", false).contains("greet the human"));
        assert!(!kickoff_prompt(&f.ctx, "lead", true).contains("greet the human"));
    }

    #[test]
    fn claude_gets_a_separator_before_the_prompt() {
        let f = fixture(ROLES);
        let words = agent_words(&f.ctx, "builder", false);
        assert_eq!(&words[..3], ["claude", "--add-dir", f.ctx.crew.to_str().unwrap()]);
        assert_eq!(words[3], "--");
        assert_eq!(words.iter().filter(|w| *w == "--").count(), 1);
        assert_eq!(words.last().unwrap(), &kickoff_prompt(&f.ctx, "builder", false));
    }

    #[test]
    fn other_agents_get_the_prompt_last_without_a_separator() {
        let f = fixture(ROLES);
        let words = agent_words(&f.ctx, "reviewer", false);
        assert_eq!(words[0], "codex");
        assert!(!words.contains(&"--".to_string()));
        assert_eq!(words.last().unwrap(), &kickoff_prompt(&f.ctx, "reviewer", false));
    }

    #[test]
    fn placeholders_are_filled_per_word() {
        let toml = "[roles.builder]\nagent = \"tool\"\n[agents.tool]\ncommand = [\"tool\", \"--as={role}\", \"{repo}\", \"-p\", \"{prompt}\"]\n";
        let f = fixture(toml);
        let words = agent_words(&f.ctx, "builder", false);
        assert_eq!(words.len(), 5);
        assert_eq!(words[1], "--as=builder");
        assert_eq!(words[2], f.ctx.root.display().to_string());
        assert_eq!(words[4], kickoff_prompt(&f.ctx, "builder", false));
    }

    #[test]
    fn launch_leaves_the_pane_alone_when_the_program_is_missing() {
        let toml = "[roles.builder]\nagent = \"ghost\"\n[agents.ghost]\ncommand = \"crew-no-such-program-xyz\"\n";
        let f = fixture(toml);
        assert!(!launch(&f.ctx, "builder", "p1", false));
        assert_eq!(f.calls(), Vec::<String>::new());
    }

    #[test]
    fn launch_starts_an_installed_program_in_the_pane() {
        let toml = "[roles.builder]\nagent = \"shell\"\n[agents.shell]\ncommand = \"sh -c\"\n";
        let f = fixture(toml);
        assert!(launch(&f.ctx, "builder", "p1", false));
        let calls = f.calls();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].starts_with("start_agent builder p1 sh -c You are the builder agent"));
    }
}
