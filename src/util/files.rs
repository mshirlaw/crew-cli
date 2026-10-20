use anyhow::{Context, Result};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

pub fn expand_home(path: &str) -> PathBuf {
    expand_home_in(path, &home())
}

fn expand_home_in(path: &str, home: &Path) -> PathBuf {
    if path == "~" {
        home.to_path_buf()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(path)
    }
}

pub fn tilde(path: &Path) -> String {
    tilde_in(path, &home())
}

fn tilde_in(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if !home.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

pub fn crew_home() -> PathBuf {
    crew_home_from(std::env::var_os("CREW_HOME"), std::env::var_os("XDG_CONFIG_HOME"), &home())
}

fn crew_home_from(crew_home: Option<OsString>, xdg_config_home: Option<OsString>, home: &Path) -> PathBuf {
    if let Some(dir) = crew_home.filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    if let Some(dir) = xdg_config_home.filter(|v| !v.is_empty()) {
        return PathBuf::from(dir).join("crew");
    }
    home.join(".config").join("crew")
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
    fn tilde_is_expanded_only_at_the_start() {
        let home = Path::new("/home/me");
        assert_eq!(expand_home_in("~", home), home);
        assert_eq!(expand_home_in("~/work/x", home), Path::new("/home/me/work/x"));
        assert_eq!(expand_home_in("~other/x", home), Path::new("~other/x"));
        assert_eq!(expand_home_in("/abs/~/x", home), Path::new("/abs/~/x"));
    }

    #[test]
    fn paths_under_home_are_shortened() {
        let home = Path::new("/home/me");
        assert_eq!(tilde_in(Path::new("/home/me/a/b"), home), "~/a/b");
        assert_eq!(tilde_in(Path::new("/elsewhere/a"), home), "/elsewhere/a");
        assert_eq!(tilde_in(Path::new("/home/mean/a"), home), "/home/mean/a");
    }

    #[test]
    fn an_empty_home_shortens_nothing() {
        assert_eq!(tilde_in(Path::new("/a/b"), Path::new("")), "/a/b");
    }

    #[test]
    fn crew_home_prefers_crew_home_then_xdg_then_dot_config() {
        let home = Path::new("/home/me");
        let set = |v: &str| Some(OsString::from(v));
        assert_eq!(crew_home_from(set("/c"), set("/x"), home), Path::new("/c"));
        assert_eq!(crew_home_from(None, set("/x"), home), Path::new("/x/crew"));
        assert_eq!(crew_home_from(None, None, home), Path::new("/home/me/.config/crew"));
    }

    #[test]
    fn empty_variables_are_ignored() {
        let home = Path::new("/home/me");
        let empty = || Some(OsString::new());
        assert_eq!(crew_home_from(empty(), empty(), home), Path::new("/home/me/.config/crew"));
    }

    #[test]
    fn write_atomic_replaces_the_file_and_leaves_no_temp_file() {
        let dir = std::env::temp_dir().join(format!("crew-files-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.txt");
        write_atomic(&path, "one").unwrap();
        write_atomic(&path, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
        assert!(!dir.join("out.txt.tmp").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn write_atomic_reports_the_path_it_could_not_write() {
        let path = Path::new("/nonexistent-crew-dir/out.txt");
        let error = write_atomic(path, "x").unwrap_err().to_string();
        assert!(error.contains("/nonexistent-crew-dir/out.txt.tmp"), "{error}");
    }
}
