#!/bin/bash
# Cài đặt LaunchAgent cho c2-engine daemon (macOS only)
# Sử dụng: ./install.sh
set -euo pipefail

PLIST_NAME="club.microai.c2tracker.engine.plist"
SRC="$(cd "$(dirname "$0")" && pwd)/$PLIST_NAME"
DST="$HOME/Library/LaunchAgents/$PLIST_NAME"
BIN="$(cd "$(dirname "$0")"/.. && pwd)/target/release/c2-engine"

if [[ ! -x "$BIN" ]]; then
    echo "Binary chưa tồn tại: $BIN"
    echo "Chạy 'cargo build --release' trong $(dirname "$BIN")/.. trước."
    exit 1
fi

# Patch plist với path binary đúng (trường hợp clone repo về vị trí khác)
mkdir -p "$HOME/Library/LaunchAgents"
sed "s|/Volumes/OWC-taipm-ssd/gitea/C2-Tracker/prototypes/jsonl-parser/target/release/c2-engine|$BIN|g" "$SRC" > "$DST"
chmod 644 "$DST"

# Reload
launchctl unload "$DST" 2>/dev/null || true
launchctl load "$DST"

sleep 2
echo ""
if launchctl list | grep -q c2tracker; then
    echo "✓ LaunchAgent đã load: club.microai.c2tracker.engine"
    echo "  Listen: 127.0.0.1:9786"
    echo "  Log: ~/.c2-tracker/launchd.out.log"
    echo ""
    echo "Tiếp theo: chạy 'c2-engine install-hooks --force-command-shim' để cài hooks Claude Code."
else
    echo "✗ LaunchAgent không load được. Check ~/.c2-tracker/launchd.err.log"
    exit 1
fi
