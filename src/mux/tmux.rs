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
    let out = run_out("tmux", &["-V"])?;
    out.chars().skip_while(|c| !c.is_ascii_digit()).take_while(char::is_ascii_digit).collect::<String>().parse().ok()
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
