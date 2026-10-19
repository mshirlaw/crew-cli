use crate::crew::config::{self, MuxKind};
use crate::crew::context::Ctx;
use crate::crew::{protocol, templates};
use crate::util::{self, ui};
use anyhow::Result;

pub fn show(ctx: &Ctx) -> Result<()> {
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
    super::role::list(ctx)
}

pub fn edit(ctx: &mut Ctx) -> Result<()> {
    ctx.require_crew()?;
    util::edit_file(&ctx.file("config.toml"))?;
    ctx.reload()?;
    protocol::write(ctx)
}

pub fn edit_global() -> Result<()> {
    init()?;
    util::edit_file(&util::crew_home().join("config.toml"))
}

// Never overwrites an existing config; install.sh runs this on every install.
pub fn init() -> Result<()> {
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

pub fn example() {
    print!("{}", templates::GLOBAL_CONFIG);
}
