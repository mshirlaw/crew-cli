// Settings come from ~/.config/crew/config.toml and .crew/config.toml; the repo file wins.
// toml_edit keeps comments and ordering when crew edits the files.

use super::templates;
use crate::util::{self, ui};
use anyhow::{anyhow, bail, Context, Result};
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Table};

// From herdr 0.9.5. These get herdr's own agent tracking; override per agent with `kind`.
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MuxKind {
    Herdr,
    Tmux,
}

impl MuxKind {
    pub fn name(self) -> &'static str {
        match self {
            MuxKind::Herdr => "herdr",
            MuxKind::Tmux => "tmux",
        }
    }

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
    pub fn load(repo_path: Option<PathBuf>) -> Result<Config> {
        let global_path = util::crew_home().join("config.toml");
        let global = parse_file(&global_path)?;
        let repo = match &repo_path {
            Some(path) => parse_file(path)?,
            None => DocumentMut::new(),
        };
        Ok(Config { global_path, repo_path, global, repo })
    }

    fn item(&self, path: &[&str]) -> Option<&Item> {
        lookup(&self.repo, path).or_else(|| lookup(&self.global, path))
    }

    pub fn string(&self, path: &[&str]) -> Option<String> {
        self.item(path)?.as_str().map(String::from)
    }

    fn boolean(&self, path: &[&str]) -> Option<bool> {
        self.item(path)?.as_bool()
    }

    fn number(&self, path: &[&str]) -> Option<f64> {
        let item = self.item(path)?;
        item.as_float().or_else(|| item.as_integer().and_then(|n| i32::try_from(n).ok()).map(f64::from))
    }

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

    pub fn worktree_dir_text(&self) -> String {
        self.string(&["worktree_dir"]).unwrap_or_else(|| "~/.crew/worktrees".to_string())
    }

    pub fn worktree_dir(&self) -> PathBuf {
        util::expand_home(&self.worktree_dir_text())
    }

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

    pub fn stall_timeout(&self) -> u64 {
        match self.item(&["stall_timeout"]).map(Item::as_integer) {
            None => 600,
            Some(n) => n.and_then(|n| u64::try_from(n).ok()).unwrap_or_else(|| {
                ui::warn("stall_timeout must be whole seconds. Using 600");
                600
            }),
        }
    }

    pub fn watch_interval(&self) -> u64 {
        self.item(&["watch_interval"])
            .and_then(Item::as_integer)
            .and_then(|n| u64::try_from(n).ok())
            .filter(|&n| n > 0)
            .unwrap_or(30)
    }

    pub fn base_branch(&self) -> Option<String> {
        self.string(&["base_branch"])
    }

    // Only the repo file defines a repo's roles; global roles are a template for `crew setup`.
    pub fn roles(&self) -> Vec<String> {
        roles_in(&self.repo)
    }

    pub fn global_has_roles(&self) -> bool {
        !roles_in(&self.global).is_empty()
    }

    pub fn role_agent(&self, role: &str) -> String {
        self.string(&["roles", role, "agent"]).unwrap_or_else(|| first_installed(&["claude", "codex", "gemini"]))
    }

    pub fn role_has_worktree(&self, role: &str) -> bool {
        self.boolean(&["roles", role, "worktree"]).unwrap_or(true)
    }

    // Words before placeholders are filled in. A string command is split shell-style first,
    // so a quoted path with spaces stays one word.
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

    // None means herdr should just type the command into the pane.
    pub fn agent_kind(&self, agent: &str) -> Option<String> {
        match self.string(&["agents", agent, "kind"]) {
            Some(kind) => (!kind.is_empty()).then_some(kind),
            None => HERDR_KINDS.contains(&agent).then(|| agent.to_string()),
        }
    }

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

    pub fn remove_role(&mut self, role: &str) -> Result<()> {
        if let Some(roles) = self.repo.get_mut("roles").and_then(Item::as_table_like_mut) {
            roles.remove(role);
        }
        self.save_repo()
    }

    pub fn global_roles_text(&self) -> String {
        roles_only(&self.global)
    }

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

    fn save_repo(&self) -> Result<()> {
        let path = self.repo_path.as_ref().ok_or_else(|| anyhow!("no repo config to save"))?;
        util::write_atomic(path, &self.repo.to_string())
    }
}

pub fn write_global_config(path: &Path, mux: MuxKind) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("could not create {}", dir.display()))?;
    }
    let text =
        templates::GLOBAL_CONFIG.replacen("multiplexer = \"herdr\"", &format!("multiplexer = \"{}\"", mux.name()), 1);
    util::write_atomic(path, &text)
}

pub fn first_installed(programs: &[&str]) -> String {
    programs.iter().find(|p| util::has(p)).unwrap_or(&programs[0]).to_string()
}

fn parse_file(path: &Path) -> Result<DocumentMut> {
    match std::fs::read_to_string(path) {
        Ok(text) => text.parse::<DocumentMut>().map_err(|e| anyhow!("{} is not valid TOML:\n{e}", util::tilde(path))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DocumentMut::new()),
        Err(e) => Err(e).with_context(|| format!("could not read {}", path.display())),
    }
}

fn lookup<'a>(doc: &'a DocumentMut, path: &[&str]) -> Option<&'a Item> {
    let mut item = doc.as_item();
    for key in path {
        item = item.get(key)?;
    }
    Some(item)
}

fn roles_in(doc: &DocumentMut) -> Vec<String> {
    doc.get("roles")
        .and_then(Item::as_table_like)
        .map(|roles| roles.iter().filter(|(_, v)| v.is_table_like()).map(|(k, _)| k.to_string()).collect())
        .unwrap_or_default()
}

fn roles_only(doc: &DocumentMut) -> String {
    let mut only = DocumentMut::new();
    if let Some(roles) = doc.get("roles") {
        only.insert("roles", roles.clone());
    }
    only.to_string()
}

fn default_command(agent: &str) -> Vec<String> {
    let text = match agent {
        "claude" => "claude --add-dir {crew}",
        "codex" => "codex --add-dir {crew}",
        "gemini" => "gemini --include-directories {crew} -i",
        other => other,
    };
    text.split_whitespace().map(String::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!((c.paste_delay() - 1.0).abs() < f64::EPSILON);
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
