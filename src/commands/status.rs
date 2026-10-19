use crate::crew::agents::agent_words;
use crate::crew::config::{Config, MuxKind};
use crate::crew::context::{repo_root, Ctx};
use crate::mux::tmux;
use crate::util::{self, ui};
use anyhow::Result;

pub fn status(ctx: &Ctx) -> Result<()> {
    ctx.require_crew()?;
    let ready = ctx.mux.ready(ctx);
    println!("{}", ui::bold(&format!("{:<10} {:<8} {:<10} WORKS IN", "ROLE", "AGENT", "STATUS")));
    for role in ctx.roles() {
        let status = if ready { ctx.mux.agent_status(ctx, &role) } else { "-".to_string() };
        println!("{role:<10} {:<8} {status:<10} {}", ctx.config.role_agent(&role), util::tilde(&ctx.role_dir(&role)));
    }
    Ok(())
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
