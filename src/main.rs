mod cli;
mod commands;
mod crew;
mod mux;
mod util;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command, ConfigCommand, RoleCommand, TaskCommand};
use commands::{config, role, say, session, setup, status, task};
use crew::context::Ctx;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            util::ui::error(format!("{e:#}"));
            ExitCode::FAILURE
        }
    }
}

// Ok(false) is a check that failed without an error, e.g. crew doctor finding something missing.
fn run(cli: Cli) -> Result<bool> {
    // These work outside a repo.
    match &cli.command {
        Command::Version => {
            println!("crew {}", env!("CARGO_PKG_VERSION"));
            return Ok(true);
        }
        Command::Doctor => return status::doctor(),
        Command::Config { command: Some(ConfigCommand::Init) } => return config::init().map(|()| true),
        Command::Config { command: Some(ConfigCommand::Example) } => {
            config::example();
            return Ok(true);
        }
        Command::Config { command: Some(ConfigCommand::Edit { global: true }) } => {
            return config::edit_global().map(|()| true);
        }
        _ => {}
    }

    let mut ctx = Ctx::load()?;
    match cli.command {
        Command::Setup => setup::setup(&mut ctx)?,
        Command::Start { no_attach } => session::start(&mut ctx, !no_attach)?,
        Command::Say { to, message } => say::say(&ctx, &to, &message)?,
        Command::Board { lines } => say::show_board(&ctx, lines)?,
        Command::Tasks | Command::Task { command: None | Some(TaskCommand::List) } => task::list(&ctx)?,
        Command::Task { command: Some(TaskCommand::Add { title, owner, notes }) } => {
            task::add(&ctx, &title, owner.as_deref(), notes.as_deref())?;
        }
        Command::Task { command: Some(TaskCommand::Set { id, changes }) } => task::set(&ctx, id, &changes)?,
        Command::Task { command: Some(TaskCommand::Done { id, notes }) } => task::done(&ctx, id, &notes)?,
        Command::Status => status::status(&ctx)?,
        Command::Restart { target } => session::restart(&mut ctx, &target)?,
        Command::Stop => session::stop(&ctx)?,
        Command::Teardown { yes } => session::teardown(&ctx, yes)?,
        Command::Role { command } => match command.unwrap_or(RoleCommand::List) {
            RoleCommand::List => role::list(&ctx)?,
            RoleCommand::Show { role } => role::show(&ctx, &role)?,
            RoleCommand::Add { role, agent, no_worktree, like } => {
                role::add(&mut ctx, &role, agent.as_deref(), !no_worktree, like.as_deref())?;
            }
            RoleCommand::Edit { role } => role::edit(&ctx, &role)?,
            RoleCommand::Set { role, changes } => role::set(&mut ctx, &role, &changes)?,
            RoleCommand::Rm { role } => role::rm(&mut ctx, &role)?,
            RoleCommand::Save { role } => role::save(&mut ctx, role.as_deref())?,
        },
        Command::Config { command } => match command.unwrap_or(ConfigCommand::Show) {
            ConfigCommand::Edit { .. } => config::edit(&mut ctx)?,
            _ => config::show(&ctx)?,
        },
        Command::Watch { once } => commands::watch(&ctx, once)?,
        Command::Version | Command::Doctor => {}
    }
    Ok(true)
}
