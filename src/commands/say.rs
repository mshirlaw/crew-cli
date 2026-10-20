use crate::crew::board;
use crate::crew::context::Ctx;
use crate::util::ui;
use anyhow::{bail, Result};

// The sender is CREW_ROLE, which is set in each agent's pane; anyone else is "human".
pub fn say(ctx: &Ctx, to: &str, words: &[String]) -> Result<()> {
    ctx.require_crew()?;
    let message = words.join(" ");
    if message.is_empty() {
        bail!("usage: crew say <role|all|human> \"message\"");
    }
    let from = std::env::var("CREW_ROLE").ok().filter(|r| !r.is_empty()).unwrap_or_else(|| "human".to_string());
    if to != "all" && to != "human" && !ctx.role_exists(to) {
        bail!("no role '{to}' (roles: {})", ctx.roles().join(" "));
    }
    board::post(ctx, &from, to, &message)?;
    if to == "human" {
        ui::ok("posted for the human");
        return Ok(());
    }
    if !ctx.mux.ready(ctx) {
        ui::ok("posted to board (crew not running, not delivered live)");
        return Ok(());
    }
    let text = board::message_text(ctx, &from, &message);
    let mut delivered = 0;
    let mut missed = Vec::new();
    for role in board::recipients(ctx, to, &from) {
        if ctx.mux.deliver(ctx, &role, &text) {
            delivered += 1;
        } else {
            missed.push(role);
        }
    }
    ui::ok(format!("posted to board, delivered to {delivered} agent(s)"));
    if !missed.is_empty() {
        ui::warn(format!("not delivered live to: {} (busy or not running). It's on the board.", missed.join(" ")));
    }
    Ok(())
}

pub fn show_board(ctx: &Ctx, lines: usize) -> Result<()> {
    ctx.require_crew()?;
    let text = std::fs::read_to_string(ctx.file("board.md")).unwrap_or_default();
    for line in last_lines(&text, lines) {
        println!("{line}");
    }
    Ok(())
}

fn last_lines(text: &str, count: usize) -> Vec<&str> {
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(count)..].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::testing::{crew_ctx, ctx_in, scratch_dir};

    fn words(text: &str) -> Vec<String> {
        text.split(' ').map(String::from).collect()
    }

    fn board(ctx: &Ctx) -> String {
        std::fs::read_to_string(ctx.file("board.md")).unwrap_or_default()
    }

    #[test]
    fn a_message_for_a_role_is_posted_even_when_nothing_is_running() {
        let ctx = crew_ctx();
        say(&ctx, "builder", &words("please build it")).unwrap();
        let text = board(&ctx);
        assert!(text.contains("→ builder\nplease build it\n"), "{text}");
    }

    #[test]
    fn a_message_for_the_human_is_posted() {
        let ctx = crew_ctx();
        say(&ctx, "human", &words("need a decision")).unwrap();
        assert!(board(&ctx).contains("→ human\nneed a decision"));
    }

    #[test]
    fn a_broadcast_is_posted_once() {
        let ctx = crew_ctx();
        say(&ctx, "all", &words("standup")).unwrap();
        assert_eq!(board(&ctx).matches("standup").count(), 1);
    }

    #[test]
    fn unknown_roles_are_refused_and_nothing_is_posted() {
        let ctx = crew_ctx();
        let message = say(&ctx, "ghost", &words("hi")).unwrap_err().to_string();
        assert!(message.contains("no role 'ghost'") && message.contains("lead builder"), "{message}");
        assert_eq!(board(&ctx), "");
    }

    #[test]
    fn an_empty_message_is_refused() {
        let ctx = crew_ctx();
        assert!(say(&ctx, "lead", &[]).unwrap_err().to_string().starts_with("usage:"));
        assert_eq!(board(&ctx), "");
    }

    #[test]
    fn say_and_board_need_a_crew() {
        let ctx = ctx_in(&scratch_dir(), None);
        assert!(say(&ctx, "lead", &words("hi")).unwrap_err().to_string().contains("crew setup"));
        assert!(show_board(&ctx, 5).unwrap_err().to_string().contains("crew setup"));
    }

    #[test]
    fn board_shows_the_last_lines_only() {
        let text = "a\nb\nc\nd\n";
        assert_eq!(last_lines(text, 2), ["c", "d"]);
        assert_eq!(last_lines(text, 10), ["a", "b", "c", "d"]);
        assert_eq!(last_lines(text, 0), Vec::<&str>::new());
        assert_eq!(last_lines("", 3), Vec::<&str>::new());
    }

    #[test]
    fn board_of_a_crew_without_messages_is_not_an_error() {
        let ctx = crew_ctx();
        show_board(&ctx, 40).unwrap();
    }
}
