use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "crew",
    version,
    about = "A team of AI coding agents in herdr or tmux, each with a role, its own worktree and a shared message board",
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    #[command(alias = "init", about = "Create .crew/ in this repo (config, roles, board, tasks)")]
    Setup,
    #[command(alias = "up", about = "Set up if needed, start or revive every agent, and open the session")]
    Start {
        #[arg(long, help = "Don't open the session in this terminal afterwards")]
        no_attach: bool,
    },
    #[command(alias = "tell", about = "Post to the board and type the message into the recipients' sessions")]
    Say {
        #[arg(help = "A role, \"all\", or \"human\"")]
        to: String,
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        message: Vec<String>,
    },
    #[command(about = "Print the last lines of the board")]
    Board {
        #[arg(default_value_t = 40)]
        lines: usize,
    },
    #[command(about = "Print the task list")]
    Tasks,
    #[command(about = "Change the shared task list (safe for several agents at once)")]
    Task {
        #[command(subcommand)]
        command: Option<TaskCommand>,
    },
    #[command(alias = "ps", about = "Each agent and what it's doing")]
    Status,
    #[command(about = "Restart agents; they resume from their notes")]
    Restart {
        #[arg(help = "A role, or \"all\"")]
        target: String,
    },
    #[command(alias = "down", about = "Close the session (files and worktrees are kept)")]
    Stop,
    #[command(about = "Stop and remove worktrees that have no uncommitted changes (branches are kept)")]
    Teardown {
        #[arg(long, short, help = "Don't ask for confirmation")]
        yes: bool,
    },
    #[command(alias = "roles", about = "List, add, edit, change, remove or save roles")]
    Role {
        #[command(subcommand)]
        command: Option<RoleCommand>,
    },
    #[command(about = "Show or edit settings")]
    Config {
        #[command(subcommand)]
        command: Option<ConfigCommand>,
    },
    #[command(about = "Check for stalled agents (crew start runs this in the background)")]
    Watch {
        #[arg(long, help = "Check once and exit")]
        once: bool,
    },
    #[command(about = "Check what's installed")]
    Doctor,
    #[command(about = "Print the version")]
    Version,
}

#[derive(Subcommand)]
pub enum TaskCommand {
    #[command(about = "Add a task")]
    Add {
        #[arg(required = true)]
        title: Vec<String>,
        #[arg(long, help = "The role doing it")]
        owner: Option<String>,
        #[arg(long, help = "Acceptance criteria or other notes")]
        notes: Option<String>,
    },
    #[command(about = "Change a task: status=<todo|doing|review|blocked|done> owner=<role> notes=\"...\"")]
    Set {
        id: u32,
        #[arg(required = true)]
        changes: Vec<String>,
    },
    #[command(about = "Mark a task done, optionally with notes")]
    Done {
        id: u32,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        notes: Vec<String>,
    },
    #[command(alias = "ls", about = "Print the task list")]
    List,
}

#[derive(Subcommand)]
pub enum RoleCommand {
    #[command(alias = "ls", about = "List roles")]
    List,
    #[command(about = "Print a role's instructions")]
    Show { role: String },
    #[command(about = "Add a role")]
    Add {
        role: String,
        #[arg(long, help = "The agent that plays it, e.g. claude, codex or gemini")]
        agent: Option<String>,
        #[arg(long, help = "Work in the main checkout instead of a worktree")]
        no_worktree: bool,
        #[arg(long, help = "Copy another role's instructions")]
        like: Option<String>,
    },
    #[command(about = "Edit a role's instructions (a running agent is told to re-read them)")]
    Edit { role: String },
    #[command(about = "Change a role, e.g. agent=codex worktree=no")]
    Set {
        role: String,
        #[arg(required = true)]
        changes: Vec<String>,
    },
    #[command(alias = "remove", about = "Remove a role (its file, worktree and branch are kept)")]
    Rm { role: String },
    #[command(about = "Save a role's instructions, or all roles, as your defaults for new repos")]
    Save { role: Option<String> },
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    #[command(about = "Show the settings in effect for this repo")]
    Show,
    #[command(about = "Edit this repo's .crew/config.toml, or the global config with --global")]
    Edit {
        #[arg(long)]
        global: bool,
    },
    #[command(about = "Create the global config if it doesn't exist (install.sh runs this)")]
    Init,
    #[command(about = "Print a sample config with every setting explained")]
    Example,
}
