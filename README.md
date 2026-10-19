# crew-cli

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

You need git and one multiplexer: [herdr](https://herdr.dev) (plus `jq`), or tmux 3.0 or newer.

```
git clone https://github.com/mshirlaw/crew-cli
cd crew-cli && ./install.sh
crew doctor
```

`install.sh` copies `crew` to `~/.local/bin` and creates `~/.config/crew/config.toml`. It picks herdr if it's installed, otherwise tmux. To update later, run `git pull && ./install.sh`; your config is left alone.

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

## How it works

| Piece | Where it lives | Why |
|---|---|---|
| Config | `.crew/config.toml` | roles, which agent runs each, worktree or not |
| Roles | `.crew/roles/<role>.md` | each agent's instructions. Edit freely |
| Protocol | `.crew/PROTOCOL.md` | shared rules + team table (regenerated on every start) |
| Board | `.crew/board.md` | the conversation. Every message is appended here |
| Tasks | `.crew/tasks.md` | who's doing what |
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
crew tasks                 show tasks.md
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
- **Upgrading from 0.2.** An old `.crew/crew.conf` is converted to `.crew/config.toml` automatically the first time you run crew.
- The tmux session is called `crew-<repo>`. In herdr, agents are named `<repo>-<role>`, so `herdr agent prompt my-app-builder "..."` also works.
