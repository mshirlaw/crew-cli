# crew-cli

One command to start a team of AI coding agents in any repo. Each agent gets a role, its own git worktree, and a shared message board, all running in [herdr](https://herdr.dev).

```
cd ~/code/my-app
crew start
```

You get a herdr workspace named `crew:my-app`:

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

## Install (macOS)

```
brew install herdr jq
./install.sh            # copies crew to ~/.local/bin
```

You also need at least one of `claude`, `codex` or `gemini` installed and logged in.

## How it works

| Piece | Where it lives | Why |
|---|---|---|
| Roster | `.crew/crew.conf` | which roles exist, which agent runs each, worktree or not |
| Roles | `.crew/roles/<role>.md` | each agent's instructions. Edit freely |
| Protocol | `.crew/PROTOCOL.md` | shared rules + team table (regenerated on every start) |
| Board | `.crew/board.md` | the conversation. Every message is appended here |
| Tasks | `.crew/tasks.md` | who's doing what |
| Notes | `.crew/notes/<role>.md` | each agent's handoff, so a restarted agent picks up where it left off |
| Worktrees | `~/.crew/worktrees/<repo>/<role>` on branch `crew/<role>` | agents never step on each other's files |

`.crew/` sits in your main checkout and is added to `.git/info/exclude`, so it never shows up in `git status`. Every agent is launched with access to it (`--add-dir` / `--include-directories`), so the shared files work across worktrees.

**Messaging:** agents run `crew say reviewer "crew/builder is ready"`. That appends to the board and types the message straight into the reviewer's session through herdr. If the reviewer is busy on an approval prompt, the message still lands on the board.

**Nothing is lost when an agent dies.** Roles, board, tasks and notes are files. `crew start` again (or `crew restart <role>`) relaunches anything that stopped and tells it to read its notes. herdr also keeps the whole session running when you close the terminal.

## Commands

```
crew start [--no-attach]   set up if needed, launch or revive the team, open herdr
crew setup                 just create .crew/ (to customise roles before starting)
crew status                each agent and its state (working / idle / blocked)
crew say <role|all> "msg"  message an agent from your own terminal
crew board [n]             last n lines of the conversation
crew tasks                 show tasks.md
crew restart <role|all>    restart agents
crew stop                  close the workspace (everything else kept)
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
crew role save builder                     make your edited builder the default for every new repo
crew role save                             make this whole roster the default for every new repo
```

Changes to a role's agent or worktree apply after `crew restart <role>`. Personal defaults live in `~/.config/crew/` (`crew.conf` plus `roles/*.md`).

## Things to know

- **First launch asks questions.** Claude, Codex and Gemini each ask you to trust a new folder. Approve it in each pane once; the kickoff prompt runs straight after.
- **Codex sandbox.** Codex may ask permission before running `crew say`, because it talks to herdr's socket. Approve it (or allow it permanently) so Codex can message the team.
- **Avoiding loops.** The protocol forbids "ok / thanks" messages. If two agents still ping-pong, tighten their role files.
- **Merging.** Only the lead merges `crew/*` branches into your base branch. Review the result like any other change before pushing.
- Agent names in herdr are `<repo>-<role>`, e.g. `my-app-builder`, so you can also use `herdr agent prompt my-app-builder "..."` directly.