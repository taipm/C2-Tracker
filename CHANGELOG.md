# Changelog

Mọi thay đổi đáng chú ý của dự án C2-Tracker.

Format theo [Keep a Changelog](https://keepachangelog.com/), version theo [SemVer](https://semver.org/).

---

## [v0.0.1] — 2025-06-18

Bản release đầu tiên. MVP Phase 1 hoàn tất (9/10 milestone, M1.10 Export Markdown defer).

### Added

#### Engine (`c2-engine`)

- **JSONL watcher** dùng `notify` 6 với debounce 100ms, theo dõi `~/.claude/projects/**/*.jsonl`.
- **Parser incremental + idempotent**: re-parse 1 file 2 lần không sinh duplicate. uuid UNIQUE cho content events, hash SHA-1 cho meta events. Detect truncate qua `file_inode`.
- **SQLite persistence**: 2 bảng `sessions` (23 cột) + `events` (3 category: content/meta/attachment) theo REQ §5.1. Auto-migrate schema khi start.
- **HTTP server (axum 0.7)** với 4 endpoint hooks T1: `/hooks/session-start`, `/hooks/user-prompt-submit`, `/hooks/stop`, `/hooks/session-end`. Bearer token auth, 401 nếu thiếu/sai. Latency POST < 50ms.
- **WebSocket endpoint `/ws`** broadcast realtime: `SessionUpsert` + `EventBatch` cho mỗi batch insert. Auth qua query token (browser WS không support custom headers). Reconnect exponential backoff client-side.
- **CLI 7 subcommand**: `parse-once`, `import-all`, `watch`, `serve`, `token`, `install-hooks`, `uninstall-hooks`.
- **`install-hooks`** merge vào `~/.claude/settings.json` idempotent (marker `_c2_tracker`). Hỗ trợ native `http` hook cho Claude Code ≥ 2.0, fallback `command` shim (curl) cho Claude Code cũ. Backup `.bak.<timestamp>` trước modify.
- **Status compute on-the-fly**: live (< 2 phút từ last event, no Stop) / idle (2–30 phút) / ended (Stop event hoặc ≥ 30 phút).
- **LaunchAgent macOS** plist + `install.sh` để daemon auto-start khi boot.

#### Desktop app (`c2-tracker`, Tauri v2 + vanilla HTML/CSS/JS)

- **UI 4:8 layout** dark theme với frosted glass (`backdrop-filter`), custom titlebar macOS Overlay.
- **Session list** sidebar với search, filter project/thời gian, status indicator (live đỏ pulse, idle vàng, ended xanh).
- **Stream chat-style** render đủ kinds: `user`, `assistant`, `thinking` (collapsible), `tool_use`, `tool_result`, `summary`. Auto-scroll xuống bottom khi user đã ở gần đáy (< 80px).
- **Realtime push qua WebSocket**: latency UI update < 50ms thay vì polling 3s. Fallback polling 5s nếu WS không kết nối.
- **Page Visibility API**: đóng WS sạch khi tab hidden để tiết kiệm pin, reconnect + refresh khi visible lại.
- **Phân trang events**: stream load 500 events mới nhất (DESC + reverse). Scroll lên đỉnh (< 80px) trigger load thêm 500 cũ hơn, giữ scroll position, throttle 500ms, `reachedOldest` flag khi hết.
- **Issues panel**: tab toggle "Stream | Issues (N)" hiển thị `hook_non_blocking_error` attachments. Badge count cập nhật realtime qua WS.
- **Quick stats today**: sessions, tokens, tool calls.
- **Session metadata header**: `started X ago · duration · tokens · events · model`.
- **Keyboard**: `Cmd+K` search palette, `↑↓` navigate session list.
- **Fallback mock data**: nếu DB không tồn tại, UI hiển thị 3 session mock để demo.

### Performance

- App idle (3 session active): RAM < 200 MB.
- Hook POST → DB → UI broadcast: < 100ms end-to-end.
- 1246 sessions / 138k events import time: ~10 giây (engine release build).
- Parse session 350KB JSONL: < 100ms.

### Security

- Token random 32-byte base64 sinh khi serve lần đầu, lưu `~/.c2-tracker/runtime.json` (chmod 600) + `~/.c2-tracker/env` (chmod 600).
- HTTP server bind `127.0.0.1` only.
- Pool DB mở `SQLITE_OPEN_READ_ONLY` ở Tauri app — engine là writer duy nhất.
- Backup `~/.claude/settings.json` trước mỗi modify.

### Tham chiếu

- 10 milestone Phase 1 tracking trong [Gitea issues](https://git.microai.club/taipm/C2-Tracker/issues): #2-#10 đã close, #11 (M1.10 Export Markdown) defer.
- Đặc tả đầy đủ: [REQ.md](./REQ.md).

### Known limitations

- Tauri build chỉ debug (chưa build release `.dmg` — cần code signing macOS).
- Search palette `Cmd+K` chỉ search local state (chưa hỗ trợ full-text DB).
- Export Markdown (M1.10) chưa làm — nút "Export MD" mock.
- Polling fallback dừng khi tab visible trở lại (giải pháp tạm; sẽ refactor Phase 2).

### Compatibility

- macOS Apple Silicon (M1/M2/M3/M4) — môi trường chính.
- Linux x86_64 — engine build OK, Tauri UI chưa verify.
- Claude Code 2.x: hỗ trợ native `http` hook.
- Claude Code < 2.0: fallback `command` shim (curl).
- Phụ thuộc runtime: SQLite (bundled), Rust 1.95+, Tauri 2.x.

[v0.0.1]: https://git.microai.club/taipm/C2-Tracker/releases/tag/v0.0.1

<!-- mirror sync test: Mon May 18 18:30:13 +07 2026 -->
