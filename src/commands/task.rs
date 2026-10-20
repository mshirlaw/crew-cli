use crate::crew::context::Ctx;
use crate::crew::tasks::{self, one_line, STATUSES};
use crate::util::ui;
use anyhow::{bail, Result};

pub fn list(ctx: &Ctx) -> Result<()> {
    ctx.require_crew()?;
    print!("{}", tasks::render(&tasks::load(ctx)));
    Ok(())
}

pub fn add(ctx: &Ctx, title: &[String], owner: Option<&str>, notes: Option<&str>) -> Result<()> {
    ctx.require_crew()?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::testing::{crew_ctx, ctx_in, scratch_dir};

    fn words(text: &str) -> Vec<String> {
        text.split(' ').map(String::from).collect()
    }

    fn error(result: Result<()>) -> String {
        result.unwrap_err().to_string()
    }

    #[test]
    fn add_joins_the_title_words_and_records_the_owner() {
        let ctx = crew_ctx();
        add(&ctx, &words("write the docs"), Some("builder"), Some("by friday")).unwrap();
        let tasks = tasks::load(&ctx);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "write the docs");
        assert_eq!(tasks[0].owner, "builder");
        assert_eq!(tasks[0].notes, "by friday");
        assert_eq!(tasks[0].status, "todo");
    }

    #[test]
    fn add_flattens_newlines_and_tabs() {
        let ctx = crew_ctx();
        add(&ctx, &["a\tb\nc".to_string()], None, Some("x\ny")).unwrap();
        let task = &tasks::load(&ctx)[0];
        assert_eq!((task.title.as_str(), task.notes.as_str()), ("a b c", "x y"));
    }

    #[test]
    fn add_rejects_a_blank_title() {
        let ctx = crew_ctx();
        assert!(error(add(&ctx, &words("  "), None, None)).starts_with("usage:"));
        assert!(tasks::load(&ctx).is_empty());
    }

    #[test]
    fn add_rejects_an_unknown_owner_but_accepts_the_human() {
        let ctx = crew_ctx();
        let message = error(add(&ctx, &words("x"), Some("ghost"), None));
        assert!(message.contains("no role 'ghost'") && message.contains("lead builder"), "{message}");
        add(&ctx, &words("x"), Some("human"), None).unwrap();
        assert_eq!(tasks::load(&ctx)[0].owner, "human");
    }

    #[test]
    fn commands_need_a_crew() {
        let ctx = ctx_in(&scratch_dir(), None);
        assert!(error(add(&ctx, &words("x"), None, None)).contains("crew setup"));
        assert!(error(set(&ctx, 1, &words("status=done"))).contains("crew setup"));
        assert!(error(done(&ctx, 1, &[])).contains("crew setup"));
        assert!(error(list(&ctx)).contains("crew setup"));
    }

    #[test]
    fn set_changes_several_fields_at_once() {
        let ctx = crew_ctx();
        add(&ctx, &words("x"), None, None).unwrap();
        set(&ctx, 1, &["status=doing".into(), "owner=lead".into(), "notes=half done".into()]).unwrap();
        let task = &tasks::load(&ctx)[0];
        assert_eq!((task.status.as_str(), task.owner.as_str(), task.notes.as_str()), ("doing", "lead", "half done"));
    }

    #[test]
    fn set_can_clear_the_owner_and_keeps_equals_signs_in_notes() {
        let ctx = crew_ctx();
        add(&ctx, &words("x"), Some("lead"), None).unwrap();
        set(&ctx, 1, &["owner=".into(), "notes=a=b".into()]).unwrap();
        let task = &tasks::load(&ctx)[0];
        assert_eq!((task.owner.as_str(), task.notes.as_str()), ("", "a=b"));
    }

    #[test]
    fn set_rejects_bad_input_without_changing_anything() {
        let ctx = crew_ctx();
        add(&ctx, &words("x"), None, None).unwrap();
        assert!(error(set(&ctx, 9, &words("status=done"))).contains("no task 9"));
        assert!(error(set(&ctx, 1, &words("status=finished"))).contains("status must be one of"));
        assert!(error(set(&ctx, 1, &words("title=new"))).contains("unknown field"));
        assert!(error(set(&ctx, 1, &words("status"))).contains("status must be one of"));
        assert!(error(set(&ctx, 1, &words("owner=ghost"))).contains("no role 'ghost'"));
        assert!(error(set(&ctx, 1, &words("priority"))).contains("unknown field 'priority'"));
        assert_eq!(tasks::load(&ctx)[0].status, "todo");
    }

    #[test]
    fn a_bad_change_stops_the_whole_set() {
        let ctx = crew_ctx();
        add(&ctx, &words("x"), None, None).unwrap();
        assert!(set(&ctx, 1, &words("status=doing bogus=1")).is_err());
        assert_eq!(tasks::load(&ctx)[0].status, "todo");
    }

    #[test]
    fn done_marks_the_task_and_optionally_adds_notes() {
        let ctx = crew_ctx();
        add(&ctx, &words("x"), None, Some("old")).unwrap();
        add(&ctx, &words("y"), None, Some("old")).unwrap();
        done(&ctx, 1, &[]).unwrap();
        done(&ctx, 2, &words("merged in abc123")).unwrap();
        let tasks = tasks::load(&ctx);
        assert_eq!((tasks[0].status.as_str(), tasks[0].notes.as_str()), ("done", "old"));
        assert_eq!((tasks[1].status.as_str(), tasks[1].notes.as_str()), ("done", "merged in abc123"));
    }

    #[test]
    fn done_rejects_an_unknown_task() {
        let ctx = crew_ctx();
        assert!(error(done(&ctx, 3, &[])).contains("no task 3"));
    }

    #[test]
    fn every_change_regenerates_tasks_md() {
        let ctx = crew_ctx();
        add(&ctx, &words("ship it"), Some("lead"), None).unwrap();
        let text = std::fs::read_to_string(ctx.file("tasks.md")).unwrap();
        assert!(text.contains("ship it"), "{text}");
        done(&ctx, 1, &[]).unwrap();
        assert!(std::fs::read_to_string(ctx.file("tasks.md")).unwrap().contains("done"));
    }
}
