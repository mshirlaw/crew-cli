//! Starting, reviving, restarting and stopping the crew, and the worktrees behind it.

use crate::agents::launch_agent;
use crate::context::Ctx;
use crate::mux::Direction;
use crate::util::{run_all, run_ok, tilde};
use crate::{board, setup, ui, watch};
use anyhow::{bail, Result};
use std::path::Path;
use std::time::Duration;

/// crew start: sets up if needed, starts or revives every agent, then opens the session.
pub fn cmd_start(ctx: &mut Ctx, attach: bool) -> Result<()> {
    if ctx.require_crew().is_err() {
        setup::create(ctx)?;
    }
    ctx.mux.ensure(ctx)?;
    forget_other_mux_state(ctx);
    setup::write_protocol(ctx)?;
    for role in ctx.roles() {
        ensure_worktree(ctx, &role)?;
    }
    if ctx.mux.workspace_exists(ctx) {
        revive_crew(ctx);
    } else {
        build_crew(ctx)?;
    }
    watch::start_watcher(ctx)?;
    println!(
        "\n{} Talk to the lead in its pane, or run: {}",
        ui::bold("Crew is up."),
        ui::bold("crew say lead \"...\"")
    );
    if attach && ui::stdout_is_terminal() {
        ctx.mux.attach(ctx)?;
    }
    Ok(())
}

/// crew restart <role|all>: closes agents' panes and starts them again.
pub fn cmd_restart(ctx: &mut Ctx, target: &str) -> Result<()> {
    ctx.require_crew()?;
    let roles = if target == "all" {
        ctx.roles()
    } else if ctx.role_exists(target) {
        vec![target.to_string()]
    } else {
        bail!("no role '{target}'");
    };
    ctx.mux.ensure(ctx)?;
    for role in &roles {
        close_role_pane(ctx, role);
    }
    cmd_start(ctx, false)
}

/// crew stop: closes the session and the watcher. Files and worktrees stay.
pub fn cmd_stop(ctx: &Ctx) -> Result<()> {
    ctx.require_crew()?;
    watch::stop_watcher(ctx);
    if ctx.mux.workspace_exists(ctx) {
        ctx.mux.close_workspace(ctx);
        ui::ok(format!("closed the crew's {} session", ctx.mux.name()));
    }
    ctx.state.clear();
    ui::info("board, tasks, notes and worktrees are kept. crew start brings everyone back.");
    Ok(())
}

/// crew teardown: stops the crew and removes worktrees that have no uncommitted changes.
pub fn cmd_teardown(ctx: &Ctx, yes: bool) -> Result<()> {
    ctx.require_crew()?;
    if !yes && !confirm("Stop the crew and remove its worktrees (crew/* branches are kept)? [y/N] ") {
        println!("cancelled");
        return Ok(());
    }
    cmd_stop(ctx)?;
    for role in ctx.roles() {
        remove_worktree(ctx, &role);
    }
    Ok(())
}

/// Asks a yes/no question on the terminal.
fn confirm(question: &str) -> bool {
    use std::io::Write;
    print!("{question}");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer).is_ok() && matches!(answer.trim(), "y" | "Y" | "yes")
}

/// Forgets saved panes if they belong to a different multiplexer than the one configured now.
fn forget_other_mux_state(ctx: &Ctx) {
    let saved = ctx.state.get("mux").unwrap_or_else(|| "herdr".to_string());
    if saved != ctx.mux.name() {
        ctx.state.clear();
    }
}

/// Builds a new session: first role on the left, the rest stacked on the right, plus a board tab.
fn build_crew(ctx: &Ctx) -> Result<()> {
    let roles = ctx.roles();
    let Some(first) = roles.first() else {
        bail!("no roles in .crew/config.toml. Add one with: crew role add <name>");
    };
    ctx.state.clear();
    ctx.state.set("mux", ctx.mux.name());
    ctx.mux.create_workspace(ctx, first)?;
    let mut right: Option<String> = None;
    for role in roles.iter().skip(1) {
        let pane = match &right {
            None => ctx
                .state
                .get(&format!("pane:{first}"))
                .and_then(|from| ctx.mux.split(ctx, role, &from, Direction::Right)),
            Some(above) => ctx.mux.split(ctx, role, above, Direction::Down),
        };
        if let Some(pane) = pane {
            ctx.state.set(&format!("pane:{role}"), &pane);
            right = Some(pane);
        }
    }
    ctx.mux.finish_layout(ctx);
    open_board_tab(ctx);
    std::thread::sleep(Duration::from_millis(500));
    for role in &roles {
        if let Some(pane) = ctx.state.get(&format!("pane:{role}")) {
            launch_agent(ctx, role, &pane, false);
        }
    }
    board::post(ctx, "crew", "all", &format!("Crew started: {}", roles.join(" ")))
}

/// Relaunches any agent that stopped, recreating its pane if it was closed.
fn revive_crew(ctx: &Ctx) {
    ui::info("crew is already open, reviving anything that stopped");
    for role in ctx.roles() {
        if ctx.mux.agent_running(ctx, &role) {
            ui::ok(format!("{role} is running"));
            continue;
        }
        if let Some(pane) = pane_for_role(ctx, &role) {
            launch_agent(ctx, &role, &pane, true);
        }
    }
    ctx.mux.finish_layout(ctx);
}

/// A role's pane id, making a new pane if its old one is gone.
fn pane_for_role(ctx: &Ctx, role: &str) -> Option<String> {
    let key = format!("pane:{role}");
    if let Some(pane) = ctx.state.get(&key).filter(|p| ctx.mux.pane_exists(p)) {
        return Some(pane);
    }
    let pane = match any_live_pane(ctx) {
        Some(anchor) => ctx.mux.split(ctx, role, &anchor, Direction::Down),
        None => ctx.mux.new_tab(ctx, "agents", &ctx.role_dir(role), Some(role)),
    };
    let Some(pane) = pane else {
        ui::warn(format!("could not create a pane for {role}"));
        return None;
    };
    ctx.state.set(&key, &pane);
    std::thread::sleep(Duration::from_millis(500));
    Some(pane)
}

/// The pane id of any role whose pane still exists.
pub fn any_live_pane(ctx: &Ctx) -> Option<String> {
    ctx.roles().iter().filter_map(|role| ctx.state.get(&format!("pane:{role}"))).find(|pane| ctx.mux.pane_exists(pane))
}

/// Opens the board tab, which shows the conversation live.
fn open_board_tab(ctx: &Ctx) {
    if let Some(pane) = ctx.mux.new_tab(ctx, "board", &ctx.crew, None) {
        ctx.state.set("board", &pane);
        std::thread::sleep(Duration::from_millis(500));
        ctx.mux.run(&pane, "tail -n 40 -F board.md");
    }
}

/// Closes a role's pane and forgets it.
pub fn close_role_pane(ctx: &Ctx, role: &str) {
    let key = format!("pane:{role}");
    if let Some(pane) = ctx.state.get(&key) {
        if ctx.mux.pane_exists(&pane) {
            ctx.mux.close_pane(&pane);
        }
    }
    ctx.state.set(&key, "");
}

/// Makes sure a role that needs a worktree has one on branch crew/<role>.
fn ensure_worktree(ctx: &Ctx, role: &str) -> Result<()> {
    if !ctx.config.role_has_worktree(role) {
        return Ok(());
    }
    let dir = ctx.role_dir(role);
    if dir.is_dir() && run_ok("git", &["-C", &dir.display().to_string(), "rev-parse", "--git-dir"]) {
        return Ok(());
    }
    create_worktree(ctx, role, &dir)
}

/// Creates a worktree for a role, reusing its branch if it already exists.
fn create_worktree(ctx: &Ctx, role: &str, dir: &Path) -> Result<()> {
    let root = ctx.root.display().to_string();
    let dir_text = dir.display().to_string();
    let branch = format!("crew/{role}");
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    run_ok("git", &["-C", &root, "worktree", "prune"]);
    let branch_exists =
        run_ok("git", &["-C", &root, "show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")]);
    let (success, out) = if branch_exists {
        run_all("git", &["-C", &root, "worktree", "add", "-q", &dir_text, &branch])
    } else {
        run_all("git", &["-C", &root, "worktree", "add", "-q", "-b", &branch, &dir_text, &ctx.base_branch()])
    };
    if !success {
        bail!("could not create a worktree for {role}: {}", out.trim());
    }
    ui::ok(format!("worktree {branch} → {}", tilde(dir)));
    Ok(())
}

/// Removes a role's worktree, but only if it has no uncommitted changes.
fn remove_worktree(ctx: &Ctx, role: &str) {
    if !ctx.config.role_has_worktree(role) {
        return;
    }
    let dir = ctx.role_dir(role);
    if !dir.is_dir() {
        return;
    }
    let dir_text = dir.display().to_string();
    if run_ok("git", &["-C", &ctx.root.display().to_string(), "worktree", "remove", &dir_text]) {
        ui::ok(format!("removed worktree {}", tilde(&dir)));
    } else {
        ui::warn(format!(
            "kept {dir_text}: it has uncommitted changes (commit them, or: git worktree remove --force {dir_text})"
        ));
    }
}
