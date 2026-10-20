use std::path::{Path, PathBuf};

pub struct State {
    dir: PathBuf,
}

impl State {
    pub fn new(dir: PathBuf) -> State {
        State { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file(&self) -> PathBuf {
        self.dir.join("session")
    }

    pub fn get(&self, key: &str) -> Option<String> {
        let text = std::fs::read_to_string(self.file()).ok()?;
        text.lines().find_map(|line| {
            let (k, v) = line.split_once(' ')?;
            (k == key && !v.is_empty()).then(|| v.to_string())
        })
    }

    // An empty value removes the key.
    pub fn set(&self, key: &str, value: &str) {
        let _ = std::fs::create_dir_all(&self.dir);
        let text = std::fs::read_to_string(self.file()).unwrap_or_default();
        let mut lines: Vec<String> = text
            .lines()
            .filter(|line| line.split_once(' ').map(|(k, _)| k) != Some(key) && *line != key)
            .map(String::from)
            .collect();
        if !value.is_empty() {
            lines.push(format!("{key} {value}"));
        }
        let mut out = lines.join("\n");
        out.push('\n');
        let _ = crate::util::write_atomic(&self.file(), &out);
    }

    pub fn clear(&self) {
        let _ = std::fs::remove_file(self.file());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_and_remove() {
        let dir = std::env::temp_dir().join(format!("crew-state-test-{}", std::process::id()));
        let state = State::new(dir.clone());
        state.set("ws", "w1");
        state.set("pane:lead", "%1");
        state.set("ws", "w2");
        assert_eq!(state.get("ws").as_deref(), Some("w2"));
        assert_eq!(state.get("pane:lead").as_deref(), Some("%1"));
        state.set("pane:lead", "");
        assert_eq!(state.get("pane:lead"), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod extra_tests {
    use crate::crew::testutil::fixture;

    #[test]
    fn missing_file_has_no_keys() {
        let f = fixture("");
        assert_eq!(f.ctx.state.get("ws"), None);
    }

    #[test]
    fn keys_are_matched_whole_and_values_keep_spaces() {
        let f = fixture("");
        f.ctx.state.set("pane", "a");
        f.ctx.state.set("pane:lead", "b c");
        assert_eq!(f.ctx.state.get("pane").as_deref(), Some("a"));
        assert_eq!(f.ctx.state.get("pane:lead").as_deref(), Some("b c"));
        f.ctx.state.set("pane", "");
        assert_eq!(f.ctx.state.get("pane"), None);
        assert_eq!(f.ctx.state.get("pane:lead").as_deref(), Some("b c"));
    }

    #[test]
    fn clear_forgets_everything() {
        let f = fixture("");
        f.ctx.state.set("ws", "w1");
        f.ctx.state.clear();
        assert_eq!(f.ctx.state.get("ws"), None);
        f.ctx.state.clear();
    }
}
