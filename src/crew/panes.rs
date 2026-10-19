use super::context::Ctx;
use super::{agents, board};
use crate::mux::Direction;
use crate::util::ui;
use anyhow::{bail, Result};
use std::time::Duration;

pub fn build(ctx: &Ctx) -> Result<()> {
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
    // Give new shells a moment to start before typing into them.
    std::thread::sleep(Duration::from_millis(500));
    for role in &roles {
        if let Some(pane) = ctx.state.get(&format!("pane:{role}")) {
            agents::launch(ctx, role, &pane, false);
        }
    }
    board::post(ctx, "crew", "all", &format!("Crew started: {}", roles.join(" ")))
}

pub fn revive(ctx: &Ctx) {
    ui::info("crew is already open, reviving anything that stopped");
    for role in ctx.roles() {
        if ctx.mux.agent_running(ctx, &role) {
            ui::ok(format!("{role} is running"));
            continue;
        }
        if let Some(pane) = pane_for(ctx, &role) {
            agents::launch(ctx, &role, &pane, true);
        }
    }
    ctx.mux.finish_layout(ctx);
}

fn pane_for(ctx: &Ctx, role: &str) -> Option<String> {
    let key = format!("pane:{role}");
    if let Some(pane) = ctx.state.get(&key).filter(|p| ctx.mux.pane_exists(p)) {
        return Some(pane);
    }
    let pane = match any_live(ctx) {
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

pub fn any_live(ctx: &Ctx) -> Option<String> {
    ctx.roles().iter().filter_map(|role| ctx.state.get(&format!("pane:{role}"))).find(|pane| ctx.mux.pane_exists(pane))
}

fn open_board_tab(ctx: &Ctx) {
    if let Some(pane) = ctx.mux.new_tab(ctx, "board", &ctx.crew, None) {
        ctx.state.set("board", &pane);
        std::thread::sleep(Duration::from_millis(500));
        ctx.mux.run(&pane, "tail -n 40 -F board.md");
    }
}

pub fn close(ctx: &Ctx, role: &str) {
    let key = format!("pane:{role}");
    if let Some(pane) = ctx.state.get(&key) {
        if ctx.mux.pane_exists(&pane) {
            ctx.mux.close_pane(&pane);
        }
    }
    ctx.state.set(&key, "");
}

// Pane ids from another multiplexer are meaningless, so switching multiplexers starts fresh.
pub fn forget_other_mux_state(ctx: &Ctx) {
    let saved = ctx.state.get("mux").unwrap_or_else(|| "herdr".to_string());
    if saved != ctx.mux.name() {
        ctx.state.clear();
    }
}
