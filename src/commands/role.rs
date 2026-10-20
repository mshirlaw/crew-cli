use crate::crew::config::first_installed;
use crate::crew::context::Ctx;
use crate::crew::{panes, protocol, templates};
use crate::util::{self, ui};
use anyhow::{bail, Result};
use std::fmt::Write;

pub fn list(ctx: &Ctx) -> Result<()> {
    print!("{}", list_text(ctx)?);
    Ok(())
}

pub(super) fn list_text(ctx: &Ctx) -> Result<String> {
    ctx.require_crew()?;
    let mut text = format!("{}\n", ui::bold(&format!("{:<10} {:<8} {:<9} ROLE FILE", "ROLE", "AGENT", "WORKTREE")));
    for role in ctx.roles() {
        let worktree = if ctx.config.role_has_worktree(&role) { "yes" } else { "no" };
        let _ = writeln!(text, "{role:<10} {:<8} {worktree:<9} .crew/roles/{role}.md", ctx.config.role_agent(&role));
    }
    Ok(text)
}

pub fn show(ctx: &Ctx, role: &str) -> Result<()> {
    require_role(ctx, role)?;
    print!("{}", std::fs::read_to_string(ctx.file(&format!("roles/{role}.md")))?);
    Ok(())
}

pub fn add(ctx: &mut Ctx, role: &str, agent: Option<&str>, worktree: bool, like: Option<&str>) -> Result<()> {
    ctx.require_crew()?;
    if !valid_role_name(role) {
        bail!("role names are lowercase letters, digits and dashes, starting with a letter, 15 characters max");
    }
    if ctx.role_exists(role) {
        bail!("role '{role}' already exists");
    }
    let agent = agent.map_or_else(|| first_installed(&["claude", "codex", "gemini"]), String::from);
    ctx.config.set_role(role, Some(&agent), Some(worktree))?;
    ctx.reload()?;
    write_new_role_file(ctx, role, like)?;
    protocol::write(ctx)?;
    ui::ok(format!("added {role} ({agent}, worktree: {worktree})"));
    ui::info(format!("describe it: crew role edit {role}   then bring it online: crew start"));
    Ok(())
}

fn write_new_role_file(ctx: &Ctx, role: &str, like: Option<&str>) -> Result<()> {
    let source = like.and_then(|other| std::fs::read_to_string(ctx.file(&format!("roles/{other}.md"))).ok());
    let text = match source {
        Some(text) => {
            text.lines()
                .map(|line| if line.starts_with("# Role: ") { format!("# Role: {role}") } else { line.to_string() })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"
        }
        None => templates::role_template(role),
    };
    std::fs::create_dir_all(ctx.file("roles"))?;
    std::fs::write(ctx.file(&format!("roles/{role}.md")), text)?;
    Ok(())
}

// After editing, a running agent is told to re-read its role so the change takes effect now.
pub fn edit(ctx: &Ctx, role: &str) -> Result<()> {
    require_role(ctx, role)?;
    let path = ctx.file(&format!("roles/{role}.md"));
    util::edit_file(&path)?;
    if ctx.mux.ready(ctx) && ctx.mux.agent_running(ctx, role) {
        let message =
            format!("[crew] Your role file was updated. Re-read {} and follow the new version.", path.display());
        if ctx.mux.deliver(ctx, role, &message) {
            ui::ok(format!("told {role} to re-read its role"));
        }
    }
    Ok(())
}

pub fn set(ctx: &mut Ctx, role: &str, changes: &[String]) -> Result<()> {
    require_role(ctx, role)?;
    let mut agent = None;
    let mut worktree = None;
    for change in changes {
        match change.split_once('=') {
            Some(("agent", value)) if !value.is_empty() => agent = Some(value.to_string()),
            Some(("worktree", value)) => worktree = Some(matches!(value, "yes" | "true" | "on" | "1")),
            _ => bail!("unknown setting '{change}' (use agent= or worktree=)"),
        }
    }
    ctx.config.set_role(role, agent.as_deref(), worktree)?;
    ctx.reload()?;
    protocol::write(ctx)?;
    ui::ok(format!("{role} → agent {}, worktree {}", ctx.config.role_agent(role), ctx.config.role_has_worktree(role)));
    ui::info(format!("apply it with: crew restart {role}"));
    Ok(())
}

// The role's file, worktree and branch are kept.
pub fn rm(ctx: &mut Ctx, role: &str) -> Result<()> {
    require_role(ctx, role)?;
    if ctx.mux.ready(ctx) {
        panes::close(ctx, role);
    }
    ctx.config.remove_role(role)?;
    ctx.reload()?;
    protocol::write(ctx)?;
    ui::ok(format!("removed {role} (its role file, worktree and branch are kept)"));
    Ok(())
}

// With a role, saves that role's instructions; without one, saves all of this repo's roles.
pub fn save(ctx: &mut Ctx, role: Option<&str>) -> Result<()> {
    ctx.require_crew()?;
    let Some(role) = role else {
        ctx.config.save_roles_to_global()?;
        ui::ok(format!(
            "saved this repo's roles as your default for new repos ({})",
            util::tilde(&ctx.config.global_path)
        ));
        return Ok(());
    };
    let source = ctx.file(&format!("roles/{role}.md"));
    if !source.is_file() {
        bail!("no role '{role}'");
    }
    let dir = util::crew_home().join("roles");
    std::fs::create_dir_all(&dir)?;
    std::fs::copy(&source, dir.join(format!("{role}.md")))?;
    ui::ok(format!("saved {role}'s instructions as your template for new repos"));
    Ok(())
}

fn require_role(ctx: &Ctx, role: &str) -> Result<()> {
    ctx.require_crew()?;
    if !ctx.role_exists(role) {
        bail!("no role '{role}' (roles: {})", ctx.roles().join(" "));
    }
    Ok(())
}

// Role names end up in TOML keys, branch names and herdr agent names.
fn valid_role_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && name.len() <= 15
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::testing::{crew_ctx, crew_home, ctx_in, scratch_dir};

    fn error(result: Result<()>) -> String {
        result.unwrap_err().to_string()
    }

    fn role_file(ctx: &Ctx, role: &str) -> String {
        std::fs::read_to_string(ctx.file(&format!("roles/{role}.md"))).unwrap()
    }

    #[test]
    fn add_registers_the_role_and_writes_its_file_and_the_protocol() {
        let mut ctx = crew_ctx();
        add(&mut ctx, "docs", Some("gemini"), true, None).unwrap();
        assert!(ctx.role_exists("docs"));
        assert_eq!(ctx.config.role_agent("docs"), "gemini");
        assert!(ctx.config.role_has_worktree("docs"));
        assert!(role_file(&ctx, "docs").starts_with("# Role: docs"));
        assert!(std::fs::read_to_string(ctx.file("PROTOCOL.md")).unwrap().contains("| docs | gemini | crew/docs |"));
    }

    #[test]
    fn add_without_a_worktree_works_in_the_main_checkout() {
        let mut ctx = crew_ctx();
        add(&mut ctx, "scribe", Some("codex"), false, None).unwrap();
        assert!(!ctx.config.role_has_worktree("scribe"));
        assert_eq!(ctx.role_dir("scribe"), ctx.root);
    }

    #[test]
    fn add_rejects_bad_and_duplicate_names() {
        let mut ctx = crew_ctx();
        assert!(error(add(&mut ctx, "Bad_Name", None, true, None)).contains("lowercase letters"));
        assert!(error(add(&mut ctx, "builder", None, true, None)).contains("already exists"));
        assert_eq!(ctx.roles(), ["lead", "builder"]);
    }

    #[test]
    fn add_like_copies_the_instructions_under_the_new_name() {
        let mut ctx = crew_ctx();
        std::fs::create_dir_all(ctx.file("roles")).unwrap();
        std::fs::write(ctx.file("roles/builder.md"), "# Role: builder\n\nBuild things.\n# Role: not a header\n")
            .unwrap();
        add(&mut ctx, "builder2", Some("codex"), true, Some("builder")).unwrap();
        assert_eq!(role_file(&ctx, "builder2"), "# Role: builder2\n\nBuild things.\n# Role: builder2\n");
    }

    #[test]
    fn add_like_a_missing_role_falls_back_to_the_template() {
        let mut ctx = crew_ctx();
        add(&mut ctx, "extra", Some("codex"), true, Some("nobody")).unwrap();
        assert!(role_file(&ctx, "extra").starts_with("# Role: extra"));
    }

    #[test]
    fn commands_need_a_crew() {
        let mut ctx = ctx_in(&scratch_dir(), None);
        assert!(error(list(&ctx)).contains("crew setup"));
        assert!(error(show(&ctx, "lead")).contains("crew setup"));
        assert!(error(add(&mut ctx, "docs", None, true, None)).contains("crew setup"));
        assert!(error(set(&mut ctx, "lead", &[])).contains("crew setup"));
        assert!(error(rm(&mut ctx, "lead")).contains("crew setup"));
        assert!(error(save(&mut ctx, None)).contains("crew setup"));
    }

    #[test]
    fn list_has_one_row_per_role_with_its_agent_and_worktree() {
        let mut ctx = crew_ctx();
        add(&mut ctx, "docs", Some("gemini"), false, None).unwrap();
        let text = list_text(&ctx).unwrap();
        let rows: Vec<&str> = text.lines().skip(1).collect();
        assert_eq!(
            rows,
            [
                "lead       claude   no        .crew/roles/lead.md",
                "builder    codex    yes       .crew/roles/builder.md",
                "docs       gemini   no        .crew/roles/docs.md",
            ]
        );
        assert!(text.lines().next().unwrap().contains("ROLE FILE"));
    }

    #[test]
    fn show_rejects_unknown_roles() {
        let mut ctx = crew_ctx();
        add(&mut ctx, "docs", Some("codex"), true, None).unwrap();
        let message = error(show(&ctx, "ghost"));
        assert!(message.contains("no role 'ghost'") && message.contains("lead builder docs"), "{message}");
    }

    #[test]
    fn set_changes_the_agent_and_worktree() {
        let mut ctx = crew_ctx();
        set(&mut ctx, "builder", &["agent=gemini".into(), "worktree=no".into()]).unwrap();
        assert_eq!(ctx.config.role_agent("builder"), "gemini");
        assert!(!ctx.config.role_has_worktree("builder"));
        set(&mut ctx, "builder", &["worktree=yes".into()]).unwrap();
        assert!(ctx.config.role_has_worktree("builder"));
        assert_eq!(ctx.config.role_agent("builder"), "gemini");
    }

    #[test]
    fn set_reads_the_usual_spellings_of_yes() {
        let mut ctx = crew_ctx();
        for yes in ["yes", "true", "on", "1"] {
            set(&mut ctx, "lead", &["worktree=no".into()]).unwrap();
            set(&mut ctx, "lead", &[format!("worktree={yes}")]).unwrap();
            assert!(ctx.config.role_has_worktree("lead"), "{yes}");
        }
        set(&mut ctx, "lead", &["worktree=maybe".into()]).unwrap();
        assert!(!ctx.config.role_has_worktree("lead"));
    }

    #[test]
    fn set_rejects_unknown_settings_and_roles_without_saving() {
        let mut ctx = crew_ctx();
        assert!(error(set(&mut ctx, "builder", &["colour=red".into()])).contains("unknown setting 'colour=red'"));
        assert!(error(set(&mut ctx, "builder", &["agent=".into()])).contains("unknown setting"));
        assert!(error(set(&mut ctx, "builder", &["agent".into()])).contains("unknown setting"));
        assert!(error(set(&mut ctx, "builder", &["agent=gemini".into(), "x=1".into()])).contains("unknown setting"));
        assert!(error(set(&mut ctx, "ghost", &["agent=codex".into()])).contains("no role 'ghost'"));
        assert_eq!(ctx.config.role_agent("builder"), "codex");
    }

    #[test]
    fn rm_drops_the_role_but_keeps_its_file() {
        let mut ctx = crew_ctx();
        add(&mut ctx, "docs", Some("codex"), true, None).unwrap();
        rm(&mut ctx, "docs").unwrap();
        assert!(!ctx.role_exists("docs"));
        assert!(ctx.file("roles/docs.md").is_file());
        assert!(!std::fs::read_to_string(ctx.file("PROTOCOL.md")).unwrap().contains("| docs |"));
        assert!(error(rm(&mut ctx, "docs")).contains("no role 'docs'"));
    }

    #[test]
    fn save_one_role_copies_its_instructions_to_the_global_templates() {
        let mut ctx = crew_ctx();
        add(&mut ctx, "savedrole", Some("codex"), true, None).unwrap();
        save(&mut ctx, Some("savedrole")).unwrap();
        let saved = crew_home().join("roles/savedrole.md");
        assert_eq!(std::fs::read_to_string(saved).unwrap(), role_file(&ctx, "savedrole"));
    }

    #[test]
    fn save_of_a_missing_role_fails() {
        let mut ctx = crew_ctx();
        assert!(error(save(&mut ctx, Some("ghost"))).contains("no role 'ghost'"));
    }

    #[test]
    fn role_names() {
        assert!(valid_role_name("builder"));
        assert!(valid_role_name("qa-2"));
        assert!(!valid_role_name("Bad_Name"));
        assert!(!valid_role_name("2fast"));
        assert!(!valid_role_name("a-very-long-role-name"));
        assert!(!valid_role_name(""));
        assert!(!valid_role_name("-lead"));
        assert!(!valid_role_name("Lead"));
        assert!(valid_role_name("a"));
        assert!(valid_role_name("abcdefghijklmno"));
        assert!(!valid_role_name("abcdefghijklmnop"));
    }
}
