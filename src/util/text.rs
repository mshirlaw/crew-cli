use std::time::{SystemTime, UNIX_EPOCH};

pub fn shell_join(words: &[String]) -> String {
    shlex::try_join(words.iter().map(String::as_str)).unwrap_or_else(|_| words.join(" "))
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub fn duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else {
        format!("{} min", secs / 60)
    }
}

// FNV-1a: stable across runs and Rust versions, unlike std's hasher. Used in stored files and paths.
pub fn fingerprint(text: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in text.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("{hash:08x}")
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
    fn durations_switch_to_minutes_at_sixty_seconds() {
        assert_eq!(duration(0), "0s");
        assert_eq!(duration(59), "59s");
        assert_eq!(duration(60), "1 min");
        assert_eq!(duration(119), "1 min");
    }

    #[test]
    fn the_clock_is_after_2020() {
        assert!(now_secs() > 1_577_836_800);
    }

    #[test]
    fn shell_join_of_nothing_is_empty() {
        assert_eq!(shell_join(&[]), "");
    }

    #[test]
    fn fingerprints_are_stable() {
        assert_eq!(fingerprint(""), "811c9dc5");
        assert_eq!(fingerprint("a"), "e40c292c");
        assert_ne!(fingerprint("a"), fingerprint("b"));
        assert_eq!(fingerprint("any text").len(), 8);
    }

    #[test]
    fn shell_join_quotes_spaces() {
        let words = vec!["claude".to_string(), "--add-dir".to_string(), "/my app/.crew".to_string()];
        assert_eq!(shell_join(&words), "claude --add-dir '/my app/.crew'");
    }
}
