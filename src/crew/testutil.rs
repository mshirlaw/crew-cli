// Test fixtures: a Ctx in a temp folder with a fake multiplexer that records what crew asked of it.

use super::config::Config;
use super::context::Ctx;
use super::state::State;
use crate::mux::{Direction, Mux};
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct FakeState {
    pub calls: Vec<String>,
    pub screens: HashMap<String, String>,
    pub live_panes: HashSet<String>,
    pub running: HashSet<String>,
    pub ready: bool,
    pub accepts_messages: bool,
}

pub struct FakeMux {
    pub state: Arc<Mutex<FakeState>>,
    name: &'static str,
}

impl FakeMux {
    fn log(&self, call: String) {
        self.state.lock().unwrap().calls.push(call);
    }
}

impl Mux for FakeMux {
    fn name(&self) -> &'static str {
        self.name
    }
    fn ensure(&self, _: &Ctx) -> Result<()> {
        Ok(())
    }
    fn ready(&self, _: &Ctx) -> bool {
        self.state.lock().unwrap().ready
    }
    fn workspace_exists(&self, _: &Ctx) -> bool {
        false
    }
    fn create_workspace(&self, ctx: &Ctx, role: &str) -> Result<()> {
        self.log(format!("create_workspace {role}"));
        let pane = format!("p-{role}");
        ctx.state.set(&format!("pane:{role}"), &pane);
        self.state.lock().unwrap().live_panes.insert(pane);
        Ok(())
    }
    fn split(&self, _: &Ctx, role: &str, from: &str, direction: Direction, keep: f32) -> Option<String> {
        let way = if direction == Direction::Right { "right" } else { "down" };
        self.log(format!("split {role} from {from} {way} {keep:.2}"));
        let pane = format!("p-{role}");
        self.state.lock().unwrap().live_panes.insert(pane.clone());
        Some(pane)
    }
    fn new_tab(&self, _: &Ctx, label: &str, _: &Path, role: Option<&str>) -> Option<String> {
        self.log(format!("new_tab {label} {}", role.unwrap_or("-")));
        let pane = format!("t-{label}");
        self.state.lock().unwrap().live_panes.insert(pane.clone());
        Some(pane)
    }
    fn pane_exists(&self, pane: &str) -> bool {
        self.state.lock().unwrap().live_panes.contains(pane)
    }
    fn run(&self, pane: &str, command: &str) -> bool {
        self.log(format!("run {pane} {command}"));
        true
    }
    fn start_agent(&self, _: &Ctx, role: &str, pane: &str, words: &[String]) -> bool {
        self.log(format!("start_agent {role} {pane} {}", words.join(" ")));
        self.state.lock().unwrap().running.insert(role.to_string());
        true
    }
    fn agent_running(&self, _: &Ctx, role: &str) -> bool {
        self.state.lock().unwrap().running.contains(role)
    }
    fn agent_status(&self, _: &Ctx, _: &str) -> String {
        "idle".to_string()
    }
    fn deliver(&self, _: &Ctx, role: &str, text: &str) -> bool {
        self.log(format!("deliver {role} {text}"));
        self.state.lock().unwrap().accepts_messages
    }
    fn close_pane(&self, pane: &str) {
        self.log(format!("close_pane {pane}"));
        self.state.lock().unwrap().live_panes.remove(pane);
    }
    fn close_workspace(&self, _: &Ctx) {
        self.log("close_workspace".to_string());
    }
    fn finish_layout(&self, _: &Ctx) {
        self.log("finish_layout".to_string());
    }
    fn screen(&self, _: &Ctx, role: &str) -> String {
        self.state.lock().unwrap().screens.get(role).cloned().unwrap_or_default()
    }
    fn notify(&self, _: &Ctx, text: &str) {
        self.log(format!("notify {text}"));
    }
    fn attach(&self, _: &Ctx) -> Result<()> {
        Ok(())
    }
}

pub struct Fixture {
    pub ctx: Ctx,
    pub fake: Arc<Mutex<FakeState>>,
    pub dir: PathBuf,
}

impl Fixture {
    pub fn calls(&self) -> Vec<String> {
        self.fake.lock().unwrap().calls.clone()
    }

    pub fn board(&self) -> String {
        std::fs::read_to_string(self.ctx.file("board.md")).unwrap_or_default()
    }

    pub fn set_screen(&self, role: &str, text: &str) {
        self.fake.lock().unwrap().screens.insert(role.to_string(), text.to_string());
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub const ROLES: &str = "[roles.lead]\nagent = \"claude\"\nworktree = false\n\
    [roles.builder]\nagent = \"claude\"\nworktree = true\n\
    [roles.reviewer]\nagent = \"codex\"\nworktree = true\n";

static COUNTER: AtomicUsize = AtomicUsize::new(0);

// `repo_toml` is the repo config; worktree_dir is always pointed into the temp folder.
pub fn fixture(repo_toml: &str) -> Fixture {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("crew-test-{}-{n}", std::process::id()));
    let root = dir.join("my-repo");
    let crew = root.join(".crew");
    std::fs::create_dir_all(&crew).unwrap();
    let toml = format!("worktree_dir = \"{}\"\n{repo_toml}", dir.join("wt").display());
    let fake = Arc::new(Mutex::new(FakeState { ready: true, accepts_messages: true, ..FakeState::default() }));
    let ctx = Ctx {
        state: State::new(crew.join("state")),
        config: Config::from_toml("", &toml),
        mux: Box::new(FakeMux { state: Arc::clone(&fake), name: "tmux" }),
        slug: "my-repo".to_string(),
        root,
        crew,
    };
    Fixture { ctx, fake, dir }
}

// A second Ctx over the same folders, e.g. for another thread (Ctx itself isn't Sync).
pub fn ctx_at(crew: &Path, state: &Path) -> Ctx {
    let fake = Arc::new(Mutex::new(FakeState::default()));
    Ctx {
        state: State::new(state.to_path_buf()),
        config: Config::from_toml("", ""),
        mux: Box::new(FakeMux { state: fake, name: "tmux" }),
        slug: "my-repo".to_string(),
        root: crew.parent().unwrap().to_path_buf(),
        crew: crew.to_path_buf(),
    }
}
