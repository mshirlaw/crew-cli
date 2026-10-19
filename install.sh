#!/usr/bin/env bash
# Installs the crew command and creates your global config if you don't have one.
#
#   ./install.sh                 install from this folder
#   curl -fsSL https://raw.githubusercontent.com/mshirlaw/crew-cli/main/install.sh | bash
#                                install without cloning (downloads crew from GitHub)
#   ./install.sh --uninstall     remove the crew command (your config and repos are kept)
#
# Environment: CREW_BIN_DIR (default ~/.local/bin), CREW_REF (git tag or branch to download, default main).
set -eu

REPO="mshirlaw/crew-cli"
DEST="${CREW_BIN_DIR:-$HOME/.local/bin}"
REF="${CREW_REF:-main}"

# Prints the shell startup file to add PATH changes to.
shell_rc() {
  case "$(basename "${SHELL:-bash}")" in
    zsh)  echo "$HOME/.zshrc" ;;
    fish) echo "$HOME/.config/fish/config.fish" ;;
    *)    echo "$HOME/.bashrc" ;;
  esac
}

# Prints how to install a package on this system.
install_hint() {
  if command -v brew >/dev/null 2>&1; then echo "brew install $1"
  elif command -v apt-get >/dev/null 2>&1; then echo "sudo apt install $1"
  elif command -v dnf >/dev/null 2>&1; then echo "sudo dnf install $1"
  elif command -v pacman >/dev/null 2>&1; then echo "sudo pacman -S $1"
  else echo "install $1 with your package manager"; fi
}

# Prints the folder this script is in, or nothing when it's piped from curl.
script_dir() {
  local src="${BASH_SOURCE[0]:-}"
  [ -n "$src" ] && [ -f "$src" ] && cd "$(dirname "$src")" && pwd
}

# Copies crew from this folder, or downloads it from GitHub when there's no local copy.
install_crew() {
  local here tmp
  here="$(script_dir || true)"
  mkdir -p "$DEST"
  if [ -n "$here" ] && [ -f "$here/crew" ]; then
    install -m 755 "$here/crew" "$DEST/crew"
  else
    command -v curl >/dev/null 2>&1 || { echo "✗ curl is needed to download crew: $(install_hint curl)" >&2; exit 1; }
    tmp="$(mktemp)"
    curl -fsSL "https://raw.githubusercontent.com/$REPO/$REF/crew" -o "$tmp" ||
      { echo "✗ could not download crew from $REPO ($REF)" >&2; rm -f "$tmp"; exit 1; }
    head -1 "$tmp" | grep -q '^#!.*bash' || { echo "✗ downloaded file doesn't look like crew" >&2; rm -f "$tmp"; exit 1; }
    install -m 755 "$tmp" "$DEST/crew"
    rm -f "$tmp"
  fi
  echo "✓ installed $("$DEST/crew" version) to $DEST/crew"
}

# Removes the crew command and says what was left behind.
uninstall_crew() {
  rm -f "$DEST/crew"
  echo "✓ removed $DEST/crew"
  echo "Left in place (delete them yourself if you want):"
  echo "  ${CREW_HOME:-${XDG_CONFIG_HOME:-$HOME/.config}/crew}   your config and saved roles"
  echo "  ~/.crew/worktrees            agent worktrees (remove per repo with: crew teardown)"
  echo "  .crew/ in each repo          boards, tasks and notes"
}

# Warns about anything crew needs that isn't installed or on the PATH.
check_requirements() {
  case ":$PATH:" in
    *":$DEST:"*) ;;
    *) echo "! $DEST is not on your PATH. Add this line to $(shell_rc), then open a new terminal:"
       echo "    export PATH=\"$DEST:\$PATH\"" ;;
  esac
  command -v git >/dev/null 2>&1 || echo "! git is missing: $(install_hint git)"
  if ! command -v herdr >/dev/null 2>&1 && ! command -v tmux >/dev/null 2>&1; then
    echo "! you need herdr (https://herdr.dev) or tmux: $(install_hint tmux)"
  fi
  if command -v herdr >/dev/null 2>&1 && ! command -v jq >/dev/null 2>&1 && ! command -v python3 >/dev/null 2>&1; then
    echo "! herdr support needs jq: $(install_hint jq)"
  fi
}

if [ "${1:-}" = --uninstall ]; then
  uninstall_crew
  exit 0
fi

install_crew
"$DEST/crew" config init
check_requirements
echo "Next: run 'crew doctor', then cd into a repo and run 'crew start'"
