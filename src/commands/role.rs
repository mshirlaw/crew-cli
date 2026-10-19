use crate::crew::config::first_installed;
use crate::crew::context::Ctx;
use crate::crew::{panes, protocol, templates};
use crate::util::{self, ui};
use anyhow::{bail, Result};

pub fn list(ctx: &Ctx) -> Result<()> {
    ctx.require_crew()?;
    println!("{}", ui::bold(&format!("{:<10} {:<8} {:<9} ROLE FILE", "ROLE", "AGENT", "WORKTREE")));
    for role in ctx.roles() {
        let worktree = if ctx.config.role_has_worktree(&role) { "yes" } else { "no" };
        println!("{role:<10} {:<8} {worktree:<9} .crew/roles/{role}.md", ctx.config.role_agent(&role));
    }
    Ok(())
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
