# crew-cli

[![test](https://github.com/mshirlaw/crew-cli/actions/workflows/test.yml/badge.svg)](https://github.com/mshirlaw/crew-cli/actions/workflows/test.yml)

One command to start a team of AI coding agents in any repo. Each agent gets a role, its own git worktree, and a shared message board, all running in [herdr](https://herdr.dev) or tmux.

```
cd ~/code/my-app
crew start
```

You get a workspace (herdr) or session (tmux) for the repo:

```
┌──────────────┬──────────────┐
│              │   builder    │
│     lead     │   (codex)    │
│   (claude)   ├──────────────┤
│              │   reviewer   │
│              │   (gemini)   │
└──────────────┴──────────────┘
  tabs: agents | board (live chat log)
```

Tell the lead what you want. It writes tasks, hands them out, and the agents message each other until the work is reviewed and merged.

## Install

crew is a single binary for macOS (Apple Silicon and Intel) and Linux (x86_64 and ARM64, including Raspberry Pi OS 64-bit). You need git and one multiplexer: [herdr](https://herdr.dev), or tmux 3.0 or newer.

```
curl -fsSL https://raw.githubusercontent.com/mshirlaw/crew-cli/main/install.sh | bash
crew doctor
```

The installer downloads the right binary for your machine from the latest GitHub release and checks its checksum. To pin a release, put `CREW_VERSION=v0.6.0` before `bash`. From a clone with [Rust](https://rustup.rs) installed, `./install.sh` builds from source instead. Or skip the installer: `cargo install --git https://github.com/mshirlaw/crew-cli`.

The installer copies `crew` to `~/.local/bin` and creates `~/.config/crew/config.toml`, picking herdr if it's installed, otherwise tmux. Run it again to update; your config is left alone.

To uninstall: `curl -fsSL https://raw.githubusercontent.com/mshirlaw/crew-cli/main/install.sh | bash -s -- --uninstall` (or `./install.sh --uninstall`). This removes the command and tells you which config and worktree folders were left in place.

You also need at least one of `claude`, `codex` or `gemini` installed and logged in, or any other agent CLI you configure.

## Configuration

There are two TOML files. The repo file overrides the global one.

| File | Holds |
|---|---|
| `~/.config/crew/config.toml` | your defaults: multiplexer, worktree folder, how to launch each agent, default roles for new repos |
| `.crew/config.toml` | this repo's roles and base branch, plus any setting you want to override here |

```toml
multiplexer = "tmux"                 # or "herdr"
worktree_dir = "~/.crew/worktrees"

[agents.claude]
command = "claude --add-dir {crew}"  # {crew} {repo} {role} {prompt} are filled in

[roles.lead]
agent = "claude"
worktree = false
```

[`config.example.toml`](config.example.toml) explains every setting. Useful commands:

```
crew config                 show the settings in effect for this repo
crew config edit            edit .crew/config.toml
crew config edit --global   edit ~/.config/crew/config.toml
crew config example         print the sample config
```

To add an agent crew doesn't know, give it a command. The kickoff prompt is added as the last argument unless you put `{prompt}` somewhere else:

```toml
[agents.mytool]
command = "mytool --workspace {crew} --message {prompt}"
kind = ""        # herdr only: type this command instead of using herdr's agent tracking
```

### herdr or tmux?

| | herdr | tmux |
|---|---|---|
| Agent status | working / idle / blocked / done | running / stopped |
| Messages to a busy agent | held back if the agent is stuck on an approval prompt | always typed in |
| Install | `brew install herdr` or the herdr.dev script | already on most systems |

With either one, every message is also written to the board.

## What crew does and doesn't do

Before you start a crew, know what it costs and what it can touch.

- **Cost.** Every role is a full agent session. Three roles use roughly three times the tokens, or three times your subscription's quota, of working with one agent. Start with two or three roles and add more only when they earn it.
- **Permissions are the agents' own.** crew launches each agent with its normal permission settings and never adds auto-approve flags. Each agent asks you before running commands, exactly as it would on its own. If you've set an agent to skip approvals, that applies here too, times every role.
- **crew never pushes, merges or deletes your work.** It creates `crew/<role>` branches and worktrees, and `crew teardown` removes only worktrees with no uncommitted changes. The lead *agent* is told to merge approved branches into your base branch locally. Review that before you push.
- **Agents talk only through `crew say`.** Every message is written to `.crew/board.md`, so you can always see what was said.
- **Everything stays on your machine.** crew sends nothing anywhere. The agents talk to their own providers as usual.

## Known limitations

- Roles are only as good as their instructions. Expect to tune `.crew/roles/*.md` for your project.
- On tmux, crew can only tell whether an agent is running, not whether it's busy or waiting for approval. Messages to a busy agent are typed in anyway.
- Stall detection is deliberately generic: crew can tell an agent has gone quiet while it owes work, not why. The lead (or you) has to look and decide.
- Two crews can't share one repo checkout at the same time.

## How it works

| Piece | Where it lives | Why |
|---|---|---|
| Config | `.crew/config.toml` | roles, which agent runs each, worktree or not |
| Roles | `.crew/roles/<role>.md` | each agent's instructions. Edit freely |
| Protocol | `.crew/PROTOCOL.md` | shared rules + team table (regenerated on every start) |
| Board | `.crew/board.md` | the conversation. Every message is appended here |
| Tasks | `.crew/tasks.md` | who's doing what. Generated from `tasks.tsv`; change it with `crew task` |
| Notes | `.crew/notes/<role>.md` | each agent's handoff, so a restarted agent picks up where it left off |
| Worktrees | `<worktree_dir>/<repo>/<role>` on branch `crew/<role>` | agents never step on each other's files |

`.crew/` sits in your main checkout and is added to `.git/info/exclude`, so it never shows up in `git status`. Every agent is launched with access to it (that's the `{crew}` in each agent's command), so the shared files work across worktrees.

**Messaging:** agents run `crew say reviewer "crew/builder is ready"`. That appends to the board and types the message into the reviewer's session.

**Nothing is lost when an agent dies.** Roles, board, tasks and notes are files. Run `crew start` again (or `crew restart <role>`) to relaunch anything that stopped; it tells the agent to read its notes. herdr and tmux both keep the session running when you close the terminal.

## Commands

```
crew start [--no-attach]   set up if needed, launch or revive the team, open the session
crew setup                 just create .crew/ (to customise roles before starting)
crew status                each agent and its state
crew say <role|all> "msg"  message an agent from your own terminal
crew board [n]             last n lines of the conversation
crew tasks                 show the task list
crew task add "title" --owner builder --notes "acceptance criteria"
crew task set 3 status=review owner=reviewer
crew task done 3 "merged"
crew restart <role|all>    restart agents
crew stop                  close the session (everything else kept)
crew teardown              stop + remove clean worktrees (branches kept)
crew doctor                check dependencies
```

## Customising roles

```
crew role                                  list roles
crew role edit builder                     edit instructions (a running agent is told to re-read)
crew role set reviewer agent=claude        switch which agent plays a role
crew role set lead worktree=yes            give a role its own worktree
crew role add tester --agent codex         add a role (built-in template for: lead builder reviewer tester)
crew role add docs --like reviewer         start a new role from an existing one
crew role rm tester                        remove a role
crew role save builder                     make your edited builder instructions the default for new repos
crew role save                             make this repo's roles the default for new repos
```

Changes to a role's agent or worktree apply after `crew restart <role>`. Saved role instructions live in `~/.config/crew/roles/`.

## Things to know

- **First launch asks questions.** Claude, Codex and Gemini each ask you to trust a new folder. Approve it in each pane once; the kickoff prompt runs straight after.
- **Codex sandbox.** Codex may ask permission before running `crew say`, because it talks to herdr or tmux. Approve it (or allow it permanently) so Codex can message the team.
- **Messages not sending on tmux?** crew pastes the message, waits `paste_delay` seconds (default 0.3), then presses Enter. If messages sit in an agent's input box unsent, raise it, e.g. `paste_delay = 1` near the top of your config, above any `[section]`.
- **Avoiding loops.** The protocol forbids "ok / thanks" messages. If two agents still ping-pong, tighten their role files.
- **Merging.** Only the lead merges `crew/*` branches into your base branch. Review the result like any other change before pushing.
- **Stalled agents.** If an agent owes work (an open task or an unanswered message) and its screen hasn't changed for `stall_timeout` seconds (default 600), crew tells the lead once, so it can check on the agent or reassign the work. That covers agents that ran out of tokens, are waiting for an approval, or crashed. If the lead stalls, you get the alert. `crew start` runs the watcher in the background and `crew stop` ends it; set `stall_timeout = 0` to turn it off.
- **Shared tasks are safe.** `crew task` takes a lock before every change, so several agents can update tasks at once without losing an edit.
- **Upgrading from 0.2 or 0.3.** An old hand-written `tasks.md` is moved to `tasks-old.md` the first time you run a task command.
- **Upgrading from 0.2.** An old `.crew/crew.conf` is converted to `.crew/config.toml` automatically the first time you run crew.
- The tmux session is called `crew-<repo>`. In herdr, agents are named `<repo>-<role>`, so `herdr agent prompt my-app-builder "..."` also works.

## Contributing

Bug reports and pull requests are welcome; see [CONTRIBUTING.md](CONTRIBUTING.md). crew is written in Rust; run `cargo test` and `test/run.sh` before sending a change.

## License

MIT. See [LICENSE](LICENSE).
