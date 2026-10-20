use crate::crew::agents::agent_words;
use crate::crew::config::{Config, MuxKind};
use crate::crew::context::{repo_root, Ctx};
use crate::mux::tmux;
use crate::util::{self, ui};
use anyhow::Result;
use std::fmt::Write;

pub fn status(ctx: &Ctx) -> Result<()> {
    print!("{}", status_text(ctx)?);
    Ok(())
}

fn status_text(ctx: &Ctx) -> Result<String> {
    ctx.require_crew()?;
    let ready = ctx.mux.ready(ctx);
    let mut text = format!("{}\n", ui::bold(&format!("{:<10} {:<8} {:<10} WORKS IN", "ROLE", "AGENT", "STATUS")));
    for role in ctx.roles() {
        let status = if ready { ctx.mux.agent_status(ctx, &role) } else { "-".to_string() };
        let _ = writeln!(
            text,
            "{role:<10} {:<8} {status:<10} {}",
            ctx.config.role_agent(&role),
            util::tilde(&ctx.role_dir(&role))
        );
    }
    Ok(text)
}

// Works outside a repo too. Returns false if something required is missing.
pub fn doctor() -> Result<bool> {
    let ctx = if repo_root().is_some() { Ctx::load().ok() } else { None };
    let mux = match &ctx {
        Some(ctx) => ctx.config.multiplexer(),
        None => Config::load(None)?.multiplexer(),
    }
    .unwrap_or(MuxKind::Herdr);
    let mut fine = required(util::has("git"), "git found", "git missing");
    ui::info(format!("multiplexer: {}", mux.name()));
    fine &= match mux {
        MuxKind::Herdr => required(util::has("herdr"), "herdr found", "herdr missing (https://herdr.dev)"),
        MuxKind::Tmux => match tmux::major_version() {
            None => required(false, "", "tmux missing"),
            Some(v) => required(v >= 3, "tmux 3.0 or newer found", "tmux 3.0 or newer needed"),
        },
    };
    for agent in agents_to_check(ctx.as_ref()) {
        if util::has(&agent) {
            ui::ok(format!("agent {agent} found"));
        } else {
            ui::warn(format!("agent {agent} not installed"));
        }
    }
    let global = util::crew_home().join("config.toml");
    let note = if global.exists() { "" } else { " (not created; crew config init)" };
    ui::info(format!("global config: {}{note}", util::tilde(&global)));
    Ok(fine)
}

fn required(passed: bool, good: &str, bad: &str) -> bool {
    if passed {
        ui::ok(good);
    } else {
        ui::warn(bad);
    }
    passed
}

// The programs this repo's roles actually run, or the usual three outside a crew.
fn agents_to_check(ctx: Option<&Ctx>) -> Vec<String> {
    let mut agents: Vec<String> = match ctx.filter(|c| c.require_crew().is_ok()) {
        Some(ctx) => ctx.roles().iter().map(|role| agent_words(ctx, role, false)[0].clone()).collect(),
        None => vec!["claude".into(), "codex".into(), "gemini".into()],
    };
    agents.sort();
    agents.dedup();
    agents
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::testing::{crew_ctx, ctx_in, scratch_dir};
    use crate::util::testing::Fake;

    fn rows(ctx: &Ctx) -> Vec<String> {
        status_text(ctx).unwrap().lines().skip(1).map(String::from).collect()
    }

    #[test]
    fn status_shows_a_dash_for_every_agent_when_the_session_is_not_running() {
        let ctx = crew_ctx();
        let rows = rows(&ctx);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].starts_with("lead       claude   -          "), "{}", rows[0]);
        assert!(rows[0].ends_with(&util::tilde(&ctx.root)), "{}", rows[0]);
        assert!(rows[1].starts_with("builder    codex    -          "), "{}", rows[1]);
    }

    #[test]
    fn status_asks_the_multiplexer_when_the_session_is_up() {
        let ctx = crew_ctx();
        let fake = Fake::new();
        fake.program("tmux", "exit 0");
        let rows = rows(&ctx);
        assert!(rows[0].starts_with("lead       claude   stopped    "), "{}", rows[0]);
        assert!(rows[1].starts_with("builder    codex    stopped    "), "{}", rows[1]);
    }

    #[test]
    fn a_role_with_a_worktree_is_shown_in_its_worktree() {
        let ctx = crew_ctx();
        assert!(rows(&ctx)[1].ends_with(&util::tilde(&ctx.role_dir("builder"))));
    }

    #[test]
    fn status_needs_a_crew() {
        let ctx = ctx_in(&scratch_dir(), None);
        assert!(status(&ctx).unwrap_err().to_string().contains("crew setup"));
    }

    #[test]
    fn outside_a_crew_the_usual_agents_are_checked() {
        let ctx = ctx_in(&scratch_dir(), None);
        assert_eq!(agents_to_check(Some(&ctx)), ["claude", "codex", "gemini"]);
        assert_eq!(agents_to_check(None), ["claude", "codex", "gemini"]);
    }

    #[test]
    fn inside_a_crew_the_programs_its_roles_run_are_checked_once_each() {
        let ctx = ctx_in(
            &scratch_dir(),
            Some(
                "[roles.a]\nagent = \"codex\"\n[roles.b]\nagent = \"codex\"\n[roles.c]\nagent = \"aider\"\n\
                 [agents.aider]\ncommand = \"aider --yes\"\n",
            ),
        );
        assert_eq!(agents_to_check(Some(&ctx)), ["aider", "codex"]);
    }

    #[test]
    fn required_returns_whether_the_check_passed() {
        assert!(required(true, "fine", "bad"));
        assert!(!required(false, "fine", "bad"));
    }
}
