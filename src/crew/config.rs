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

    #[cfg(test)]
    pub fn from_toml(global: &str, repo: &str) -> Config {
        Config {
            global_path: PathBuf::from("/nonexistent/global.toml"),
            repo_path: None,
            global: global.parse().unwrap(),
            repo: repo.parse().unwrap(),
        }
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

    pub fn panes_per_tab(&self) -> usize {
        self.item(&["panes_per_tab"])
            .and_then(Item::as_integer)
            .and_then(|n| usize::try_from(n).ok())
            .filter(|&n| n > 0)
            .unwrap_or(4)
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
    programs.iter().copied().find(|p| util::has(p)).unwrap_or(programs[0]).to_string()
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
        "claude" => "claude --add-dir {crew} --",
        // In a folder it doesn't trust yet, Codex defaults to read-only and then refuses --add-dir.
        "codex" => "codex --sandbox workspace-write --add-dir {crew}",
        "gemini" => "gemini --include-directories {crew} -i",
        other => other,
    };
    text.split_whitespace().map(String::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(global: &str, repo: &str) -> Config {
        Config::from_toml(global, repo)
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
        assert_eq!(c.agent_command("claude"), vec!["claude", "--add-dir", "{crew}", "--"]);
        assert_eq!(c.agent_command("codex")[1..3], ["--sandbox", "workspace-write"]);
    }

    #[test]
    fn herdr_kinds_default_and_override() {
        let c = config("[agents.codex]\nkind = \"\"\n", "");
        assert_eq!(c.agent_kind("claude").as_deref(), Some("claude"));
        assert_eq!(c.agent_kind("codex"), None);
        assert_eq!(c.agent_kind("mytool"), None);
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;

    fn config(global: &str, repo: &str) -> Config {
        Config::from_toml(global, repo)
    }

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("crew-config-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("config.toml")
    }

    fn cleanup(path: &Path) {
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn defaults_apply_when_nothing_is_set() {
        let c = config("", "");
        assert_eq!(c.multiplexer().unwrap(), MuxKind::Herdr);
        assert_eq!(c.stall_timeout(), 600);
        assert_eq!(c.panes_per_tab(), 4);
        assert_eq!(c.watch_interval(), 30);
        assert!((c.paste_delay() - 0.3).abs() < f64::EPSILON);
        assert_eq!(c.base_branch(), None);
        assert_eq!(c.worktree_dir_text(), "~/.crew/worktrees");
        assert!(c.roles().is_empty());
    }

    #[test]
    fn multiplexer_names_round_trip() {
        assert_eq!(config("multiplexer = \"tmux\"", "").multiplexer().unwrap(), MuxKind::Tmux);
        assert_eq!(MuxKind::Tmux.name(), "tmux");
        assert_eq!(MuxKind::Herdr.name(), "herdr");
        let err = config("multiplexer = 3", "").multiplexer().unwrap_err().to_string();
        assert!(err.contains("must be \"herdr\" or \"tmux\""), "{err}");
    }

    #[test]
    fn stall_timeout_accepts_zero_and_falls_back_on_nonsense() {
        assert_eq!(config("stall_timeout = 0", "").stall_timeout(), 0);
        assert_eq!(config("stall_timeout = 90", "").stall_timeout(), 90);
        assert_eq!(config("stall_timeout = -5", "").stall_timeout(), 600);
        assert_eq!(config("stall_timeout = \"soon\"", "").stall_timeout(), 600);
    }

    #[test]
    fn counts_must_be_positive_whole_numbers() {
        assert_eq!(config("panes_per_tab = 6", "").panes_per_tab(), 6);
        assert_eq!(config("panes_per_tab = 0", "").panes_per_tab(), 4);
        assert_eq!(config("panes_per_tab = -1", "").panes_per_tab(), 4);
        assert_eq!(config("watch_interval = 5", "").watch_interval(), 5);
        assert_eq!(config("watch_interval = 0", "").watch_interval(), 30);
        assert_eq!(config("watch_interval = \"x\"", "").watch_interval(), 30);
    }

    #[test]
    fn paste_delay_takes_integers_and_floats_but_not_negatives_or_text() {
        assert!((config("paste_delay = 2", "").paste_delay() - 2.0).abs() < f64::EPSILON);
        assert!((config("paste_delay = 0.75", "").paste_delay() - 0.75).abs() < f64::EPSILON);
        assert!((config("paste_delay = 0", "").paste_delay()).abs() < f64::EPSILON);
        assert!((config("paste_delay = -1", "").paste_delay() - 0.3).abs() < f64::EPSILON);
        assert!((config("paste_delay = \"slow\"", "").paste_delay() - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn worktree_dir_expands_a_leading_tilde() {
        let c = config("worktree_dir = \"/var/wt\"", "");
        assert_eq!(c.worktree_dir(), PathBuf::from("/var/wt"));
        assert!(!config("worktree_dir = \"~/wt\"", "").worktree_dir().starts_with("~"));
    }

    #[test]
    fn only_the_repo_file_defines_roles() {
        let c = config("[roles.global]\nagent = \"claude\"\n", "[roles.a]\n[roles.b]\n");
        assert_eq!(c.roles(), ["a", "b"]);
        assert!(c.global_has_roles());
        assert!(!config("", "[roles.a]\n").global_has_roles());
    }

    #[test]
    fn role_settings_have_defaults() {
        let c = config("", "[roles.a]\nagent = \"codex\"\nworktree = false\n[roles.b]\n");
        assert_eq!(c.role_agent("a"), "codex");
        assert!(!c.role_has_worktree("a"));
        assert!(c.role_has_worktree("b"));
        assert!(c.role_has_worktree("unknown"));
        assert!(["claude", "codex", "gemini"].contains(&c.role_agent("b").as_str()));
    }

    #[test]
    fn built_in_agent_commands_include_the_crew_folder() {
        let c = config("", "");
        assert_eq!(c.agent_command("claude"), ["claude", "--add-dir", "{crew}", "--"]);
        let codex = c.agent_command("codex");
        assert!(codex.windows(2).any(|w| w == ["--sandbox", "workspace-write"]));
        assert!(codex.contains(&"{crew}".to_string()));
        assert!(c.agent_command("gemini").contains(&"--include-directories".to_string()));
    }

    #[test]
    fn unknown_agents_run_as_their_own_name() {
        assert_eq!(config("", "").agent_command("aider"), ["aider"]);
    }

    #[test]
    fn string_commands_keep_quoted_words_together() {
        let c = config("", "[agents.x]\ncommand = \"x --dir '/my path' -v\"\n");
        assert_eq!(c.agent_command("x"), ["x", "--dir", "/my path", "-v"]);
    }

    #[test]
    fn an_unbalanced_quote_falls_back_to_whitespace_splitting() {
        let c = config("", "[agents.x]\ncommand = \"x 'oops\"\n");
        assert_eq!(c.agent_command("x"), ["x", "'oops"]);
    }

    #[test]
    fn empty_or_wrongly_typed_commands_fall_back_to_the_agent_name() {
        assert_eq!(config("", "[agents.x]\ncommand = []\n").agent_command("x"), ["x"]);
        assert_eq!(config("", "[agents.x]\ncommand = 5\n").agent_command("x"), ["x"]);
    }

    #[test]
    fn list_commands_ignore_non_string_entries() {
        let c = config("", "[agents.x]\ncommand = [\"x\", 1, \"-v\"]\n");
        assert_eq!(c.agent_command("x"), ["x", "-v"]);
    }

    #[test]
    fn agent_kind_defaults_to_known_agents_and_can_be_blanked() {
        let c = config("", "[agents.claude]\nkind = \"\"\n[agents.mine]\nkind = \"codex\"\n");
        assert_eq!(c.agent_kind("claude"), None);
        assert_eq!(c.agent_kind("mine").as_deref(), Some("codex"));
        assert_eq!(c.agent_kind("codex").as_deref(), Some("codex"));
        assert_eq!(c.agent_kind("aider"), None);
    }

    #[test]
    fn set_role_adds_and_updates_while_keeping_comments() {
        let path = temp_file("set-role");
        let mut c = config("", "# my notes\nbase_branch = \"main\"\n\n[roles.a]\nagent = \"claude\"\n");
        c.repo_path = Some(path.clone());
        c.set_role("a", None, Some(false)).unwrap();
        c.set_role("b", Some("codex"), None).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# my notes"));
        let reloaded = config("", &text);
        assert_eq!(reloaded.roles(), ["a", "b"]);
        assert_eq!(reloaded.role_agent("a"), "claude");
        assert!(!reloaded.role_has_worktree("a"));
        assert_eq!(reloaded.role_agent("b"), "codex");
        cleanup(&path);
    }

    #[test]
    fn set_role_creates_the_roles_table_when_missing() {
        let path = temp_file("set-role-new");
        let mut c = config("", "");
        c.repo_path = Some(path.clone());
        c.set_role("solo", Some("gemini"), Some(true)).unwrap();
        assert_eq!(config("", &std::fs::read_to_string(&path).unwrap()).roles(), ["solo"]);
        cleanup(&path);
    }

    #[test]
    fn set_role_refuses_a_roles_value_that_is_not_a_table() {
        let mut c = config("", "roles = 3\n");
        c.repo_path = Some(temp_file("bad-roles"));
        assert!(c.set_role("a", None, None).is_err());
        let mut c = config("", "[roles]\na = 1\n");
        c.repo_path = Some(temp_file("bad-role"));
        let err = c.set_role("a", None, None).unwrap_err().to_string();
        assert!(err.contains("roles.a"), "{err}");
    }

    #[test]
    fn saving_without_a_repo_file_is_an_error() {
        let mut c = config("", "");
        assert!(c.set_role("a", None, None).is_err());
    }

    #[test]
    fn remove_role_drops_only_that_role() {
        let path = temp_file("remove-role");
        let mut c = config("", "[roles.a]\n[roles.b]\n");
        c.repo_path = Some(path.clone());
        c.remove_role("a").unwrap();
        assert_eq!(c.roles(), ["b"]);
        assert_eq!(config("", &std::fs::read_to_string(&path).unwrap()).roles(), ["b"]);
        c.remove_role("missing").unwrap();
        assert_eq!(c.roles(), ["b"]);
        cleanup(&path);
    }

    #[test]
    fn load_reads_the_repo_file_and_rejects_bad_toml() {
        let path = temp_file("load");
        std::fs::write(&path, "stall_timeout = 5\n").unwrap();
        assert_eq!(Config::load(Some(path.clone())).unwrap().stall_timeout(), 5);
        std::fs::write(&path, "stall_timeout = [\n").unwrap();
        let err = Config::load(Some(path.clone())).err().unwrap().to_string();
        assert!(err.contains("is not valid TOML"), "{err}");
        std::fs::remove_file(&path).unwrap();
        assert!(Config::load(Some(path.clone())).is_ok());
        cleanup(&path);
    }

    #[test]
    fn write_global_config_picks_the_multiplexer_and_makes_folders() {
        let path = temp_file("write-global");
        let nested = path.parent().unwrap().join("a/b/config.toml");
        write_global_config(&nested, MuxKind::Tmux).unwrap();
        let text = std::fs::read_to_string(&nested).unwrap();
        assert_eq!(config(&text, "").multiplexer().unwrap(), MuxKind::Tmux);
        write_global_config(&nested, MuxKind::Herdr).unwrap();
        let text = std::fs::read_to_string(&nested).unwrap();
        assert_eq!(config(&text, "").multiplexer().unwrap(), MuxKind::Herdr);
        cleanup(&path);
    }

    #[test]
    fn saving_roles_to_global_replaces_the_old_ones_and_keeps_other_settings() {
        let path = temp_file("save-global");
        std::fs::write(&path, "paste_delay = 2\n\n[roles.old]\nagent = \"claude\"\n").unwrap();
        let mut c = config("", "[roles.new]\nagent = \"codex\"\n");
        c.global_path = path.clone();
        c.global = std::fs::read_to_string(&path).unwrap().parse().unwrap();
        c.save_roles_to_global().unwrap();
        let saved = config(&std::fs::read_to_string(&path).unwrap(), "");
        assert!((saved.paste_delay() - 2.0).abs() < f64::EPSILON);
        assert!(c.global_roles_text().contains("roles.new"));
        assert!(!c.global_roles_text().contains("roles.old"));
        cleanup(&path);
    }

    #[test]
    fn saving_roles_creates_the_global_config_when_missing() {
        let path = temp_file("save-global-new");
        let mut c = config("", "[roles.a]\nagent = \"claude\"\n");
        c.global_path = path.clone();
        c.save_roles_to_global().unwrap();
        assert!(path.exists());
        assert!(c.global_has_roles());
        cleanup(&path);
    }

    #[test]
    fn first_installed_falls_back_to_the_first_name() {
        assert_eq!(first_installed(&["crew-no-such-a", "crew-no-such-b"]), "crew-no-such-a");
        assert_eq!(first_installed(&["crew-no-such-a", "sh"]), "sh");
    }
}
