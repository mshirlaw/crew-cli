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
    let live = first_tab_live_panes(ctx);
    let Some(last) = live.last() else { return };
    // Rejoin the right column under its last pane, or open it if only the left pane is left.
    let direction = if live.len() > 1 { Direction::Down } else { Direction::Right };
    if let Some(pane) = ctx.mux.split(ctx, HUMAN, last, direction, 0.5) {
        ctx.state.set("terminal", &pane);
    }
}

// The panes of the first tab's roles, in layout order: the left pane, then the right column.
fn first_tab_live_panes(ctx: &Ctx) -> Vec<String> {
    let roles = ctx.roles();
    let first_tab = roles.chunks(ctx.config.panes_per_tab()).next().unwrap_or_default();
    first_tab
        .iter()
        .filter_map(|role| ctx.state.get(&pane_key(role)))
        .filter(|pane| ctx.mux.pane_exists(pane))
        .collect()
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
    use crate::crew::testutil::{fixture, Fixture, ROLES};

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

    const SHELL_AGENTS: &str = "[agents.claude]\ncommand = \"sh\"\n[agents.codex]\ncommand = \"sh\"\n";

    fn crew(extra: &str) -> Fixture {
        fixture(&format!("{extra}{SHELL_AGENTS}{ROLES}"))
    }

    fn mark_live(f: &Fixture, role: &str) {
        let pane = format!("p-{role}");
        f.ctx.state.set(&format!("pane:{role}"), &pane);
        f.fake.lock().unwrap().live_panes.insert(pane);
    }

    #[test]
    fn build_needs_at_least_one_role() {
        let f = fixture("");
        assert!(build(&f.ctx).is_err());
        assert_eq!(f.calls(), Vec::<String>::new());
    }

    #[test]
    fn build_lays_out_the_first_tab_and_a_terminal() {
        let f = crew("");
        build(&f.ctx).unwrap();
        let calls = f.calls();
        assert_eq!(calls[0], "create_workspace lead");
        assert_eq!(calls[1], "split builder from p-lead right 0.50");
        assert_eq!(calls[2], "split reviewer from p-builder down 0.33");
        assert_eq!(calls[3], "split human from p-reviewer down 0.50");
        assert_eq!(calls[4], "finish_layout");
        assert_eq!(calls[5], "new_tab board -");
        assert_eq!(calls[6], "run t-board tail -n 40 -F board.md");
        for (key, pane) in [
            ("pane:builder", "p-builder"),
            ("pane:reviewer", "p-reviewer"),
            ("terminal", "p-human"),
            ("board", "t-board"),
        ] {
            assert_eq!(f.ctx.state.get(key).as_deref(), Some(pane), "{key}");
        }
        assert_eq!(f.ctx.state.get("pane:human"), None);
        assert_eq!(f.ctx.state.get("mux").as_deref(), Some("tmux"));
    }

    #[test]
    fn build_starts_each_agent_in_its_pane_and_announces_the_crew() {
        let f = crew("");
        build(&f.ctx).unwrap();
        let starts: Vec<String> = f.calls().into_iter().filter(|c| c.starts_with("start_agent")).collect();
        assert_eq!(starts.len(), 3);
        assert!(starts[0].starts_with("start_agent lead p-lead sh -- You are the lead agent"));
        assert!(starts[1].starts_with("start_agent builder p-builder sh -- "));
        assert!(f.board().contains("  crew → all\nCrew started: lead builder reviewer"));
    }

    #[test]
    fn build_discards_old_state() {
        let f = crew("");
        f.ctx.state.set("pane:ghost", "%9");
        build(&f.ctx).unwrap();
        assert_eq!(f.ctx.state.get("pane:ghost"), None);
    }

    #[test]
    fn extra_agents_go_on_numbered_tabs_without_a_terminal() {
        let roles = "[roles.a]\n[roles.b]\n[roles.c]\n[roles.d]\n[roles.e]\n";
        let f = fixture(&format!("panes_per_tab = 2\n{SHELL_AGENTS}{roles}"));
        build(&f.ctx).unwrap();
        let calls = f.calls();
        assert!(calls.contains(&"new_tab agents 2 c".to_string()));
        assert!(calls.contains(&"new_tab agents 3 e".to_string()));
        assert!(calls.contains(&"split d from t-agents 2 right 0.50".to_string()));
        assert_eq!(calls.iter().filter(|c| c.starts_with("split human")).count(), 1);
        assert_eq!(f.ctx.state.get("pane:c").as_deref(), Some("t-agents 2"));
        assert_eq!(f.ctx.state.get("pane:e").as_deref(), Some("t-agents 3"));
    }

    #[test]
    fn a_lone_first_role_still_gets_a_terminal() {
        let f = fixture(&format!("{SHELL_AGENTS}[roles.solo]\n"));
        build(&f.ctx).unwrap();
        assert!(f.calls().contains(&"split human from p-solo right 0.50".to_string()));
    }

    #[test]
    fn revive_restarts_only_stopped_agents_with_their_handoff_prompt() {
        let f = crew("");
        for role in ["lead", "builder", "reviewer"] {
            mark_live(&f, role);
        }
        f.ctx.state.set("terminal", "p-human");
        f.fake.lock().unwrap().live_panes.insert("p-human".into());
        f.fake.lock().unwrap().running.insert("lead".into());
        revive(&f.ctx);
        let starts: Vec<String> = f.calls().into_iter().filter(|c| c.starts_with("start_agent")).collect();
        assert_eq!(starts.len(), 2);
        assert!(starts[0].starts_with("start_agent builder p-builder"));
        assert!(starts[0].contains("notes/builder.md"));
        assert!(!f.calls().iter().any(|c| c.starts_with("split")));
        assert_eq!(f.calls().last().unwrap(), "finish_layout");
    }

    #[test]
    fn revive_recreates_a_closed_terminal_under_the_last_live_pane() {
        let f = crew("");
        mark_live(&f, "lead");
        mark_live(&f, "builder");
        f.fake.lock().unwrap().running.extend(["lead".to_string(), "builder".to_string(), "reviewer".to_string()]);
        revive(&f.ctx);
        assert!(f.calls().contains(&"split human from p-builder down 0.50".to_string()));
        assert_eq!(f.ctx.state.get("terminal").as_deref(), Some("p-human"));
    }

    #[test]
    fn revive_opens_the_terminal_to_the_right_when_only_the_left_pane_is_left() {
        let f = crew("");
        mark_live(&f, "lead");
        f.fake.lock().unwrap().running.extend(["lead".to_string(), "builder".to_string(), "reviewer".to_string()]);
        revive(&f.ctx);
        assert!(f.calls().contains(&"split human from p-lead right 0.50".to_string()));
    }

    #[test]
    fn revive_with_no_live_panes_does_not_invent_a_terminal() {
        let f = crew("");
        f.fake.lock().unwrap().running.extend(["lead".to_string(), "builder".to_string(), "reviewer".to_string()]);
        revive(&f.ctx);
        assert_eq!(f.ctx.state.get("terminal"), None);
    }

    #[test]
    fn revive_gives_a_missing_pane_a_new_split_beside_a_live_one() {
        let f = crew("");
        mark_live(&f, "lead");
        f.fake.lock().unwrap().running.extend(["lead".to_string(), "reviewer".to_string()]);
        f.ctx.state.set("terminal", "p-lead");
        revive(&f.ctx);
        assert!(f.calls().contains(&"split builder from p-lead down 0.50".to_string()));
        assert_eq!(f.ctx.state.get("pane:builder").as_deref(), Some("p-builder"));
        assert!(f.calls().iter().any(|c| c.starts_with("start_agent builder p-builder")));
    }

    #[test]
    fn revive_with_nothing_alive_opens_a_new_tab() {
        let f = fixture(&format!("{SHELL_AGENTS}[roles.solo]\n"));
        revive(&f.ctx);
        assert!(f.calls().contains(&"new_tab agents solo".to_string()));
        assert_eq!(f.ctx.state.get("pane:solo").as_deref(), Some("t-agents"));
    }

    #[test]
    fn close_closes_a_live_pane_and_forgets_it() {
        let f = crew("");
        mark_live(&f, "builder");
        close(&f.ctx, "builder");
        assert_eq!(f.calls(), ["close_pane p-builder"]);
        assert_eq!(f.ctx.state.get("pane:builder"), None);
    }

    #[test]
    fn close_forgets_a_pane_that_is_already_gone() {
        let f = crew("");
        f.ctx.state.set("pane:builder", "p-gone");
        close(&f.ctx, "builder");
        assert_eq!(f.calls(), Vec::<String>::new());
        assert_eq!(f.ctx.state.get("pane:builder"), None);
        close(&f.ctx, "reviewer");
    }

    #[test]
    fn state_from_another_multiplexer_is_dropped() {
        let f = crew("");
        f.ctx.state.set("mux", "herdr");
        f.ctx.state.set("pane:lead", "1-2");
        forget_other_mux_state(&f.ctx);
        assert_eq!(f.ctx.state.get("pane:lead"), None);
    }

    #[test]
    fn state_without_a_recorded_multiplexer_counts_as_herdr() {
        let f = crew("");
        f.ctx.state.set("pane:lead", "1-2");
        forget_other_mux_state(&f.ctx);
        assert_eq!(f.ctx.state.get("pane:lead"), None);
    }

    #[test]
    fn state_from_the_same_multiplexer_is_kept() {
        let f = crew("");
        f.ctx.state.set("mux", "tmux");
        f.ctx.state.set("pane:lead", "%1");
        forget_other_mux_state(&f.ctx);
        assert_eq!(f.ctx.state.get("pane:lead").as_deref(), Some("%1"));
    }

    #[test]
    fn the_terminal_is_not_a_role_pane() {
        assert_eq!(pane_key(HUMAN), "terminal");
        assert_eq!(pane_key("lead"), "pane:lead");
    }

    #[test]
    fn even_share_is_one_over_the_pane_count() {
        assert!((even_share(4) - 0.25).abs() < f32::EPSILON);
    }
}
