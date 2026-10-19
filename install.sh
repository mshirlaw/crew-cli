#!/usr/bin/env bash
# Installs the crew command and creates your global config if you don't have one.
# Set CREW_BIN_DIR to install somewhere other than ~/.local/bin.
set -e

here="$(cd "$(dirname "$0")" && pwd)"
dest="${CREW_BIN_DIR:-$HOME/.local/bin}"

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

mkdir -p "$dest"
install -m 755 "$here/crew" "$dest/crew"
echo "✓ installed crew to $dest/crew"

"$dest/crew" config init

case ":$PATH:" in
  *":$dest:"*) ;;
  *) echo "! $dest is not on your PATH. Add this line to $(shell_rc), then open a new terminal:"
     echo "    export PATH=\"$dest:\$PATH\"" ;;
esac

command -v git >/dev/null 2>&1 || echo "! git is missing: $(install_hint git)"
if ! command -v herdr >/dev/null 2>&1 && ! command -v tmux >/dev/null 2>&1; then
  echo "! you need herdr (https://herdr.dev) or tmux: $(install_hint tmux)"
fi
if command -v herdr >/dev/null 2>&1 && ! command -v jq >/dev/null 2>&1 && ! command -v python3 >/dev/null 2>&1; then
  echo "! herdr support needs jq: $(install_hint jq)"
fi

echo "Next: run 'crew doctor', then cd into a repo and run 'crew start'"
