//! Small helpers: running commands, paths, time, hashing.

use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

/// Succeeds if a program can be run: a name on the PATH, or a path to an executable file.
pub fn has(program: &str) -> bool {
    if program.contains('/') {
        return is_executable(Path::new(program));
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|dir| is_executable(&dir.join(program))))
        .unwrap_or(false)
}

/// Succeeds if a path is an executable file.
fn is_executable(path: &Path) -> bool {
    path.metadata().map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

/// Runs a command quietly and reports whether it succeeded.
pub fn run_ok<S: AsRef<OsStr>>(program: &str, args: &[S]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Runs a command and returns its output if it succeeded.
pub fn run_out<S: AsRef<OsStr>>(program: &str, args: &[S]) -> Option<String> {
    let out = Command::new(program).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Runs a command and returns whether it succeeded, plus its output and errors together.
pub fn run_all<S: AsRef<OsStr>>(program: &str, args: &[S]) -> (bool, String) {
    match Command::new(program).args(args).stdin(Stdio::null()).output() {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            (out.status.success(), text)
        }
        Err(e) => (false, e.to_string()),
    }
}

/// Starts a program in the background, detached from this terminal, with its output in a log file.
/// Returns its process id.
pub fn spawn_detached<S: AsRef<OsStr>>(
    program: &OsStr,
    args: &[S],
    envs: &[(&str, &OsStr)],
    log: &Path,
) -> Result<u32> {
    use std::os::unix::process::CommandExt;
    let log_file = std::fs::File::create(log).with_context(|| format!("could not create {}", log.display()))?;
    let child = Command::new("nohup")
        .arg(program)
        .args(args)
        .envs(envs.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(log_file.try_clone()?)
        .stderr(log_file)
        .process_group(0)
        .spawn()
        .with_context(|| format!("could not start {}", program.to_string_lossy()))?;
    Ok(child.id())
}

/// Returns the home folder.
pub fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

/// Turns a leading ~ into the home folder.
pub fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        home()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home().join(rest)
    } else {
        PathBuf::from(path)
    }
}

/// Shows a path with the home folder written as ~.
pub fn tilde(path: &Path) -> String {
    match path.strip_prefix(home()) {
        Ok(rest) if !home().as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

/// Joins words into one shell-safe command line.
pub fn shell_join(words: &[String]) -> String {
    shlex::try_join(words.iter().map(String::as_str)).unwrap_or_else(|_| words.join(" "))
}

/// Seconds since the Unix epoch.
pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Formats a number of seconds as "45s" or "12 min".
pub fn duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{} min", secs / 60)
    }
}

/// A short stable fingerprint of some text (FNV-1a), the same on every run and Rust version.
pub fn fingerprint(text: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in text.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("{hash:08x}")
}

/// Writes a file by writing a temporary file next to it and renaming, so readers never see half a file.
pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let tmp = path.with_file_name(name);
    std::fs::write(&tmp, contents).with_context(|| format!("could not write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("could not write {}", path.display()))
}

/// Opens a file in the user's editor ($VISUAL, then $EDITOR, then vi).
pub fn edit_file(path: &Path) -> Result<()> {
    let editor = std::env::var("VISUAL").or_else(|_| std::env::var("EDITOR")).unwrap_or_else(|_| "vi".to_string());
    Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("sh")
        .arg(path)
        .status()
        .with_context(|| format!("could not start editor {editor}"))?;
    Ok(())
}

/// Returns crew's personal settings folder: $CREW_HOME, else $XDG_CONFIG_HOME/crew, else ~/.config/crew.
pub fn crew_home() -> PathBuf {
    if let Some(dir) = std::env::var_os("CREW_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir).join("crew");
    }
    home().join(".config").join("crew")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_naturally() {
        assert_eq!(duration(45), "45s");
        assert_eq!(duration(600), "10 min");
    }

    #[test]
    fn fingerprints_are_stable() {
        assert_eq!(fingerprint(""), "811c9dc5");
        assert_ne!(fingerprint("a"), fingerprint("b"));
    }

    #[test]
    fn shell_join_quotes_spaces() {
        let words = vec!["claude".to_string(), "--add-dir".to_string(), "/my app/.crew".to_string()];
        assert_eq!(shell_join(&words), "claude --add-dir '/my app/.crew'");
    }
}
