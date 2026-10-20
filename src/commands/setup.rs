use crate::crew::config::first_installed;
use crate::crew::context::Ctx;
use crate::crew::{protocol, tasks, templates};
use crate::util::{self, ui};
use anyhow::{bail, Result};
use std::path::Path;

pub fn setup(ctx: &mut Ctx) -> Result<()> {
    if ctx.require_crew().is_ok() {
        ui::info(format!("crew already set up in {}", ctx.crew.display()));
        return protocol::write(ctx);
    }
    create(ctx)?;
    ui::ok(format!("crew set up in {} (multiplexer: {})", ctx.crew.display(), ctx.mux.name()));
    println!();
    super::role::list(ctx)?;
    println!(
        "\n{} crew start   {}",
        ui::bold("Next:"),
        ui::dim("(customise first with crew role edit / crew role add / crew config edit)")
    );
    Ok(())
}

pub fn create(ctx: &mut Ctx) -> Result<()> {
    if !util::run_ok("git", &["-C", &ctx.root.display().to_string(), "rev-parse", "--verify", "-q", "HEAD"]) {
        bail!("this repo has no commits yet. Make one first.");
    }
    std::fs::create_dir_all(ctx.file("roles"))?;
    std::fs::create_dir_all(ctx.file("notes"))?;
    std::fs::create_dir_all(ctx.state.dir())?;
    write_repo_config(ctx)?;
    ctx.reload()?;
    write_role_files(ctx)?;
    if !ctx.file("board.md").exists() {
        std::fs::write(ctx.file("board.md"), "# Crew board\n\n")?;
    }
    tasks::init(ctx)?;
    protocol::write(ctx)?;
    hide_from_git(ctx)
}

// Roles come from your global config if you've saved some, otherwise lead/builder/reviewer
// using whichever agents are installed.
fn write_repo_config(ctx: &Ctx) -> Result<()> {
    let root = ctx.root.display().to_string();
    let base = util::run_out("git", &["-C", &root, "symbolic-ref", "--short", "HEAD"])
        .or_else(|| util::run_out("git", &["-C", &root, "rev-parse", "--short", "HEAD"]))
        .map_or_else(|| "main".to_string(), |s| s.trim().to_string());
    let roles = if ctx.config.global_has_roles() {
        format!("\n{}", ctx.config.global_roles_text())
    } else {
        if !["claude", "codex", "gemini"].iter().any(|a| util::has(a)) {
            ui::warn("none of claude, codex or gemini found. Edit .crew/config.toml");
        }
        templates::default_roles(
            &first_installed(&["claude", "codex", "gemini"]),
            &first_installed(&["codex", "claude", "gemini"]),
            &first_installed(&["gemini", "claude", "codex"]),
        )
    };
    std::fs::write(ctx.file("config.toml"), templates::repo_config_header(&base) + &roles)?;
    Ok(())
}

fn write_role_files(ctx: &Ctx) -> Result<()> {
    for role in ctx.roles() {
        let path = ctx.file(&format!("roles/{role}.md"));
        if !path.exists() {
            std::fs::write(path, templates::role_template(&role))?;
        }
    }
    Ok(())
}

// .git/info/exclude rather than .gitignore, so crew never changes tracked files.
fn hide_from_git(ctx: &Ctx) -> Result<()> {
    let root = ctx.root.display().to_string();
    let Some(path) = util::run_out("git", &["-C", &root, "rev-parse", "--git-path", "info/exclude"]) else {
        return Ok(());
    };
    let path = Path::new(path.trim());
    let path = if path.is_absolute() { path.to_path_buf() } else { ctx.root.join(path) };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    if !text.lines().any(|line| line.trim() == ".crew/") {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let separator = if text.is_empty() || text.ends_with('\n') { "" } else { "\n" };
        std::fs::write(&path, format!("{text}{separator}.crew/\n"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::testing::{ctx_in, scratch_dir};
    use std::path::PathBuf;

    fn git(root: &Path, args: &[&str]) {
        let mut full = vec!["-C", root.to_str().unwrap()];
        full.extend_from_slice(args);
        assert!(util::run_ok("git", &full), "git {args:?} failed");
    }

    fn repo(commit: bool) -> PathBuf {
        let root = scratch_dir().join("my repo");
        std::fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "-q", "-b", "trunk"]);
        if commit {
            git(
                &root,
                &[
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@example.com",
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "-q",
                    "--allow-empty",
                    "-m",
                    "first",
                ],
            );
        }
        root
    }

    fn exclude(root: &Path) -> String {
        std::fs::read_to_string(root.join(".git/info/exclude")).unwrap_or_default()
    }

    #[test]
    fn setup_refuses_a_repo_without_commits() {
        let root = repo(false);
        let mut ctx = ctx_in(&root, None);
        assert!(setup(&mut ctx).unwrap_err().to_string().contains("no commits"));
        assert!(!root.join(".crew").exists());
    }

    #[test]
    fn setup_creates_everything_a_crew_needs() {
        let root = repo(true);
        let mut ctx = ctx_in(&root, None);
        setup(&mut ctx).unwrap();
        for path in [
            "config.toml",
            "roles/lead.md",
            "roles/builder.md",
            "roles/reviewer.md",
            "board.md",
            "tasks.tsv",
            "tasks.md",
            "PROTOCOL.md",
        ] {
            assert!(ctx.file(path).is_file(), "{path}");
        }
        assert!(ctx.file("notes").is_dir());
        assert!(ctx.file("state").is_dir());
        assert_eq!(ctx.roles(), ["lead", "builder", "reviewer"]);
        assert_eq!(std::fs::read_to_string(ctx.file("board.md")).unwrap(), "# Crew board\n\n");
    }

    #[test]
    fn setup_records_the_current_branch_as_the_base() {
        let root = repo(true);
        let mut ctx = ctx_in(&root, None);
        create(&mut ctx).unwrap();
        assert_eq!(ctx.base_branch(), "trunk");
    }

    #[test]
    fn the_lead_works_in_the_main_checkout_and_the_others_in_worktrees() {
        let root = repo(true);
        let mut ctx = ctx_in(&root, None);
        create(&mut ctx).unwrap();
        assert!(!ctx.config.role_has_worktree("lead"));
        assert!(ctx.config.role_has_worktree("builder"));
        assert!(ctx.config.role_has_worktree("reviewer"));
    }

    #[test]
    fn running_setup_again_keeps_the_files_you_edited() {
        let root = repo(true);
        let mut ctx = ctx_in(&root, None);
        setup(&mut ctx).unwrap();
        std::fs::write(ctx.file("roles/lead.md"), "# Role: lead\ncustom\n").unwrap();
        std::fs::write(ctx.file("board.md"), "# Crew board\n\nhello\n").unwrap();
        setup(&mut ctx).unwrap();
        assert_eq!(std::fs::read_to_string(ctx.file("roles/lead.md")).unwrap(), "# Role: lead\ncustom\n");
        assert!(std::fs::read_to_string(ctx.file("board.md")).unwrap().contains("hello"));
    }

    #[test]
    fn create_fills_in_only_the_missing_role_files() {
        let root = repo(true);
        let mut ctx = ctx_in(&root, None);
        create(&mut ctx).unwrap();
        std::fs::write(ctx.file("roles/lead.md"), "custom").unwrap();
        std::fs::remove_file(ctx.file("roles/builder.md")).unwrap();
        write_role_files(&ctx).unwrap();
        assert_eq!(std::fs::read_to_string(ctx.file("roles/lead.md")).unwrap(), "custom");
        assert!(ctx.file("roles/builder.md").is_file());
    }

    #[test]
    fn crew_is_hidden_through_the_git_exclude_file() {
        let root = repo(true);
        let mut ctx = ctx_in(&root, None);
        create(&mut ctx).unwrap();
        assert!(exclude(&root).lines().any(|l| l == ".crew/"));
        assert!(!root.join(".gitignore").exists());
    }

    #[test]
    fn hiding_twice_adds_one_line() {
        let root = repo(true);
        let ctx = ctx_in(&root, None);
        hide_from_git(&ctx).unwrap();
        hide_from_git(&ctx).unwrap();
        assert_eq!(exclude(&root).matches(".crew/").count(), 1);
    }

    #[test]
    fn hiding_keeps_existing_exclude_lines_even_without_a_final_newline() {
        let root = repo(true);
        let ctx = ctx_in(&root, None);
        let path = root.join(".git/info/exclude");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "target/").unwrap();
        hide_from_git(&ctx).unwrap();
        assert_eq!(exclude(&root), "target/\n.crew/\n");
    }

    #[test]
    fn hiding_works_from_a_repo_that_is_not_a_git_checkout() {
        let ctx = ctx_in(&scratch_dir(), None);
        hide_from_git(&ctx).unwrap();
    }
}
