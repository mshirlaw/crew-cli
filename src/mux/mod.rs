pub mod herdr;
pub mod tmux;

use crate::crew::config::MuxKind;
use crate::crew::context::Ctx;
use anyhow::Result;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Right,
    Down,
}

pub trait Mux {
    fn name(&self) -> &'static str;
    // Checks the multiplexer is installed and usable, starting herdr's server if needed.
    fn ensure(&self, ctx: &Ctx) -> Result<()>;
    // Whether crew can talk to the session right now (herdr: server answering; tmux: session exists).
    fn ready(&self, ctx: &Ctx) -> bool;
    fn workspace_exists(&self, ctx: &Ctx) -> bool;
    // Creates the workspace with the first role's pane and saves both ids in the state.
    fn create_workspace(&self, ctx: &Ctx, role: &str) -> Result<()>;
    fn split(&self, ctx: &Ctx, role: &str, from: &str, direction: Direction) -> Option<String>;
    // A role sets the new tab up for that agent; None is a plain tab (the board).
    fn new_tab(&self, ctx: &Ctx, label: &str, dir: &Path, role: Option<&str>) -> Option<String>;
    fn pane_exists(&self, pane: &str) -> bool;
    fn run(&self, pane: &str, command: &str) -> bool;
    fn start_agent(&self, ctx: &Ctx, role: &str, pane: &str, words: &[String]) -> bool;
    fn agent_running(&self, ctx: &Ctx, role: &str) -> bool;
    fn agent_status(&self, ctx: &Ctx, role: &str) -> String;
    // False if the agent can't take the message right now; it's still on the board.
    fn deliver(&self, ctx: &Ctx, role: &str, text: &str) -> bool;
    fn close_pane(&self, pane: &str);
    fn close_workspace(&self, ctx: &Ctx);
    fn finish_layout(&self, ctx: &Ctx);
    fn screen(&self, ctx: &Ctx, role: &str) -> String;
    fn notify(&self, ctx: &Ctx, text: &str);
    fn attach(&self, ctx: &Ctx) -> Result<()>;
}

pub fn make(kind: MuxKind) -> Box<dyn Mux> {
    match kind {
        MuxKind::Herdr => Box::new(herdr::Herdr),
        MuxKind::Tmux => Box::new(tmux::Tmux),
    }
}
