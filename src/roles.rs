//! crew role: list, show, add, edit, change, remove and save roles.

use crate::config::util_first_installed;
use crate::context::Ctx;
use crate::{lifecycle, setup, templates, ui, util};
use anyhow::{bail, Result};

/// Succeeds if a role name is lowercase letters, digits and dashes, starting with a letter, 15 chars max.
pub fn valid_role_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && name.len() <= 15
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Prints the roles as a table.
pub fn cmd_list(ctx: &Ctx) -> Result<()> {
    ctx.require_crew()?;
    println!("{}", ui::bold(&format!("{:<10} {:<8} {:<9} {}", "ROLE", "AGENT", "WORKTREE", "ROLE FILE")));
    for role in ctx.roles() {
        let worktree = if ctx.config.role_has_worktree(&role) { "yes" } else { "no" };
        println!("{role:<10} {:<8} {worktree:<9} .crew/roles/{role}.md", ctx.config.role_agent(&role));
    }
    Ok(())
}

/// Prints a role's instructions.
pub fn cmd_show(ctx: &Ctx, role: &str) -> Result<()> {
    require_role(ctx, role)?;
    print!("{}", std::fs::read_to_string(ctx.file(&format!("roles/{role}.md")))?);
    Ok(())
}

/// Adds a role, optionally copying another role's instructions.
pub fn cmd_add(ctx: &mut Ctx, role: &str, agent: Option<&str>, worktree: bool, like: Option<&str>) -> Result<()> {
    ctx.require_crew()?;
    if !valid_role_name(role) {
        bail!("role names are lowercase letters, digits and dashes, starting with a letter, 15 characters max");
    }
    if ctx.role_exists(role) {
        bail!("role '{role}' already exists");
    }
    let agent = agent.map(String::from).unwrap_or_else(|| util_first_installed(&["claude", "codex", "gemini"]));
    ctx.config.set_role(role, Some(&agent), Some(worktree))?;
    ctx.reload()?;
    write_new_role_file(ctx, role, like)?;
    setup::write_protocol(ctx)?;
    ui::ok(format!("added {role} ({agent}, worktree: {worktree})"));
    ui::info(format!("describe it: crew role edit {role}   then bring it online: crew start"));
    Ok(())
}

/// Writes a new role's file, copied from another role if one was given.
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

/// Opens a role's file in your editor, then tells the running agent to re-read it.
pub fn cmd_edit(ctx: &Ctx, role: &str) -> Result<()> {
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

/// Changes a role's settings, e.g. agent=codex worktree=no.
pub fn cmd_set(ctx: &mut Ctx, role: &str, changes: &[String]) -> Result<()> {
    require_role(ctx, role)?;
    if changes.is_empty() {
        bail!("usage: crew role set <role> agent=<x> worktree=<yes|no>");
    }
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
    setup::write_protocol(ctx)?;
    ui::ok(format!("{role} → agent {}, worktree {}", ctx.config.role_agent(role), ctx.config.role_has_worktree(role)));
    ui::info(format!("apply it with: crew restart {role}"));
    Ok(())
}

/// Removes a role and closes its pane. Its file, worktree and branch are kept.
pub fn cmd_rm(ctx: &mut Ctx, role: &str) -> Result<()> {
    require_role(ctx, role)?;
    if ctx.mux.ready(ctx) {
        lifecycle::close_role_pane(ctx, role);
    }
    ctx.config.remove_role(role)?;
    ctx.reload()?;
    setup::write_protocol(ctx)?;
    ui::ok(format!("removed {role} (its role file, worktree and branch are kept)"));
    Ok(())
}

/// Saves a role's instructions, or all of this repo's roles, as your defaults for new repos.
pub fn cmd_save(ctx: &mut Ctx, role: Option<&str>) -> Result<()> {
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

/// Fails unless the crew is set up and has this role.
fn require_role(ctx: &Ctx, role: &str) -> Result<()> {
    ctx.require_crew()?;
    if !ctx.role_exists(role) {
        bail!("no role '{role}' (roles: {})", ctx.roles().join(" "));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::valid_role_name;

    #[test]
    fn role_names() {
        assert!(valid_role_name("builder"));
        assert!(valid_role_name("qa-2"));
        assert!(!valid_role_name("Bad_Name"));
        assert!(!valid_role_name("2fast"));
        assert!(!valid_role_name("a-very-long-role-name"));
    }
}
