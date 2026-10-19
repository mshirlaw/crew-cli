//! Launching agents: the kickoff prompt and each agent's command line.

use crate::context::Ctx;
use crate::{ui, util};

/// The first message an agent receives, telling it where its instructions are.
pub fn kickoff_prompt(ctx: &Ctx, role: &str, revive: bool) -> String {
    let crew = ctx.crew.display();
    let mut msg = format!(
        "You are the {role} agent in a multi-agent crew. Read {crew}/PROTOCOL.md and {crew}/roles/{role}.md \
         now and follow them, then read {crew}/board.md and {crew}/tasks.md."
    );
    if revive {
        msg.push_str(&format!(
            " You were restarted: also read {crew}/notes/{role}.md for your own handoff and continue from there."
        ));
    } else if role == "lead" {
        msg.push_str(" Then briefly greet the human and ask what the crew should work on.");
    }
    msg
}

/// A role's full agent command, with {crew} {repo} {role} and {prompt} filled in.
/// The prompt is added as the last word unless the command uses {prompt}.
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

/// Starts a role's agent in a pane. Leaves the pane as a shell if the agent isn't installed.
pub fn launch_agent(ctx: &Ctx, role: &str, pane: &str, revive: bool) -> bool {
    let words = agent_words(ctx, role, revive);
    if !util::has(&words[0]) {
        ui::warn(format!("{role}: '{}' is not installed, pane left as a shell", words[0]));
        return false;
    }
    ctx.mux.start_agent(ctx, role, pane, &words)
}
