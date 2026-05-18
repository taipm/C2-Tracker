#!/usr/bin/env bash
# C2-Tracker installer — one-line install:
#
#   curl -fsSL https://raw.githubusercontent.com/taipm/C2-Tracker/main/install.sh | bash
#
# Env vars:
#   C2_VERSION       (default: v0.0.1)
#   C2_INSTALL_DIR   (default: ~/.c2-tracker)
#   C2_SKIP_HOOKS=1  bỏ qua bước install hooks vào ~/.claude/settings.json
#   C2_SKIP_AGENT=1  bỏ qua LaunchAgent (chạy daemon thủ công)
#   C2_SOURCE        (default: github) — đặt "gitea" để tải từ git.microai.club (cần GITEA_TOKEN hoặc netrc)

set -euo pipefail

# ── Source selection ──────────────────────────────────────────────────────────
SOURCE="${C2_SOURCE:-github}"
AUTH_FLAGS=()

case "$SOURCE" in
  github)
    # Public GitHub mirror — anonymous, không cần auth
    RELEASE_BASE="https://github.com/taipm/C2-Tracker/releases/download"
    ;;
  gitea)
    # Self-hosted Gitea (internal) — REQUIRE_SIGNIN_VIEW bật, cần auth
    RELEASE_BASE="https://git.microai.club/taipm/C2-Tracker/releases/download"
    if [[ -n "${GITEA_TOKEN:-}" ]]; then
      AUTH_FLAGS=(-H "Authorization: token $GITEA_TOKEN")
    elif [[ -f "$HOME/.netrc" ]] && grep -q "git.microai.club" "$HOME/.netrc" 2>/dev/null; then
      AUTH_FLAGS=(--netrc)
    else
      echo "✗ C2_SOURCE=gitea cần GITEA_TOKEN hoặc ~/.netrc entry cho git.microai.club" >&2
      exit 1
    fi
    ;;
  *)
    echo "✗ C2_SOURCE phải là 'github' hoặc 'gitea' (đang là: $SOURCE)" >&2
    exit 1
    ;;
esac

# ── Config ────────────────────────────────────────────────────────────────────
VERSION="${C2_VERSION:-v0.0.1}"
INSTALL_DIR="${C2_INSTALL_DIR:-$HOME/.c2-tracker}"
BIN_DIR="$INSTALL_DIR/bin"
SKIP_HOOKS="${C2_SKIP_HOOKS:-0}"
SKIP_AGENT="${C2_SKIP_AGENT:-0}"
UA="Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120 Safari/537.36"

# ── Helpers ───────────────────────────────────────────────────────────────────
say()  { printf "\033[1;36m→\033[0m %s\n" "$*"; }
ok()   { printf "\033[1;32m✓\033[0m %s\n" "$*"; }
warn() { printf "\033[1;33m!\033[0m %s\n" "$*"; }
err()  { printf "\033[1;31m✗\033[0m %s\n" "$*" >&2; exit 1; }

# ── Platform detection ────────────────────────────────────────────────────────
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$OS-$ARCH" in
  darwin-arm64)
    TARGET="darwin-arm64"
    ;;
  darwin-x86_64)
    err "macOS Intel chưa có build sẵn. Vui lòng build từ source: https://git.microai.club/taipm/C2-Tracker"
    ;;
  linux-x86_64|linux-aarch64)
    err "Linux $ARCH chưa có build sẵn ở $VERSION. Vui lòng build từ source: https://git.microai.club/taipm/C2-Tracker"
    ;;
  *)
    err "Platform $OS-$ARCH chưa hỗ trợ."
    ;;
esac

URL="$RELEASE_BASE/$VERSION/c2-tracker-$VERSION-$TARGET.tar.gz"

cat <<EOF

  ╔═══════════════════════════════╗
  ║   C2-Tracker Installer        ║
  ╚═══════════════════════════════╝

  Source  : $SOURCE
  Version : $VERSION
  Target  : $TARGET
  Dest    : $INSTALL_DIR
  URL     : $URL

EOF

# ── Download ──────────────────────────────────────────────────────────────────
TMPDIR=$(mktemp -d)
trap "rm -rf '$TMPDIR'" EXIT

say "Tải tarball..."
# Expand AUTH_FLAGS safely khi rỗng (set -u nghiêm cấm unbound array)
if ! curl -fsSL -A "$UA" ${AUTH_FLAGS[@]+"${AUTH_FLAGS[@]}"} "$URL" -o "$TMPDIR/release.tar.gz"; then
  err "Tải release thất bại từ $URL"
fi
SIZE=$(stat -f%z "$TMPDIR/release.tar.gz" 2>/dev/null || stat -c%s "$TMPDIR/release.tar.gz")
ok "Đã tải $(( SIZE / 1024 )) KB"

# ── Extract ───────────────────────────────────────────────────────────────────
say "Giải nén..."
tar -xzf "$TMPDIR/release.tar.gz" -C "$TMPDIR"
EXTRACT_DIR=$(ls -d "$TMPDIR"/c2-tracker-*/)
EXTRACT_DIR="${EXTRACT_DIR%/}"
if [[ ! -x "$EXTRACT_DIR/c2-engine" ]]; then
  err "Không tìm thấy c2-engine binary trong tarball"
fi

# ── Install binary ────────────────────────────────────────────────────────────
mkdir -p "$BIN_DIR"
install -m 755 "$EXTRACT_DIR/c2-engine" "$BIN_DIR/c2-engine"
ok "Binary cài tại $BIN_DIR/c2-engine"

# ── LaunchAgent ───────────────────────────────────────────────────────────────
if [[ "$OS" == "darwin" && "$SKIP_AGENT" != "1" ]]; then
  PLIST_DST="$HOME/Library/LaunchAgents/club.microai.c2tracker.engine.plist"
  mkdir -p "$(dirname "$PLIST_DST")"
  sed "s|/Volumes/OWC-taipm-ssd/gitea/C2-Tracker/prototypes/jsonl-parser/target/release/c2-engine|$BIN_DIR/c2-engine|g" \
      "$EXTRACT_DIR/club.microai.c2tracker.engine.plist" > "$PLIST_DST"
  chmod 644 "$PLIST_DST"

  launchctl unload "$PLIST_DST" 2>/dev/null || true
  launchctl load "$PLIST_DST"
  sleep 2

  if launchctl list | grep -q "club.microai.c2tracker.engine"; then
    ok "LaunchAgent đã chạy (listen 127.0.0.1:9786)"
  else
    warn "LaunchAgent không load được. Check $INSTALL_DIR/launchd.err.log"
  fi
fi

# ── PATH ──────────────────────────────────────────────────────────────────────
PATH_LINE="export PATH=\"$BIN_DIR:\$PATH\""
PATH_HINT=""
case "$(basename "${SHELL:-zsh}")" in
  zsh)  SHELL_RC="$HOME/.zshrc" ;;
  bash) SHELL_RC="$HOME/.bashrc" ;;
  fish) SHELL_RC="$HOME/.config/fish/config.fish" ; PATH_LINE="fish_add_path $BIN_DIR" ;;
  *)    SHELL_RC="" ;;
esac

if [[ -n "$SHELL_RC" ]] && ! grep -Fq "$BIN_DIR" "$SHELL_RC" 2>/dev/null; then
  mkdir -p "$(dirname "$SHELL_RC")"
  {
    echo ""
    echo "# C2-Tracker"
    echo "$PATH_LINE"
  } >> "$SHELL_RC"
  PATH_HINT="$SHELL_RC"
  ok "PATH thêm vào $SHELL_RC"
fi

export PATH="$BIN_DIR:$PATH"

# ── Install hooks ─────────────────────────────────────────────────────────────
if [[ "$SKIP_HOOKS" != "1" ]] && [[ -d "$HOME/.claude" ]]; then
  say "Cài hooks vào ~/.claude/settings.json..."
  if "$BIN_DIR/c2-engine" install-hooks --force-command-shim 2>&1 | grep -E "(cai vao|đã được cài|Token)" >/dev/null; then
    ok "Hooks Claude Code đã cài (4 events: SessionStart, UserPromptSubmit, Stop, SessionEnd)"
  else
    warn "Cài hooks không thành công — chạy thủ công: c2-engine install-hooks --force-command-shim"
  fi
elif [[ "$SKIP_HOOKS" != "1" ]]; then
  warn "~/.claude không tồn tại — bỏ qua install-hooks (cài Claude Code trước, rồi chạy: c2-engine install-hooks --force-command-shim)"
fi

# ── Summary ───────────────────────────────────────────────────────────────────
cat <<EOF

  ╔═══════════════════════════════╗
  ║   ✓ Cài đặt thành công        ║
  ╚═══════════════════════════════╝

  Daemon       : $(launchctl list 2>/dev/null | grep -q c2tracker && echo "running" || echo "manual start cần thiết")
  Binary       : $BIN_DIR/c2-engine
  Log          : $INSTALL_DIR/launchd.out.log
  DB           : $INSTALL_DIR/c2.db (sẽ tạo khi có session đầu)

  Bước tiếp theo:
    1. Mở terminal mới (hoặc: source ${PATH_HINT:-~/.zshrc})
    2. Verify: c2-engine token   # nên in port + token
    3. Import session lịch sử (tùy chọn): c2-engine import-all
    4. Bắt đầu Claude Code session — events sẽ tự track qua hooks

  Build UI desktop (tuỳ chọn, ~10 phút):
    git clone https://github.com/taipm/C2-Tracker.git
    cd C2-Tracker/app && python3 gen_icons.py && cargo tauri dev

  Gỡ cài đặt:
    launchctl unload ~/Library/LaunchAgents/club.microai.c2tracker.engine.plist
    $BIN_DIR/c2-engine uninstall-hooks
    rm -rf $INSTALL_DIR ~/Library/LaunchAgents/club.microai.c2tracker.engine.plist

EOF
