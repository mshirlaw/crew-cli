//! The stall watcher. An agent is stalled when it owes work (an open task or an unanswered message)
//! and its screen hasn't changed for `stall_timeout` seconds. The lead is told once per stall;
//! if the lead itself is stalled, the human is told instead.

use crate::context::Ctx;
use crate::{board, tasks, ui, util};
use anyhow::Result;
use std::path::PathBuf;
use std::time::Duration;

/// What the watcher remembers about a role between checks.
struct Seen {
    fingerprint: String,
    changed_at: u64,
    notified: bool,
}

/// The file a role's watch record is kept in.
fn seen_file(ctx: &Ctx, role: &str) -> PathBuf {
    ctx.state.dir().join(format!("watch-{role}"))
}

/// Reads a role's watch record.
fn read_seen(ctx: &Ctx, role: &str) -> Option<Seen> {
    let text = std::fs::read_to_string(seen_file(ctx, role)).ok()?;
    let mut parts = text.split_whitespace();
    Some(Seen {
        fingerprint: parts.next()?.to_string(),
        changed_at: parts.next()?.parse().ok()?,
        notified: parts.next()? == "yes",
    })
}

/// Saves a role's watch record.
fn write_seen(ctx: &Ctx, role: &str, seen: &Seen) {
    let notified = if seen.notified { "yes" } else { "no" };
    let _ = std::fs::create_dir_all(ctx.state.dir());
    let _ = std::fs::write(seen_file(ctx, role), format!("{} {} {notified}\n", seen.fingerprint, seen.changed_at));
}

/// Why a role owes work, or None if it doesn't.
fn owed_work(ctx: &Ctx, role: &str) -> Option<String> {
    let open: Vec<String> = tasks::open_tasks(ctx, role).iter().map(u32::to_string).collect();
    let board_text = std::fs::read_to_string(ctx.file("board.md")).unwrap_or_default();
    let unanswered = board::has_unanswered_message(&board_text, role);
    match (open.is_empty(), unanswered) {
        (true, false) => None,
        (true, true) => Some("an unanswered message".to_string()),
        (false, false) => Some(format!("open task(s) {}", open.join(" "))),
        (false, true) => Some(format!("open task(s) {} and an unanswered message", open.join(" "))),
    }
}

/// Checks one role and raises an alert if it has just stalled.
fn check_role(ctx: &Ctx, role: &str) -> Result<()> {
    let now = util::now_secs();
    let fingerprint = util::fingerprint(&ctx.mux.screen(ctx, role));
    let seen = read_seen(ctx, role);
    let Some(seen) = seen.filter(|s| s.fingerprint == fingerprint) else {
        write_seen(ctx, role, &Seen { fingerprint, changed_at: now, notified: false });
        return Ok(());
    };
    if seen.notified {
        return Ok(());
    }
    let Some(owed) = owed_work(ctx, role) else {
        return Ok(());
    };
    let idle = now.saturating_sub(seen.changed_at);
    if idle < ctx.config.stall_timeout() {
        return Ok(());
    }
    write_seen(ctx, role, &Seen { notified: true, ..seen });
    raise_alert(ctx, role, &owed, idle)
}

/// Tells the lead (or the human, if the lead is the one stalled) that a role looks stuck.
fn raise_alert(ctx: &Ctx, role: &str, owed: &str, idle: u64) -> Result<()> {
    let text = format!(
        "{role} has shown no activity for {} but has {owed}. It may be out of tokens, waiting for an approval, or stuck.",
        util::duration(idle)
    );
    if role == "lead" || !ctx.role_exists("lead") {
        board::post(ctx, "crew", "human", &format!("{text} Check its pane."))?;
        ctx.mux.notify(ctx, &text);
    } else {
        board::post(
            ctx,
            "crew",
            "lead",
            &format!("{text} Check on it with crew say {role}, or reassign its work with crew task set."),
        )?;
        let message = format!(
            "[crew] {text} Check on it with: crew say {role} \"...\", or reassign its work with: crew task set <id> owner=<role>."
        );
        if !ctx.mux.deliver(ctx, "lead", &message) {
            ctx.mux.notify(ctx, &text);
        }
    }
    ui::warn(format!("stall: {text}"));
    Ok(())
}

/// Checks every role once.
pub fn watch_once(ctx: &Ctx) -> Result<()> {
    for role in ctx.roles() {
        check_role(ctx, &role)?;
    }
    Ok(())
}

/// The file holding the background watcher's process id.
fn pid_file(ctx: &Ctx) -> PathBuf {
    ctx.state.dir().join("watch.pid")
}

/// The background watcher's process id, if it's running.
fn watcher_pid(ctx: &Ctx) -> Option<String> {
    let pid = std::fs::read_to_string(pid_file(ctx)).ok()?.trim().to_string();
    util::run_ok("kill", &["-0", &pid]).then_some(pid)
}

/// Starts the watcher in the background unless it's turned off or already running.
pub fn start_watcher(ctx: &Ctx) -> Result<()> {
    if ctx.config.stall_timeout() == 0 || watcher_pid(ctx).is_some() {
        return Ok(());
    }
    for role in ctx.roles() {
        let _ = std::fs::remove_file(seen_file(ctx, &role));
    }
    let program = std::env::current_exe()?;
    let pid = util::spawn_detached(
        program.as_os_str(),
        &["watch"],
        &[("CREW_DIR", ctx.crew.as_os_str())],
        &ctx.state.dir().join("watch.log"),
    )?;
    std::fs::write(pid_file(ctx), format!("{pid}\n"))?;
    ui::info(format!("watching for stalled agents (stall_timeout: {}s)", ctx.config.stall_timeout()));
    Ok(())
}

/// Stops the background watcher.
pub fn stop_watcher(ctx: &Ctx) {
    if let Some(pid) = watcher_pid(ctx) {
        util::run_ok("kill", &[pid.as_str()]);
    }
    let _ = std::fs::remove_file(pid_file(ctx));
}

/// crew watch [--once]: checks for stalled agents every `watch_interval` seconds until the crew stops.
pub fn cmd_watch(ctx: &Ctx, once: bool) -> Result<()> {
    ctx.require_crew()?;
    if once {
        return watch_once(ctx);
    }
    let me = std::process::id().to_string();
    let mut misses = 0;
    let mut first = true;
    while misses < 3 {
        // Stop if crew stop removed our pid file, or another watcher replaced us. On the first
        // check the file may not be written yet, because crew start writes it just after starting us.
        let owner = std::fs::read_to_string(pid_file(ctx)).ok().map(|s| s.trim().to_string());
        match owner {
            Some(pid) if pid != me => break,
            None if !first => break,
            _ => {}
        }
        first = false;
        if ctx.mux.ready(ctx) {
            misses = 0;
            if let Err(e) = watch_once(ctx) {
                ui::warn(format!("watch: {e:#}"));
            }
        } else {
            misses += 1;
        }
        std::thread::sleep(Duration::from_secs(ctx.config.watch_interval()));
    }
    Ok(())
}
