#!/usr/bin/env bash
# Build and install arcanaeum for the current user. No root needed.
#   ./install.sh                 build + install binary and desktop entry
#   ./install.sh --default       also make arcanaeum the default app for .md files
set -euo pipefail
cd "$(dirname "$0")"

command -v cargo >/dev/null || { echo "cargo not found: install Rust from https://rustup.rs" >&2; exit 1; }

cargo install --path . --locked
echo "installed Arcanaeum, run it with: arca <file.md>"
echo "binary: $(command -v arca || echo "$HOME/.cargo/bin/arca (add ~/.cargo/bin to PATH)")"

[[ "$(uname)" == "Linux" ]] || exit 0

# Pick a terminal for launching arcanaeum from a file manager.
term="${TERMINAL:-}"
if [[ -z "$term" ]]; then
  for t in kitty ghostty foot alacritty wezterm gnome-terminal konsole xterm; do
    command -v "$t" >/dev/null && { term="$t"; break; }
  done
fi

if [[ -n "$term" ]]; then
  case "$term" in
    kitty)          exec_line="kitty --class arcanaeum -e arca %f" ;;
    ghostty)        exec_line="ghostty --class=arcanaeum -e arca %f" ;;
    foot)           exec_line="foot --app-id arcanaeum arca %f" ;;
    alacritty)      exec_line="alacritty --class arcanaeum -e arca %f" ;;
    wezterm)        exec_line="wezterm start --class arcanaeum -- arca %f" ;;
    gnome-terminal) exec_line="gnome-terminal -- arca %f" ;;
    *)              exec_line="$term -e arca %f" ;;
  esac
  dest="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
  mkdir -p "$dest"
  # Clean up the entry from when this project was called "mdr".
  if grep -qs '^Exec=.* mdr %f' "$dest/mdr.desktop"; then rm -f "$dest/mdr.desktop"; fi
  sed "s|^Exec=.*|Exec=$exec_line|" arcanaeum.desktop > "$dest/arcanaeum.desktop"
  command -v update-desktop-database >/dev/null && update-desktop-database "$dest" || true
  echo "desktop entry: $dest/arcanaeum.desktop (terminal: $term)"
  if [[ "${1:-}" == "--default" ]]; then
    xdg-mime default arcanaeum.desktop text/markdown text/x-markdown
    echo "arcanaeum is now the default for markdown files"
  fi
fi
