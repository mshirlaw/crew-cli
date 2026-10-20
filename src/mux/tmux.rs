use super::{Direction, Mux};
use crate::crew::context::Ctx;
use crate::util::ui;
use crate::util::{has, run_all, run_ok, run_out, shell_join};
use anyhow::{bail, Result};
use std::path::Path;

pub struct Tmux;
fn session(ctx: &Ctx) -> String {
    format!("crew-{}", ctx.slug)
}
fn exact(ctx: &Ctx) -> String {
    format!("={}", session(ctx))
}
pub fn major_version() -> Option<u32> {
    parse_major(&run_out("tmux", &["-V"])?)
}
fn parse_major(version_output: &str) -> Option<u32> {
    version_output
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}
fn env_args(ctx: &Ctx, role: &str) -> Vec<String> {
    vec!["-e".into(), format!("CREW_DIR={}", ctx.crew.display()), "-e".into(), format!("CREW_ROLE={role}")]
}
fn role_pane(ctx: &Ctx, role: &str) -> Option<String> {
    ctx.state.get(&format!("pane:{role}"))
}
fn agent_windows(ctx: &Ctx) -> Vec<String> {
    let mut windows: Vec<String> = ctx
        .roles()
        .iter()
        .filter_map(|role| role_pane(ctx, role))
        .filter_map(|pane| run_out("tmux", &["display-message", "-p", "-t", &pane, "#{window_id}"]))
        .map(|out| out.trim().to_string())
        .filter(|window| !window.is_empty())
        .collect();
    windows.sort();
    windows.dedup();
    windows
}

impl Mux for Tmux {
    fn name(&self) -> &'static str {
        "tmux"
    }

    fn ensure(&self, _ctx: &Ctx) -> Result<()> {
        if !has("tmux") {
            bail!("tmux not found. Install it (apt install tmux / brew install tmux), or set multiplexer = \"herdr\"");
        }
        if major_version().unwrap_or(0) < 3 {
            bail!("crew needs tmux 3.0 or newer (you have {})", run_out("tmux", &["-V"]).unwrap_or_default().trim());
        }
        Ok(())
    }

    fn ready(&self, ctx: &Ctx) -> bool {
        run_ok("tmux", &["has-session", "-t", &exact(ctx)])
    }

    fn workspace_exists(&self, ctx: &Ctx) -> bool {
        self.ready(ctx)
    }

    fn create_workspace(&self, ctx: &Ctx, role: &str) -> Result<()> {
        let mut args = vec![
            "new-session".to_string(),
            "-d".into(),
            "-s".into(),
            session(ctx),
            "-n".into(),
            "agents".into(),
            "-x".into(),
            "220".into(),
            "-y".into(),
            "55".into(),
            "-c".into(),
            ctx.role_dir(role).display().to_string(),
        ];
        args.extend(env_args(ctx, role));
        args.extend(["-P".into(), "-F".into(), "#{pane_id}".into()]);
        let (success, out) = run_all("tmux", &args);
        if !success {
            bail!("could not create tmux session: {}", out.trim());
        }
        ctx.state.set("ws", &session(ctx));
        ctx.state.set(&format!("pane:{role}"), out.trim());
        ui::ok(format!("tmux session {}", session(ctx)));
        Ok(())
    }

    // Sizes are left to finish_layout, which evens out every window at once.
    fn split(&self, ctx: &Ctx, role: &str, from: &str, direction: Direction, _keep: f32) -> Option<String> {
        let flag = if direction == Direction::Right { "-h" } else { "-v" };
        let mut args = vec![
            "split-window".to_string(),
            "-d".into(),
            "-t".into(),
            from.into(),
            flag.into(),
            "-c".into(),
            ctx.role_dir(role).display().to_string(),
        ];
        args.extend(env_args(ctx, role));
        args.extend(["-P".into(), "-F".into(), "#{pane_id}".into()]);
        run_out("tmux", &args).map(|out| out.trim().to_string()).filter(|p| !p.is_empty())
    }

    fn new_tab(&self, ctx: &Ctx, label: &str, dir: &Path, role: Option<&str>) -> Option<String> {
        let mut args = vec![
            "new-window".to_string(),
            "-d".into(),
            "-t".into(),
            format!("{}:", exact(ctx)),
            "-n".into(),
            label.into(),
            "-c".into(),
            dir.display().to_string(),
        ];
        if let Some(role) = role {
            args.extend(env_args(ctx, role));
        }
        args.extend(["-P".into(), "-F".into(), "#{pane_id}".into()]);
        run_out("tmux", &args).map(|out| out.trim().to_string()).filter(|p| !p.is_empty())
    }

    fn pane_exists(&self, pane: &str) -> bool {
        !pane.is_empty() && run_ok("tmux", &["display-message", "-p", "-t", pane, "#{pane_id}"])
    }

    fn run(&self, pane: &str, command: &str) -> bool {
        run_ok("tmux", &["send-keys", "-t", pane, "-l", "--", command])
            && run_ok("tmux", &["send-keys", "-t", pane, "Enter"])
    }

    fn start_agent(&self, _ctx: &Ctx, role: &str, pane: &str, words: &[String]) -> bool {
        let launched = self.run(pane, &shell_join(words));
        if launched {
            ui::ok(format!("{role} → {} launched", words[0]));
        } else {
            ui::warn(format!("{role}: could not launch {}", words[0]));
        }
        launched
    }

    fn agent_running(&self, ctx: &Ctx, role: &str) -> bool {
        // The agent is running if the pane's shell has a child process. Matching on the
        // process name wouldn't work, because many agent wrappers are shell scripts.
        let Some(pane) = role_pane(ctx, role).filter(|p| self.pane_exists(p)) else {
            return false;
        };
        let Some(shell_pid) = run_out("tmux", &["display-message", "-p", "-t", &pane, "#{pane_pid}"]) else {
            return false;
        };
        let shell_pid = shell_pid.trim();
        run_out("ps", &["-A", "-o", "ppid="]).is_some_and(|out| out.lines().any(|parent| parent.trim() == shell_pid))
    }

    fn agent_status(&self, ctx: &Ctx, role: &str) -> String {
        if self.agent_running(ctx, role) { "running" } else { "stopped" }.to_string()
    }

    fn deliver(&self, ctx: &Ctx, role: &str, text: &str) -> bool {
        if !self.agent_running(ctx, role) {
            return false;
        }
        let Some(pane) = role_pane(ctx, role) else {
            return false;
        };
        let buffer = format!("crew-{}", std::process::id());
        let pasted = run_ok("tmux", &["set-buffer", "-b", &buffer, "--", text])
            && run_ok("tmux", &["paste-buffer", "-p", "-d", "-b", &buffer, "-t", &pane]);
        if !pasted {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_secs_f64(ctx.config.paste_delay()));
        run_ok("tmux", &["send-keys", "-t", &pane, "Enter"])
    }

    fn close_pane(&self, pane: &str) {
        run_ok("tmux", &["kill-pane", "-t", pane]);
    }

    fn close_workspace(&self, ctx: &Ctx) {
        run_ok("tmux", &["kill-session", "-t", &exact(ctx)]);
    }

    fn finish_layout(&self, ctx: &Ctx) {
        // In each agents window: first pane on the left, the rest stacked on the right.
        for window in agent_windows(ctx) {
            run_ok("tmux", &["set-option", "-w", "-t", &window, "main-pane-width", "50%"]);
            run_ok("tmux", &["select-layout", "-t", &window, "main-vertical"]);
        }
    }

    fn screen(&self, ctx: &Ctx, role: &str) -> String {
        role_pane(ctx, role).and_then(|pane| run_out("tmux", &["capture-pane", "-p", "-t", &pane])).unwrap_or_default()
    }

    fn notify(&self, ctx: &Ctx, text: &str) {
        run_ok("tmux", &["display-message", "-t", &exact(ctx), &format!("crew: {text}")]);
    }

    fn attach(&self, ctx: &Ctx) -> Result<()> {
        use std::os::unix::process::CommandExt;
        if std::env::var_os("TMUX").is_some() {
            run_ok("tmux", &["switch-client", "-t", &exact(ctx)]);
            return Ok(());
        }
        let err = std::process::Command::new("tmux").args(["attach-session", "-t", &exact(ctx)]).exec();
        bail!("could not open tmux: {err}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::testing::{ctx_in, scratch_dir};
    use crate::util::testing::Fake;

    fn ctx() -> Ctx {
        let root = scratch_dir().join("my app");
        std::fs::create_dir_all(&root).unwrap();
        ctx_in(
            &root,
            Some("multiplexer = \"tmux\"\npaste_delay = 0\n[roles.lead]\nworktree = false\n[roles.builder]\nworktree = false\n"),
        )
    }

    fn words(text: &str) -> Vec<String> {
        text.split(' ').map(String::from).collect()
    }

    // A tmux whose answers depend on the subcommand; each line is "<match> => <shell>".
    fn tmux(fake: &Fake, body: &str) {
        fake.program("tmux", body);
    }

    #[test]
    fn major_version_is_read_from_the_version_banner() {
        assert_eq!(parse_major("tmux 3.4\n"), Some(3));
        assert_eq!(parse_major("tmux 2.9a"), Some(2));
        assert_eq!(parse_major("tmux next-3.5"), Some(3));
        assert_eq!(parse_major("tmux 10.0"), Some(10));
        assert_eq!(parse_major("tmux master"), None);
        assert_eq!(parse_major(""), None);
    }

    #[test]
    fn major_version_asks_tmux() {
        let fake = Fake::new();
        tmux(&fake, "echo 'tmux 3.3a'");
        assert_eq!(major_version(), Some(3));
        assert_eq!(fake.log("tmux"), ["-V"]);
    }

    #[test]
    fn major_version_is_none_without_tmux_or_when_it_fails() {
        let fake = Fake::new();
        assert_eq!(major_version(), None);
        tmux(&fake, "exit 1");
        assert_eq!(major_version(), None);
    }

    #[test]
    fn the_session_is_named_after_the_repo_and_matched_exactly() {
        let ctx = ctx();
        assert_eq!(session(&ctx), "crew-my-app");
        assert_eq!(exact(&ctx), "=crew-my-app");
    }

    #[test]
    fn new_panes_get_the_crew_environment() {
        let ctx = ctx();
        let args = env_args(&ctx, "builder");
        assert_eq!(args[0], "-e");
        assert_eq!(args[1], format!("CREW_DIR={}", ctx.crew.display()));
        assert_eq!(args[2], "-e");
        assert_eq!(args[3], "CREW_ROLE=builder");
    }

    #[test]
    fn ensure_needs_tmux() {
        let _fake = Fake::new();
        let error = Tmux.ensure(&ctx()).unwrap_err().to_string();
        assert!(error.contains("tmux not found") && error.contains("herdr"), "{error}");
    }

    #[test]
    fn ensure_rejects_old_versions_and_says_which() {
        let fake = Fake::new();
        tmux(&fake, "echo 'tmux 2.9a'");
        let error = Tmux.ensure(&ctx()).unwrap_err().to_string();
        assert!(error.contains("3.0 or newer") && error.contains("tmux 2.9a"), "{error}");
    }

    #[test]
    fn ensure_accepts_tmux_3() {
        let fake = Fake::new();
        tmux(&fake, "echo 'tmux 3.0'");
        Tmux.ensure(&ctx()).unwrap();
    }

    #[test]
    fn ready_and_workspace_exist_while_the_session_does() {
        let ctx = ctx();
        let fake = Fake::new();
        tmux(&fake, "exit 0");
        assert!(Tmux.ready(&ctx));
        assert!(Tmux.workspace_exists(&ctx));
        assert_eq!(fake.log("tmux"), ["has-session -t =crew-my-app", "has-session -t =crew-my-app"]);
        tmux(&fake, "exit 1");
        assert!(!Tmux.ready(&ctx));
        assert!(!Tmux.workspace_exists(&ctx));
    }

    #[test]
    fn create_workspace_starts_a_detached_session_and_saves_the_ids() {
        let ctx = ctx();
        let fake = Fake::new();
        tmux(&fake, "echo '%7'");
        Tmux.create_workspace(&ctx, "lead").unwrap();
        let line = &fake.log("tmux")[0];
        assert!(line.starts_with("new-session -d -s crew-my-app -n agents -x 220 -y 55 -c "), "{line}");
        assert!(line.contains(&format!("-e CREW_DIR={}", ctx.crew.display())), "{line}");
        assert!(line.contains("-e CREW_ROLE=lead"), "{line}");
        assert!(line.ends_with("-P -F #{pane_id}"), "{line}");
        assert_eq!(ctx.state.get("ws").as_deref(), Some("crew-my-app"));
        assert_eq!(ctx.state.get("pane:lead").as_deref(), Some("%7"));
    }

    #[test]
    fn create_workspace_reports_tmux_errors_and_saves_nothing() {
        let ctx = ctx();
        let fake = Fake::new();
        tmux(&fake, "echo 'duplicate session' >&2; exit 1");
        let error = Tmux.create_workspace(&ctx, "lead").unwrap_err().to_string();
        assert!(error.contains("could not create tmux session: duplicate session"), "{error}");
        assert_eq!(ctx.state.get("ws"), None);
    }

    #[test]
    fn split_right_and_down_use_the_matching_flags() {
        let ctx = ctx();
        let fake = Fake::new();
        tmux(&fake, "echo '%8'");
        assert_eq!(Tmux.split(&ctx, "builder", "%1", Direction::Right, 0.5).as_deref(), Some("%8"));
        assert_eq!(Tmux.split(&ctx, "builder", "%1", Direction::Down, 0.5).as_deref(), Some("%8"));
        let log = fake.log("tmux");
        assert!(log[0].starts_with("split-window -d -t %1 -h -c "), "{}", log[0]);
        assert!(log[0].contains("-e CREW_ROLE=builder"));
        assert!(log[1].starts_with("split-window -d -t %1 -v -c "), "{}", log[1]);
    }

    #[test]
    fn split_gives_nothing_back_when_tmux_fails_or_prints_nothing() {
        let ctx = ctx();
        let fake = Fake::new();
        tmux(&fake, "exit 1");
        assert_eq!(Tmux.split(&ctx, "builder", "%1", Direction::Right, 0.5), None);
        tmux(&fake, "exit 0");
        assert_eq!(Tmux.split(&ctx, "builder", "%1", Direction::Right, 0.5), None);
    }

    #[test]
    fn new_tab_adds_a_window_to_the_session() {
        let ctx = ctx();
        let fake = Fake::new();
        tmux(&fake, "echo '%9'");
        let pane = Tmux.new_tab(&ctx, "board", Path::new("/work"), None);
        assert_eq!(pane.as_deref(), Some("%9"));
        assert_eq!(fake.log("tmux")[0], "new-window -d -t =crew-my-app: -n board -c /work -P -F #{pane_id}");
    }

    #[test]
    fn new_tab_for_a_role_sets_the_environment() {
        let ctx = ctx();
        let fake = Fake::new();
        tmux(&fake, "echo '%9'");
        Tmux.new_tab(&ctx, "agents 2", Path::new("/work"), Some("builder"));
        assert!(fake.log("tmux")[0].contains("-e CREW_ROLE=builder"));
    }

    #[test]
    fn new_tab_is_none_when_tmux_fails() {
        let ctx = ctx();
        let fake = Fake::new();
        tmux(&fake, "exit 1");
        assert_eq!(Tmux.new_tab(&ctx, "board", Path::new("/work"), None), None);
    }

    #[test]
    fn pane_exists_asks_tmux_except_for_an_empty_id() {
        let fake = Fake::new();
        tmux(&fake, "exit 0");
        assert!(!Tmux.pane_exists(""));
        assert!(fake.log("tmux").is_empty());
        assert!(Tmux.pane_exists("%1"));
        tmux(&fake, "exit 1");
        assert!(!Tmux.pane_exists("%1"));
    }

    #[test]
    fn run_types_the_text_literally_then_presses_enter() {
        let fake = Fake::new();
        tmux(&fake, "exit 0");
        assert!(Tmux.run("%1", "echo Enter"));
        assert_eq!(fake.log("tmux"), ["send-keys -t %1 -l -- echo Enter", "send-keys -t %1 Enter"]);
    }

    #[test]
    fn run_does_not_press_enter_if_typing_failed() {
        let fake = Fake::new();
        tmux(&fake, "exit 1");
        assert!(!Tmux.run("%1", "ls"));
        assert_eq!(fake.log("tmux").len(), 1);
    }

    #[test]
    fn start_agent_types_the_quoted_command_line() {
        let ctx = ctx();
        let fake = Fake::new();
        tmux(&fake, "exit 0");
        assert!(Tmux.start_agent(&ctx, "lead", "%1", &words("claude --add-dir /x")));
        assert_eq!(fake.log("tmux")[0], "send-keys -t %1 -l -- claude --add-dir /x");
        let spaced = vec!["codex".to_string(), "a b".to_string()];
        Tmux.start_agent(&ctx, "lead", "%1", &spaced);
        assert_eq!(fake.log("tmux")[2], "send-keys -t %1 -l -- codex 'a b'");
    }

    #[test]
    fn start_agent_reports_failure() {
        let ctx = ctx();
        let fake = Fake::new();
        tmux(&fake, "exit 1");
        assert!(!Tmux.start_agent(&ctx, "lead", "%1", &words("claude")));
    }

    fn fake_pane(fake: &Fake, ps_parents: &str, pane_ok: bool) {
        let alive = if pane_ok { "" } else { "exit 1" };
        tmux(fake, &format!("case \"$*\" in *pane_pid*) echo 4242;; *pane_id*) {alive};; esac"));
        fake.program("ps", &format!("printf '%s\\n' {ps_parents}"));
    }

    #[test]
    fn an_agent_runs_while_its_shell_has_a_child() {
        let ctx = ctx();
        ctx.state.set("pane:lead", "%1");
        let fake = Fake::new();
        fake_pane(&fake, "1 4242 77", true);
        assert!(Tmux.agent_running(&ctx, "lead"));
        assert_eq!(Tmux.agent_status(&ctx, "lead"), "running");
        assert_eq!(fake.log("ps"), ["-A -o ppid=", "-A -o ppid="]);
    }

    #[test]
    fn a_shell_without_children_means_the_agent_stopped() {
        let ctx = ctx();
        ctx.state.set("pane:lead", "%1");
        let fake = Fake::new();
        fake_pane(&fake, "1 77 424", true);
        assert!(!Tmux.agent_running(&ctx, "lead"));
        assert_eq!(Tmux.agent_status(&ctx, "lead"), "stopped");
    }

    #[test]
    fn an_agent_without_a_saved_or_live_pane_is_not_running() {
        let ctx = ctx();
        let fake = Fake::new();
        fake_pane(&fake, "4242", true);
        assert!(!Tmux.agent_running(&ctx, "lead"), "no saved pane");
        ctx.state.set("pane:lead", "%1");
        fake_pane(&fake, "4242", false);
        assert!(!Tmux.agent_running(&ctx, "lead"), "pane is gone");
    }

    #[test]
    fn an_agent_is_stopped_when_ps_or_the_pid_lookup_fails() {
        let ctx = ctx();
        ctx.state.set("pane:lead", "%1");
        let fake = Fake::new();
        tmux(&fake, "case \"$*\" in *pane_pid*) exit 1;; esac");
        assert!(!Tmux.agent_running(&ctx, "lead"));
        fake_pane(&fake, "4242", true);
        fake.program("ps", "exit 1");
        assert!(!Tmux.agent_running(&ctx, "lead"));
    }

    #[test]
    fn deliver_pastes_into_the_pane_then_presses_enter() {
        let ctx = ctx();
        ctx.state.set("pane:lead", "%1");
        let fake = Fake::new();
        fake_pane(&fake, "4242", true);
        assert!(Tmux.deliver(&ctx, "lead", "hello there"));
        let log = fake.log("tmux");
        let buffer = format!("crew-{}", std::process::id());
        assert!(log.contains(&format!("set-buffer -b {buffer} -- hello there")), "{log:?}");
        assert!(log.contains(&format!("paste-buffer -p -d -b {buffer} -t %1")), "{log:?}");
        assert_eq!(log.last().unwrap(), "send-keys -t %1 Enter");
    }

    #[test]
    fn deliver_refuses_when_the_agent_is_not_running() {
        let ctx = ctx();
        ctx.state.set("pane:lead", "%1");
        let fake = Fake::new();
        fake_pane(&fake, "77", true);
        assert!(!Tmux.deliver(&ctx, "lead", "hi"));
        assert!(fake.log("tmux").iter().all(|l| !l.starts_with("set-buffer")));
    }

    #[test]
    fn deliver_does_not_press_enter_when_pasting_failed() {
        let ctx = ctx();
        ctx.state.set("pane:lead", "%1");
        let fake = Fake::new();
        fake_pane(&fake, "4242", true);
        tmux(&fake, "case \"$*\" in *pane_pid*) echo 4242;; *paste-buffer*) exit 1;; esac");
        assert!(!Tmux.deliver(&ctx, "lead", "hi"));
        assert!(fake.log("tmux").iter().all(|l| !l.ends_with("Enter")));
    }

    #[test]
    fn closing_a_pane_or_the_workspace() {
        let ctx = ctx();
        let fake = Fake::new();
        tmux(&fake, "exit 0");
        Tmux.close_pane("%3");
        Tmux.close_workspace(&ctx);
        assert_eq!(fake.log("tmux"), ["kill-pane -t %3", "kill-session -t =crew-my-app"]);
    }

    #[test]
    fn finish_layout_evens_out_each_agents_window_once() {
        let ctx = ctx();
        ctx.state.set("pane:lead", "%1");
        ctx.state.set("pane:builder", "%2");
        let fake = Fake::new();
        tmux(&fake, "case \"$*\" in *window_id*) echo @5;; esac");
        Tmux.finish_layout(&ctx);
        let log = fake.log("tmux");
        let layout: Vec<&String> =
            log.iter().filter(|l| l.starts_with("set-option") || l.starts_with("select-layout")).collect();
        assert_eq!(layout, ["set-option -w -t @5 main-pane-width 50%", "select-layout -t @5 main-vertical"]);
    }

    #[test]
    fn finish_layout_skips_roles_without_a_pane() {
        let ctx = ctx();
        let fake = Fake::new();
        tmux(&fake, "echo @5");
        Tmux.finish_layout(&ctx);
        assert!(fake.log("tmux").is_empty());
    }

    #[test]
    fn screen_is_the_captured_pane_or_empty() {
        let ctx = ctx();
        let fake = Fake::new();
        tmux(&fake, "echo 'on screen'");
        assert_eq!(Tmux.screen(&ctx, "lead"), "", "no pane saved yet");
        ctx.state.set("pane:lead", "%1");
        assert_eq!(Tmux.screen(&ctx, "lead"), "on screen\n");
        assert_eq!(fake.log("tmux"), ["capture-pane -p -t %1"]);
        tmux(&fake, "exit 1");
        assert_eq!(Tmux.screen(&ctx, "lead"), "");
    }

    #[test]
    fn notify_shows_a_status_line_message() {
        let ctx = ctx();
        let fake = Fake::new();
        tmux(&fake, "exit 0");
        Tmux.notify(&ctx, "lead stalled");
        assert_eq!(fake.log("tmux"), ["display-message -t =crew-my-app crew: lead stalled"]);
    }

    #[test]
    fn the_fake_folder_is_cleaned_up() {
        let path = {
            let fake = Fake::new();
            fake.path().to_path_buf()
        };
        assert!(!path.exists());
    }
}
