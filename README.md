# C2-Tracker

> Desktop app theo dõi realtime và lưu trữ mọi phiên Claude Code — không bỏ lỡ prompt, reasoning, hay tool call nào.

[![Version](https://img.shields.io/badge/version-0.0.1-blue)](https://git.microai.club/taipm/C2-Tracker/releases/tag/v0.0.1)
[![Rust](https://img.shields.io/badge/rust-1.95+-orange)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/tauri-v2-yellow)](https://v2.tauri.app/)

## Tổng quan

Khi làm việc với Claude Code, mọi prompt + response + reasoning + tool call đều được lưu trong file JSONL tại `~/.claude/projects/<encoded-cwd>/<session-uuid>.jsonl`. Tuy nhiên người dùng không có công cụ trực quan để xem lại, so sánh, hoặc phân tích các session này.

**C2-Tracker** đóng vai trò "recording studio" cho mọi phiên Claude Code:

- **Capture realtime** mọi session đang chạy + import session cũ.
- **Visualize** dạng chat 2 panel (sidebar sessions ↔ stream chính).
- **Analyze** metrics token spend, tool error rate, thinking time.
- **Persist** local lâu dài (SQLite), không phụ thuộc cloud.

## Kiến trúc

```
Claude Code session
    ├─ Ghi ~/.claude/projects/.../{session}.jsonl
    └─ Hooks settings.json → POST sang local server
              │
              ▼
       c2-engine daemon
         ├─ JSONL file watcher (notify, debounce 100ms)
         ├─ HTTP server 127.0.0.1:9786 + Bearer auth
         ├─ WebSocket broadcast
         └─ SQLite ~/.c2-tracker/c2.db
              │
              ▼
       c2-tracker UI (Tauri v2)
         ├─ Vanilla HTML/CSS/JS, dark frosted glass
         ├─ WS client realtime push (< 50ms)
         └─ Read-only SQLite query
```

## Cài đặt

### Yêu cầu

- macOS Apple Silicon (Linux x86_64 chưa được verify đầy đủ).
- Rust 1.95+ (`rustup toolchain install stable`).
- Tauri CLI v2: `cargo install tauri-cli --version "^2.0" --locked`.
- Claude Code đã được cài (CLI `claude --version` ≥ 2.0 khuyến nghị).

### Bước

```bash
git clone https://git.microai.club/taipm/C2-Tracker.git
cd C2-Tracker

# 1. Build engine (release, ~30s)
cd prototypes/jsonl-parser && cargo build --release && cd ../..

# 2. Cài LaunchAgent (macOS) — daemon auto-start
./prototypes/jsonl-parser/launchd/install.sh

# 3. Cài hooks vào Claude Code settings
./prototypes/jsonl-parser/target/release/c2-engine install-hooks --force-command-shim

# 4. Import toàn bộ session lịch sử (tùy chọn)
./prototypes/jsonl-parser/target/release/c2-engine import-all

# 5. Build & chạy UI
cd app
python3 gen_icons.py
cd src-tauri && cargo build --release
cargo tauri dev    # hoặc cargo tauri build cho production
```

## Sử dụng

- Mở app → sidebar trái hiển thị mọi session Claude Code (sort theo last_event_at).
- Click 1 session → stream phải render đầy đủ user/assistant/thinking/tool calls.
- Tab "Issues" → xem các `hook_non_blocking_error` attachments.
- Scroll lên đỉnh stream → tự load thêm 500 events cũ hơn.
- Mỗi prompt/response mới hiển thị < 100ms qua WebSocket push.

## Pipeline E2E

| Bước | Latency typical |
|------|-----------------|
| Claude Code ghi JSONL | ~50ms |
| `notify` detect + debounce | 100ms |
| Parser → SQLite insert | < 50ms |
| Broadcast WS → UI render | < 20ms |
| **Total** | **~200ms** |

## Documentation

- [REQ.md](./REQ.md) — đặc tả yêu cầu đầy đủ.
- [CHANGELOG.md](./CHANGELOG.md) — lịch sử versions.

## Status

- **Phase 1 (MVP)**: 9/10 milestone hoàn tất (v0.0.1).
- **Phase 1.1**: Export Markdown (M1.10) đang chờ.
- **Phase 2**: Redis cache + Postgres backend + dbpier integration.
- **Phase 3**: Multi-device sync + team mode.

## License

MIT.

---

Tác giả: [Phan Minh Tài](https://git.microai.club/taipm) — taipm.vn@gmail.com.
