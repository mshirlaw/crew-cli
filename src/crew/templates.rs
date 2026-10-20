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
    fn default_roles_preserve_role_agents_and_worktree_policy() {
        let text = default_roles("claude", "codex", "gemini");
        assert!(text.contains("[roles.lead]"));
        assert!(text.contains("agent = \"claude\"\nworktree = false"));
        assert!(text.contains("[roles.builder]\nagent = \"codex\"\nworktree = true"));
        assert!(text.contains("[roles.reviewer]\nagent = \"gemini\"\nworktree = true"));
    }

    #[test]
    fn unknown_role_template_explains_what_to_edit() {
        let text = role_template("qa");
        assert!(text.contains("# Role: qa"));
        assert!(text.contains("crew role edit qa"));
    }
}
