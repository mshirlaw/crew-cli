//! Terminal multiplexers. herdr and tmux each implement the same small set of actions.

pub mod herdr;
pub mod tmux;

use crate::config::MuxKind;
use crate::context::Ctx;
use anyhow::Result;
use std::path::Path;

/// Which way to split a pane.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Right,
    Down,
}

/// What crew needs from a terminal multiplexer. Each method is one action.
pub trait Mux {
    /// The multiplexer's name.
    fn name(&self) -> &'static str;
    /// Checks the multiplexer is installed and usable, starting a server if it needs one.
    fn ensure(&self, ctx: &Ctx) -> Result<()>;
    /// Succeeds if the crew's session can be reached right now.
    fn ready(&self, ctx: &Ctx) -> bool;
    /// Succeeds if the crew's workspace or session still exists.
    fn workspace_exists(&self, ctx: &Ctx) -> bool;
    /// Creates the crew's workspace with the first role's pane, and saves both in the state.
    fn create_workspace(&self, ctx: &Ctx, role: &str) -> Result<()>;
    /// Splits a pane to make one for a role, returning the new pane's id.
    fn split(&self, ctx: &Ctx, role: &str, from: &str, direction: Direction) -> Option<String>;
    /// Opens a new tab or window, returning its pane id. A role sets it up for that agent.
    fn new_tab(&self, ctx: &Ctx, label: &str, dir: &Path, role: Option<&str>) -> Option<String>;
    /// Succeeds if a pane still exists.
    fn pane_exists(&self, pane: &str) -> bool;
    /// Types a command into a pane and presses Enter.
    fn run(&self, pane: &str, command: &str) -> bool;
    /// Starts a role's agent in a pane from its command words.
    fn start_agent(&self, ctx: &Ctx, role: &str, pane: &str, words: &[String]) -> bool;
    /// Succeeds if a role's agent is running.
    fn agent_running(&self, ctx: &Ctx, role: &str) -> bool;
    /// A role's agent state, e.g. working, idle, running or stopped.
    fn agent_status(&self, ctx: &Ctx, role: &str) -> String;
    /// Types a message into a role's agent. Fails if the agent can't take it right now.
    fn deliver(&self, ctx: &Ctx, role: &str, text: &str) -> bool;
    /// Closes a pane.
    fn close_pane(&self, pane: &str);
    /// Closes the crew's workspace or session.
    fn close_workspace(&self, ctx: &Ctx);
    /// Tidies the layout after panes are added.
    fn finish_layout(&self, ctx: &Ctx);
    /// What's currently on screen in a role's pane.
    fn screen(&self, ctx: &Ctx, role: &str) -> String;
    /// Shows the human a notification.
    fn notify(&self, ctx: &Ctx, text: &str);
    /// Opens the crew's session in this terminal.
    fn attach(&self, ctx: &Ctx) -> Result<()>;
}

/// Returns the implementation for a multiplexer.
pub fn make(kind: MuxKind) -> Box<dyn Mux> {
    match kind {
        MuxKind::Herdr => Box::new(herdr::Herdr),
        MuxKind::Tmux => Box::new(tmux::Tmux),
    }
}
