// An agent is stalled when it owes work (an open task or an unanswered message) and its screen
// hasn't changed for stall_timeout seconds. The lead is told once per stall; if the lead is the
// one stalled, the human is told instead.

use super::context::Ctx;
use super::{board, tasks};
use crate::util::{self, ui};
use anyhow::Result;
use std::path::PathBuf;
use std::time::Duration;

struct Seen {
    fingerprint: String,
    changed_at: u64,
    notified: bool,
}

fn seen_file(ctx: &Ctx, role: &str) -> PathBuf {
    ctx.state.dir().join(format!("watch-{role}"))
}

fn read_seen(ctx: &Ctx, role: &str) -> Option<Seen> {
    let text = std::fs::read_to_string(seen_file(ctx, role)).ok()?;
    let mut parts = text.split_whitespace();
    Some(Seen {
        fingerprint: parts.next()?.to_string(),
        changed_at: parts.next()?.parse().ok()?,
        notified: parts.next()? == "yes",
    })
}

fn write_seen(ctx: &Ctx, role: &str, seen: &Seen) {
    let notified = if seen.notified { "yes" } else { "no" };
    let _ = std::fs::create_dir_all(ctx.state.dir());
    let _ = std::fs::write(seen_file(ctx, role), format!("{} {} {notified}\n", seen.fingerprint, seen.changed_at));
}

fn owed_work(ctx: &Ctx, role: &str) -> Option<String> {
    let open: Vec<String> = tasks::open_for(ctx, role).iter().map(u32::to_string).collect();
    let board_text = std::fs::read_to_string(ctx.file("board.md")).unwrap_or_default();
    let unanswered = board::has_unanswered_message(&board_text, role);
    match (open.is_empty(), unanswered) {
        (true, false) => None,
        (true, true) => Some("an unanswered message".to_string()),
        (false, false) => Some(format!("open task(s) {}", open.join(" "))),
        (false, true) => Some(format!("open task(s) {} and an unanswered message", open.join(" "))),
    }
}

fn check_role(ctx: &Ctx, role: &str) -> Result<()> {
    let now = util::now_secs();
    let fingerprint = util::fingerprint(&ctx.mux.screen(ctx, role));
    let Some(seen) = read_seen(ctx, role).filter(|s| s.fingerprint == fingerprint) else {
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

pub fn check_all(ctx: &Ctx) -> Result<()> {
    for role in ctx.roles() {
        check_role(ctx, &role)?;
    }
    Ok(())
}

fn pid_file(ctx: &Ctx) -> PathBuf {
    ctx.state.dir().join("watch.pid")
}

fn running_pid(ctx: &Ctx) -> Option<String> {
    let pid = std::fs::read_to_string(pid_file(ctx)).ok()?.trim().to_string();
    util::run_ok("kill", &["-0", &pid]).then_some(pid)
}

pub fn start(ctx: &Ctx) -> Result<()> {
    if ctx.config.stall_timeout() == 0 || running_pid(ctx).is_some() {
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

pub fn stop(ctx: &Ctx) {
    if let Some(pid) = running_pid(ctx) {
        util::run_ok("kill", &[pid.as_str()]);
    }
    let _ = std::fs::remove_file(pid_file(ctx));
}

// Exits when crew stop removes the pid file, another watcher takes over, or the session has gone.
pub fn run(ctx: &Ctx) {
    let me = std::process::id().to_string();
    let mut misses = 0;
    let mut first = true;
    while misses < 3 {
        // crew start writes the pid file just after starting us, so it may be missing on the first pass.
        match std::fs::read_to_string(pid_file(ctx)).ok().map(|s| s.trim().to_string()) {
            Some(pid) if pid != me => break,
            None if !first => break,
            _ => {}
        }
        first = false;
        if ctx.mux.ready(ctx) {
            misses = 0;
            if let Err(e) = check_all(ctx) {
                ui::warn(format!("watch: {e:#}"));
            }
        } else {
            misses += 1;
        }
        std::thread::sleep(Duration::from_secs(ctx.config.watch_interval()));
    }
}
