//! Where the crew lives: the repo, its .crew folder, the config, the session state and the multiplexer.

use crate::config::Config;
use crate::mux::{self, Mux};
use crate::state::State;
use crate::{setup, util};
use anyhow::{anyhow, bail, Result};
use std::path::{Path, PathBuf};

pub struct Ctx {
    pub root: PathBuf,
    pub crew: PathBuf,
    pub slug: String,
    pub config: Config,
    pub state: State,
    pub mux: Box<dyn Mux>,
}

impl Ctx {
    /// Finds the repo and its .crew folder, upgrades files from older versions, and loads the config.
    pub fn load() -> Result<Ctx> {
        let (root, crew) = if let Some(crew) = std::env::var_os("CREW_DIR").map(PathBuf::from).filter(|d| d.is_dir()) {
            (crew.parent().map(Path::to_path_buf).unwrap_or_default(), crew)
        } else {
            let root = repo_root().ok_or_else(|| anyhow!("not inside a git repository"))?;
            let crew = root.join(".crew");
            (root, crew)
        };
        setup::migrate_old_roster(&crew)?;
        let config = Config::load(Some(crew.join("config.toml")))?;
        let mux = mux::make(config.multiplexer()?);
        let slug = slugify(&root.file_name().unwrap_or_default().to_string_lossy());
        Ok(Ctx { state: State::new(crew.join("state")), root, crew, slug, config, mux })
    }

    /// Reloads the config after it changed on disk.
    pub fn reload(&mut self) -> Result<()> {
        self.config = Config::load(Some(self.crew.join("config.toml")))?;
        self.mux = mux::make(self.config.multiplexer()?);
        Ok(())
    }

    /// Fails with a hint if this repo has no crew yet.
    pub fn require_crew(&self) -> Result<()> {
        if !self.crew.join("config.toml").is_file() {
            bail!("no crew here yet. Run: crew setup");
        }
        Ok(())
    }

    /// Role names, in config order.
    pub fn roles(&self) -> Vec<String> {
        self.config.roles()
    }

    /// Succeeds if the role is in the repo config.
    pub fn role_exists(&self, role: &str) -> bool {
        self.roles().iter().any(|r| r == role)
    }

    /// The folder this repo's worktrees go in, e.g. ~/.crew/worktrees/my-app-1a2b3c.
    pub fn worktree_base(&self) -> PathBuf {
        let id = &util::fingerprint(&self.root.to_string_lossy())[..6];
        self.config.worktree_dir().join(format!("{}-{id}", self.slug))
    }

    /// The folder a role works in: its worktree, or the main checkout.
    pub fn role_dir(&self, role: &str) -> PathBuf {
        if self.config.role_has_worktree(role) {
            self.worktree_base().join(role)
        } else {
            self.root.clone()
        }
    }

    /// The herdr agent name for a role, e.g. my-app-builder.
    pub fn agent_name(&self, role: &str) -> String {
        format!("{}-{role}", self.slug)
    }

    /// The branch the crew merges into.
    pub fn base_branch(&self) -> String {
        self.config
            .base_branch()
            .or_else(|| std::fs::read_to_string(self.crew.join("base")).ok().map(|s| s.trim().to_string()))
            .filter(|b| !b.is_empty())
            .unwrap_or_else(|| "main".to_string())
    }

    /// Path of a file in .crew.
    pub fn file(&self, name: &str) -> PathBuf {
        self.crew.join(name)
    }
}

/// The main checkout's root folder, even when run from a worktree.
pub fn repo_root() -> Option<PathBuf> {
    let out = util::run_out("git", &["rev-parse", "--git-common-dir"])?;
    let common = PathBuf::from(out.trim());
    let common = if common.is_absolute() { common } else { std::env::current_dir().ok()?.join(common) };
    std::fs::canonicalize(common).ok()?.parent().map(Path::to_path_buf)
}

/// Turns a folder name into a short lowercase id usable in agent and session names.
pub fn slugify(name: &str) -> String {
    let mapped: String =
        name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    let trimmed = mapped.trim_matches('-');
    let prefixed =
        if trimmed.starts_with(|c: char| c.is_ascii_lowercase()) { trimmed.to_string() } else { format!("r{trimmed}") };
    prefixed.chars().take(16).collect::<String>().trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn slugs_are_short_and_safe() {
        assert_eq!(slugify("My App"), "my-app");
        assert_eq!(slugify("2048-game"), "r2048-game");
        assert_eq!(slugify("a-really-long-repository-name"), "a-really-long-re");
        assert_eq!(slugify("--x--"), "x");
    }
}
