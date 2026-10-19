//! crew: start a team of AI coding agents in herdr or tmux, each with a role,
//! its own git worktree, and a shared message board.

mod agents;
mod board;
mod commands;
mod config;
mod context;
mod lifecycle;
mod mux;
mod roles;
mod setup;
mod state;
mod tasks;
mod templates;
mod ui;
mod util;
mod watch;

use anyhow::Result;
use clap::{Parser, Subcommand};
use context::Ctx;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "crew",
    version,
    about = "A team of AI coding agents in herdr or tmux, each with a role, its own worktree and a shared message board",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create .crew/ in this repo (config, roles, board, tasks)
    #[command(alias = "init")]
    Setup,
    /// Set up if needed, start or revive every agent, and open the session
    #[command(alias = "up")]
    Start {
        /// Don't open the session in this terminal afterwards
        #[arg(long)]
        no_attach: bool,
    },
    /// Post to the board and type the message into the recipients' sessions
    #[command(alias = "tell")]
    Say {
        /// A role, "all", or "human"
        to: String,
        /// The message
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        message: Vec<String>,
    },
    /// Print the last lines of the board
    Board {
        /// How many lines
        #[arg(default_value_t = 40)]
        lines: usize,
    },
    /// Print the task list
    Tasks,
    /// Change the shared task list (safe for several agents at once)
    Task {
        #[command(subcommand)]
        command: Option<TaskCommand>,
    },
    /// Each agent and what it's doing
    #[command(alias = "ps")]
    Status,
    /// Restart agents; they resume from their notes
    Restart {
        /// A role, or "all"
        target: String,
    },
    /// Close the session (files and worktrees are kept)
    #[command(alias = "down")]
    Stop,
    /// Stop and remove worktrees that have no uncommitted changes (branches are kept)
    Teardown {
        /// Don't ask for confirmation
        #[arg(long, short)]
        yes: bool,
    },
    /// List, add, edit, change, remove or save roles
    #[command(alias = "roles")]
    Role {
        #[command(subcommand)]
        command: Option<RoleCommand>,
    },
    /// Show or edit settings
    Config {
        #[command(subcommand)]
        command: Option<ConfigCommand>,
    },
    /// Check for stalled agents (crew start runs this in the background)
    Watch {
        /// Check once and exit
        #[arg(long)]
        once: bool,
    },
    /// Check what's installed
    Doctor,
    /// Print the version
    Version,
}

#[derive(Subcommand)]
enum TaskCommand {
    /// Add a task
    Add {
        /// The task's title
        #[arg(required = true)]
        title: Vec<String>,
        /// The role doing it
        #[arg(long)]
        owner: Option<String>,
        /// Acceptance criteria or other notes
        #[arg(long)]
        notes: Option<String>,
    },
    /// Change a task: status=<todo|doing|review|blocked|done> owner=<role> notes="..."
    Set {
        id: u32,
        #[arg(required = true)]
        changes: Vec<String>,
    },
    /// Mark a task done, optionally with notes
    Done {
        id: u32,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        notes: Vec<String>,
    },
    /// Print the task list
    #[command(alias = "ls")]
    List,
}

#[derive(Subcommand)]
enum RoleCommand {
    /// List roles
    #[command(alias = "ls")]
    List,
    /// Print a role's instructions
    Show { role: String },
    /// Add a role
    Add {
        role: String,
        /// The agent that plays it, e.g. claude, codex or gemini
        #[arg(long)]
        agent: Option<String>,
        /// Work in the main checkout instead of a worktree
        #[arg(long)]
        no_worktree: bool,
        /// Copy another role's instructions
        #[arg(long)]
        like: Option<String>,
    },
    /// Edit a role's instructions (a running agent is told to re-read them)
    Edit { role: String },
    /// Change a role, e.g. agent=codex worktree=no
    Set {
        role: String,
        #[arg(required = true)]
        changes: Vec<String>,
    },
    /// Remove a role (its file, worktree and branch are kept)
    #[command(alias = "remove")]
    Rm { role: String },
    /// Save a role's instructions, or all roles, as your defaults for new repos
    Save { role: Option<String> },
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Show the settings in effect for this repo
    Show,
    /// Edit this repo's .crew/config.toml, or the global config with --global
    Edit {
        #[arg(long)]
        global: bool,
    },
    /// Create the global config if it doesn't exist (install.sh runs this)
    Init,
    /// Print a sample config with every setting explained
    Example,
}

/// Runs the chosen command. Returns false for a check that should exit with failure.
fn run(cli: Cli) -> Result<bool> {
    // Commands that work outside a repo.
    match &cli.command {
        Command::Version => {
            println!("crew {}", env!("CARGO_PKG_VERSION"));
            return Ok(true);
        }
        Command::Doctor => return commands::cmd_doctor(),
        Command::Config { command: Some(ConfigCommand::Init) } => return commands::cmd_config_init().map(|_| true),
        Command::Config { command: Some(ConfigCommand::Example) } => {
            commands::cmd_config_example();
            return Ok(true);
        }
        Command::Config { command: Some(ConfigCommand::Edit { global: true }) } => {
            return commands::cmd_config_edit_global().map(|_| true)
        }
        _ => {}
    }
    let mut ctx = Ctx::load()?;
    match cli.command {
        Command::Setup => setup::cmd_setup(&mut ctx)?,
        Command::Start { no_attach } => lifecycle::cmd_start(&mut ctx, !no_attach)?,
        Command::Say { to, message } => board::cmd_say(&ctx, &to, &message)?,
        Command::Board { lines } => board::cmd_board(&ctx, lines)?,
        Command::Tasks | Command::Task { command: None | Some(TaskCommand::List) } => tasks::cmd_list(&ctx)?,
        Command::Task { command: Some(TaskCommand::Add { title, owner, notes }) } => {
            tasks::cmd_add(&ctx, &title, owner.as_deref(), notes.as_deref())?
        }
        Command::Task { command: Some(TaskCommand::Set { id, changes }) } => tasks::cmd_set(&ctx, id, &changes)?,
        Command::Task { command: Some(TaskCommand::Done { id, notes }) } => tasks::cmd_done(&ctx, id, &notes)?,
        Command::Status => commands::cmd_status(&ctx)?,
        Command::Restart { target } => lifecycle::cmd_restart(&mut ctx, &target)?,
        Command::Stop => lifecycle::cmd_stop(&ctx)?,
        Command::Teardown { yes } => lifecycle::cmd_teardown(&ctx, yes)?,
        Command::Role { command } => match command.unwrap_or(RoleCommand::List) {
            RoleCommand::List => roles::cmd_list(&ctx)?,
            RoleCommand::Show { role } => roles::cmd_show(&ctx, &role)?,
            RoleCommand::Add { role, agent, no_worktree, like } => {
                roles::cmd_add(&mut ctx, &role, agent.as_deref(), !no_worktree, like.as_deref())?
            }
            RoleCommand::Edit { role } => roles::cmd_edit(&ctx, &role)?,
            RoleCommand::Set { role, changes } => roles::cmd_set(&mut ctx, &role, &changes)?,
            RoleCommand::Rm { role } => roles::cmd_rm(&mut ctx, &role)?,
            RoleCommand::Save { role } => roles::cmd_save(&mut ctx, role.as_deref())?,
        },
        Command::Config { command } => match command.unwrap_or(ConfigCommand::Show) {
            ConfigCommand::Edit { .. } => commands::cmd_config_edit(&mut ctx)?,
            _ => commands::cmd_config_show(&ctx)?,
        },
        Command::Watch { once } => watch::cmd_watch(&ctx, once)?,
        Command::Version | Command::Doctor => {}
    }
    Ok(true)
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            ui::error(format!("{e:#}"));
            ExitCode::FAILURE
        }
    }
}
