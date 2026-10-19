# Contributing to crew

Thanks for helping. crew is a single bash script, so changes are usually small. Please keep it that way.

## Ground rules

- **One file, no dependencies.** `crew` must run on macOS's built-in bash 3.2 and on Linux with only git and herdr or tmux. jq or python3 is needed only for herdr. Don't add other tools.
- **Small functions with a comment.** Every function does one thing and has a one-line plain-English comment above it saying what it does.
- **No bash 4+ features.** That means no associative arrays, `mapfile`, `${var,,}` or `|&`. CI runs the tests under `/bin/bash` on macOS to catch these.
- **Keep the sample config in sync.** If you change `global_config_template`, regenerate the sample with `./crew config example > config.example.toml`. A test checks this.

## Running the tests

```
test/run.sh                         # everything
CREW_BASH=/bin/bash test/run.sh     # under a specific bash
shellcheck crew install.sh test/run.sh
```

The tests use a throwaway home folder and stand-in agents, so they never touch your real config or start real agents. The tmux tests run only when tmux 3.0+ is installed. herdr is tested against a fake that records the commands crew sends it.

Add a test for any change in behaviour. Each test is a single `check "what it proves" command...` line in `test/run.sh`.

## Supporting another multiplexer

Each multiplexer is a set of functions named `<name>_<action>`, e.g. `tmux_split` and `herdr_split`, which crew calls through `mux <action>`. To add one, implement the same actions as `tmux_*`, allow the name in `load_config`, and add tests.

## Reporting bugs

Please include the output of `crew doctor`, `crew config`, your OS, and the multiplexer and agent versions. If messages or agents misbehave, the last lines of `.crew/board.md` help too.
