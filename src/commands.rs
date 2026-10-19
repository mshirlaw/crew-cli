//! crew status, crew doctor and crew config.

use crate::agents::agent_words;
use crate::config::{self, Config, MuxKind};
use crate::context::{repo_root, Ctx};
use crate::mux::tmux;
use crate::{setup, templates, ui, util};
use anyhow::Result;

/// crew status: prints each role, its agent, its state and its folder.
pub fn cmd_status(ctx: &Ctx) -> Result<()> {
    ctx.require_crew()?;
    let ready = ctx.mux.ready(ctx);
    println!("{}", ui::bold(&format!("{:<10} {:<8} {:<10} {}", "ROLE", "AGENT", "STATUS", "WORKS IN")));
    for role in ctx.roles() {
        let status = if ready { ctx.mux.agent_status(ctx, &role) } else { "-".to_string() };
        println!("{role:<10} {:<8} {status:<10} {}", ctx.config.role_agent(&role), util::tilde(&ctx.role_dir(&role)));
    }
    Ok(())
}

/// crew doctor: checks that everything crew needs is installed. Fails if something required is missing.
pub fn cmd_doctor() -> Result<bool> {
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
    for agent in doctor_agents(ctx.as_ref()) {
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

/// Reports a required check and returns whether it passed.
fn required(passed: bool, good: &str, bad: &str) -> bool {
    if passed {
        ui::ok(good);
    } else {
        ui::warn(bad);
    }
    passed
}

/// The agent programs to check: the ones this repo's roles use, or the usual three.
fn doctor_agents(ctx: Option<&Ctx>) -> Vec<String> {
    let mut agents: Vec<String> = match ctx.filter(|c| c.require_crew().is_ok()) {
        Some(ctx) => ctx.roles().iter().map(|role| agent_words(ctx, role, false)[0].clone()).collect(),
        None => vec!["claude".into(), "codex".into(), "gemini".into()],
    };
    agents.sort();
    agents.dedup();
    agents
}

/// crew config init: creates the global config if it doesn't exist yet, choosing herdr or tmux.
pub fn cmd_config_init() -> Result<()> {
    let path = util::crew_home().join("config.toml");
    if path.exists() {
        ui::info(format!("{} already exists, left unchanged", util::tilde(&path)));
        return Ok(());
    }
    let mux = MuxKind::detect();
    config::write_global_config(&path, mux)?;
    ui::ok(format!("created {} (multiplexer: {})", util::tilde(&path), mux.name()));
    Ok(())
}

/// crew config example: prints the sample config.
pub fn cmd_config_example() {
    print!("{}", templates::GLOBAL_CONFIG);
}

/// crew config edit --global: opens the global config, creating it first if needed.
pub fn cmd_config_edit_global() -> Result<()> {
    cmd_config_init()?;
    util::edit_file(&util::crew_home().join("config.toml"))
}

/// crew config edit: opens this repo's config, then refreshes PROTOCOL.md.
pub fn cmd_config_edit(ctx: &mut Ctx) -> Result<()> {
    ctx.require_crew()?;
    util::edit_file(&ctx.file("config.toml"))?;
    ctx.reload()?;
    setup::write_protocol(ctx)
}

/// crew config: prints the config files and the settings in effect for this repo.
pub fn cmd_config_show(ctx: &Ctx) -> Result<()> {
    ctx.require_crew()?;
    let global = &ctx.config.global_path;
    let note = if global.exists() { "" } else { "  (not created; crew config init)" };
    println!("{}", ui::bold("Config files"));
    println!("  global  {}{note}", util::tilde(global));
    println!("  repo    .crew/config.toml\n");
    println!("{}", ui::bold("Settings"));
    println!("  multiplexer     {}", ctx.mux.name());
    println!("  worktree_dir    {}", ctx.config.worktree_dir_text());
    println!("  base_branch     {}", ctx.base_branch());
    if ctx.mux.name() == "tmux" {
        println!("  paste_delay     {}s", ctx.config.paste_delay());
    }
    println!("  stall_timeout   {}s", ctx.config.stall_timeout());
    println!("  watch_interval  {}s\n", ctx.config.watch_interval());
    println!("{}", ui::bold("Agents in use"));
    let mut agents: Vec<String> = ctx.roles().iter().map(|r| ctx.config.role_agent(r)).collect();
    agents.sort();
    agents.dedup();
    for agent in agents {
        let command = ctx.config.agent_command(&agent).join(" ");
        if ctx.mux.name() == "herdr" {
            let kind = ctx.config.agent_kind(&agent).unwrap_or_else(|| "none, typed in".into());
            println!("  {agent:<8} {command}   (herdr kind: {kind})");
        } else {
            println!("  {agent:<8} {command}");
        }
    }
    println!();
    crate::roles::cmd_list(ctx)
}
