# Contributing to crew

Thanks for helping. crew is a small Rust program; please keep it small and readable.

## Ground rules

- **Small single-purpose functions with clear names.** Command functions should read as a list of steps.
- **Few comments, and `//` only.** No `///` doc comments or `//!` module headers; clap help text uses `#[command(about = ...)]` and `#[arg(help = ...)]`. Comment only what the code can't say for itself: why, a constraint, a gotcha. Don't restate what a function or line does.
- **Put code where it belongs:** `src/commands/` for command-line commands, `src/crew/` for the core, `src/mux/` for herdr and tmux, `src/util/` for small helpers. See the layout in [AGENTS.md](AGENTS.md).
- **Few dependencies.** crew uses anyhow, chrono, clap, serde_json, shlex and toml_edit. Ask before adding another.
- **No runtime requirements beyond git and herdr or tmux.** crew is a single binary; don't make it need Python, jq or anything else at runtime.
- **The sample config is the template.** `crew config init` writes `config.example.toml` as it is (it's compiled in), so edit that file to change the starter config.
- Read [AGENTS.md](AGENTS.md) for the design decisions before changing behaviour.

## Building and testing

```
cargo build --release
cargo fmt                                  # format
cargo clippy --all-targets -- -D warnings  # lint (pedantic is on, see Cargo.toml); CI fails on any warning
cargo test                                 # unit tests
test/run.sh                                # end-to-end tests against target/release/crew
shellcheck install.sh test/run.sh
```

The end-to-end tests use a throwaway home folder and stand-in agents, so they never touch your real config or start real agents. The tmux tests run only when tmux 3.0+ is installed. herdr is tested against a fake that records the commands crew sends it.

Add tests for any change in behaviour: a unit test next to the code for pure logic, or a `check "what it proves" command...` line in `test/run.sh` for anything you'd see from the command line.

## Releasing

1. Bump `version` in `Cargo.toml` and run `cargo build` so `Cargo.lock` updates.
2. Commit, then tag and push: `git tag v0.6.0 && git push origin v0.6.0`.
3. The release workflow checks the tag matches `Cargo.toml`, builds macOS and Linux binaries, and publishes a GitHub release with checksums. `install.sh` downloads from the latest release.

## Supporting another multiplexer

Implement the `Mux` trait in `src/mux/` (see `tmux.rs`), add it to `MuxKind` in `src/crew/config.rs`, and add tests.

## Reporting bugs

Please include the output of `crew doctor`, `crew config` and `crew version`, your OS, and the multiplexer and agent versions. If messages or agents misbehave, the last lines of `.crew/board.md` help too.
