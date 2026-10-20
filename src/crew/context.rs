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
    let out = util::run_out("git", &["rev-parse", "--git-common-dir"])?;
    let common = PathBuf::from(out.trim());
    let common = if common.is_absolute() { common } else { std::env::current_dir().ok()?.join(common) };
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
    use super::{crew_key, resolve_paths, slugify};
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
}
