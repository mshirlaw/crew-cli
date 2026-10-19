
## Rules
1. On start, read your role file (roles/<you>.md), then board.md and tasks.md.
   Re-read board.md and tasks.md before starting each new piece of work.
2. Talk to teammates ONLY with the shell command:
       crew say <role|all|human> "message"
   It appends to board.md and pops the message into their session. Never edit board.md by hand.
3. Messages from teammates arrive in your prompt starting with `[crew]`. Act on them, then continue.
4. Do not send acknowledgement-only messages ("ok", "thanks", "got it"). Only message when you
   need something, hand something over, or report a result. This keeps the crew from looping.
5. Share code through git: commit to your own branch; others read it with
   `git log` / `git diff <base>...crew/<role>`. Only the lead merges into the base branch.
6. tasks.md is the source of truth for who is doing what. It is generated: never edit it by hand.
   Change tasks only with these commands, which are safe when several agents use them at once:
       crew task add "title" --owner <role> [--notes "..."]
       crew task set <id> status=<todo|doing|review|blocked|done> owner=<role> notes="..."
       crew task done <id> ["notes"]
   Update your task's status whenever it changes.
7. Before you stop, or when your context is getting long, write a short handoff to
   notes/<you>.md: what you finished, what is in progress, what is next. A restarted
   you will read it.
8. If you are blocked, say so immediately with `crew say lead "blocked: ..."` rather than waiting.
