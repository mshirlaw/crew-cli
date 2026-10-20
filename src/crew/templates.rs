use crate::util::{crew_home, tilde};

// The sample config doubles as the template `crew config init` writes.
pub const GLOBAL_CONFIG: &str = include_str!("../../config.example.toml");

pub const PROTOCOL_RULES: &str = include_str!("templates/protocol-rules.md");

pub fn role_template(role: &str) -> String {
    let saved = crew_home().join("roles").join(format!("{role}.md"));
    if let Ok(text) = std::fs::read_to_string(saved) {
        return text;
    }
    match role {
        "lead" => include_str!("templates/lead.md").to_string(),
        "builder" => include_str!("templates/builder.md").to_string(),
        "reviewer" => include_str!("templates/reviewer.md").to_string(),
        "tester" => include_str!("templates/tester.md").to_string(),
        _ => format!(
            "# Role: {role}\n\nDescribe what this agent is responsible for, what it should and should not touch,\n\
             and who it hands work to. Edit with: crew role edit {role}\n"
        ),
    }
}

pub fn repo_config_header(base_branch: &str) -> String {
    format!(
        "# crew settings for this repo. Anything here overrides {}.\n\
         # Run \"crew config\" to see the settings in effect. Settings go above the first [section].\n\
         \n\
         base_branch = \"{base_branch}\"\n\
         \n\
         # multiplexer = \"tmux\"   # uncomment to use a different multiplexer for this repo\n",
        tilde(&crew_home().join("config.toml"))
    )
}

pub fn default_roles(lead: &str, builder: &str, reviewer: &str) -> String {
    format!(
        "\n[roles.lead]\nagent = \"{lead}\"\nworktree = false\n\
         \n[roles.builder]\nagent = \"{builder}\"\nworktree = true\n\
         \n[roles.reviewer]\nagent = \"{reviewer}\"\nworktree = true\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_config_is_valid_toml() {
        assert!(GLOBAL_CONFIG.parse::<toml_edit::DocumentMut>().is_ok());
    }

    #[test]
    fn built_in_roles_have_templates() {
        for role in ["lead", "builder", "reviewer", "tester"] {
            assert!(role_template(role).starts_with("# Role:"), "{role}");
        }
    }

    #[test]
    fn unknown_roles_get_a_stub_that_says_how_to_edit_it() {
        let text = role_template("zz-no-such-role");
        assert!(text.starts_with("# Role: zz-no-such-role"));
        assert!(text.contains("crew role edit zz-no-such-role"));
    }

    #[test]
    fn repo_config_header_sets_the_base_branch_above_any_section() {
        let text = repo_config_header("develop");
        assert!(text.contains("\nbase_branch = \"develop\"\n"));
        let doc: toml_edit::DocumentMut = text.parse().unwrap();
        assert_eq!(doc["base_branch"].as_str(), Some("develop"));
    }

    #[test]
    fn default_roles_name_each_agent_and_the_lead_has_no_worktree() {
        let doc: toml_edit::DocumentMut = default_roles("claude", "codex", "gemini").parse().unwrap();
        assert_eq!(doc["roles"]["lead"]["agent"].as_str(), Some("claude"));
        assert_eq!(doc["roles"]["builder"]["agent"].as_str(), Some("codex"));
        assert_eq!(doc["roles"]["reviewer"]["agent"].as_str(), Some("gemini"));
        assert_eq!(doc["roles"]["lead"]["worktree"].as_bool(), Some(false));
        assert_eq!(doc["roles"]["builder"]["worktree"].as_bool(), Some(true));
    }

    #[test]
    fn protocol_rules_mention_the_commands_agents_use() {
        for needle in ["crew say", "crew task add", "notes/"] {
            assert!(PROTOCOL_RULES.contains(needle), "{needle}");
        }
    }
}
