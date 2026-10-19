# AGENTS.md

Guidance for AI coding agents (and humans) working on this repo: what crew is, how it's built, and the decisions behind it. Read this before changing anything.

## What crew is

`crew` is a small Rust command-line tool that starts a team of AI coding agents (Claude Code, Codex, Gemini, or any CLI) in herdr or tmux. Each agent gets a role, its own git worktree, and a shared message board. The human talks to the lead; agents message each other with `crew say` and track work with `crew task`.

## Layout

| Path | What it is |
|---|---|
| `src/main.rs` | command-line interface (clap) and dispatch |
| `src/context.rs` | finds the repo and `.crew/`, loads config, picks the multiplexer |
| `src/config.rs` | global + repo TOML config (toml_edit), role and agent settings |
| `src/mux/` | the `Mux` trait plus `herdr.rs` and `tmux.rs` |
| `src/lifecycle.rs` | start, revive, restart, stop, teardown, worktrees |
| `src/agents.rs` | kickoff prompt and each agent's command line |
| `src/board.rs` | `crew say`, the board, unanswered-message detection |
| `src/tasks.rs` | `crew task`, `tasks.tsv` + generated `tasks.md`, file lock |
| `src/watch.rs` | stall watcher |
| `src/roles.rs`, `src/setup.rs`, `src/commands.rs` | role commands; setup and PROTOCOL.md; status, doctor, config |
| `src/templates/` | role instructions and protocol rules (markdown, compiled in) |
| `config.example.toml` | sample global config; compiled in and written by `crew config init` |
| `install.sh` | bash installer: downloads a release binary, or builds from a clone |
| `test/run.sh` | end-to-end tests (stand-in agents, real tmux, fake herdr) |
| `.github/workflows/` | `test.yml` (fmt, clippy, unit, shellcheck, end-to-end on Linux and macOS), `release.yml` |

In a repo where crew runs, everything lives in `.crew/`: `config.toml`, `roles/*.md`, `PROTOCOL.md` (generated), `board.md`, `tasks.tsv` with `tasks.md` (generated), `notes/`, and `state/` (session ids, watcher pid, task lock).

## Rules for changing crew

- **Small single-purpose functions, each with a one-line plain-English `///` comment.** Command functions (`cmd_*`) should read as a list of steps.
- **Before committing:** `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `test/run.sh`, `shellcheck install.sh test/run.sh`. CI runs all of them.
- **Add tests for behaviour changes:** unit tests for pure logic, a `check` line in `test/run.sh` for anything visible from the command line.
- **Few dependencies, no runtime requirements** beyond git and herdr or tmux.
- Bump `version` in `Cargo.toml` for user-visible changes; releases are cut by pushing a matching `v*` tag.

## Decisions and why

These were settled while building crew. Don't reverse one without a good reason, and record any new decision here.

### Shape of the tool
- **Rust, as a single binary.** crew started as a bash script. At ~1,500 lines it was mostly working around bash: a hand-written TOML subset parser, JSON through jq or python3, `mkdir` locks, a bash 3.2 floor, a nohup'd watcher loop. With no users yet, the rewrite cost nothing to anyone. Rust over Go because herdr is Rust and the human chose it. Releases ship static binaries for macOS (arm64, x86_64) and Linux musl (x86_64, aarch64), so they also run on older systems like Raspberry Pi OS.
- **The installer stays in bash.** It has to run before crew exists, and `curl | bash` needs a shell script.
- **Named `crew`.** It's a command people type.
- **Agents are disposable; files are the memory.** Roles, board, tasks and handoff notes are files, so a killed agent loses nothing. A restarted agent is told to read `notes/<role>.md`.

### Sharing across worktrees
- **Each role (except the lead by default) gets a git worktree** under `<worktree_dir>/<repo-slug>-<hash>/<role>` on branch `crew/<role>`. They live outside the repo so test runners and file watchers don't see duplicate code. The hash is FNV-1a of the repo path, so it's stable across versions.
- **There is exactly one `.crew/` folder**, in the main checkout. Worktrees don't get a copy (it's untracked). Agents reach it by absolute path, launch flags give them access (the `{crew}` placeholder in each agent's command), and `CREW_DIR` is set in each pane so `crew` finds it from any worktree.
- **`.crew/` is hidden through `.git/info/exclude`,** not `.gitignore`, so crew never changes tracked files.
- **Only the lead agent merges** `crew/*` branches into the base branch, locally. crew itself never pushes, merges or deletes branches.

### Messaging
- **`crew say` appends to `board.md` (one write per message) and types the message into the recipient's session.** herdr uses `herdr agent prompt`, which refuses while an agent is blocked; tmux pastes with `paste-buffer -p`, waits `paste_delay` seconds (default 0.3), then presses Enter.
- **The protocol forbids acknowledgement-only messages** ("ok", "thanks") to stop agents looping.
- `say all` goes to everyone except the sender. Messages from `crew` itself don't count as owed replies.

### Configuration
- **TOML with two layers: `~/.config/crew/config.toml` (global) and `.crew/config.toml` (repo); repo wins.** It's edited with `toml_edit`, so comments and ordering survive `crew role add/set/rm/save`. TOML was chosen over YAML because it's flat and doesn't depend on indentation.
- **Roles are read only from the repo file.** Global roles are a template that `crew setup` copies and `crew role save` replaces.
- **Agent launch commands are config, not code:** `command = "claude --add-dir {crew}"` or a list of words, with `{crew}`, `{repo}`, `{role}` and `{prompt}` placeholders. The kickoff prompt is appended unless `{prompt}` is used. String commands are split shell-style before placeholders are filled in, so paths with spaces stay intact.
- **herdr `kind`** defaults to the agent's name when herdr knows it (list copied from herdr 0.9.5). Then crew uses `herdr agent start` for naming and status tracking; `kind = ""` makes herdr type the command instead.
- **Settings must sit above any `[section]`** to be top-level. That's TOML, and the docs and generated files say so.
- **`crew config init` creates the global config** from `config.example.toml`, choosing herdr if it's installed, otherwise tmux. It never overwrites an existing config; the installer runs it.

### Multiplexers
- **herdr and tmux implement one `Mux` trait.** Adding a multiplexer means implementing it and adding a `MuxKind`.
- **herdr is driven through its CLI,** whose command shapes are tested against a fake. Its JSON is parsed with serde_json (no jq). Talking to herdr's socket API directly is a possible future improvement.
- **tmux can only report running or stopped.** An agent counts as running when the pane's shell has a child process. It isn't matched by process name, because many agent wrappers are shell scripts.
- **The tmux layout is `main-vertical`:** the first role on the left, the rest stacked on the right, plus a `board` window tailing `board.md`.
- **State records which multiplexer created it.** Switching multiplexers discards stale pane ids instead of reusing them.

### Tasks
- **Tasks live in `tasks.tsv`; `tasks.md` is generated** after every change. Agents read `tasks.md` and change tasks only with `crew task add | set | done`.
- **Every change holds an exclusive file lock** (`std::fs::File::lock` on `state/tasks.lock`). The minimum Rust version is 1.89 because of this. 20 concurrent adds lose nothing (tested).

### Stalled agents
- **Generic detection, not matching agents' error messages.** The human decided against hard-coding each agent's "usage limit" wording, because it's brittle and changes between versions. Instead: an agent is stalled when it **owes work** (a todo or doing task it owns, or a board message to it, or to all, posted after its own last message) **and its screen hasn't changed for `stall_timeout` seconds** (default 600; 0 turns it off).
- This catches running out of tokens, waiting on an approval, crashes and hangs alike. An idle agent with nothing owed is never flagged, and a busy agent's spinner keeps its screen changing.
- **The lead is told once per stall** (on the board and in its session) so it can check or reassign the work. If the lead stalls, or there's no lead, the human is told: board, plus a herdr notification or the tmux status line.
- **`crew start` runs `crew watch` in the background** (via nohup, in its own process group; pid in `state/watch.pid`); `crew stop` kills it. The watcher also exits when its pid file disappears, or after 3 checks with no session.

### Distribution
- **GitHub releases are built by `release.yml`** on a `v*` tag that must match `Cargo.toml`. Each archive has a `.sha256` that `install.sh` checks.
- **Homebrew comes later.** First a tap (a `Formula/crew.rb` in this repo is enough; users run `brew tap mshirlaw/crew https://github.com/mshirlaw/crew-cli`). Official homebrew-core needs 225+ stars (self-submitted) and a repo at least 30 days old.

### Upgrades
- 0.2's `.crew/crew.conf` is converted to `.crew/config.toml` automatically (the old file is kept as `crew.conf.old`). A hand-written `tasks.md` from 0.2 or 0.3 is moved to `tasks-old.md`.

## Not done yet

- A full session with real Claude, Codex and Gemini under herdr and tmux. The tests use stand-in agents and a fake herdr.
- The first tagged release (v0.6.0), then the Homebrew tap and a demo recording for the README.
