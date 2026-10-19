use super::context::Ctx;
use anyhow::{Context, Result};
use std::io::Write;

pub fn post(ctx: &Ctx, from: &str, to: &str, message: &str) -> Result<()> {
    let path = ctx.file("board.md");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("could not open {}", path.display()))?;
    let time = chrono::Local::now().format("%Y-%m-%d %H:%M");
    // A single write, so messages posted at the same moment don't interleave.
    file.write_all(format!("### {time}  {from} → {to}\n{message}\n\n").as_bytes())?;
    Ok(())
}

pub fn message_text(ctx: &Ctx, from: &str, message: &str) -> String {
    let message = if message.chars().count() > 600 {
        let short: String = message.chars().take(600).collect();
        format!("{short}… (full message in {})", ctx.file("board.md").display())
    } else {
        message.to_string()
    };
    if from == "human" {
        format!("[crew] from the human: {message}")
    } else {
        format!("[crew] from {from}: {message}  (reply with: crew say {from} \"...\")")
    }
}

pub fn recipients(ctx: &Ctx, to: &str, from: &str) -> Vec<String> {
    if to == "all" {
        ctx.roles().into_iter().filter(|r| r != from).collect()
    } else {
        vec![to.to_string()]
    }
}

// True if someone messaged the role after its own last post. Messages from crew itself don't count.
pub fn has_unanswered_message(board: &str, role: &str) -> bool {
    let mut last_in = 0;
    let mut last_out = 0;
    let headers = board.lines().filter_map(|line| line.strip_prefix("### "));
    for (n, header) in headers.enumerate() {
        // e.g. "2026-10-20 03:00  builder → reviewer"
        let Some((_, route)) = header.split_once("  ") else { continue };
        let Some((from, to)) = route.split_once(" → ") else { continue };
        if from == role {
            last_out = n + 1;
        } else if from != "crew" && (to == role || to == "all") {
            last_in = n + 1;
        }
    }
    last_in > last_out
}

#[cfg(test)]
mod tests {
    use super::has_unanswered_message;

    const BOARD: &str = "# Crew board\n\n\
        ### 2026-10-20 03:00  crew → all\nCrew started\n\n\
        ### 2026-10-20 03:01  lead → builder\nbuild it\n\n\
        ### 2026-10-20 03:02  builder → reviewer\nready\n\n\
        ### 2026-10-20 03:03  lead → all\nstandup\n\n";

    #[test]
    fn replies_clear_owed_messages() {
        assert!(has_unanswered_message(BOARD, "reviewer"));
        assert!(has_unanswered_message(BOARD, "builder"));
        assert!(!has_unanswered_message(BOARD, "lead"));
    }

    #[test]
    fn crew_messages_are_not_owed() {
        assert!(!has_unanswered_message("### 2026-10-20 03:00  crew → lead\nalert\n", "lead"));
    }
}
