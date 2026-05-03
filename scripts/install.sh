#!/usr/bin/env bash
# install.sh — one-shot setup for `notclaude` on macOS.
# Idempotent: safe to re-run. Prompts before any heavy install (Homebrew,
# 9GB model pull). Run from repo root or scripts/.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEFAULT_MODEL="qwen2.5-coder:14b"
OLLAMA_URL="http://localhost:11434"
LOCAL_BIN="$HOME/.local/bin"

# --- output helpers ---------------------------------------------------------
if [ -t 1 ]; then
  C_BOLD=$(printf '\033[1m'); C_DIM=$(printf '\033[2m')
  C_GREEN=$(printf '\033[32m'); C_YELLOW=$(printf '\033[33m')
  C_RED=$(printf '\033[31m'); C_RESET=$(printf '\033[0m')
else
  C_BOLD=""; C_DIM=""; C_GREEN=""; C_YELLOW=""; C_RED=""; C_RESET=""
fi

step()  { printf "\n${C_BOLD}==>${C_RESET} %s\n" "$1"; }
ok()    { printf "  ${C_GREEN}✓${C_RESET} %s\n" "$1"; }
warn()  { printf "  ${C_YELLOW}!${C_RESET} %s\n" "$1"; }
fail()  { printf "  ${C_RED}✗${C_RESET} %s\n" "$1" >&2; exit 1; }
info()  { printf "  ${C_DIM}%s${C_RESET}\n" "$1"; }

confirm() {
  # confirm "prompt" -> 0 if yes, 1 if no. Default no on empty.
  local prompt="$1"
  local reply
  if [ ! -t 0 ]; then
    warn "non-interactive shell; defaulting to NO for: $prompt"
    return 1
  fi
  printf "${C_BOLD}?${C_RESET} %s [y/N] " "$prompt"
  read -r reply
  case "$reply" in
    y|Y|yes|YES) return 0 ;;
    *) return 1 ;;
  esac
}

# --- checks -----------------------------------------------------------------

require_macos() {
  step "Checking platform"
  if [ "$(uname -s)" != "Darwin" ]; then
    fail "this installer targets macOS (uname says: $(uname -s))"
  fi
  local arch; arch="$(uname -m)"
  ok "macOS $(sw_vers -productVersion) on ${arch}"
  if [ "$arch" != "arm64" ]; then
    warn "Apple Silicon (arm64) is strongly recommended for local model performance"
  fi
}

ensure_xcode_clt() {
  step "Checking Xcode Command Line Tools"
  if xcode-select -p >/dev/null 2>&1; then
    ok "Command Line Tools present at $(xcode-select -p)"
    return
  fi
  warn "Command Line Tools missing — these are required to compile Rust"
  if confirm "Run 'xcode-select --install' now? (opens a system dialog)"; then
    xcode-select --install || true
    info "complete the system installer, then re-run this script"
    exit 1
  else
    fail "cannot proceed without Command Line Tools"
  fi
}

ensure_homebrew() {
  step "Checking Homebrew"
  if command -v brew >/dev/null 2>&1; then
    ok "brew $(brew --version | head -1)"
    return
  fi
  warn "Homebrew not found"
  info "this installs Homebrew via the official script:"
  info "  https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh"
  if confirm "Install Homebrew now?"; then
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    # add brew to PATH for the rest of this script
    if [ -x /opt/homebrew/bin/brew ]; then
      eval "$(/opt/homebrew/bin/brew shellenv)"
    elif [ -x /usr/local/bin/brew ]; then
      eval "$(/usr/local/bin/brew shellenv)"
    fi
    ok "Homebrew installed"
  else
    fail "Homebrew is required (used to install Rust + Ollama)"
  fi
}

ensure_rust() {
  step "Checking Rust toolchain"
  if command -v cargo >/dev/null 2>&1; then
    ok "$(cargo --version)"
    return
  fi
  warn "cargo not found — installing rust via brew"
  brew install rust
  ok "$(cargo --version)"
}

ensure_ollama() {
  step "Checking Ollama"
  if command -v ollama >/dev/null 2>&1; then
    ok "$(ollama --version 2>&1 | head -1)"
  else
    warn "ollama not found — installing via brew"
    brew install ollama
    ok "$(ollama --version 2>&1 | head -1)"
  fi

  # start the daemon if it's not already listening
  if curl -sf -o /dev/null --max-time 2 "$OLLAMA_URL/api/tags" 2>/dev/null; then
    ok "ollama daemon is running"
  else
    info "starting ollama daemon as a background service"
    brew services start ollama >/dev/null 2>&1 || true
    # wait up to ~6s for daemon to come up
    local attempts=0
    until curl -sf -o /dev/null --max-time 1 "$OLLAMA_URL/api/tags" 2>/dev/null; do
      attempts=$((attempts + 1))
      if [ "$attempts" -ge 20 ]; then
        fail "ollama daemon did not come up after ~6s; try: brew services restart ollama"
      fi
      sleep 0.3
    done
    ok "ollama daemon is up"
  fi
}

check_disk_space() {
  step "Checking disk space (~15 GB headroom recommended)"
  # df -g returns gigabytes on macOS; column 4 is "avail" for the home volume
  local avail_gb
  avail_gb=$(df -g "$HOME" | awk 'NR==2 {print $4}')
  if [ -z "$avail_gb" ]; then
    warn "could not determine free space"
    return
  fi
  if [ "$avail_gb" -lt 15 ]; then
    warn "only ${avail_gb} GB free on \$HOME volume; build + model pull need ~12 GB"
    if ! confirm "continue anyway?"; then
      fail "abort: insufficient disk"
    fi
  else
    ok "${avail_gb} GB free"
  fi
}

build_release() {
  step "Building release binary (cargo build --release)"
  info "first build takes ~2 min on a clean machine; cached builds are seconds"
  ( cd "$REPO_ROOT/rust" && cargo build --release -p claw-cli )
  if [ ! -x "$REPO_ROOT/rust/target/release/notclaude" ]; then
    fail "build finished but binary not found at rust/target/release/notclaude"
  fi
  ok "binary at rust/target/release/notclaude"
}

install_wrapper_symlink() {
  step "Installing notclaude command to $LOCAL_BIN"
  mkdir -p "$LOCAL_BIN"
  ln -sf "$REPO_ROOT/scripts/notclaude" "$LOCAL_BIN/notclaude"
  ok "symlink: $LOCAL_BIN/notclaude → $REPO_ROOT/scripts/notclaude"

  case ":$PATH:" in
    *":$LOCAL_BIN:"*)
      ok "$LOCAL_BIN is already on PATH"
      ;;
    *)
      warn "$LOCAL_BIN is NOT on your PATH"
      local rc
      if [ -n "${ZSH_VERSION:-}" ] || [ "${SHELL##*/}" = "zsh" ]; then
        rc="$HOME/.zshrc"
      else
        rc="$HOME/.bashrc"
      fi
      info "add this line to $rc, then restart your shell:"
      printf "    export PATH=\"%s:\$PATH\"\n" "$LOCAL_BIN"
      ;;
  esac
}

pull_default_model() {
  step "Checking default model ($DEFAULT_MODEL)"
  if curl -sf "$OLLAMA_URL/api/tags" 2>/dev/null | grep -q "\"$DEFAULT_MODEL\""; then
    ok "$DEFAULT_MODEL already pulled"
    return
  fi
  warn "$DEFAULT_MODEL not pulled — this is a ~9 GB download"
  if confirm "Pull $DEFAULT_MODEL now?"; then
    ollama pull "$DEFAULT_MODEL"
    ok "$DEFAULT_MODEL ready"
  else
    warn "skipped; you can pull it later with: ollama pull $DEFAULT_MODEL"
    warn "notclaude will fail to start until at least the default model is pulled"
  fi
}

print_next_steps() {
  step "Done"
  printf "  ${C_GREEN}notclaude${C_RESET} is installed.\n\n"
  printf "  Try it:\n"
  printf "    ${C_BOLD}notclaude${C_RESET}                            # interactive REPL\n"
  printf "    ${C_BOLD}notclaude${C_RESET} \"write fizzbuzz in rust\"   # one-shot prompt\n\n"
  printf "  Inside the REPL:\n"
  printf "    ${C_BOLD}/model${C_RESET}              # interactive model picker\n"
  printf "    ${C_BOLD}/model glm-flash${C_RESET}    # switch to GLM-4.7-Flash (auto-pulls if needed)\n"
  printf "    ${C_BOLD}/help${C_RESET}               # all commands\n\n"
  case ":$PATH:" in
    *":$LOCAL_BIN:"*) ;;
    *) printf "  ${C_YELLOW}Heads up:${C_RESET} you'll need to add %s to your PATH first.\n\n" "$LOCAL_BIN" ;;
  esac
}

# --- main -------------------------------------------------------------------
main() {
  printf "${C_BOLD}notclaude installer${C_RESET}\n"
  printf "${C_DIM}repo: %s${C_RESET}\n" "$REPO_ROOT"

  require_macos
  ensure_xcode_clt
  ensure_homebrew
  ensure_rust
  ensure_ollama
  check_disk_space
  build_release
  install_wrapper_symlink
  pull_default_model
  print_next_steps
}

main "$@"
