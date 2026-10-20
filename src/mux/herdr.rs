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
fn env_args(crew: &Path, role: &str) -> Vec<String> {
    vec!["--env".into(), format!("CREW_DIR={}", crew.display()), "--env".into(), format!("CREW_ROLE={role}")]
}
fn split_args(from: &str, direction: Direction, keep: f32, dir: &Path, crew: &Path, role: &str) -> Vec<String> {
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
        dir.display().to_string(),
    ];
    args.extend(env_args(crew, role));
    args.push("--no-focus".into());
    args
}
fn agent_start_args(name: &str, kind: &str, pane: &str, words: &[String]) -> Vec<String> {
    let mut args =
        ["agent", "start", name, "--kind", kind, "--pane", pane, "--timeout", "25000", "--"].map(String::from).to_vec();
    // herdr runs its own executable for a known kind, so only the arguments are passed.
    args.extend(words.iter().skip(1).cloned());
    args
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
        args.extend(env_args(&ctx.crew, role));
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
        let args = split_args(from, direction, keep, &ctx.role_dir(role), &ctx.crew, role);
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
            args.extend(env_args(&ctx.crew, role));
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
        let args = agent_start_args(&ctx.agent_name(role), &kind, pane, words);
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
    use super::*;

    fn words(items: &[&str]) -> Vec<String> {
        items.iter().map(|w| (*w).to_string()).collect()
    }

    #[test]
    fn json_field_reads_a_nested_string() {
        let out = "{\"result\":{\"pane\":{\"pane_id\":\"p_7\"}}}\n";
        assert_eq!(json_field(out, "/result/pane/pane_id").as_deref(), Some("p_7"));
    }

    #[test]
    fn json_field_is_none_for_bad_input() {
        assert_eq!(json_field("not json", "/a"), None);
        assert_eq!(json_field("", "/a"), None);
        assert_eq!(json_field("{\"a\":{}}", "/a/b"), None);
        assert_eq!(json_field("{\"a\":5}", "/a"), None);
    }

    #[test]
    fn env_args_pass_the_crew_dir_and_role() {
        assert_eq!(
            env_args(Path::new("/r/.crew"), "human"),
            words(&["--env", "CREW_DIR=/r/.crew", "--env", "CREW_ROLE=human"])
        );
    }

    #[test]
    fn split_args_put_the_ratio_and_cwd_before_the_env() {
        let args = split_args("p1", Direction::Right, 1.0 / 3.0, Path::new("/w/b"), Path::new("/r/.crew"), "builder");
        assert_eq!(
            args,
            words(&[
                "pane",
                "split",
                "p1",
                "--direction",
                "right",
                "--ratio",
                "0.333",
                "--cwd",
                "/w/b",
                "--env",
                "CREW_DIR=/r/.crew",
                "--env",
                "CREW_ROLE=builder",
                "--no-focus",
            ])
        );
    }

    #[test]
    fn split_args_down_direction() {
        let args = split_args("p1", Direction::Down, 0.5, Path::new("/w"), Path::new("/c"), "x");
        assert_eq!(args[3..7], words(&["--direction", "down", "--ratio", "0.500"])[..]);
    }

    #[test]
    fn agent_start_forwards_only_arguments_after_a_delimiter() {
        let args =
            agent_start_args("app-lead", "claude", "p2", &words(&["claude", "--add-dir", "/c", "--", "kickoff"]));
        assert_eq!(
            args,
            words(&[
                "agent",
                "start",
                "app-lead",
                "--kind",
                "claude",
                "--pane",
                "p2",
                "--timeout",
                "25000",
                "--",
                "--add-dir",
                "/c",
                "--",
                "kickoff",
            ])
        );
    }

    #[test]
    fn agent_start_with_no_arguments_ends_at_the_delimiter() {
        let args = agent_start_args("n", "codex", "p", &words(&["codex"]));
        assert_eq!(args.last().map(String::as_str), Some("--"));
        assert!(agent_start_args("n", "codex", "p", &[]).ends_with(&words(&["--"])));
    }
    use crate::commands::testing::{ctx_in, scratch_dir};
    use crate::util::testing::Fake;

    fn ctx() -> Ctx {
        let root = scratch_dir().join("my app");
        std::fs::create_dir_all(&root).unwrap();
        let mut ctx = ctx_in(
            &root,
            Some(
                "multiplexer = \"herdr\"\n[agents.shellish]\nkind = \"\"\n[roles.lead]\nagent = \"claude\"\nworktree = false\n\
                 [roles.builder]\nagent = \"codex\"\nworktree = false\n[roles.typed]\nagent = \"shellish\"\nworktree = false\n",
            ),
        );
        ctx.key = "my-app".to_string();
        ctx
    }

    fn herdr(fake: &Fake, body: &str) {
        fake.program("herdr", body);
    }

    fn split_words(text: &str) -> Vec<String> {
        text.split(' ').map(String::from).collect()
    }

    const CREATED: &str = "echo '{\"result\":{\"workspace\":{\"workspace_id\":\"w1\"},\"tab\":{\"tab_id\":\"w1:t1\"},\"root_pane\":{\"pane_id\":\"w1:p1\"}}}'";

    #[test]
    fn json_fields_are_read_by_pointer() {
        let out = " {\"result\":{\"pane\":{\"pane_id\":\"p1\",\"n\":3}}}\n";
        assert_eq!(json_field(out, "/result/pane/pane_id").as_deref(), Some("p1"));
        assert_eq!(json_field(out, "/result/pane/missing"), None);
        assert_eq!(json_field(out, "/result/pane/n"), None, "numbers are not strings");
        assert_eq!(json_field("not json", "/a"), None);
        assert_eq!(json_field("", "/a"), None);
    }

    #[test]
    fn new_panes_get_the_crew_environment() {
        let ctx = ctx();
        assert_eq!(
            env_args(&ctx.crew, "lead"),
            ["--env".to_string(), format!("CREW_DIR={}", ctx.crew.display()), "--env".into(), "CREW_ROLE=lead".into()]
        );
    }

    #[test]
    fn ensure_needs_herdr() {
        let _fake = Fake::new();
        let error = Herdr.ensure(&ctx()).unwrap_err().to_string();
        assert!(error.contains("herdr not found") && error.contains("herdr.dev") && error.contains("tmux"), "{error}");
    }

    #[test]
    fn ensure_does_not_start_a_server_that_is_already_answering() {
        let fake = Fake::new();
        herdr(&fake, "exit 0");
        Herdr.ensure(&ctx()).unwrap();
        assert_eq!(fake.log("herdr"), ["workspace list"]);
    }

    #[test]
    fn ensure_starts_the_server_in_the_background_and_waits_for_it() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, "case \"$1\" in server) touch \"$D/up\";; workspace) test -f \"$D/up\";; esac");
        Herdr.ensure(&ctx).unwrap();
        let log = fake.log("herdr");
        assert_eq!(log[0], "workspace list");
        assert!(log.contains(&"server".to_string()), "{log:?}");
        assert!(ctx.state.dir().join("herdr-server.log").is_file());
    }

    #[test]
    fn workspace_exists_needs_a_saved_workspace_that_herdr_still_has() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, "exit 0");
        assert!(!Herdr.workspace_exists(&ctx));
        assert!(fake.log("herdr").is_empty());
        ctx.state.set("ws", "w9");
        assert!(Herdr.workspace_exists(&ctx));
        assert_eq!(fake.log("herdr"), ["workspace get w9"]);
        herdr(&fake, "exit 1");
        assert!(!Herdr.workspace_exists(&ctx));
    }

    #[test]
    fn create_workspace_saves_the_ids_and_names_the_first_tab() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, CREATED);
        Herdr.create_workspace(&ctx, "lead").unwrap();
        let log = fake.log("herdr");
        assert_eq!(
            log[0],
            format!(
                "workspace create --cwd {} --label crew:my-app --env CREW_DIR={} --env CREW_ROLE=lead --no-focus",
                ctx.root.display(),
                ctx.crew.display()
            )
        );
        assert_eq!(log[1], "tab rename w1:t1 agents");
        assert_eq!(ctx.state.get("ws").as_deref(), Some("w1"));
        assert_eq!(ctx.state.get("pane:lead").as_deref(), Some("w1:p1"));
    }

    #[test]
    fn create_workspace_works_without_a_tab_in_the_answer() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(
            &fake,
            "echo '{\"result\":{\"workspace\":{\"workspace_id\":\"w2\"},\"root_pane\":{\"pane_id\":\"w2:p1\"}}}'",
        );
        Herdr.create_workspace(&ctx, "lead").unwrap();
        assert_eq!(fake.log("herdr").len(), 1, "no rename without a tab id");
        assert_eq!(ctx.state.get("ws").as_deref(), Some("w2"));
    }

    #[test]
    fn create_workspace_reports_what_herdr_said_when_it_fails() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, "echo 'server not running' >&2; exit 1");
        let error = Herdr.create_workspace(&ctx, "lead").unwrap_err().to_string();
        assert!(error.contains("could not create a herdr workspace: server not running"), "{error}");
        assert_eq!(ctx.state.get("ws"), None);
    }

    #[test]
    fn create_workspace_rejects_an_answer_without_the_ids() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, "echo '{\"result\":{}}'");
        assert!(Herdr.create_workspace(&ctx, "lead").is_err());
        assert_eq!(ctx.state.get("pane:lead"), None);
    }

    #[test]
    fn split_passes_direction_ratio_directory_and_environment() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, "echo '{\"result\":{\"pane\":{\"pane_id\":\"w1:p2\"}}}'");
        assert_eq!(Herdr.split(&ctx, "builder", "w1:p1", Direction::Right, 0.5).as_deref(), Some("w1:p2"));
        Herdr.split(&ctx, "builder", "w1:p1", Direction::Down, 1.0 / 3.0);
        let log = fake.log("herdr");
        let tail = format!(
            "--cwd {} --env CREW_DIR={} --env CREW_ROLE=builder --no-focus",
            ctx.root.display(),
            ctx.crew.display()
        );
        assert_eq!(log[0], format!("pane split w1:p1 --direction right --ratio 0.500 {tail}"));
        assert_eq!(log[1], format!("pane split w1:p1 --direction down --ratio 0.333 {tail}"));
    }

    #[test]
    fn split_gives_nothing_back_when_herdr_fails() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, "echo 'no such pane' >&2; exit 1");
        assert_eq!(Herdr.split(&ctx, "builder", "gone", Direction::Right, 0.5), None);
    }

    #[test]
    fn new_tab_needs_a_saved_workspace() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, "echo '{\"result\":{\"root_pane\":{\"pane_id\":\"w1:b1\"}}}'");
        assert_eq!(Herdr.new_tab(&ctx, "board", Path::new("/work"), None), None);
        assert!(fake.log("herdr").is_empty());
    }

    #[test]
    fn new_tab_creates_a_labelled_tab_in_the_workspace() {
        let ctx = ctx();
        ctx.state.set("ws", "w1");
        let fake = Fake::new();
        herdr(&fake, "echo '{\"result\":{\"root_pane\":{\"pane_id\":\"w1:b1\"}}}'");
        assert_eq!(Herdr.new_tab(&ctx, "board", Path::new("/work"), None).as_deref(), Some("w1:b1"));
        Herdr.new_tab(&ctx, "agents 2", Path::new("/work"), Some("builder"));
        let log = fake.log("herdr");
        assert_eq!(log[0], "tab create --workspace w1 --cwd /work --label board --no-focus");
        assert!(log[1].contains("--label agents 2 --env") && log[1].contains("CREW_ROLE=builder"), "{}", log[1]);
        assert!(log[1].ends_with("--no-focus"));
    }

    #[test]
    fn pane_exists_asks_herdr_except_for_an_empty_id() {
        let fake = Fake::new();
        herdr(&fake, "exit 0");
        assert!(!Herdr.pane_exists(""));
        assert!(fake.log("herdr").is_empty());
        assert!(Herdr.pane_exists("w1:p1"));
        assert_eq!(fake.log("herdr"), ["pane get w1:p1"]);
        herdr(&fake, "exit 1");
        assert!(!Herdr.pane_exists("w1:p1"));
    }

    #[test]
    fn run_types_a_command_into_the_pane() {
        let fake = Fake::new();
        herdr(&fake, "exit 0");
        assert!(Herdr.run("w1:p1", "tail -F board.md"));
        assert_eq!(fake.log("herdr"), ["pane run w1:p1 tail -F board.md"]);
        herdr(&fake, "exit 1");
        assert!(!Herdr.run("w1:p1", "ls"));
    }

    #[test]
    fn a_known_agent_is_started_through_herdr_with_only_its_arguments() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, "exit 0");
        assert!(Herdr.start_agent(&ctx, "builder", "w1:p2", &split_words("codex --add-dir /x kickoff")));
        assert_eq!(
            fake.log("herdr"),
            ["agent start my-app-builder --kind codex --pane w1:p2 --timeout 25000 -- --add-dir /x kickoff"]
        );
    }

    #[test]
    fn an_agent_waiting_on_a_prompt_or_busy_still_counts_as_started() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, "echo 'error: agent_not_ready' >&2; exit 1");
        assert!(Herdr.start_agent(&ctx, "builder", "p", &split_words("codex x")));
        herdr(&fake, "echo 'timeout waiting' >&2; exit 1");
        assert!(Herdr.start_agent(&ctx, "builder", "p", &split_words("codex x")));
        assert_eq!(fake.log("herdr").len(), 2, "neither case is retried");
    }

    #[test]
    fn a_failing_start_is_retried_then_reported() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, "echo 'boom' >&2; exit 1");
        assert!(!Herdr.start_agent(&ctx, "builder", "p", &split_words("codex x")));
        assert_eq!(fake.log("herdr").len(), 4);
    }

    #[test]
    fn an_agent_without_a_herdr_kind_is_typed_into_the_pane() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, "exit 0");
        let command = vec!["mytool".to_string(), "two words".to_string()];
        assert!(Herdr.start_agent(&ctx, "typed", "w1:p3", &command));
        assert_eq!(fake.log("herdr"), ["pane run w1:p3 mytool 'two words'"]);
        herdr(&fake, "exit 1");
        assert!(!Herdr.start_agent(&ctx, "typed", "w1:p3", &command));
    }

    #[test]
    fn agent_status_comes_from_herdr_and_defaults_to_stopped() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, "echo '{\"result\":{\"agent\":{\"agent_status\":\"working\"}}}'");
        assert_eq!(Herdr.agent_status(&ctx, "lead"), "working");
        assert!(Herdr.agent_running(&ctx, "lead"));
        assert_eq!(fake.log("herdr")[0], "agent get my-app-lead");
        herdr(&fake, "echo 'not json'");
        assert_eq!(Herdr.agent_status(&ctx, "lead"), "stopped");
        herdr(&fake, "exit 1");
        assert_eq!(Herdr.agent_status(&ctx, "lead"), "stopped");
        assert!(!Herdr.agent_running(&ctx, "lead"));
    }

    #[test]
    fn deliver_prompts_the_agent_by_name() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, "exit 0");
        assert!(Herdr.deliver(&ctx, "lead", "hello there"));
        assert_eq!(fake.log("herdr"), ["agent prompt my-app-lead hello there"]);
    }

    #[test]
    fn deliver_falls_back_to_the_saved_pane() {
        let ctx = ctx();
        ctx.state.set("pane:lead", "w1:p1");
        let fake = Fake::new();
        herdr(&fake, "case \"$3\" in my-app-lead) exit 1;; esac");
        assert!(Herdr.deliver(&ctx, "lead", "hi"));
        assert_eq!(fake.log("herdr"), ["agent prompt my-app-lead hi", "agent prompt w1:p1 hi"]);
    }

    #[test]
    fn deliver_fails_when_herdr_refuses_and_there_is_no_pane() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, "exit 1");
        assert!(!Herdr.deliver(&ctx, "lead", "hi"));
        assert_eq!(fake.log("herdr").len(), 1);
        ctx.state.set("pane:lead", "w1:p1");
        assert!(!Herdr.deliver(&ctx, "lead", "hi"));
    }

    #[test]
    fn closing_panes_and_the_workspace() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, "exit 0");
        Herdr.close_pane("w1:p2");
        Herdr.close_workspace(&ctx);
        assert_eq!(fake.log("herdr"), ["pane close w1:p2"], "no workspace saved yet");
        ctx.state.set("ws", "w1");
        Herdr.close_workspace(&ctx);
        assert_eq!(fake.log("herdr")[1], "workspace close w1");
    }

    #[test]
    fn the_layout_needs_no_finishing_touches() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, "exit 0");
        Herdr.finish_layout(&ctx);
        assert!(fake.log("herdr").is_empty());
    }

    #[test]
    fn screen_reads_the_visible_text_of_the_saved_pane() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, "echo 'on screen'");
        assert_eq!(Herdr.screen(&ctx, "lead"), "", "no pane saved yet");
        ctx.state.set("pane:lead", "w1:p1");
        assert_eq!(Herdr.screen(&ctx, "lead"), "on screen\n");
        assert_eq!(fake.log("herdr"), ["pane read w1:p1 --source visible"]);
        herdr(&fake, "exit 1");
        assert_eq!(Herdr.screen(&ctx, "lead"), "");
    }

    #[test]
    fn notify_shows_a_herdr_notification() {
        let ctx = ctx();
        let fake = Fake::new();
        herdr(&fake, "exit 0");
        Herdr.notify(&ctx, "lead stalled");
        assert_eq!(fake.log("herdr"), ["notification show crew --body lead stalled --sound request"]);
    }

    #[test]
    fn herdr_is_named_herdr() {
        assert_eq!(Herdr.name(), "herdr");
    }
}
