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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crew::testutil::{fixture, Fixture, ROLES};
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-C"])
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn repo_fixture() -> Fixture {
        let f = fixture(ROLES);
        git(&f.ctx.root, &["init", "-q", "-b", "main"]);
        git(&f.ctx.root, &["commit", "-q", "--allow-empty", "-m", "start"]);
        f
    }

    #[test]
    fn roles_without_a_worktree_are_left_alone() {
        let f = repo_fixture();
        ensure(&f.ctx, "lead").unwrap();
        assert!(!f.ctx.worktree_base().exists());
        remove(&f.ctx, "lead");
    }

    #[test]
    fn creates_a_worktree_on_a_new_branch_from_the_base() {
        let f = repo_fixture();
        ensure(&f.ctx, "builder").unwrap();
        let dir = f.ctx.role_dir("builder");
        assert!(dir.is_dir());
        assert_eq!(git(&dir, &["rev-parse", "--abbrev-ref", "HEAD"]), "crew/builder");
        assert_eq!(git(&dir, &["rev-parse", "HEAD"]), git(&f.ctx.root, &["rev-parse", "main"]));
    }

    #[test]
    fn ensure_is_idempotent() {
        let f = repo_fixture();
        ensure(&f.ctx, "builder").unwrap();
        std::fs::write(f.ctx.role_dir("builder").join("work.txt"), "x").unwrap();
        ensure(&f.ctx, "builder").unwrap();
        assert!(f.ctx.role_dir("builder").join("work.txt").exists());
    }

    #[test]
    fn a_recreated_worktree_keeps_the_roles_branch() {
        let f = repo_fixture();
        ensure(&f.ctx, "builder").unwrap();
        let dir = f.ctx.role_dir("builder");
        git(&dir, &["commit", "-q", "--allow-empty", "-m", "builder work"]);
        let tip = git(&dir, &["rev-parse", "HEAD"]);
        remove(&f.ctx, "builder");
        assert!(!dir.exists());
        ensure(&f.ctx, "builder").unwrap();
        assert_eq!(git(&dir, &["rev-parse", "HEAD"]), tip);
    }

    #[test]
    fn a_deleted_directory_is_pruned_and_recreated() {
        let f = repo_fixture();
        ensure(&f.ctx, "builder").unwrap();
        std::fs::remove_dir_all(f.ctx.role_dir("builder")).unwrap();
        ensure(&f.ctx, "builder").unwrap();
        assert!(f.ctx.role_dir("builder").join(".git").exists());
    }

    #[test]
    fn a_missing_base_branch_is_an_error_naming_the_role() {
        let f = fixture(&format!("base_branch = \"nope\"\n{ROLES}"));
        git(&f.ctx.root, &["init", "-q", "-b", "main"]);
        git(&f.ctx.root, &["commit", "-q", "--allow-empty", "-m", "start"]);
        let err = ensure(&f.ctx, "builder").unwrap_err().to_string();
        assert!(err.contains("could not create a worktree for builder"), "{err}");
    }

    #[test]
    fn remove_deletes_a_clean_worktree_but_keeps_the_branch() {
        let f = repo_fixture();
        ensure(&f.ctx, "builder").unwrap();
        remove(&f.ctx, "builder");
        assert!(!f.ctx.role_dir("builder").exists());
        assert_eq!(git(&f.ctx.root, &["branch", "--list", "crew/builder"]), "crew/builder");
    }

    #[test]
    fn remove_never_forces_a_dirty_worktree() {
        let f = repo_fixture();
        ensure(&f.ctx, "builder").unwrap();
        let dir = f.ctx.role_dir("builder");
        std::fs::write(dir.join("wip.txt"), "x").unwrap();
        git(&dir, &["add", "wip.txt"]);
        remove(&f.ctx, "builder");
        assert!(dir.join("wip.txt").exists());
    }

    #[test]
    fn removing_a_worktree_that_does_not_exist_is_fine() {
        let f = repo_fixture();
        remove(&f.ctx, "builder");
    }
}
