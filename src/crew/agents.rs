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
