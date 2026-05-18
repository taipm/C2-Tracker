#!/usr/bin/env bash
# Smoke test cho install-hooks / uninstall-hooks
# KHÔNG đụng ~/.claude/settings.json thật
set -e

BINARY="./target/release/c2-engine"
SETTINGS="/tmp/c2-test-settings.json"
RUNTIME="$HOME/.c2-tracker/runtime.json"

echo "=== Build ==="
cargo build --release --quiet

echo ""
echo "=== Kiểm tra runtime.json ==="
if [ ! -f "$RUNTIME" ]; then
    echo "runtime.json chưa có — tạo giả cho test"
    mkdir -p "$HOME/.c2-tracker"
    echo '{"port":9786,"token":"ct_testtoken123"}' > "$RUNTIME"
    CREATED_RUNTIME=1
fi

echo ""
echo "=== Test 1: Install lần đầu ==="
rm -f "$SETTINGS"
$BINARY install-hooks --settings "$SETTINGS"
echo "--- Output settings.json ---"
cat "$SETTINGS"
echo ""

echo "=== Test 2: Idempotency (chạy lại không có --force) ==="
$BINARY install-hooks --settings "$SETTINGS"

echo ""
echo "=== Test 3: Force reinstall ==="
$BINARY install-hooks --settings "$SETTINGS" --force

echo ""
echo "=== Verify: vẫn có đúng 4 events sau force ==="
EVENTS=$(python3 -c "
import json
with open('$SETTINGS') as f:
    d = json.load(f)
hooks = d.get('hooks', {})
total = 0
for event in hooks.values():
    for matcher in event:
        for h in matcher.get('hooks', []):
            if h.get('_c2_tracker'):
                total += 1
print(total)
")
echo "Số hook entries với _c2_tracker: $EVENTS (expected: 4)"

echo ""
echo "=== Test 4: Uninstall ==="
cp "$SETTINGS" "/tmp/c2-test-settings-before-uninstall.json"
$BINARY uninstall-hooks --settings "$SETTINGS"
echo "--- Output sau uninstall ---"
cat "$SETTINGS"

echo ""
echo "=== Test 5: command shim fallback ==="
rm -f "$SETTINGS"
$BINARY install-hooks --settings "$SETTINGS" --force-command-shim
python3 -c "
import json
with open('$SETTINGS') as f:
    d = json.load(f)
first_event = list(d['hooks'].values())[0]
hook_type = first_event[0]['hooks'][0]['type']
print(f'Hook type: {hook_type} (expected: command)')
"

echo ""
echo "=== DONE ==="
if [ "${CREATED_RUNTIME:-0}" = "1" ]; then
    echo "(runtime.json giả đã được tạo cho test — không phải token thật)"
fi
