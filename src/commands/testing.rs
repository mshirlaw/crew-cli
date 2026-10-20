use crate::crew::config::Config;
use crate::crew::context::Ctx;
use crate::crew::state::State;
use crate::mux;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Once;

pub fn scratch_dir() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "crew-cmd-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// One empty CREW_HOME for the whole test process, so no test reads or writes the developer's real config.
pub fn crew_home() -> PathBuf {
    static ONCE: Once = Once::new();
    let dir = std::env::temp_dir().join(format!("crew-cmd-test-home-{}", std::process::id()));
    ONCE.call_once(|| {
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("CREW_HOME", &dir);
    });
    dir
}

// A crew in a fresh folder. tmux is used because nothing is running in it, so agents are never reachable.
pub fn crew_ctx() -> Ctx {
    ctx_in(&scratch_dir(), Some("multiplexer = \"tmux\"\n\n[roles.lead]\nagent = \"claude\"\nworktree = false\n\n[roles.builder]\nagent = \"codex\"\n"))
}

pub fn ctx_in(root: &Path, config: Option<&str>) -> Ctx {
    crew_home();
    let crew = root.join(".crew");
    if let Some(config) = config {
        std::fs::create_dir_all(&crew).unwrap();
        std::fs::write(crew.join("config.toml"), config).unwrap();
    }
    let config = Config::load(Some(crew.join("config.toml"))).unwrap();
    let mux = mux::make(config.multiplexer().unwrap());
    let slug = crate::crew::context::slugify(&root.file_name().unwrap().to_string_lossy());
    Ctx { state: State::new(crew.join("state")), root: root.to_path_buf(), crew, key: slug.clone(), slug, config, mux }
}
