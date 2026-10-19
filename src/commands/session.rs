use crate::crew::context::Ctx;
use crate::crew::{panes, protocol, watcher, worktree};
use crate::util::ui;
use anyhow::{bail, Result};

pub fn start(ctx: &mut Ctx, attach: bool) -> Result<()> {
    if attach {
        refuse_nesting(ctx.mux.name())?;
    }
    if ctx.require_crew().is_err() {
        super::setup::create(ctx)?;
    }
    ctx.mux.ensure(ctx)?;
    panes::forget_other_mux_state(ctx);
    protocol::write(ctx)?;
    for role in ctx.roles() {
        worktree::ensure(ctx, &role)?;
    }
    if ctx.mux.workspace_exists(ctx) {
        panes::revive(ctx);
    } else {
        panes::build(ctx)?;
    }
    watcher::start(ctx)?;
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

pub fn restart(ctx: &mut Ctx, target: &str) -> Result<()> {
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
        panes::close(ctx, role);
    }
    start(ctx, false)
}

pub fn stop(ctx: &Ctx) -> Result<()> {
    ctx.require_crew()?;
    watcher::stop(ctx);
    if ctx.mux.workspace_exists(ctx) {
        ctx.mux.close_workspace(ctx);
        ui::ok(format!("closed the crew's {} session", ctx.mux.name()));
    }
    ctx.state.clear();
    ui::info("board, tasks, notes and worktrees are kept. crew start brings everyone back.");
    Ok(())
}

pub fn teardown(ctx: &Ctx, yes: bool) -> Result<()> {
    ctx.require_crew()?;
    if !yes && !confirm("Stop the crew and remove its worktrees (crew/* branches are kept)? [y/N] ") {
        println!("cancelled");
        return Ok(());
    }
    stop(ctx)?;
    for role in ctx.roles() {
        worktree::remove(ctx, &role);
    }
    Ok(())
}

fn confirm(question: &str) -> bool {
    use std::io::Write;
    print!("{question}");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer).is_ok() && matches!(answer.trim(), "y" | "Y" | "yes")
}

// Opening one multiplexer inside the other is confusing (both default to ctrl+b), so refuse rather than nest.
fn refuse_nesting(mux: &str) -> Result<()> {
    let in_tmux = std::env::var_os("TMUX").is_some();
    let in_herdr = std::env::var("HERDR_ENV").as_deref() == Ok("1");
    if let Some(current) = nesting_conflict(mux, in_tmux, in_herdr) {
        bail!(
            "you're inside {current}, but this crew uses {mux}. Run crew start from a plain terminal or from {mux}, \
             set multiplexer = \"{current}\" in .crew/config.toml, or use crew start --no-attach"
        );
    }
    Ok(())
}

// tmux is checked first: TMUX is set in the innermost tmux even when that tmux runs inside herdr.
fn nesting_conflict(mux: &str, in_tmux: bool, in_herdr: bool) -> Option<&'static str> {
    let current = if in_tmux {
        "tmux"
    } else if in_herdr {
        "herdr"
    } else {
        return None;
    };
    (current != mux).then_some(current)
}

#[cfg(test)]
mod tests {
    use super::nesting_conflict;

    #[test]
    fn only_the_other_multiplexer_conflicts() {
        assert_eq!(nesting_conflict("herdr", true, false), Some("tmux"));
        assert_eq!(nesting_conflict("tmux", false, true), Some("herdr"));
        assert_eq!(nesting_conflict("tmux", true, false), None);
        assert_eq!(nesting_conflict("herdr", false, true), None);
        assert_eq!(nesting_conflict("herdr", false, false), None);
        assert_eq!(nesting_conflict("herdr", true, true), Some("tmux"));
    }
}
