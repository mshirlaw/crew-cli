//! Settings from ~/.config/crew/config.toml (global) and .crew/config.toml (repo; wins over global).
//! Files are edited with toml_edit so comments and ordering survive.

use crate::{templates, ui, util};
use anyhow::{anyhow, bail, Context, Result};
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Table};

/// Agent kinds herdr can start and track itself (from herdr 0.9.5). Override per agent with `kind`.
pub const HERDR_KINDS: &[&str] = &[
    "pi",
    "claude",
    "codex",
    "gemini",
    "cursor",
    "devin",
    "agy",
    "cline",
    "omp",
    "mastracode",
    "opencode",
    "copilot",
    "kimi",
    "kiro",
    "droid",
    "amp",
    "grok",
    "hermes",
    "kilo",
    "qodercli",
    "qwen",
    "letta",
    "maki",
    "muse",
];

/// Which terminal multiplexer runs the agents.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MuxKind {
    Herdr,
    Tmux,
}

impl MuxKind {
    /// The name used in config files.
    pub fn name(self) -> &'static str {
        match self {
            MuxKind::Herdr => "herdr",
            MuxKind::Tmux => "tmux",
        }
    }

    /// herdr if it's installed, otherwise tmux if that is, otherwise herdr.
    pub fn detect() -> MuxKind {
        if !util::has("herdr") && util::has("tmux") {
            MuxKind::Tmux
        } else {
            MuxKind::Herdr
        }
    }
}

pub struct Config {
    pub global_path: PathBuf,
    pub repo_path: Option<PathBuf>,
    global: DocumentMut,
    repo: DocumentMut,
}

impl Config {
    /// Loads the global config and, if a path is given, the repo config. Missing files count as empty.
    pub fn load(repo_path: Option<PathBuf>) -> Result<Config> {
        let global_path = util::crew_home().join("config.toml");
        let global = parse_file(&global_path)?;
        let repo = match &repo_path {
            Some(path) => parse_file(path)?,
            None => DocumentMut::new(),
        };
        Ok(Config { global_path, repo_path, global, repo })
    }

    /// Finds a setting, looking in the repo config first, then the global one.
    fn item(&self, path: &[&str]) -> Option<&Item> {
        lookup(&self.repo, path).or_else(|| lookup(&self.global, path))
    }

    /// Reads a text setting.
    pub fn string(&self, path: &[&str]) -> Option<String> {
        self.item(path)?.as_str().map(String::from)
    }

    /// Reads a true/false setting.
    fn boolean(&self, path: &[&str]) -> Option<bool> {
        self.item(path)?.as_bool()
    }

    /// Reads a number setting, whole or decimal.
    fn number(&self, path: &[&str]) -> Option<f64> {
        let item = self.item(path)?;
        item.as_float().or_else(|| item.as_integer().map(|n| n as f64))
    }

    /// The configured multiplexer (default herdr).
    pub fn multiplexer(&self) -> Result<MuxKind> {
        match self.item(&["multiplexer"]) {
            None => Ok(MuxKind::Herdr),
            Some(item) => match item.as_str() {
                Some("herdr") => Ok(MuxKind::Herdr),
                Some("tmux") => Ok(MuxKind::Tmux),
                _ => bail!(
                    "multiplexer must be \"herdr\" or \"tmux\", not {} (see crew config)",
                    item.to_string().trim()
                ),
            },
        }
    }

    /// The worktree folder setting as written (default ~/.crew/worktrees).
    pub fn worktree_dir_text(&self) -> String {
        self.string(&["worktree_dir"]).unwrap_or_else(|| "~/.crew/worktrees".to_string())
    }

    /// The folder agent worktrees go in.
    pub fn worktree_dir(&self) -> PathBuf {
        util::expand_home(&self.worktree_dir_text())
    }

    /// Seconds between pasting a message and pressing Enter in tmux (default 0.3).
    pub fn paste_delay(&self) -> f64 {
        match self.number(&["paste_delay"]) {
            Some(n) if n >= 0.0 => n,
            None if self.item(&["paste_delay"]).is_none() => 0.3,
            _ => {
                ui::warn("paste_delay must be a number of seconds. Using 0.3");
                0.3
            }
        }
    }

    /// Seconds of no screen change before an agent that owes work counts as stalled (default 600; 0 is off).
    pub fn stall_timeout(&self) -> u64 {
        match self.item(&["stall_timeout"]).map(|i| i.as_integer()) {
            None => 600,
            Some(Some(n)) if n >= 0 => n as u64,
            _ => {
                ui::warn("stall_timeout must be whole seconds. Using 600");
                600
            }
        }
    }

    /// Seconds between stall checks (default 30).
    pub fn watch_interval(&self) -> u64 {
        match self.item(&["watch_interval"]).and_then(Item::as_integer) {
            Some(n) if n > 0 => n as u64,
            _ => 30,
        }
    }

    /// The branch the crew merges into, if set.
    pub fn base_branch(&self) -> Option<String> {
        self.string(&["base_branch"])
    }

    /// Role names in the order they appear in the repo config.
    pub fn roles(&self) -> Vec<String> {
        roles_in(&self.repo)
    }

    /// Succeeds if the global config defines any roles.
    pub fn global_has_roles(&self) -> bool {
        !roles_in(&self.global).is_empty()
    }

    /// The agent that plays a role (default: the first of claude, codex, gemini that's installed).
    pub fn role_agent(&self, role: &str) -> String {
        self.string(&["roles", role, "agent"]).unwrap_or_else(|| util_first_installed(&["claude", "codex", "gemini"]))
    }

    /// Succeeds if a role works in its own worktree (default yes).
    pub fn role_has_worktree(&self, role: &str) -> bool {
        self.boolean(&["roles", role, "worktree"]).unwrap_or(true)
    }

    /// An agent's launch command as words, before placeholders are filled in.
    pub fn agent_command(&self, agent: &str) -> Vec<String> {
        let words = match self.item(&["agents", agent, "command"]) {
            Some(item) => match (item.as_array(), item.as_str()) {
                (Some(array), _) => array.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
                (None, Some(text)) => {
                    shlex::split(text).unwrap_or_else(|| text.split_whitespace().map(String::from).collect())
                }
                _ => Vec::new(),
            },
            None => default_command(agent),
        };
        if words.is_empty() {
            vec![agent.to_string()]
        } else {
            words
        }
    }

    /// The herdr kind for an agent, or None if herdr should just type the command.
    pub fn agent_kind(&self, agent: &str) -> Option<String> {
        match self.string(&["agents", agent, "kind"]) {
            Some(kind) => (!kind.is_empty()).then_some(kind),
            None => HERDR_KINDS.contains(&agent).then(|| agent.to_string()),
        }
    }

    /// Sets a role's agent and/or worktree setting in the repo config, adding the role if needed.
    pub fn set_role(&mut self, role: &str, agent: Option<&str>, worktree: Option<bool>) -> Result<()> {
        let roles = self.repo.entry("roles").or_insert_with(|| {
            let mut table = Table::new();
            table.set_implicit(true);
            Item::Table(table)
        });
        let roles = roles.as_table_mut().ok_or_else(|| anyhow!("roles in .crew/config.toml must be a table"))?;
        let entry = roles.entry(role).or_insert(Item::Table(Table::new()));
        let table =
            entry.as_table_like_mut().ok_or_else(|| anyhow!("roles.{role} in .crew/config.toml must be a table"))?;
        if let Some(agent) = agent {
            table.insert("agent", toml_edit::value(agent));
        }
        if let Some(worktree) = worktree {
            table.insert("worktree", toml_edit::value(worktree));
        }
        self.save_repo()
    }

    /// Removes a role from the repo config.
    pub fn remove_role(&mut self, role: &str) -> Result<()> {
        if let Some(roles) = self.repo.get_mut("roles").and_then(Item::as_table_like_mut) {
            roles.remove(role);
        }
        self.save_repo()
    }

    /// The roles in the global config, as TOML text.
    pub fn global_roles_text(&self) -> String {
        roles_only(&self.global)
    }

    /// Replaces the global config's roles with this repo's, creating the global config if needed.
    pub fn save_roles_to_global(&mut self) -> Result<()> {
        if !self.global_path.exists() {
            write_global_config(&self.global_path, MuxKind::detect())?;
            self.global = parse_file(&self.global_path)?;
        }
        let mut global = self.global.clone();
        global.remove("roles");
        let text = format!("{}\n\n{}", global.to_string().trim_end(), roles_only(&self.repo));
        util::write_atomic(&self.global_path, &text)?;
        self.global = parse_file(&self.global_path)?;
        Ok(())
    }

    /// Writes the repo config back to its file.
    fn save_repo(&self) -> Result<()> {
        let path = self.repo_path.as_ref().ok_or_else(|| anyhow!("no repo config to save"))?;
        util::write_atomic(path, &self.repo.to_string())
    }
}

/// Writes the starter global config with the given multiplexer.
pub fn write_global_config(path: &Path, mux: MuxKind) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("could not create {}", dir.display()))?;
    }
    let text =
        templates::GLOBAL_CONFIG.replacen("multiplexer = \"herdr\"", &format!("multiplexer = \"{}\"", mux.name()), 1);
    util::write_atomic(path, &text)
}

/// Reads and parses a TOML file; a missing file is an empty config.
fn parse_file(path: &Path) -> Result<DocumentMut> {
    match std::fs::read_to_string(path) {
        Ok(text) => text.parse::<DocumentMut>().map_err(|e| anyhow!("{} is not valid TOML:\n{e}", util::tilde(path))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DocumentMut::new()),
        Err(e) => Err(e).with_context(|| format!("could not read {}", path.display())),
    }
}

/// Follows a dotted path like roles.lead.agent through a document.
fn lookup<'a>(doc: &'a DocumentMut, path: &[&str]) -> Option<&'a Item> {
    let mut item = doc.as_item();
    for key in path {
        item = item.get(key)?;
    }
    Some(item)
}

/// Role names in a document, in order.
fn roles_in(doc: &DocumentMut) -> Vec<String> {
    doc.get("roles")
        .and_then(Item::as_table_like)
        .map(|roles| roles.iter().filter(|(_, v)| v.is_table_like()).map(|(k, _)| k.to_string()).collect())
        .unwrap_or_default()
}

/// A document's [roles.*] sections as TOML text.
fn roles_only(doc: &DocumentMut) -> String {
    let mut only = DocumentMut::new();
    if let Some(roles) = doc.get("roles") {
        only.insert("roles", roles.clone());
    }
    only.to_string()
}

/// The built-in launch command for the agents crew knows.
fn default_command(agent: &str) -> Vec<String> {
    let text = match agent {
        "claude" => "claude --add-dir {crew}",
        "codex" => "codex --add-dir {crew}",
        "gemini" => "gemini --include-directories {crew} -i",
        other => other,
    };
    text.split_whitespace().map(String::from).collect()
}

/// The first program in the list that's installed, or the first one if none are.
pub fn util_first_installed(programs: &[&str]) -> String {
    programs.iter().find(|p| util::has(p)).unwrap_or(&programs[0]).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a config from text instead of files.
    fn config(global: &str, repo: &str) -> Config {
        Config {
            global_path: PathBuf::from("/nonexistent/global.toml"),
            repo_path: None,
            global: global.parse().unwrap(),
            repo: repo.parse().unwrap(),
        }
    }

    #[test]
    fn repo_settings_win() {
        let c = config("multiplexer = \"tmux\"\npaste_delay = 1", "multiplexer = \"herdr\"");
        assert_eq!(c.multiplexer().unwrap(), MuxKind::Herdr);
        assert_eq!(c.paste_delay(), 1.0);
    }

    #[test]
    fn bad_multiplexer_is_an_error() {
        let c = config("", "multiplexer = \"screen\"");
        assert!(c.multiplexer().unwrap_err().to_string().contains("must be \"herdr\" or \"tmux\""));
    }

    #[test]
    fn roles_keep_their_order() {
        let c = config("", "[roles.lead]\nagent = \"claude\"\n[roles.builder]\nagent = \"codex\"\nworktree = false\n");
        assert_eq!(c.roles(), vec!["lead", "builder"]);
        assert_eq!(c.role_agent("builder"), "codex");
        assert!(!c.role_has_worktree("builder"));
        assert!(c.role_has_worktree("lead"));
    }

    #[test]
    fn commands_can_be_strings_or_lists() {
        let c =
            config("[agents.a]\ncommand = \"a --dir '{crew}' -x\"\n[agents.b]\ncommand = [\"b\", \"two words\"]\n", "");
        assert_eq!(c.agent_command("a"), vec!["a", "--dir", "{crew}", "-x"]);
        assert_eq!(c.agent_command("b"), vec!["b", "two words"]);
        assert_eq!(c.agent_command("claude"), vec!["claude", "--add-dir", "{crew}"]);
    }

    #[test]
    fn herdr_kinds_default_and_override() {
        let c = config("[agents.codex]\nkind = \"\"\n", "");
        assert_eq!(c.agent_kind("claude").as_deref(), Some("claude"));
        assert_eq!(c.agent_kind("codex"), None);
        assert_eq!(c.agent_kind("mytool"), None);
    }
}
