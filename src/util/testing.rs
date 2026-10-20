use std::cell::RefCell;
use std::ffi::OsString;
use std::path::PathBuf;

thread_local! {
    static BIN_DIR: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

// While a Fake is alive on this thread, programs run through util::process come from its folder
// (and anything else isn't found), so tests can run in parallel without touching PATH.
pub fn resolve(program: &str) -> OsString {
    BIN_DIR.with(|dir| match &*dir.borrow() {
        Some(_) if matches!(program, "nohup" | "sh") => OsString::from(program),
        Some(dir) => dir.join(program).into_os_string(),
        None => OsString::from(program),
    })
}

pub struct Fake {
    dir: PathBuf,
}

impl Fake {
    pub fn new() -> Fake {
        let dir = crate::commands::testing::scratch_dir();
        BIN_DIR.with(|d| *d.borrow_mut() = Some(dir.clone()));
        Fake { dir }
    }

    // Logs its arguments to <name>.log, then runs `body`. $D is the fake folder.
    pub fn program(&self, name: &str, body: &str) -> &Fake {
        use std::os::unix::fs::PermissionsExt;
        let script = format!("#!/bin/sh\nD=\"$(dirname \"$0\")\"\necho \"$*\" >> \"$D/{name}.log\"\n{body}\n");
        let path = self.dir.join(name);
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        self
    }

    pub fn log(&self, name: &str) -> Vec<String> {
        std::fs::read_to_string(self.dir.join(format!("{name}.log")))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    pub fn path(&self) -> &std::path::Path {
        &self.dir
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        BIN_DIR.with(|d| *d.borrow_mut() = None);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
