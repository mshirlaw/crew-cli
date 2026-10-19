use super::templates;
use crate::util::ui;
use anyhow::Result;
use std::fmt::Write;
use std::path::Path;

// crew 0.2 kept roles in .crew/crew.conf as "<role> <agent> <worktree yes|no>" lines.
pub fn migrate_old_roster(crew: &Path) -> Result<()> {
    let old = crew.join("crew.conf");
    let new = crew.join("config.toml");
    if !old.is_file() || new.exists() {
        return Ok(());
    }
    let base = std::fs::read_to_string(crew.join("base")).map_or_else(|_| "main".into(), |s| s.trim().to_string());
    let mut text = templates::repo_config_header(&base);
    for line in std::fs::read_to_string(&old)?.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 2 || fields[0].starts_with('#') {
            continue;
        }
        let worktree = fields.get(2) != Some(&"no");
        let _ = write!(text, "\n[roles.{}]\nagent = \"{}\"\nworktree = {worktree}\n", fields[0], fields[1]);
    }
    std::fs::write(&new, text)?;
    std::fs::rename(&old, crew.join("crew.conf.old"))?;
    ui::info("converted .crew/crew.conf to .crew/config.toml");
    Ok(())
}
