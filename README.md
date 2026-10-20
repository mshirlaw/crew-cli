# crew-cli

[![test](https://github.com/mshirlaw/crew-cli/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/mshirlaw/crew-cli/actions/workflows/test.yml)

Start a team of AI coding agents in any repo with one command. Each agent gets a role, its own git worktree and a shared message board, running in [herdr](https://herdr.dev) or tmux.

```
┌──────────────┬──────────────┐
│              │   builder    │
│     lead     │   (codex)    │
│   (claude)   ├──────────────┤
│              │   reviewer   │
│              │   (gemini)   │
└──────────────┴──────────────┘
  tabs: agents | board
```

Tell the lead what you want. It splits the work into tasks, hands them out, and the agents message each other until it's reviewed and merged.

## Install

macOS or Linux, with git and either herdr or tmux 3.0+:

```
curl -fsSL https://raw.githubusercontent.com/mshirlaw/crew-cli/main/install.sh | bash
crew doctor
```

You also need at least one agent CLI (`claude`, `codex`, `gemini`, or another you configure) installed and logged in. With Rust you can instead run `cargo install --git https://github.com/mshirlaw/crew-cli`.

## Use

```
cd ~/code/my-app
crew start
```

```
crew start / stop / restart <role|all>   run the crew
crew status                              what each agent is doing
crew say <role|all> "msg"                message agents from your terminal
crew board                               the conversation so far
crew task add "title" --owner builder    shared task list (also: set, done, tasks)
crew role add | edit | set | rm | save   change the team
crew config [edit [--global]]            settings
crew teardown                            stop and remove clean worktrees
```

`crew help <command>` has the details.

## Configure

`~/.config/crew/config.toml` holds your defaults; `.crew/config.toml` in a repo overrides them and lists that repo's roles. [`config.example.toml`](config.example.toml) explains every setting.

```toml
multiplexer = "tmux"                  # or "herdr"

[agents.claude]
command = "claude --add-dir {crew} --"   # how to launch an agent

[roles.reviewer]
agent = "claude"
worktree = true
```

Each role's instructions are in `.crew/roles/<role>.md`. Edit them freely.

## How it works

Everything shared lives in `.crew/` in your main checkout, hidden from git: the board, the task list, each role's instructions and handoff notes. Agents talk with `crew say`, which appends to the board and types the message into the other agent's session. Each role except the lead works on its own `crew/<role>` branch in a separate worktree, and the lead merges approved work. Because everything is in files, a crashed or restarted agent picks up where it left off.

If an agent has open work but its screen hasn't changed for 10 minutes, crew tells the lead. That usually means it ran out of tokens, is waiting for an approval, or is stuck.

## Worth knowing

- **Cost:** every role is a full agent session, so three roles use about three times the tokens.
- **Permissions:** agents run with their normal settings, so they still ask before running commands. On first launch each one asks you to trust the folder. Codex is started with `--sandbox workspace-write` so it can write to its worktree and the shared `.crew` folder.
- **Your work is safe:** crew never pushes, and never deletes a branch or a worktree with uncommitted changes. Review the lead's merges before you push.
- **herdr vs tmux:** herdr shows whether each agent is working, idle or blocked. tmux only knows running or stopped.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). MIT licence.
