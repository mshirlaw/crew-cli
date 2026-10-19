use super::context::Ctx;
use crate::util::{run_all, run_ok, tilde, ui};
use anyhow::{bail, Result};
use std::path::Path;

pub fn ensure(ctx: &Ctx, role: &str) -> Result<()> {
    if !ctx.config.role_has_worktree(role) {
        return Ok(());
    }
    let dir = ctx.role_dir(role);
    if dir.is_dir() && run_ok("git", &["-C", &dir.display().to_string(), "rev-parse", "--git-dir"]) {
        return Ok(());
    }
    create(ctx, role, &dir)
}

// Reuses crew/<role> if the branch already exists, so a recreated worktree keeps its work.
fn create(ctx: &Ctx, role: &str, dir: &Path) -> Result<()> {
    let root = ctx.root.display().to_string();
    let dir_text = dir.display().to_string();
    let branch = format!("crew/{role}");
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    run_ok("git", &["-C", &root, "worktree", "prune"]);
    let branch_exists =
        run_ok("git", &["-C", &root, "show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")]);
    let (success, out) = if branch_exists {
        run_all("git", &["-C", &root, "worktree", "add", "-q", &dir_text, &branch])
    } else {
        run_all("git", &["-C", &root, "worktree", "add", "-q", "-b", &branch, &dir_text, &ctx.base_branch()])
    };
    if !success {
        bail!("could not create a worktree for {role}: {}", out.trim());
    }
    ui::ok(format!("worktree {branch} → {}", tilde(dir)));
    Ok(())
}

// Never forces: a worktree with uncommitted changes is kept.
pub fn remove(ctx: &Ctx, role: &str) {
    if !ctx.config.role_has_worktree(role) {
        return;
    }
    let dir = ctx.role_dir(role);
    if !dir.is_dir() {
        return;
    }
    let dir_text = dir.display().to_string();
    if run_ok("git", &["-C", &ctx.root.display().to_string(), "worktree", "remove", &dir_text]) {
        ui::ok(format!("removed worktree {}", tilde(&dir)));
    } else {
        ui::warn(format!(
            "kept {dir_text}: it has uncommitted changes (commit them, or: git worktree remove --force {dir_text})"
        ));
    }
}
