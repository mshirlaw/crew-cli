use crate::crew::board;
use crate::crew::context::Ctx;
use crate::util::ui;
use anyhow::{bail, Result};

// The sender is CREW_ROLE, which is set in each agent's pane; anyone else is "human".
pub fn say(ctx: &Ctx, to: &str, words: &[String]) -> Result<()> {
    ctx.require_crew()?;
    let message = words.join(" ");
    if message.is_empty() {
        bail!("usage: crew say <role|all|human> \"message\"");
    }
    let from = std::env::var("CREW_ROLE").ok().filter(|r| !r.is_empty()).unwrap_or_else(|| "human".to_string());
    if to != "all" && to != "human" && !ctx.role_exists(to) {
        bail!("no role '{to}' (roles: {})", ctx.roles().join(" "));
    }
    board::post(ctx, &from, to, &message)?;
    if to == "human" {
        ui::ok("posted for the human");
        return Ok(());
    }
    if !ctx.mux.ready(ctx) {
        ui::ok("posted to board (crew not running, not delivered live)");
        return Ok(());
    }
    let text = board::message_text(ctx, &from, &message);
    let mut delivered = 0;
    let mut missed = Vec::new();
    for role in board::recipients(ctx, to, &from) {
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

pub fn show_board(ctx: &Ctx, lines: usize) -> Result<()> {
    ctx.require_crew()?;
    let text = std::fs::read_to_string(ctx.file("board.md")).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    for line in &all[all.len().saturating_sub(lines)..] {
        println!("{line}");
    }
    Ok(())
}
