//! The message board: every message is appended to .crew/board.md and typed into the recipient's session.

use crate::context::Ctx;
use crate::ui;
use anyhow::{bail, Context, Result};
use std::io::Write;

/// Appends a message to the board.
pub fn post(ctx: &Ctx, from: &str, to: &str, message: &str) -> Result<()> {
    let path = ctx.file("board.md");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("could not open {}", path.display()))?;
    let time = chrono::Local::now().format("%Y-%m-%d %H:%M");
    // One write call, so messages posted at the same moment don't interleave.
    file.write_all(format!("### {time}  {from} → {to}\n{message}\n\n").as_bytes())?;
    Ok(())
}

/// The text an agent sees for a message, shortened if long.
pub fn message_text(ctx: &Ctx, from: &str, message: &str) -> String {
    let message = if message.chars().count() > 600 {
        let short: String = message.chars().take(600).collect();
        format!("{short}… (full message in {})", ctx.file("board.md").display())
    } else {
        message.to_string()
    };
    if from == "human" {
        format!("[crew] from the human: {message}")
    } else {
        format!("[crew] from {from}: {message}  (reply with: crew say {from} \"...\")")
    }
}

/// Who should receive a message: one role, or everyone except the sender.
pub fn recipients(ctx: &Ctx, to: &str, from: &str) -> Vec<String> {
    if to == "all" {
        ctx.roles().into_iter().filter(|r| r != from).collect()
    } else {
        vec![to.to_string()]
    }
}

/// Succeeds if someone messaged a role on the board after the role last posted there.
/// Messages from crew itself don't count.
pub fn has_unanswered_message(board: &str, role: &str) -> bool {
    let mut last_in = 0;
    let mut last_out = 0;
    let headers = board.lines().filter_map(|line| line.strip_prefix("### "));
    for (n, header) in headers.enumerate() {
        // "2026-10-20 03:00  builder → reviewer"
        let Some((_, route)) = header.split_once("  ") else { continue };
        let Some((from, to)) = route.split_once(" → ") else { continue };
        if from == role {
            last_out = n + 1;
        } else if from != "crew" && (to == role || to == "all") {
            last_in = n + 1;
        }
    }
    last_in > last_out
}

/// crew say <role|all|human> "msg": posts to the board and types it into the recipients' sessions.
pub fn cmd_say(ctx: &Ctx, to: &str, words: &[String]) -> Result<()> {
    ctx.require_crew()?;
    let message = words.join(" ");
    if message.is_empty() {
        bail!("usage: crew say <role|all|human> \"message\"");
    }
    let from = std::env::var("CREW_ROLE").ok().filter(|r| !r.is_empty()).unwrap_or_else(|| "human".to_string());
    if to != "all" && to != "human" && !ctx.role_exists(to) {
        bail!("no role '{to}' (roles: {})", ctx.roles().join(" "));
    }
    post(ctx, &from, to, &message)?;
    if to == "human" {
        ui::ok("posted for the human");
        return Ok(());
    }
    if !ctx.mux.ready(ctx) {
        ui::ok("posted to board (crew not running, not delivered live)");
        return Ok(());
    }
    let text = message_text(ctx, &from, &message);
    let mut delivered = 0;
    let mut missed = Vec::new();
    for role in recipients(ctx, to, &from) {
        if ctx.mux.deliver(ctx, &role, &text) {
            delivered += 1;
        } else {
            missed.push(role);
        }
    }
    ui::ok(format!("posted to board, delivered to {delivered} agent(s)"));
    if !missed.is_empty() {
        ui::warn(format!("not delivered live to: {} (busy or not running). It's on the board.", missed.join(" ")));
    }
    Ok(())
}

/// crew board [n]: prints the last n lines of the board.
pub fn cmd_board(ctx: &Ctx, lines: usize) -> Result<()> {
    ctx.require_crew()?;
    let text = std::fs::read_to_string(ctx.file("board.md")).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    for line in &all[all.len().saturating_sub(lines)..] {
        println!("{line}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::has_unanswered_message;

    const BOARD: &str = "# Crew board\n\n\
        ### 2026-10-20 03:00  crew → all\nCrew started\n\n\
        ### 2026-10-20 03:01  lead → builder\nbuild it\n\n\
        ### 2026-10-20 03:02  builder → reviewer\nready\n\n\
        ### 2026-10-20 03:03  lead → all\nstandup\n\n";

    #[test]
    fn replies_clear_owed_messages() {
        assert!(has_unanswered_message(BOARD, "reviewer"));
        assert!(has_unanswered_message(BOARD, "builder"));
        assert!(!has_unanswered_message(BOARD, "lead"));
    }

    #[test]
    fn crew_messages_are_not_owed() {
        assert!(!has_unanswered_message("### 2026-10-20 03:00  crew → lead\nalert\n", "lead"));
    }
}
