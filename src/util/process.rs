use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn has(program: &str) -> bool {
    if program.contains('/') {
        return is_executable(Path::new(program));
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| is_executable(&dir.join(program))))
}

fn is_executable(path: &Path) -> bool {
    path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

pub fn run_ok<S: AsRef<OsStr>>(program: &str, args: &[S]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

pub fn run_out<S: AsRef<OsStr>>(program: &str, args: &[S]) -> Option<String> {
    let out = Command::new(program).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

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

// nohup plus its own process group, so closing the terminal doesn't take it down. Returns the pid.
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

// Runs through sh so $EDITOR can include arguments, e.g. "code -w".
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
