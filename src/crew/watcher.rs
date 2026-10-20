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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crew::testutil::{fixture, Fixture, ROLES};

    const SCREEN: &str = "thinking...";

    fn prime(f: &Fixture, role: &str, idle: u64, notified: bool) {
        f.set_screen(role, SCREEN);
        let seen = Seen { fingerprint: util::fingerprint(SCREEN), changed_at: util::now_secs() - idle, notified };
        write_seen(&f.ctx, role, &seen);
    }

    fn give_work(f: &Fixture, role: &str) {
        tasks::add(&f.ctx, role, "do it", "").unwrap();
    }

    fn notified(f: &Fixture, role: &str) -> bool {
        read_seen(&f.ctx, role).unwrap().notified
    }

    #[test]
    fn seen_files_round_trip() {
        let f = fixture(ROLES);
        write_seen(&f.ctx, "builder", &Seen { fingerprint: "abc".into(), changed_at: 42, notified: true });
        let seen = read_seen(&f.ctx, "builder").unwrap();
        assert_eq!((seen.fingerprint.as_str(), seen.changed_at, seen.notified), ("abc", 42, true));
        assert!(read_seen(&f.ctx, "reviewer").is_none());
    }

    #[test]
    fn garbled_seen_files_are_ignored() {
        let f = fixture(ROLES);
        std::fs::create_dir_all(f.ctx.state.dir()).unwrap();
        std::fs::write(seen_file(&f.ctx, "builder"), "abc not-a-number yes\n").unwrap();
        assert!(read_seen(&f.ctx, "builder").is_none());
        std::fs::write(seen_file(&f.ctx, "builder"), "abc 5\n").unwrap();
        assert!(read_seen(&f.ctx, "builder").is_none());
    }

    #[test]
    fn owed_work_names_tasks_and_messages() {
        let f = fixture(ROLES);
        assert_eq!(owed_work(&f.ctx, "builder"), None);
        board::post(&f.ctx, "lead", "builder", "go").unwrap();
        assert_eq!(owed_work(&f.ctx, "builder").as_deref(), Some("an unanswered message"));
        give_work(&f, "builder");
        give_work(&f, "builder");
        assert_eq!(owed_work(&f.ctx, "builder").as_deref(), Some("open task(s) 1 2 and an unanswered message"));
        board::post(&f.ctx, "builder", "lead", "on it").unwrap();
        assert_eq!(owed_work(&f.ctx, "builder").as_deref(), Some("open task(s) 1 2"));
    }

    #[test]
    fn done_tasks_are_not_owed() {
        let f = fixture(ROLES);
        give_work(&f, "builder");
        tasks::update(&f.ctx, 1, &[("status".into(), "review".into())]).unwrap();
        assert_eq!(owed_work(&f.ctx, "builder"), None);
    }

    #[test]
    fn first_sighting_records_the_screen_without_alerting() {
        let f = fixture(ROLES);
        give_work(&f, "builder");
        f.set_screen("builder", SCREEN);
        check_role(&f.ctx, "builder").unwrap();
        let seen = read_seen(&f.ctx, "builder").unwrap();
        assert_eq!(seen.fingerprint, util::fingerprint(SCREEN));
        assert!(!seen.notified);
        assert!(f.calls().is_empty());
    }

    #[test]
    fn a_changed_screen_resets_the_clock() {
        let f = fixture(ROLES);
        give_work(&f, "builder");
        prime(&f, "builder", 5000, false);
        f.set_screen("builder", "something new");
        check_role(&f.ctx, "builder").unwrap();
        let seen = read_seen(&f.ctx, "builder").unwrap();
        assert_eq!(seen.fingerprint, util::fingerprint("something new"));
        assert!(util::now_secs() - seen.changed_at < 5);
        assert!(f.calls().is_empty());
    }

    #[test]
    fn a_stalled_agent_with_work_is_reported_to_the_lead_once() {
        let f = fixture(ROLES);
        give_work(&f, "builder");
        prime(&f, "builder", 700, false);
        check_role(&f.ctx, "builder").unwrap();
        assert!(notified(&f, "builder"));
        let calls = f.calls();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].starts_with("deliver lead [crew] builder has shown no activity for"));
        assert!(calls[0].contains("open task(s) 1"));
        assert!(f.board().contains("  crew → lead\nbuilder has shown no activity"));
        check_role(&f.ctx, "builder").unwrap();
        assert_eq!(f.calls().len(), 1);
        assert_eq!(f.board().matches("crew → lead").count(), 1);
    }

    #[test]
    fn an_idle_agent_with_nothing_owed_is_left_alone() {
        let f = fixture(ROLES);
        prime(&f, "builder", 5000, false);
        check_role(&f.ctx, "builder").unwrap();
        assert!(!notified(&f, "builder"));
        assert!(f.calls().is_empty());
    }

    #[test]
    fn an_agent_quiet_for_less_than_the_timeout_is_not_stalled() {
        let f = fixture(ROLES);
        give_work(&f, "builder");
        prime(&f, "builder", 100, false);
        check_role(&f.ctx, "builder").unwrap();
        assert!(!notified(&f, "builder"));
        assert!(f.calls().is_empty());
    }

    #[test]
    fn the_timeout_is_configurable() {
        let f = fixture(&format!("stall_timeout = 50\n{ROLES}"));
        give_work(&f, "builder");
        prime(&f, "builder", 60, false);
        check_role(&f.ctx, "builder").unwrap();
        assert!(notified(&f, "builder"));
    }

    #[test]
    fn an_unreachable_lead_falls_back_to_a_notification() {
        let f = fixture(ROLES);
        f.fake.lock().unwrap().accepts_messages = false;
        give_work(&f, "builder");
        prime(&f, "builder", 700, false);
        check_role(&f.ctx, "builder").unwrap();
        let calls = f.calls();
        assert!(calls[0].starts_with("deliver lead"));
        assert!(calls[1].starts_with("notify builder has shown no activity"));
    }

    #[test]
    fn a_stalled_lead_is_reported_to_the_human() {
        let f = fixture(ROLES);
        give_work(&f, "lead");
        prime(&f, "lead", 700, false);
        check_role(&f.ctx, "lead").unwrap();
        let calls = f.calls();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].starts_with("notify lead has shown no activity"));
        assert!(f.board().contains("  crew → human\nlead has shown no activity"));
        assert!(f.board().contains("Check its pane."));
    }

    #[test]
    fn with_no_lead_the_human_is_told() {
        let f = fixture("[roles.builder]\nagent = \"claude\"\n");
        give_work(&f, "builder");
        prime(&f, "builder", 700, false);
        check_role(&f.ctx, "builder").unwrap();
        assert!(f.board().contains("  crew → human\n"));
        assert!(f.calls()[0].starts_with("notify builder"));
    }

    #[test]
    fn check_all_covers_every_role() {
        let f = fixture(ROLES);
        check_all(&f.ctx).unwrap();
        for role in ["lead", "builder", "reviewer"] {
            assert!(read_seen(&f.ctx, role).is_some(), "{role}");
        }
    }

    #[test]
    fn durations_in_alerts_are_readable() {
        let f = fixture(ROLES);
        raise_alert(&f.ctx, "builder", "open task(s) 3", 725).unwrap();
        assert!(f.board().contains(&format!("no activity for {} but has open task(s) 3", util::duration(725))));
    }

    #[test]
    fn running_pid_needs_a_live_process() {
        let f = fixture(ROLES);
        assert_eq!(running_pid(&f.ctx), None);
        std::fs::create_dir_all(f.ctx.state.dir()).unwrap();
        std::fs::write(pid_file(&f.ctx), format!("{}\n", std::process::id())).unwrap();
        assert_eq!(running_pid(&f.ctx), Some(std::process::id().to_string()));
        std::fs::write(pid_file(&f.ctx), "999999999\n").unwrap();
        assert_eq!(running_pid(&f.ctx), None);
    }

    #[test]
    fn stop_removes_the_pid_file_and_a_missing_one_is_fine() {
        let f = fixture(ROLES);
        stop(&f.ctx);
        std::fs::create_dir_all(f.ctx.state.dir()).unwrap();
        std::fs::write(pid_file(&f.ctx), "999999999\n").unwrap();
        stop(&f.ctx);
        assert!(!pid_file(&f.ctx).exists());
    }

    #[test]
    fn stop_ends_a_running_watcher() {
        let f = fixture(ROLES);
        let mut child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
        std::fs::create_dir_all(f.ctx.state.dir()).unwrap();
        std::fs::write(pid_file(&f.ctx), format!("{}\n", child.id())).unwrap();
        stop(&f.ctx);
        assert!(!child.wait().unwrap().success());
        assert!(!pid_file(&f.ctx).exists());
    }

    #[test]
    fn start_does_nothing_when_the_watcher_is_off_or_running() {
        let off = fixture(&format!("stall_timeout = 0\n{ROLES}"));
        start(&off.ctx).unwrap();
        assert!(!pid_file(&off.ctx).exists());

        let on = fixture(ROLES);
        std::fs::create_dir_all(on.ctx.state.dir()).unwrap();
        std::fs::write(pid_file(&on.ctx), format!("{}\n", std::process::id())).unwrap();
        write_seen(&on.ctx, "builder", &Seen { fingerprint: "x".into(), changed_at: 1, notified: false });
        start(&on.ctx).unwrap();
        assert!(read_seen(&on.ctx, "builder").is_some());
    }

    #[test]
    fn run_exits_when_another_watcher_owns_the_pid_file() {
        let f = fixture(ROLES);
        std::fs::create_dir_all(f.ctx.state.dir()).unwrap();
        std::fs::write(pid_file(&f.ctx), "1\n").unwrap();
        run(&f.ctx);
        assert!(read_seen(&f.ctx, "builder").is_none());
    }

    #[test]
    fn run_checks_once_then_exits_when_the_pid_file_is_gone() {
        let f = fixture(&format!("watch_interval = 1\n{ROLES}"));
        run(&f.ctx);
        assert!(read_seen(&f.ctx, "builder").is_some());
    }

    #[test]
    fn run_gives_up_after_three_misses_without_checking() {
        let f = fixture(&format!("watch_interval = 1\n{ROLES}"));
        f.fake.lock().unwrap().ready = false;
        std::fs::create_dir_all(f.ctx.state.dir()).unwrap();
        std::fs::write(pid_file(&f.ctx), format!("{}\n", std::process::id())).unwrap();
        run(&f.ctx);
        assert!(read_seen(&f.ctx, "builder").is_none());
    }
}
