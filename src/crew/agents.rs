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
    use crate::crew::config::Config;
    use crate::crew::state::State;
    use crate::mux;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    fn context(config_text: &str) -> (Ctx, PathBuf) {
        let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("crew-agents-test-{}-{id}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let config_path = dir.join("config.toml");
        std::fs::write(&config_path, config_text).unwrap();
        let config = Config::load(Some(config_path)).unwrap();
        let crew = dir.join(".crew");
        let ctx = Ctx {
            root: dir.clone(),
            crew: crew.clone(),
            slug: "repo".to_string(),
            key: "repo-test".to_string(),
            config,
            state: State::new(crew.join("state")),
            mux: mux::make(crate::crew::config::MuxKind::Tmux),
        };
        (ctx, dir)
    }

    #[test]
    fn kickoff_prompt_includes_handoff_only_when_restarting() {
        let (ctx, dir) = context("[roles.builder]\nagent = \"codex\"\n");
        let fresh = kickoff_prompt(&ctx, "builder", false);
        let revived = kickoff_prompt(&ctx, "builder", true);
        assert!(!fresh.contains("notes/builder.md"));
        assert!(revived.contains("notes/builder.md"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn lead_prompt_asks_for_the_human_goal() {
        let (ctx, dir) = context("[roles.lead]\nagent = \"codex\"\n");
        assert!(kickoff_prompt(&ctx, "lead", false).contains("ask what the crew should work on"));
        assert!(!kickoff_prompt(&ctx, "lead", true).contains("ask what the crew should work on"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn claude_gets_a_prompt_delimiter() {
        let (ctx, dir) =
            context("[roles.builder]\nagent = \"claude\"\n[agents.claude]\ncommand = \"claude --add-dir {crew}\"\n");
        let words = agent_words(&ctx, "builder", false);
        assert_eq!(words[0], "claude");
        assert_eq!(words[1], "--add-dir");
        assert_eq!(words[3], "--");
        assert!(words[4].starts_with("You are the builder agent"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn explicit_prompt_placeholder_is_not_duplicated() {
        let (ctx, dir) = context(
            "[roles.builder]\nagent = \"custom\"\n[agents.custom]\ncommand = [\"custom\", \"--prompt\", \"{prompt}\"]\n",
        );
        let words = agent_words(&ctx, "builder", false);
        assert_eq!(words.len(), 3);
        assert!(words[2].contains("You are the builder agent"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
