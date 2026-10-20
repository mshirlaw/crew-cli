use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

pub fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        home()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home().join(rest)
    } else {
        PathBuf::from(path)
    }
}

pub fn tilde(path: &Path) -> String {
    match path.strip_prefix(home()) {
        Ok(rest) if !home().as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

pub fn crew_home() -> PathBuf {
    if let Some(dir) = std::env::var_os("CREW_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir).join("crew");
    }
    home().join(".config").join("crew")
}

// Write then rename, so a reader never sees a half-written file.
pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let tmp = path.with_file_name(name);
    std::fs::write(&tmp, contents).with_context(|| format!("could not write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("could not write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_home_only_expands_home_prefix() {
        assert_eq!(expand_home("relative/path"), PathBuf::from("relative/path"));
        assert_eq!(expand_home("~/crew"), home().join("crew"));
        assert_eq!(expand_home("~other/crew"), PathBuf::from("~other/crew"));
    }

    #[test]
    fn atomic_write_replaces_contents_without_leaving_temp_file() {
        let path = std::env::temp_dir().join(format!("crew-files-test-{}", std::process::id()));
        write_atomic(&path, "new contents").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new contents");
        assert!(!path.with_file_name(format!("{}.tmp", path.file_name().unwrap().to_string_lossy())).exists());
        let _ = std::fs::remove_file(path);
    }
}
