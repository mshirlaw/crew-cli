use crate::crew::config::{self, MuxKind};
use crate::crew::context::Ctx;
use crate::crew::{protocol, templates};
use crate::util::{self, ui};
use anyhow::Result;
use std::fmt::Write;
use std::path::Path;

pub fn show(ctx: &Ctx) -> Result<()> {
    print!("{}", show_text(ctx)?);
    Ok(())
}

fn show_text(ctx: &Ctx) -> Result<String> {
    ctx.require_crew()?;
    let mut out = String::new();
    let global = &ctx.config.global_path;
    let note = if global.exists() { "" } else { "  (not created; crew config init)" };
    let _ = writeln!(out, "{}", ui::bold("Config files"));
    let _ = writeln!(out, "  global  {}{note}", util::tilde(global));
    let _ = writeln!(out, "  repo    .crew/config.toml\n");
    let _ = writeln!(out, "{}", ui::bold("Settings"));
    let _ = writeln!(out, "  multiplexer     {}", ctx.mux.name());
    let _ = writeln!(out, "  worktree_dir    {}", ctx.config.worktree_dir_text());
    let _ = writeln!(out, "  base_branch     {}", ctx.base_branch());
    if ctx.mux.name() == "tmux" {
        let _ = writeln!(out, "  paste_delay     {}s", ctx.config.paste_delay());
    }
    let _ = writeln!(out, "  stall_timeout   {}s", ctx.config.stall_timeout());
    let _ = writeln!(out, "  watch_interval  {}s\n", ctx.config.watch_interval());
    let _ = writeln!(out, "{}", ui::bold("Agents in use"));
    let mut agents: Vec<String> = ctx.roles().iter().map(|r| ctx.config.role_agent(r)).collect();
    agents.sort();
    agents.dedup();
    for agent in agents {
        let command = ctx.config.agent_command(&agent).join(" ");
        if ctx.mux.name() == "herdr" {
            let kind = ctx.config.agent_kind(&agent).unwrap_or_else(|| "none, typed in".into());
            let _ = writeln!(out, "  {agent:<8} {command}   (herdr kind: {kind})");
        } else {
            let _ = writeln!(out, "  {agent:<8} {command}");
        }
    }
    out.push('\n');
    out.push_str(&super::role::list_text(ctx)?);
    Ok(out)
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
    init_at(&util::crew_home().join("config.toml"), MuxKind::detect())
}

fn init_at(path: &Path, mux: MuxKind) -> Result<()> {
    if path.exists() {
        ui::info(format!("{} already exists, left unchanged", util::tilde(path)));
        return Ok(());
    }
    config::write_global_config(path, mux)?;
    ui::ok(format!("created {} (multiplexer: {})", util::tilde(path), mux.name()));
    Ok(())
}

pub fn example() {
    print!("{}", templates::GLOBAL_CONFIG);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::testing::{crew_ctx, ctx_in, scratch_dir};

    #[test]
    fn show_lists_the_settings_in_effect_for_tmux() {
        let text = show_text(&crew_ctx()).unwrap();
        for line in [
            "  multiplexer     tmux",
            "  worktree_dir    ~/.crew/worktrees",
            "  base_branch     main",
            "  paste_delay     0.3s",
            "  stall_timeout   600s",
            "  watch_interval  30s",
            "  repo    .crew/config.toml",
        ] {
            assert!(text.lines().any(|l| l == line), "missing {line:?} in\n{text}");
        }
        assert!(text.contains("builder    codex"), "roles table follows the settings:\n{text}");
    }

    #[test]
    fn show_agents_once_each_with_their_command() {
        let ctx = ctx_in(
            &scratch_dir(),
            Some("multiplexer = \"tmux\"\n[roles.a]\nagent = \"codex\"\n[roles.b]\nagent = \"codex\"\n[agents.codex]\ncommand = \"codex --x\"\n"),
        );
        let text = show_text(&ctx).unwrap();
        assert_eq!(text.lines().filter(|l| l.starts_with("  codex    ")).count(), 1, "{text}");
        assert!(text.contains("  codex    codex --x\n"), "{text}");
    }

    #[test]
    fn show_needs_a_crew() {
        let ctx = ctx_in(&scratch_dir(), None);
        assert!(show(&ctx).unwrap_err().to_string().contains("crew setup"));
    }

    #[test]
    fn show_with_herdr_has_no_paste_delay_but_shows_each_agents_herdr_kind() {
        let ctx = ctx_in(
            &scratch_dir(),
            Some("multiplexer = \"herdr\"\n[roles.a]\nagent = \"claude\"\n[roles.b]\nagent = \"aider\"\n[agents.aider]\ncommand = \"aider\"\n"),
        );
        let text = show_text(&ctx).unwrap();
        assert!(!text.contains("paste_delay"), "{text}");
        assert!(text.contains("(herdr kind: claude)"), "{text}");
        assert!(text.contains("aider    aider   (herdr kind: none, typed in)"), "{text}");
    }

    #[test]
    fn edit_needs_a_crew() {
        let mut ctx = ctx_in(&scratch_dir(), None);
        assert!(edit(&mut ctx).unwrap_err().to_string().contains("crew setup"));
    }

    #[test]
    fn init_writes_the_sample_config_for_the_detected_multiplexer() {
        let path = scratch_dir().join("nested/config.toml");
        init_at(&path, MuxKind::Tmux).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("multiplexer = \"tmux\""));
        assert_eq!(text, templates::GLOBAL_CONFIG.replacen("multiplexer = \"herdr\"", "multiplexer = \"tmux\"", 1));
    }

    #[test]
    fn init_never_overwrites_an_existing_config() {
        let path = scratch_dir().join("config.toml");
        std::fs::write(&path, "multiplexer = \"herdr\"\n# mine\n").unwrap();
        init_at(&path, MuxKind::Tmux).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "multiplexer = \"herdr\"\n# mine\n");
    }

    #[test]
    fn the_sample_config_is_valid_toml_that_names_a_multiplexer() {
        let doc: toml_edit::DocumentMut = templates::GLOBAL_CONFIG.parse().unwrap();
        assert!(doc.get("multiplexer").is_some());
    }
}
