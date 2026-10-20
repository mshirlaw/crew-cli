use anyhow::{Context, Result};
use std::ffi::{OsStr, OsString};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

#[cfg(test)]
use super::testing::resolve;

#[cfg(not(test))]
fn resolve(program: &str) -> OsString {
    OsString::from(program)
}

pub fn has(program: &str) -> bool {
    let program = resolve(program);
    let program = program.to_string_lossy();
    if program.contains('/') {
        return is_executable(Path::new(&*program));
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| is_executable(&dir.join(&*program))))
}

fn is_executable(path: &Path) -> bool {
    path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

pub fn run_ok<S: AsRef<OsStr>>(program: &str, args: &[S]) -> bool {
    Command::new(resolve(program))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

pub fn run_out<S: AsRef<OsStr>>(program: &str, args: &[S]) -> Option<String> {
    let out = Command::new(resolve(program)).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn run_all<S: AsRef<OsStr>>(program: &str, args: &[S]) -> (bool, String) {
    match Command::new(resolve(program)).args(args).stdin(Stdio::null()).output() {
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
        .arg(resolve(&program.to_string_lossy()))
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
    run_editor(&editor, path)
}

fn run_editor(editor: &str, path: &Path) -> Result<()> {
    Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("sh")
        .arg(path)
        .status()
        .with_context(|| format!("could not start editor {editor}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("crew-process-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn has_finds_programs_on_the_path() {
        assert!(has("sh"));
        assert!(!has("crew-no-such-program"));
    }

    #[test]
    fn has_checks_paths_directly_and_needs_the_executable_bit() {
        let dir = scratch("has");
        let plain = dir.join("plain");
        std::fs::write(&plain, "x").unwrap();
        assert!(!has(plain.to_str().unwrap()));
        std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(has(plain.to_str().unwrap()));
        assert!(!has(dir.to_str().unwrap()), "a directory is not a program");
        assert!(!has("/nonexistent/program"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn run_ok_reports_the_exit_status() {
        assert!(run_ok("sh", &["-c", "exit 0"]));
        assert!(!run_ok("sh", &["-c", "exit 3"]));
        assert!(!run_ok("crew-no-such-program", &["x"]));
    }

    #[test]
    fn run_out_returns_stdout_only_on_success() {
        assert_eq!(run_out("sh", &["-c", "echo hi; echo err >&2"]).as_deref(), Some("hi\n"));
        assert_eq!(run_out("sh", &["-c", "echo hi; exit 1"]), None);
        assert_eq!(run_out("crew-no-such-program", &["x"]), None);
    }

    #[test]
    fn run_all_combines_stdout_and_stderr() {
        let (ok, text) = run_all("sh", &["-c", "echo out; echo err >&2"]);
        assert!(ok);
        assert!(text.contains("out") && text.contains("err"), "{text}");
        let (ok, text) = run_all("sh", &["-c", "echo boom >&2; exit 1"]);
        assert!(!ok);
        assert!(text.contains("boom"));
    }

    #[test]
    fn run_all_returns_the_error_when_the_program_is_missing() {
        let (ok, text) = run_all("crew-no-such-program", &["x"]);
        assert!(!ok);
        assert!(!text.is_empty());
    }

    #[test]
    fn spawn_detached_passes_arguments_and_environment_and_logs_output() {
        let dir = scratch("spawn");
        let log = dir.join("out.log");
        let value = OsStr::new("from-test");
        spawn_detached(
            OsStr::new("sh"),
            &["-c", "echo \"$CREW_TEST_VALUE $0\"", "arg"],
            &[("CREW_TEST_VALUE", value)],
            &log,
        )
        .unwrap();
        let mut text = String::new();
        for _ in 0..100 {
            text = std::fs::read_to_string(&log).unwrap_or_default();
            if text.contains('\n') {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(text.trim(), "from-test arg");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn spawn_detached_fails_when_the_log_cannot_be_created() {
        let error = spawn_detached(OsStr::new("true"), &[] as &[&str], &[], Path::new("/nonexistent-dir/x.log"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("could not create"), "{error}");
    }

    #[test]
    fn the_editor_command_may_carry_arguments() {
        let dir = scratch("editor");
        let file = dir.join("note.md");
        run_editor("printf edited >", &file).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "edited");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_editor_gets_a_path_with_spaces_as_one_argument() {
        let dir = scratch("spaces");
        let file = dir.join("my note.md");
        run_editor("printf x >", &file).unwrap();
        assert!(file.is_file());
        let _ = std::fs::remove_dir_all(dir);
    }
}
