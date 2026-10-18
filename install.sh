#!/usr/bin/env bash
# Installs the `crew` command to ~/.local/bin (or $CREW_BIN_DIR).
set -e
here="$(cd "$(dirname "$0")" && pwd)"
dest="${CREW_BIN_DIR:-$HOME/.local/bin}"
mkdir -p "$dest"
install -m 755 "$here/crew" "$dest/crew"
echo "✓ installed crew to $dest/crew"

case ":$PATH:" in
  *":$dest:"*) ;;
  *) echo "! $dest is not on your PATH. Add this to ~/.zshrc, then open a new terminal:"
     echo "    export PATH=\"$dest:\$PATH\"" ;;
esac

command -v herdr >/dev/null 2>&1 || echo "! herdr not found — install it with: brew install herdr"
command -v jq >/dev/null 2>&1 || command -v python3 >/dev/null 2>&1 || echo "! install jq: brew install jq"
echo "Next: cd into a repo and run: crew start"
