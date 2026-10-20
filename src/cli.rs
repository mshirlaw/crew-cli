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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Command {
        let mut full = vec!["crew"];
        full.extend_from_slice(args);
        Cli::try_parse_from(full).unwrap().command
    }

    fn rejected(args: &[&str]) -> ErrorKind {
        let mut full = vec!["crew"];
        full.extend_from_slice(args);
        Cli::try_parse_from(full).err().unwrap().kind()
    }

    #[test]
    fn the_definition_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn no_arguments_shows_help() {
        assert_eq!(rejected(&[]), ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand);
    }

    #[test]
    fn unknown_commands_are_rejected() {
        assert_eq!(rejected(&["frobnicate"]), ErrorKind::InvalidSubcommand);
    }

    #[test]
    fn aliases_reach_the_same_commands() {
        assert!(matches!(parse(&["init"]), Command::Setup));
        assert!(matches!(parse(&["up"]), Command::Start { .. }));
        assert!(matches!(parse(&["down"]), Command::Stop));
        assert!(matches!(parse(&["ps"]), Command::Status));
        assert!(matches!(parse(&["tell", "lead", "hi"]), Command::Say { .. }));
        assert!(matches!(parse(&["task", "ls"]), Command::Task { command: Some(TaskCommand::List) }));
        assert!(matches!(parse(&["role", "ls"]), Command::Role { command: Some(RoleCommand::List) }));
        assert!(matches!(parse(&["role", "remove", "x"]), Command::Role { command: Some(RoleCommand::Rm { .. }) }));
    }

    #[test]
    fn start_attaches_unless_told_not_to() {
        assert!(matches!(parse(&["start"]), Command::Start { no_attach: false }));
        assert!(matches!(parse(&["start", "--no-attach"]), Command::Start { no_attach: true }));
    }

    #[test]
    fn say_takes_a_target_and_the_rest_as_the_message() {
        let Command::Say { to, message } = parse(&["say", "builder", "please", "build", "it"]) else { panic!() };
        assert_eq!(to, "builder");
        assert_eq!(message, ["please", "build", "it"]);
    }

    #[test]
    fn say_messages_may_contain_dashes() {
        let Command::Say { message, .. } = parse(&["say", "lead", "--fix", "-x", "now"]) else { panic!() };
        assert_eq!(message, ["--fix", "-x", "now"]);
    }

    #[test]
    fn say_needs_a_message() {
        assert_eq!(rejected(&["say", "lead"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(rejected(&["say"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn board_shows_forty_lines_by_default() {
        assert!(matches!(parse(&["board"]), Command::Board { lines: 40 }));
        assert!(matches!(parse(&["board", "5"]), Command::Board { lines: 5 }));
        assert_eq!(rejected(&["board", "many"]), ErrorKind::ValueValidation);
    }

    #[test]
    fn task_add_takes_a_title_and_options() {
        let Command::Task { command: Some(TaskCommand::Add { title, owner, notes }) } =
            parse(&["task", "add", "write", "docs", "--owner", "builder", "--notes", "soon"])
        else {
            panic!()
        };
        assert_eq!(title, ["write", "docs"]);
        assert_eq!(owner.as_deref(), Some("builder"));
        assert_eq!(notes.as_deref(), Some("soon"));
    }

    #[test]
    fn task_add_needs_a_title() {
        assert_eq!(rejected(&["task", "add"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn task_set_takes_a_numeric_id_and_changes() {
        let Command::Task { command: Some(TaskCommand::Set { id, changes }) } =
            parse(&["task", "set", "3", "status=doing", "owner=lead"])
        else {
            panic!()
        };
        assert_eq!(id, 3);
        assert_eq!(changes, ["status=doing", "owner=lead"]);
        assert_eq!(rejected(&["task", "set", "three", "status=done"]), ErrorKind::ValueValidation);
        assert_eq!(rejected(&["task", "set", "3"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn task_done_notes_are_optional_and_may_contain_dashes() {
        let Command::Task { command: Some(TaskCommand::Done { id, notes }) } = parse(&["task", "done", "2"]) else {
            panic!()
        };
        assert_eq!((id, notes.len()), (2, 0));
        let Command::Task { command: Some(TaskCommand::Done { notes, .. }) } =
            parse(&["task", "done", "2", "merged", "--squash"])
        else {
            panic!()
        };
        assert_eq!(notes, ["merged", "--squash"]);
    }

    #[test]
    fn task_and_tasks_without_a_subcommand_are_allowed() {
        assert!(matches!(parse(&["tasks"]), Command::Tasks));
        assert!(matches!(parse(&["task"]), Command::Task { command: None }));
    }

    #[test]
    fn role_add_takes_its_options() {
        let Command::Role { command: Some(RoleCommand::Add { role, agent, no_worktree, like }) } =
            parse(&["role", "add", "qa", "--agent", "codex", "--no-worktree", "--like", "builder"])
        else {
            panic!()
        };
        assert_eq!(
            (role.as_str(), agent.as_deref(), no_worktree, like.as_deref()),
            ("qa", Some("codex"), true, Some("builder"))
        );
    }

    #[test]
    fn role_add_defaults() {
        let Command::Role { command: Some(RoleCommand::Add { agent, no_worktree, like, .. }) } =
            parse(&["role", "add", "qa"])
        else {
            panic!()
        };
        assert_eq!((agent, no_worktree, like), (None, false, None));
    }

    #[test]
    fn role_set_and_save() {
        let Command::Role { command: Some(RoleCommand::Set { role, changes }) } =
            parse(&["role", "set", "qa", "agent=gemini", "worktree=no"])
        else {
            panic!()
        };
        assert_eq!((role.as_str(), changes.len()), ("qa", 2));
        assert!(matches!(parse(&["role", "save"]), Command::Role { command: Some(RoleCommand::Save { role: None }) }));
        assert!(matches!(
            parse(&["role", "save", "qa"]),
            Command::Role { command: Some(RoleCommand::Save { role: Some(_) }) }
        ));
        assert_eq!(rejected(&["role", "set", "qa"]), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn config_subcommands() {
        assert!(matches!(parse(&["config"]), Command::Config { command: None }));
        assert!(matches!(parse(&["config", "show"]), Command::Config { command: Some(ConfigCommand::Show) }));
        assert!(matches!(parse(&["config", "init"]), Command::Config { command: Some(ConfigCommand::Init) }));
        assert!(matches!(parse(&["config", "example"]), Command::Config { command: Some(ConfigCommand::Example) }));
        assert!(matches!(
            parse(&["config", "edit"]),
            Command::Config { command: Some(ConfigCommand::Edit { global: false }) }
        ));
        assert!(matches!(
            parse(&["config", "edit", "--global"]),
            Command::Config { command: Some(ConfigCommand::Edit { global: true }) }
        ));
    }

    #[test]
    fn restart_and_teardown_arguments() {
        let Command::Restart { target } = parse(&["restart", "all"]) else { panic!() };
        assert_eq!(target, "all");
        assert_eq!(rejected(&["restart"]), ErrorKind::MissingRequiredArgument);
        assert!(matches!(parse(&["teardown"]), Command::Teardown { yes: false }));
        assert!(matches!(parse(&["teardown", "-y"]), Command::Teardown { yes: true }));
        assert!(matches!(parse(&["teardown", "--yes"]), Command::Teardown { yes: true }));
    }

    #[test]
    fn watch_can_check_once() {
        assert!(matches!(parse(&["watch"]), Command::Watch { once: false }));
        assert!(matches!(parse(&["watch", "--once"]), Command::Watch { once: true }));
    }

    #[test]
    fn version_is_available_as_a_command_and_a_flag() {
        assert!(matches!(parse(&["version"]), Command::Version));
        assert_eq!(rejected(&["--version"]), ErrorKind::DisplayVersion);
    }
}
