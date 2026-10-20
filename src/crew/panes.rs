use super::context::{Ctx, HUMAN};
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
    for (index, group) in roles.chunks(ctx.config.panes_per_tab()).enumerate() {
        lay_out_tab(ctx, index, group);
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

// The tab's first role fills the left half; the others share the right half evenly.
// The first tab ends with a plain terminal for the human, last in the right column.
fn lay_out_tab(ctx: &Ctx, index: usize, group: &[String]) {
    let Some((main_role, agents)) = group.split_first() else { return };
    let mut stacked: Vec<&str> = agents.iter().map(String::as_str).collect();
    if index == 0 {
        stacked.push(HUMAN);
    }
    let main = if index == 0 {
        ctx.state.get(&format!("pane:{main_role}"))
    } else {
        ctx.mux.new_tab(ctx, &tab_label(index), &ctx.role_dir(main_role), Some(main_role))
    };
    let Some(main) = main else {
        ui::warn(format!("could not create a pane for {main_role}"));
        return;
    };
    ctx.state.set(&pane_key(main_role), &main);
    let mut above: Option<String> = None;
    for (i, role) in stacked.iter().enumerate() {
        let pane = match &above {
            None => ctx.mux.split(ctx, role, &main, Direction::Right, 0.5),
            Some(pane) => ctx.mux.split(ctx, role, pane, Direction::Down, even_share(stacked.len() - i + 1)),
        };
        if let Some(pane) = pane {
            ctx.state.set(&pane_key(role), &pane);
            above = Some(pane);
        }
    }
}

// Splitting a pane that must still hold `panes` panes leaves it 1/panes of the space,
// so a column built top to bottom ends up with equal heights.
#[allow(clippy::cast_precision_loss)] // pane counts are tiny
fn even_share(panes: usize) -> f32 {
    1.0 / panes as f32
}

// The terminal isn't a role, so it stays out of the `pane:<role>` keys that status and the watcher read.
fn pane_key(name: &str) -> String {
    if name == HUMAN {
        "terminal".to_string()
    } else {
        format!("pane:{name}")
    }
}

fn tab_label(index: usize) -> String {
    if index == 0 {
        "agents".to_string()
    } else {
        format!("agents {}", index + 1)
    }
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
    revive_terminal(ctx);
    ctx.mux.finish_layout(ctx);
}

fn revive_terminal(ctx: &Ctx) {
    if ctx.state.get("terminal").is_some_and(|pane| ctx.mux.pane_exists(&pane)) {
        return;
    }
    let Some(anchor) = any_live(ctx) else { return };
    if let Some(pane) = ctx.mux.split(ctx, HUMAN, &anchor, Direction::Down, 0.5) {
        ctx.state.set("terminal", &pane);
    }
}

fn pane_for(ctx: &Ctx, role: &str) -> Option<String> {
    let key = format!("pane:{role}");
    if let Some(pane) = ctx.state.get(&key).filter(|p| ctx.mux.pane_exists(p)) {
        return Some(pane);
    }
    let pane = match any_live(ctx) {
        Some(anchor) => ctx.mux.split(ctx, role, &anchor, Direction::Down, 0.5),
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

fn any_live(ctx: &Ctx) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_column_built_top_down_comes_out_even() {
        // Three stacked panes of height 1: each split keeps 1/remaining of what's left.
        let mut remaining = 1.0;
        let mut heights = Vec::new();
        for panes in (2..=3).rev() {
            let kept = remaining * even_share(panes);
            heights.push(kept);
            remaining -= kept;
        }
        heights.push(remaining);
        assert!(heights.iter().all(|h| (h - 1.0 / 3.0).abs() < 1e-6), "{heights:?}");
    }

    #[test]
    fn extra_tabs_are_numbered_from_two() {
        assert_eq!(tab_label(0), "agents");
        assert_eq!(tab_label(1), "agents 2");
    }
}
