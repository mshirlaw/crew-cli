use super::{Direction, Mux};
use crate::crew::context::Ctx;
use crate::util::ui;
use crate::util::{has, run_all, run_ok, run_out, shell_join, spawn_detached};
use anyhow::{bail, Result};
use std::ffi::OsStr;
use std::path::Path;

pub struct Herdr;
fn json_field(output: &str, pointer: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(output.trim()).ok()?;
    value.pointer(pointer)?.as_str().map(String::from)
}
fn env_args(ctx: &Ctx, role: &str) -> Vec<String> {
    vec!["--env".into(), format!("CREW_DIR={}", ctx.crew.display()), "--env".into(), format!("CREW_ROLE={role}")]
}

impl Mux for Herdr {
    fn name(&self) -> &'static str {
        "herdr"
    }

    fn ensure(&self, ctx: &Ctx) -> Result<()> {
        if !has("herdr") {
            bail!("herdr not found. Install it from https://herdr.dev, or set multiplexer = \"tmux\"");
        }
        if self.ready(ctx) {
            return Ok(());
        }
        ui::info("starting herdr server in the background");
        std::fs::create_dir_all(ctx.state.dir())?;
        let log = ctx.state.dir().join("herdr-server.log");
        spawn_detached(OsStr::new("herdr"), &["server"], &[], &log)?;
        for _ in 0..40 {
            if self.ready(ctx) {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        bail!("herdr server did not start (see {})", log.display())
    }

    fn ready(&self, _ctx: &Ctx) -> bool {
        run_ok("herdr", &["workspace", "list"])
    }

    fn workspace_exists(&self, ctx: &Ctx) -> bool {
        ctx.state.get("ws").is_some_and(|ws| run_ok("herdr", &["workspace", "get", &ws]))
    }

    fn create_workspace(&self, ctx: &Ctx, role: &str) -> Result<()> {
        let dir = ctx.role_dir(role).display().to_string();
        let label = format!("crew:{}", ctx.key);
        let mut args = vec!["workspace".to_string(), "create".into(), "--cwd".into(), dir, "--label".into(), label];
        args.extend(env_args(ctx, role));
        args.push("--no-focus".into());
        let (_, out) = run_all("herdr", &args);
        let (Some(ws), Some(pane)) =
            (json_field(&out, "/result/workspace/workspace_id"), json_field(&out, "/result/root_pane/pane_id"))
        else {
            bail!("could not create a herdr workspace: {}", out.trim());
        };
        if let Some(tab) = json_field(&out, "/result/tab/tab_id") {
            run_ok("herdr", &["tab", "rename", &tab, "agents"]);
        }
        ctx.state.set("ws", &ws);
        ctx.state.set(&format!("pane:{role}"), &pane);
        ui::ok(format!("herdr workspace crew:{} ({ws})", ctx.key));
        Ok(())
    }

    fn split(&self, ctx: &Ctx, role: &str, from: &str, direction: Direction, keep: f32) -> Option<String> {
        let direction = if direction == Direction::Right { "right" } else { "down" };
        let mut args = vec![
            "pane".to_string(),
            "split".into(),
            from.into(),
            "--direction".into(),
            direction.into(),
            "--ratio".into(),
            format!("{keep:.3}"),
            "--cwd".into(),
            ctx.role_dir(role).display().to_string(),
        ];
        args.extend(env_args(ctx, role));
        args.push("--no-focus".into());
        let (success, out) = run_all("herdr", &args);
        if !success {
            ui::warn(format!("split failed: {}", out.trim()));
        }
        json_field(&out, "/result/pane/pane_id")
    }

    fn new_tab(&self, ctx: &Ctx, label: &str, dir: &Path, role: Option<&str>) -> Option<String> {
        let ws = ctx.state.get("ws")?;
        let mut args = vec![
            "tab".to_string(),
            "create".into(),
            "--workspace".into(),
            ws,
            "--cwd".into(),
            dir.display().to_string(),
            "--label".into(),
            label.into(),
        ];
        if let Some(role) = role {
            args.extend(env_args(ctx, role));
        }
        args.push("--no-focus".into());
        let (_, out) = run_all("herdr", &args);
        json_field(&out, "/result/root_pane/pane_id")
    }

    fn pane_exists(&self, pane: &str) -> bool {
        !pane.is_empty() && run_ok("herdr", &["pane", "get", pane])
    }

    fn run(&self, pane: &str, command: &str) -> bool {
        run_ok("herdr", &["pane", "run", pane, command])
    }

    fn start_agent(&self, ctx: &Ctx, role: &str, pane: &str, words: &[String]) -> bool {
        let Some(kind) = ctx.config.agent_kind(&ctx.config.role_agent(role)) else {
            let launched = self.run(pane, &shell_join(words));
            if launched {
                ui::ok(format!("{role} → {} launched", words[0]));
            } else {
                ui::warn(format!("{role}: could not launch {}", words[0]));
            }
            return launched;
        };
        // herdr runs its own executable for a known kind, so only the arguments are passed.
        let mut args = vec![
            "agent".to_string(),
            "start".into(),
            ctx.agent_name(role),
            "--kind".into(),
            kind.clone(),
            "--pane".into(),
            pane.into(),
            "--timeout".into(),
            "25000".into(),
            "--".into(),
        ];
        args.extend(words.iter().skip(1).cloned());
        let mut last = String::new();
        for _ in 0..4 {
            let (success, out) = run_all("herdr", &args);
            if success {
                ui::ok(format!("{role} → {kind} ready"));
                return true;
            }
            if out.contains("agent_not_ready") {
                ui::ok(format!("{role} → {kind} started (approve the prompt in its pane)"));
                return true;
            }
            if out.contains("timeout") {
                ui::ok(format!("{role} → {kind} started (busy with its kickoff)"));
                return true;
            }
            last = out;
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        ui::warn(format!("{role}: herdr agent start failed: {}", last.trim().chars().take(300).collect::<String>()));
        false
    }

    fn agent_running(&self, ctx: &Ctx, role: &str) -> bool {
        run_ok("herdr", &["agent", "get", &ctx.agent_name(role)])
    }

    fn agent_status(&self, ctx: &Ctx, role: &str) -> String {
        run_out("herdr", &["agent", "get", &ctx.agent_name(role)])
            .and_then(|out| json_field(&out, "/result/agent/agent_status"))
            .unwrap_or_else(|| "stopped".to_string())
    }

    fn deliver(&self, ctx: &Ctx, role: &str, text: &str) -> bool {
        if run_ok("herdr", &["agent", "prompt", &ctx.agent_name(role), text]) {
            return true;
        }
        ctx.state.get(&format!("pane:{role}")).is_some_and(|pane| run_ok("herdr", &["agent", "prompt", &pane, text]))
    }

    fn close_pane(&self, pane: &str) {
        run_ok("herdr", &["pane", "close", pane]);
    }

    fn close_workspace(&self, ctx: &Ctx) {
        if let Some(ws) = ctx.state.get("ws") {
            run_ok("herdr", &["workspace", "close", &ws]);
        }
    }

    fn finish_layout(&self, _ctx: &Ctx) {}

    fn screen(&self, ctx: &Ctx, role: &str) -> String {
        ctx.state
            .get(&format!("pane:{role}"))
            .and_then(|pane| run_out("herdr", &["pane", "read", &pane, "--source", "visible"]))
            .unwrap_or_default()
    }

    fn notify(&self, _ctx: &Ctx, text: &str) {
        run_ok("herdr", &["notification", "show", "crew", "--body", text, "--sound", "request"]);
    }

    fn attach(&self, ctx: &Ctx) -> Result<()> {
        use std::os::unix::process::CommandExt;
        if let Some(ws) = ctx.state.get("ws") {
            run_ok("herdr", &["workspace", "focus", &ws]);
        }
        if std::env::var("HERDR_ENV").as_deref() == Ok("1") {
            ui::info(format!("already inside herdr, switched to crew:{}", ctx.key));
            return Ok(());
        }
        let err = std::process::Command::new("herdr").exec();
        bail!("could not open herdr: {err}")
    }
}

#[cfg(test)]
mod tests {
    use super::json_field;

    #[test]
    fn json_field_reads_string_pointer() {
        let output = r#"{"result":{"pane":{"pane_id":"p-123"}}}"#;
        assert_eq!(json_field(output, "/result/pane/pane_id").as_deref(), Some("p-123"));
    }

    #[test]
    fn json_field_rejects_invalid_or_non_string_values() {
        assert_eq!(json_field("not json", "/result"), None);
        assert_eq!(json_field(r#"{"result":{"count":3}}"#, "/result/count"), None);
        assert_eq!(json_field(r#"{"result":{}}"#, "/result/missing"), None);
    }
}
