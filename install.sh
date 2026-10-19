#!/usr/bin/env bash
# Installs crew and creates your global config if you don't have one.
#
#   curl -fsSL https://raw.githubusercontent.com/mshirlaw/crew-cli/main/install.sh | bash
#                                downloads the prebuilt crew for your OS and chip
#   ./install.sh                 from a clone: builds with cargo if you have Rust, otherwise downloads
#   ./install.sh --uninstall     removes crew (your config and repos are kept)
#
# Environment:
#   CREW_BIN_DIR   where to install (default ~/.local/bin)
#   CREW_VERSION   release to download, e.g. v0.6.0 (default: the latest release)
#   CREW_BINARY    install this already-built binary instead of building or downloading
set -eu

REPO="mshirlaw/crew-cli"
DEST="${CREW_BIN_DIR:-$HOME/.local/bin}"

# Prints an error and exits.
fail() { echo "✗ $*" >&2; exit 1; }

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

# Prints the release target for this OS and chip, e.g. aarch64-apple-darwin.
target_triple() {
  case "$(uname -s)/$(uname -m)" in
    Darwin/arm64)                echo aarch64-apple-darwin ;;
    Darwin/x86_64)               echo x86_64-apple-darwin ;;
    Linux/x86_64)                echo x86_64-unknown-linux-musl ;;
    Linux/aarch64 | Linux/arm64) echo aarch64-unknown-linux-musl ;;
    *) fail "there's no prebuilt crew for $(uname -s) $(uname -m). Install Rust (https://rustup.rs) and run: cargo install --git https://github.com/$REPO" ;;
  esac
}

# Prints the SHA-256 of a file.
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

# Downloads the prebuilt binary for this machine, checks its checksum, and installs it.
download_and_install() {
  local target base tmp
  command -v curl >/dev/null 2>&1 || fail "curl is needed to download crew: $(install_hint curl)"
  target="$(target_triple)"
  if [ -n "${CREW_VERSION:-}" ]; then base="https://github.com/$REPO/releases/download/$CREW_VERSION"
  else base="https://github.com/$REPO/releases/latest/download"; fi
  tmp="$(mktemp -d)"
  curl -fsSL "$base/crew-$target.tar.gz" -o "$tmp/crew.tar.gz" ||
    fail "could not download crew for $target from $base"
  curl -fsSL "$base/crew-$target.tar.gz.sha256" -o "$tmp/crew.sha256" ||
    fail "could not download the checksum for crew"
  [ "$(sha256_of "$tmp/crew.tar.gz")" = "$(cut -d' ' -f1 < "$tmp/crew.sha256")" ] ||
    fail "the download's checksum doesn't match. Try again, or report it at https://github.com/$REPO/issues"
  tar -xzf "$tmp/crew.tar.gz" -C "$tmp"
  install -m 755 "$tmp/crew" "$DEST/crew"
  rm -rf "$tmp"
}

# Builds crew from this clone with cargo and installs it.
build_and_install() {
  echo "• building crew with cargo (this takes a minute the first time)"
  cargo build --release --quiet --manifest-path "$1/Cargo.toml"
  install -m 755 "$1/target/release/crew" "$DEST/crew"
}

# Installs crew from a given binary, this clone, or a GitHub release.
install_crew() {
  local here
  here="$(script_dir || true)"
  mkdir -p "$DEST"
  if [ -n "${CREW_BINARY:-}" ]; then
    install -m 755 "$CREW_BINARY" "$DEST/crew"
  elif [ -n "$here" ] && [ -f "$here/Cargo.toml" ] && command -v cargo >/dev/null 2>&1; then
    build_and_install "$here"
  else
    download_and_install
  fi
  echo "✓ installed $("$DEST/crew" version) to $DEST/crew"
}

# Removes crew and says what was left behind.
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
}

if [ "${1:-}" = --uninstall ]; then
  uninstall_crew
  exit 0
fi

install_crew
"$DEST/crew" config init
check_requirements
echo "Next: run 'crew doctor', then cd into a repo and run 'crew start'"
