#!/usr/bin/env bash
# Runs crew's end-to-end tests in a throwaway home folder, with stand-in agents instead of real ones.
#
#   test/run.sh                          build crew (cargo build --release) and run everything
#   CREW_BIN=/path/to/crew test/run.sh   test an already-built binary
#
# The tmux tests run only if tmux 3.0+ is installed. herdr is tested against a fake
# that records the commands crew sends it, so real herdr isn't needed.
set -u

TEST_DIR="$(cd "$(dirname "$0")" && pwd)"
SOURCE_DIR="$(dirname "$TEST_DIR")"
WORK="$(cd "$(mktemp -d)" && pwd -P)"   # physical path, so it matches what crew resolves
PASSED=0
FAILED=0

# tmux's socket goes in a short path: macOS limits socket paths to about 104 characters.
TMUX_TMPDIR="/tmp/crew-test-$$"
export HOME="$WORK/home" PATH="$WORK/bin:$PATH" TMUX_TMPDIR GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.com GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.com
unset TMUX CREW_DIR CREW_ROLE CREW_HOME XDG_CONFIG_HOME HERDR_ENV
mkdir -p "$HOME" "$WORK/bin" "$WORK/log" "$TMUX_TMPDIR"

# ------------------------------------------------------------ helpers

# Removes the temporary folder and any tmux server the tests started.
cleanup() { tmux kill-server >/dev/null 2>&1; rm -rf "$WORK" "$TMUX_TMPDIR"; }
trap cleanup EXIT

CREW_BIN="${CREW_BIN:-$SOURCE_DIR/target/release/crew}"
if [ "$CREW_BIN" = "$SOURCE_DIR/target/release/crew" ]; then
  (cd "$SOURCE_DIR" && cargo build --release --quiet) || { echo "cargo build failed" >&2; exit 1; }
fi

# Runs the crew binary under test.
crew() { "$CREW_BIN" "$@"; }

# The same command as text, for checks that run inside sh -c.
CREW_RUN="'$CREW_BIN'"

# Records a passing or failing check.
check() {
  local name="$1" output; shift
  if output="$("$@" 2>&1)"; then PASSED=$((PASSED + 1)); echo "ok      $name"
  else FAILED=$((FAILED + 1)); echo "FAILED  $name"; report_failure "$name" "$output"; fi
}

# Shows a failed check's output, and on GitHub Actions also as an annotation on the run.
report_failure() {
  local name="$1" output="$2"
  [ -n "$output" ] && printf '%s\n' "$output" | head -20 | sed 's/^/        | /'
  if [ "${GITHUB_ACTIONS:-}" = true ]; then
    output="$(printf '%s' "$output" | head -c 800 | sed -e 's/%/%25/g' | awk 'BEGIN { ORS = "%0A" } { print }')"
    echo "::error title=crew test failed::$name: ${output:-no output}"
  fi
}

# Succeeds if a command's output contains some text.
output_has() { local text="$1"; shift; "$@" 2>&1 | grep -qF -- "$text"; }

# Succeeds if a command's output matches an extended regular expression.
output_matches() { local pattern="$1"; shift; "$@" 2>&1 | grep -qE -- "$pattern"; }

# Succeeds if a file contains some text.
file_has() { grep -qF -- "$2" "$1" 2>/dev/null; }

# Retries a check for up to 5 seconds, for things that happen in the background.
eventually() { local i; for i in $(seq 1 50); do "$@" >/dev/null 2>&1 && return 0; sleep 0.1; done; return 1; }

# Creates an empty git repo with one commit and prints its path.
new_repo() {
  local dir="$WORK/repos/$1"
  mkdir -p "$dir" && git -C "$dir" init -q -b main && git -C "$dir" commit -q --allow-empty -m init
  printf '%s' "$dir"
}

# Inserts a top-level setting into a repo's config, right after base_branch.
set_repo_setting() {
  local file="$1/.crew/config.toml"
  awk -v line="$2" '{ print } /^base_branch =/ { print line }' "$file" > "$file.tmp" && mv "$file.tmp" "$file"
}

# Writes a stand-in agent that logs its arguments and every line typed into it.
make_stub_agent() {
  cat > "$WORK/bin/$1" <<EOF
#!/bin/bash
printf '%s\n' "\$@" > "$WORK/log/started-\$CREW_ROLE"
while IFS= read -r line; do printf '%s\n' "\$line" >> "$WORK/log/inbox-\$CREW_ROLE"; done
EOF
  chmod +x "$WORK/bin/$1"
}

# Writes a fake herdr that records each command and answers like the real one.
make_fake_herdr() {
  cat > "$WORK/bin/herdr" <<EOF
#!/bin/bash
echo "\$*" >> "$WORK/log/herdr"
n=\$(wc -l < "$WORK/log/herdr" | tr -d ' ')
case "\$1 \$2" in
  "workspace list")   exit 0 ;;
  "workspace get")    test -f "$WORK/log/herdr-ws-\$3" ;;
  "workspace close")  rm -f "$WORK/log/herdr-ws-\$3" ;;
  "workspace create") touch "$WORK/log/herdr-ws-w1"; echo '{"result":{"workspace":{"workspace_id":"w1"},"tab":{"tab_id":"w1:t1"},"root_pane":{"pane_id":"w1:p1"}}}' ;;
  "pane split")       echo "{\"result\":{\"pane\":{\"pane_id\":\"w1:p\$n\"}}}" ;;
  "tab create")       echo "{\"result\":{\"tab\":{\"tab_id\":\"w1:t\$n\"},\"root_pane\":{\"pane_id\":\"w1:b\$n\"}}}" ;;
  *)                  exit 0 ;;
esac
EOF
  chmod +x "$WORK/bin/herdr"
}

# Writes a global config that points every agent at its stand-in by full path.
write_test_global_config() {
  mkdir -p "$HOME/.config/crew"
  cat > "$HOME/.config/crew/config.toml" <<EOF
multiplexer = "tmux"
paste_delay = 0.2
stall_timeout = 0

[agents.claude]
command = "$WORK/bin/claude --add-dir {crew}"

[agents.codex]
command = "$WORK/bin/codex --add-dir {crew}"

[agents.gemini]
command = "$WORK/bin/gemini --include-directories {crew} -i"
EOF
}

# Prints the tmux pane id saved for a role.
saved_pane() { awk -v k="pane:$2" '$1 == k { print $2 }' "$1/.crew/state/session"; }

# Succeeds if tmux 3.0 or newer is installed.
have_tmux3() { command -v tmux >/dev/null 2>&1 && [ "$(tmux -V | sed 's/[^0-9]*\([0-9]*\).*/\1/')" -ge 3 ]; }

for a in claude codex gemini; do make_stub_agent "$a"; done
write_test_global_config

# ------------------------------------------------------------ setup and config

echo "# setup and config"
R="$(new_repo setup)"; cd "$R" || exit 1
check "setup succeeds"                       crew setup
check "setup writes a repo config"           test -f .crew/config.toml
check "setup writes role files"              test -f .crew/roles/lead.md -a -f .crew/roles/builder.md -a -f .crew/roles/reviewer.md
check "setup writes the protocol"            file_has .crew/PROTOCOL.md "crew say <role|all|human>"
check "setup hides .crew from git"           file_has .git/info/exclude ".crew/"
check "git status stays clean"               test -z "$(git status --porcelain)"
check "setup records the base branch"        file_has .crew/config.toml 'base_branch = "main"'
check "global multiplexer is used"           output_matches "multiplexer +tmux" crew config
set_repo_setting "$R" 'multiplexer = "herdr"'
check "repo setting overrides global"        output_matches "multiplexer +herdr" crew config
R3="$(new_repo bad-mux)"; (cd "$R3" && crew setup >/dev/null); set_repo_setting "$R3" 'multiplexer = "screen"'
check "bad multiplexer is rejected"          sh -c "cd '$R3' && ! $CREW_RUN role list 2>/dev/null"
check "bad multiplexer error is clear"       sh -c "cd '$R3' && $CREW_RUN role list 2>&1 | grep -q 'must be \"herdr\" or \"tmux\"'"
check "sample config matches crew config example" sh -c "$CREW_RUN config example | diff -q - '$SOURCE_DIR/config.example.toml'"
check "doctor runs outside a repo"           sh -c "cd '$WORK' && $CREW_RUN doctor >/dev/null 2>&1; true"

# ------------------------------------------------------------ roles

echo "# roles"
check "role add"                             crew role add tester --agent codex --no-worktree
check "role add writes the config"           file_has .crew/config.toml "[roles.tester]"
check "role add makes a role file"           test -f .crew/roles/tester.md
check "role add rejects bad names"           sh -c "! $CREW_RUN role add Bad_Name"
check "role add rejects duplicates"          sh -c "! $CREW_RUN role add tester"
check "role set changes the agent"           crew role set tester agent=gemini worktree=yes
check "role set result"                      output_has "tester     gemini   yes" crew role
check "role add --like copies instructions"  crew role add docs --like reviewer
check "copied role is renamed"               file_has .crew/roles/docs.md "# Role: docs"
check "role rm"                              crew role rm docs
check "role rm removes it from the config"   sh -c "! grep -q 'roles.docs' .crew/config.toml"
check "role save writes global roles"        crew role save
check "global config has the roles"          file_has "$HOME/.config/crew/config.toml" "[roles.tester]"
check "global agent settings survive save"   file_has "$HOME/.config/crew/config.toml" "[agents.claude]"
R2="$(new_repo from-global)"
check "new repos use saved roles"            sh -c "cd '$R2' && $CREW_RUN setup && grep -q 'roles.tester' .crew/config.toml"
write_test_global_config

# ------------------------------------------------------------ upgrades from 0.2

echo "# upgrading"
R="$(new_repo upgrade)"; cd "$R" || exit 1
mkdir -p .crew/roles .crew/notes
printf 'lead     claude no\nbuilder  codex yes\n' > .crew/crew.conf
printf '# Tasks\n\n| task | owner | status | notes |\n|------|-------|--------|-------|\n| old work | builder | doing | |\n' > .crew/tasks.md
check "old crew.conf is converted"           output_has "builder    codex    yes" crew role
check "converted config has both roles"      file_has .crew/config.toml "[roles.lead]"
check "old file is kept as a backup"         test -f .crew/crew.conf.old
check "old task list is moved aside"         sh -c "$CREW_RUN tasks >/dev/null && test -f .crew/tasks-old.md"

# ------------------------------------------------------------ tasks

echo "# tasks"
R="$(new_repo tasks)"; cd "$R" || exit 1
crew setup >/dev/null
check "task add"                             output_has "task 1 added for builder" crew task add "Build login" --owner builder --notes "form + checks"
check "task add without owner"               output_has "task 2 added" crew task add "Write docs"
check "task set"                             crew task set 1 status=review notes='ready on crew/builder'
check "task done"                            crew task "done" 2 'C:\new folder'
check "tasks.md is generated"                file_has .crew/tasks.md "| 1 | review | builder | Build login | ready on crew/builder |"
check "backslashes in notes survive"         file_has .crew/tasks.md 'C:\new folder'
check "bad status is rejected"               sh -c "! $CREW_RUN task set 1 status=finished"
check "unknown task is rejected"             sh -c "! $CREW_RUN task done 99"
check "unknown owner is rejected"            sh -c "! $CREW_RUN task add x --owner nobody"
for i in $(seq 1 20); do crew task add "parallel $i" >/dev/null 2>&1 & done; wait
check "20 parallel adds keep every task"     test "$(wc -l < .crew/tasks.tsv | tr -d ' ')" -eq 22
check "20 parallel adds give unique ids"     test -z "$(cut -f1 .crew/tasks.tsv | sort | uniq -d)"

# ------------------------------------------------------------ messaging without a session

echo "# messaging"
check "say posts to the board"               crew say builder "hello builder"
check "board has the message"                file_has .crew/board.md "hello builder"
check "say records the sender"               sh -c "CREW_ROLE=builder $CREW_RUN say reviewer ready && grep -q 'builder → reviewer' .crew/board.md"
check "say rejects unknown roles"            sh -c "! $CREW_RUN say nobody hi"
check "board shows recent lines"             output_has "hello builder" crew board 20

# ------------------------------------------------------------ herdr (fake)

echo "# herdr (fake)"
make_fake_herdr
R="$(new_repo herdr)"; cd "$R" || exit 1
crew setup >/dev/null
set_repo_setting "$R" 'multiplexer = "herdr"'
check "start with herdr"                     crew start --no-attach
check "creates a workspace"                  file_has "$WORK/log/herdr" "workspace create"
check "splits a pane per extra role"         test "$(grep -c '^pane split' "$WORK/log/herdr")" -eq 2
check "opens the board tab"                  file_has "$WORK/log/herdr" "--label board"
check "starts lead as a herdr agent"         file_has "$WORK/log/herdr" "agent start herdr-lead --kind claude"
check "passes .crew to codex"                file_has "$WORK/log/herdr" "--kind codex --pane"
check "say goes through agent prompt"        sh -c "$CREW_RUN say builder hi >/dev/null && grep -q 'agent prompt herdr-builder' '$WORK/log/herdr'"
check "stop closes the workspace"            sh -c "$CREW_RUN stop >/dev/null && grep -q 'workspace close w1' '$WORK/log/herdr'"
rm -f "$WORK/bin/herdr"

# ------------------------------------------------------------ tmux (real)

if have_tmux3; then
  echo "# tmux"
  R="$(new_repo tmux)"; cd "$R" || exit 1
  crew setup >/dev/null
  check "start with tmux"                    crew start --no-attach
  check "session exists"                     tmux has-session -t "=crew-tmux"
  check "three agent panes"                  test "$(tmux list-panes -t '=crew-tmux:agents' | wc -l | tr -d ' ')" -eq 3
  check "board window"                       output_has board tmux list-windows -t "=crew-tmux"
  check "lead started with its kickoff"      eventually file_has "$WORK/log/started-lead" "You are the lead agent"
  check "builder can reach .crew"            eventually file_has "$WORK/log/started-builder" "$R/.crew"
  BUILDER_DIR="$(tmux display -p -t "$(saved_pane "$R" builder)" '#{pane_current_path}')"
  check "builder runs in its worktree"       sh -c "case '$BUILDER_DIR' in '$HOME'/.crew/worktrees/tmux-*/builder) true ;; *) false ;; esac"
  check "status shows running"               eventually output_has "builder    codex    running" crew status
  check "say delivers to one agent"          sh -c "CREW_ROLE=builder $CREW_RUN say reviewer 'branch ready'"
  check "reviewer received it"               eventually file_has "$WORK/log/inbox-reviewer" "[crew] from builder: branch ready"
  check "say all skips the sender"           sh -c "CREW_ROLE=lead $CREW_RUN say all 'standup'"
  check "builder received the broadcast"     eventually file_has "$WORK/log/inbox-builder" "from lead: standup"
  check "lead did not message itself"        sh -c "! grep -q standup '$WORK/log/inbox-lead' 2>/dev/null"
  check "restart one agent"                  crew restart builder
  check "still three panes after restart"    test "$(tmux list-panes -t '=crew-tmux:agents' | wc -l | tr -d ' ')" -eq 3
  check "restarted agent is told it was"     eventually file_has "$WORK/log/started-builder" "You were restarted"
  pkill -P "$(tmux display -p -t "$(saved_pane "$R" reviewer)" '#{pane_pid}')"
  check "status notices a dead agent"        eventually output_has "reviewer   gemini   stopped" crew status
  check "start revives it"                   output_has "reviewer → $WORK/bin/gemini launched" crew start --no-attach
  check "revived agent is running"           eventually output_has "reviewer   gemini   running" crew status

  set_repo_setting "$R" 'stall_timeout = 1'
  crew task add "Build the thing" --owner builder >/dev/null
  crew watch --once; sleep 2; crew watch --once
  check "stalled agent with a task alerts the lead"  eventually file_has "$WORK/log/inbox-lead" "builder has shown no activity"
  check "alert says what work is owed"       file_has .crew/board.md "open task(s) 1"
  check "agents without work are left alone" sh -c "! grep -q 'lead has shown' .crew/board.md"
  sleep 1; crew watch --once
  check "a stall is reported only once"      test "$(grep -c 'builder has shown' "$WORK/log/inbox-lead")" -eq 1
  crew task add "Plan the release" --owner lead >/dev/null
  crew watch --once; sleep 2; crew watch --once
  check "a stalled lead alerts the human"    file_has .crew/board.md "crew → human"
  crew say builder "are you there" >/dev/null; crew watch --once
  check "new screen activity clears a stall" eventually sh -c "test \"\$(awk '{print \$3}' .crew/state/watch-builder)\" = no"
  check "start launches the watcher"         crew start --no-attach
  check "watcher is running"                 sh -c "kill -0 \"\$(cat .crew/state/watch.pid)\""
  WATCH_PID="$(cat .crew/state/watch.pid)"
  check "stop closes the session"            crew stop
  check "session is gone"                    sh -c "! tmux has-session -t '=crew-tmux' 2>/dev/null"
  check "stop ends the watcher"              eventually sh -c "case \"\$(ps -p $WATCH_PID -o stat= 2>/dev/null)\" in ''|Z*) true ;; *) false ;; esac"
  check "teardown keeps branches"            sh -c "echo y | $CREW_RUN teardown >/dev/null && git -C '$R' show-ref -q refs/heads/crew/builder"
  check "teardown removes worktrees"         sh -c "! ls -d '$HOME'/.crew/worktrees/tmux-*/builder 2>/dev/null"
else
  echo "# tmux: skipped (tmux 3.0+ not installed)"
fi

# ------------------------------------------------------------ installer

echo "# installer"
INSTALL_HOME="$WORK/install-home"; mkdir -p "$INSTALL_HOME"
check "install from a folder"              env HOME="$INSTALL_HOME" CREW_BIN_DIR="$INSTALL_HOME/bin" CREW_BINARY="$CREW_BIN" bash "$SOURCE_DIR/install.sh"
check "installed command runs"             output_has "crew " "$INSTALL_HOME/bin/crew" version
check "install creates the global config"  test -f "$INSTALL_HOME/.config/crew/config.toml"
check "reinstall keeps the config"         output_has "left unchanged" env HOME="$INSTALL_HOME" CREW_BIN_DIR="$INSTALL_HOME/bin" CREW_BINARY="$CREW_BIN" bash "$SOURCE_DIR/install.sh"
check "uninstall removes the command"      sh -c "HOME='$INSTALL_HOME' CREW_BIN_DIR='$INSTALL_HOME/bin' bash '$SOURCE_DIR/install.sh' --uninstall >/dev/null && ! test -e '$INSTALL_HOME/bin/crew'"

# ------------------------------------------------------------ summary

echo
echo "$PASSED passed, $FAILED failed"
[ "$FAILED" -eq 0 ]
