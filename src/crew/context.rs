use super::config::Config;
use super::state::State;
use crate::mux::{self, Mux};
use crate::util;
use anyhow::{anyhow, bail, Result};
use std::path::{Path, PathBuf};

// The plain terminal pane's name, and the sender `crew say` uses outside an agent pane.
pub const HUMAN: &str = "human";

pub struct Ctx {
    pub root: PathBuf,
    pub crew: PathBuf,
    pub slug: String,
    pub key: String,
    pub config: Config,
    pub state: State,
    pub mux: Box<dyn Mux>,
}

impl Ctx {
    // CREW_DIR is set in every agent pane, so `crew` finds the shared folder from any worktree.
    pub fn load() -> Result<Ctx> {
        let discovered_root = repo_root();
        let env_crew = std::env::var_os("CREW_DIR").map(PathBuf::from).filter(|d| d.is_dir());
        let (root, crew) = resolve_paths(discovered_root, env_crew)?;
        let config = Config::load(Some(crew.join("config.toml")))?;
        let mux = mux::make(config.multiplexer()?);
        let slug = slugify(&root.file_name().unwrap_or_default().to_string_lossy());
        let key = crew_key(&crew, &slug);
        Ok(Ctx { state: State::new(crew.join("state")), root, crew, slug, key, config, mux })
    }

    pub fn reload(&mut self) -> Result<()> {
        self.config = Config::load(Some(self.crew.join("config.toml")))?;
        self.mux = mux::make(self.config.multiplexer()?);
        Ok(())
    }

    pub fn require_crew(&self) -> Result<()> {
        if !self.crew.join("config.toml").is_file() {
            bail!("no crew here yet. Run: crew setup");
        }
        Ok(())
    }

    pub fn roles(&self) -> Vec<String> {
        self.config.roles()
    }

    pub fn role_exists(&self, role: &str) -> bool {
        self.roles().iter().any(|r| r == role)
    }

    // e.g. ~/.crew/worktrees/my-app-1a2b3c. The hash keeps two repos with the same name apart.
    pub fn worktree_base(&self) -> PathBuf {
        let id = &util::fingerprint(&self.root.to_string_lossy())[..6];
        self.config.worktree_dir().join(format!("{}-{id}", self.slug))
    }

    pub fn role_dir(&self, role: &str) -> PathBuf {
        if role != HUMAN && self.config.role_has_worktree(role) {
            self.worktree_base().join(role)
        } else {
            self.root.clone()
        }
    }

    pub fn agent_name(&self, role: &str) -> String {
        format!("{}-{role}", self.key)
    }

    pub fn base_branch(&self) -> String {
        self.config.base_branch().filter(|b| !b.is_empty()).unwrap_or_else(|| "main".to_string())
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.crew.join(name)
    }
}

fn resolve_paths(discovered_root: Option<PathBuf>, env_crew: Option<PathBuf>) -> Result<(PathBuf, PathBuf)> {
    match (discovered_root, env_crew) {
        (Some(root), Some(crew)) if crew.parent().is_some_and(|parent| parent == root) => Ok((root, crew)),
        (Some(root), Some(_) | None) => Ok((root.clone(), root.join(".crew"))),
        (None, Some(crew)) => Ok((crew.parent().map(Path::to_path_buf).unwrap_or_default(), crew)),
        (None, None) => Err(anyhow!("not inside a git repository")),
    }
}

// The main checkout's root, even when run from a worktree.
pub fn repo_root() -> Option<PathBuf> {
    repo_root_from(&std::env::current_dir().ok()?)
}

fn repo_root_from(dir: &Path) -> Option<PathBuf> {
    let out = util::run_out("git", &["-C", &dir.display().to_string(), "rev-parse", "--git-common-dir"])?;
    let common = PathBuf::from(out.trim());
    let common = if common.is_absolute() { common } else { dir.join(common) };
    std::fs::canonicalize(common).ok()?.parent().map(Path::to_path_buf)
}

// Short and safe for agent and tmux session names.
pub fn slugify(name: &str) -> String {
    let mapped: String =
        name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    let trimmed = mapped.trim_matches('-');
    let prefixed =
        if trimmed.starts_with(|c: char| c.is_ascii_lowercase()) { trimmed.to_string() } else { format!("r{trimmed}") };
    prefixed.chars().take(16).collect::<String>().trim_end_matches('-').to_string()
}

fn crew_key(root: &Path, slug: &str) -> String {
    format!("{slug}-{}", &util::fingerprint(&root.to_string_lossy())[..6])
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::{crew_key, resolve_paths, slugify};
    use crate::crew::testutil::{fixture, ROLES};
    use std::path::{Path, PathBuf};

    #[test]
    fn slugs_are_short_and_safe() {
        assert_eq!(slugify("My App"), "my-app");
        assert_eq!(slugify("2048-game"), "r2048-game");
        assert_eq!(slugify("a-really-long-repository-name"), "a-really-long-re");
        assert_eq!(slugify("--x--"), "x");
    }

    #[test]
    fn keys_distinguish_crew_directories() {
        assert_ne!(crew_key(Path::new("/repo/.crew-a"), "api"), crew_key(Path::new("/repo/.crew-b"), "api"));
    }

    #[test]
    fn keys_distinguish_same_named_repositories() {
        assert_ne!(crew_key(Path::new("/one/api/.crew"), "api"), crew_key(Path::new("/two/api/.crew"), "api"));
    }

    #[test]
    fn stale_crew_dir_falls_back_to_current_repository() {
        let root = PathBuf::from("/current-repo");
        let paths = resolve_paths(Some(root.clone()), Some(PathBuf::from("/other-repo/.crew"))).unwrap();
        assert_eq!(paths, (root.clone(), root.join(".crew")));
    }

    #[test]
    fn crew_dir_is_used_from_a_worktree() {
        let root = PathBuf::from("/current-repo");
        let crew = root.join(".crew");
        assert_eq!(resolve_paths(Some(root.clone()), Some(crew.clone())).unwrap(), (root, crew));
    }

    #[test]
    fn worktree_dirs_are_per_repo_and_per_role() {
        let f = fixture(ROLES);
        let base = f.ctx.worktree_base();
        let name = base.file_name().unwrap().to_string_lossy().to_string();
        assert!(name.starts_with("my-repo-") && name.len() == "my-repo-".len() + 6, "{name}");
        assert_eq!(base, f.ctx.worktree_base());
        assert_eq!(f.ctx.role_dir("builder"), base.join("builder"));
        assert!(base.starts_with(f.dir.join("wt")));
    }

    #[test]
    fn same_named_repos_in_different_places_get_different_worktree_dirs() {
        let mut f = fixture(ROLES);
        let before = f.ctx.worktree_base();
        f.ctx.root = PathBuf::from("/elsewhere/my-repo");
        assert_ne!(f.ctx.worktree_base(), before);
    }

    #[test]
    fn roles_without_a_worktree_and_the_human_work_in_the_main_checkout() {
        let f = fixture(ROLES);
        assert_eq!(f.ctx.role_dir("lead"), f.ctx.root);
        assert_eq!(f.ctx.role_dir(HUMAN), f.ctx.root);
    }

    #[test]
    fn a_role_named_human_never_gets_a_worktree() {
        let f = fixture("[roles.human]\nworktree = true\n");
        assert_eq!(f.ctx.role_dir(HUMAN), f.ctx.root);
    }

    #[test]
    fn roles_are_listed_in_config_order_and_looked_up_by_name() {
        let f = fixture(ROLES);
        assert_eq!(f.ctx.roles(), ["lead", "builder", "reviewer"]);
        assert!(f.ctx.role_exists("builder"));
        assert!(!f.ctx.role_exists("tester"));
    }

    #[test]
    fn base_branch_defaults_to_main_and_ignores_an_empty_setting() {
        assert_eq!(fixture("").ctx.base_branch(), "main");
        assert_eq!(fixture("base_branch = \"\"").ctx.base_branch(), "main");
        assert_eq!(fixture("base_branch = \"trunk\"").ctx.base_branch(), "trunk");
    }

    #[test]
    fn names_and_files_are_derived_from_the_slug_and_crew_folder() {
        let f = fixture(ROLES);
        assert_eq!(f.ctx.agent_name("lead"), "my-repo-abc123-lead");
        assert_eq!(f.ctx.file("board.md"), f.ctx.crew.join("board.md"));
    }

    #[test]
    fn require_crew_wants_a_repo_config() {
        let f = fixture(ROLES);
        let err = f.ctx.require_crew().unwrap_err().to_string();
        assert!(err.contains("crew setup"));
        std::fs::write(f.ctx.file("config.toml"), "").unwrap();
        f.ctx.require_crew().unwrap();
    }

    #[test]
    fn slugs_cover_odd_names() {
        assert_eq!(slugify(""), "r");
        assert_eq!(slugify("___"), "r");
        assert_eq!(slugify("Über Repo"), "ber-repo");
        assert_eq!(slugify("sixteen-chars-xx-y"), "sixteen-chars-xx");
    }

    #[test]
    fn the_repo_root_is_found_from_a_checkout_and_from_a_subfolder() {
        let f = fixture(ROLES);
        let git = |args: &[&str]| assert!(util::run_ok("git", &[&["-C", f.ctx.root.to_str().unwrap()], args].concat()));
        git(&["init", "-q"]);
        let sub = f.ctx.root.join("a/b");
        std::fs::create_dir_all(&sub).unwrap();
        let root = std::fs::canonicalize(&f.ctx.root).unwrap();
        assert_eq!(repo_root_from(&f.ctx.root), Some(root.clone()));
        assert_eq!(repo_root_from(&sub), Some(root));
    }

    #[test]
    fn there_is_no_repo_root_outside_a_checkout() {
        let f = fixture(ROLES);
        let outside = f.dir.join("plain");
        std::fs::create_dir_all(&outside).unwrap();
        assert_eq!(repo_root_from(&outside), None);
    }
}
