use crate::crew::context::Ctx;
use crate::crew::tasks::{self, one_line, STATUSES};
use crate::util::ui;
use anyhow::{bail, Result};

pub fn list(ctx: &Ctx) -> Result<()> {
    ctx.require_crew()?;
    tasks::migrate_old_tasks(ctx)?;
    print!("{}", std::fs::read_to_string(ctx.file("tasks.md")).unwrap_or_default());
    Ok(())
}

pub fn add(ctx: &Ctx, title: &[String], owner: Option<&str>, notes: Option<&str>) -> Result<()> {
    ctx.require_crew()?;
    tasks::migrate_old_tasks(ctx)?;
    let title = one_line(&title.join(" "));
    if title.trim().is_empty() {
        bail!("usage: crew task add \"title\" [--owner role] [--notes \"...\"]");
    }
    let owner = one_line(owner.unwrap_or(""));
    check_owner(ctx, &owner)?;
    let id = tasks::add(ctx, &owner, &title, &one_line(notes.unwrap_or("")))?;
    let for_owner = if owner.is_empty() { String::new() } else { format!(" for {owner}") };
    ui::ok(format!("task {id} added{for_owner}: {title}"));
    Ok(())
}

pub fn set(ctx: &Ctx, id: u32, changes: &[String]) -> Result<()> {
    ctx.require_crew()?;
    tasks::migrate_old_tasks(ctx)?;
    if !tasks::exists(ctx, id) {
        bail!("no task {id}");
    }
    let mut parsed = Vec::new();
    for change in changes {
        let (field, value) = change.split_once('=').unwrap_or((change.as_str(), ""));
        match field {
            "status" if !STATUSES.contains(&value) => bail!("status must be one of: {}", STATUSES.join(" ")),
            "owner" => check_owner(ctx, value)?,
            "status" | "notes" => {}
            _ => bail!("unknown field '{change}' (use status=, owner= or notes=)"),
        }
        parsed.push((field.to_string(), one_line(value)));
    }
    tasks::update(ctx, id, &parsed)?;
    ui::ok(format!("task {id} updated"));
    Ok(())
}

pub fn done(ctx: &Ctx, id: u32, notes: &[String]) -> Result<()> {
    ctx.require_crew()?;
    tasks::migrate_old_tasks(ctx)?;
    if !tasks::exists(ctx, id) {
        bail!("no task {id}");
    }
    let mut changes = vec![("status".to_string(), "done".to_string())];
    let notes = one_line(&notes.join(" "));
    if !notes.is_empty() {
        changes.push(("notes".to_string(), notes));
    }
    tasks::update(ctx, id, &changes)?;
    ui::ok(format!("task {id} done"));
    Ok(())
}

fn check_owner(ctx: &Ctx, owner: &str) -> Result<()> {
    if owner.is_empty() || owner == "human" || ctx.role_exists(owner) {
        Ok(())
    } else {
        bail!("no role '{owner}' (roles: {})", ctx.roles().join(" "))
    }
}
