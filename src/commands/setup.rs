use crate::crew::config::first_installed;
use crate::crew::context::Ctx;
use crate::crew::{protocol, tasks, templates};
use crate::util::{self, ui};
use anyhow::{bail, Result};
use std::path::Path;

pub fn setup(ctx: &mut Ctx) -> Result<()> {
    if ctx.require_crew().is_ok() {
        ui::info(format!("crew already set up in {}", ctx.crew.display()));
        return protocol::write(ctx);
    }
    create(ctx)?;
    ui::ok(format!("crew set up in {} (multiplexer: {})", ctx.crew.display(), ctx.mux.name()));
    println!();
    super::role::list(ctx)?;
    println!(
        "\n{} crew start   {}",
        ui::bold("Next:"),
        ui::dim("(customise first with crew role edit / crew role add / crew config edit)")
    );
    Ok(())
}

pub fn create(ctx: &mut Ctx) -> Result<()> {
    if !util::run_ok("git", &["-C", &ctx.root.display().to_string(), "rev-parse", "--verify", "-q", "HEAD"]) {
        bail!("this repo has no commits yet. Make one first.");
    }
    std::fs::create_dir_all(ctx.file("roles"))?;
    std::fs::create_dir_all(ctx.file("notes"))?;
    std::fs::create_dir_all(ctx.state.dir())?;
    write_repo_config(ctx)?;
    ctx.reload()?;
    write_role_files(ctx)?;
    if !ctx.file("board.md").exists() {
        std::fs::write(ctx.file("board.md"), "# Crew board\n\n")?;
    }
    tasks::init(ctx)?;
    protocol::write(ctx)?;
    hide_from_git(ctx)
}

// Roles come from your global config if you've saved some, otherwise lead/builder/reviewer
// using whichever agents are installed.
fn write_repo_config(ctx: &Ctx) -> Result<()> {
    let root = ctx.root.display().to_string();
    let base = util::run_out("git", &["-C", &root, "symbolic-ref", "--short", "HEAD"])
        .or_else(|| util::run_out("git", &["-C", &root, "rev-parse", "--short", "HEAD"]))
        .map_or_else(|| "main".to_string(), |s| s.trim().to_string());
    let roles = if ctx.config.global_has_roles() {
        format!("\n{}", ctx.config.global_roles_text())
    } else {
        if !["claude", "codex", "gemini"].iter().any(|a| util::has(a)) {
            ui::warn("none of claude, codex or gemini found. Edit .crew/config.toml");
        }
        templates::default_roles(
            &first_installed(&["claude", "codex", "gemini"]),
            &first_installed(&["codex", "claude", "gemini"]),
            &first_installed(&["gemini", "claude", "codex"]),
        )
    };
    std::fs::write(ctx.file("config.toml"), templates::repo_config_header(&base) + &roles)?;
    Ok(())
}

fn write_role_files(ctx: &Ctx) -> Result<()> {
    for role in ctx.roles() {
        let path = ctx.file(&format!("roles/{role}.md"));
        if !path.exists() {
            std::fs::write(path, templates::role_template(&role))?;
        }
    }
    Ok(())
}

// .git/info/exclude rather than .gitignore, so crew never changes tracked files.
fn hide_from_git(ctx: &Ctx) -> Result<()> {
    let root = ctx.root.display().to_string();
    let Some(path) = util::run_out("git", &["-C", &root, "rev-parse", "--git-path", "info/exclude"]) else {
        return Ok(());
    };
    let path = Path::new(path.trim());
    let path = if path.is_absolute() { path.to_path_buf() } else { ctx.root.join(path) };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    if !text.lines().any(|line| line.trim() == ".crew/") {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let separator = if text.is_empty() || text.ends_with('\n') { "" } else { "\n" };
        std::fs::write(&path, format!("{text}{separator}.crew/\n"))?;
    }
    Ok(())
}
