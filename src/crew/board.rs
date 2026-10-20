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

#[cfg(test)]
mod extra_tests {
    use super::*;
    use crate::crew::testutil::{fixture, ROLES};

    #[test]
    fn posts_append_with_a_header_per_message() {
        let f = fixture(ROLES);
        post(&f.ctx, "lead", "builder", "first").unwrap();
        post(&f.ctx, "builder", "lead", "second").unwrap();
        let board = f.board();
        let headers: Vec<&str> = board.lines().filter(|l| l.starts_with("### ")).collect();
        assert_eq!(headers.len(), 2);
        assert!(headers[0].ends_with("  lead → builder"));
        assert!(headers[1].ends_with("  builder → lead"));
        assert!(board.contains("\nfirst\n\n"));
        assert!(board.find("first").unwrap() < board.find("second").unwrap());
    }

    #[test]
    fn posted_messages_are_read_back_as_owed() {
        let f = fixture(ROLES);
        post(&f.ctx, "lead", "builder", "do it").unwrap();
        assert!(has_unanswered_message(&f.board(), "builder"));
        post(&f.ctx, "builder", "lead", "done").unwrap();
        assert!(!has_unanswered_message(&f.board(), "builder"));
        assert!(has_unanswered_message(&f.board(), "lead"));
    }

    #[test]
    fn message_text_tells_the_recipient_how_to_reply() {
        let f = fixture(ROLES);
        assert_eq!(message_text(&f.ctx, "lead", "hi"), "[crew] from lead: hi  (reply with: crew say lead \"...\")");
    }

    #[test]
    fn messages_from_the_human_have_no_reply_hint() {
        let f = fixture(ROLES);
        assert_eq!(message_text(&f.ctx, "human", "hi"), "[crew] from the human: hi");
    }

    #[test]
    fn long_messages_are_cut_with_a_pointer_to_the_board() {
        let f = fixture(ROLES);
        let text = message_text(&f.ctx, "lead", &"é".repeat(601));
        assert!(text.contains(&"é".repeat(600)));
        assert!(!text.contains(&"é".repeat(601)));
        assert!(text.contains(&format!("full message in {}", f.ctx.file("board.md").display())));
        assert!(!message_text(&f.ctx, "lead", &"x".repeat(600)).contains("full message"));
    }

    #[test]
    fn all_goes_to_everyone_but_the_sender() {
        let f = fixture(ROLES);
        assert_eq!(recipients(&f.ctx, "all", "builder"), ["lead", "reviewer"]);
        assert_eq!(recipients(&f.ctx, "all", "human"), ["lead", "builder", "reviewer"]);
    }

    #[test]
    fn a_named_recipient_is_used_as_given() {
        let f = fixture(ROLES);
        assert_eq!(recipients(&f.ctx, "reviewer", "lead"), ["reviewer"]);
    }

    #[test]
    fn unparsable_headers_are_ignored() {
        let board = "### no route here\n### 2026-10-20 03:00  lead builder\n### 2026-10-20 03:01  lead → builder\n";
        assert!(has_unanswered_message(board, "builder"));
        assert!(!has_unanswered_message("", "builder"));
    }
}
