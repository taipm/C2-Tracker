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

### Cách nhanh nhất — one-line installer (~30 giây, anonymous)

```bash
curl -fsSL https://raw.githubusercontent.com/taipm/C2-Tracker/main/install.sh | bash
```

Cần macOS Apple Silicon + Claude Code đã cài (tùy chọn — hook tự skip nếu chưa có).

Installer tự động:
1. Tải binary `c2-engine` về `~/.c2-tracker/bin/` (từ GitHub release, không cần auth)
2. Cài LaunchAgent (daemon auto-start mỗi lần boot)
3. Cài hooks vào `~/.claude/settings.json` nếu Claude Code đã có
4. Thêm `~/.c2-tracker/bin` vào `PATH` qua `~/.zshrc` / `~/.bashrc` / fish config

#### Tùy chỉnh

```bash
# Skip hooks (cài thủ công sau)
curl -fsSL https://raw.githubusercontent.com/taipm/C2-Tracker/main/install.sh | C2_SKIP_HOOKS=1 bash

# Skip LaunchAgent (chạy daemon thủ công bằng c2-engine serve)
curl -fsSL https://raw.githubusercontent.com/taipm/C2-Tracker/main/install.sh | C2_SKIP_AGENT=1 bash

# Đổi version / install dir
curl -fsSL https://raw.githubusercontent.com/taipm/C2-Tracker/main/install.sh | \
  C2_VERSION=v0.0.1 C2_INSTALL_DIR=/opt/c2 bash
```

#### Tải từ Gitea mirror internal (`git.microai.club`)

Cần `GITEA_TOKEN` hoặc `~/.netrc` entry:
```bash
GITEA_TOKEN=xxx bash -c "$(curl -fsSL -H "Authorization: token $GITEA_TOKEN" \
  https://git.microai.club/taipm/C2-Tracker/raw/branch/main/install.sh)" -- C2_SOURCE=gitea
```

---

### Yêu cầu

- **macOS Apple Silicon** (Linux x86_64 chưa được verify đầy đủ).
- **Rust 1.95+**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Tauri CLI v2** (chỉ cần khi muốn dùng UI): `cargo install tauri-cli --version "^2.0" --locked`
- **Python 3** (chỉ cần để sinh icons placeholder cho UI build).
- **Claude Code** CLI ≥ 2.0 khuyến nghị (Claude Code < 2.0 vẫn work với fallback command shim).

### Cách A — Dùng release tarball (chỉ engine, không UI)

Nhanh nhất nếu chỉ cần daemon thu thập (chưa cần UI). Phù hợp khi muốn xem qua sqlite CLI hoặc tích hợp downstream.

```bash
# 1. Tải artifact từ Gitea release
curl -L -O https://git.microai.club/taipm/C2-Tracker/releases/download/v0.0.1/c2-tracker-v0.0.1-darwin-arm64.tar.gz
tar xzf c2-tracker-v0.0.1-darwin-arm64.tar.gz
cd c2-tracker-v0.0.1-darwin-arm64

# 2. Cài LaunchAgent (daemon auto-start)
chmod +x install.sh c2-engine
./install.sh

# 3. Cài hooks vào ~/.claude/settings.json
./c2-engine install-hooks --force-command-shim

# 4. (Tùy chọn) Import session lịch sử có sẵn
./c2-engine import-all
```

Kiểm tra: `launchctl list | grep c2tracker` phải thấy `club.microai.c2tracker.engine`.

### Cách B — Build từ source (full bao gồm UI)

Bắt buộc nếu muốn dùng giao diện desktop. ~5-10 phút lần đầu (Rust + Tauri compile).

```bash
git clone https://github.com/taipm/C2-Tracker.git
cd C2-Tracker

# 1. Build engine release
cd prototypes/jsonl-parser
cargo build --release
cd ../..

# 2. Cài LaunchAgent
./prototypes/jsonl-parser/launchd/install.sh

# 3. Cài hooks
./prototypes/jsonl-parser/target/release/c2-engine install-hooks --force-command-shim

# 4. Import session lịch sử (tùy chọn)
./prototypes/jsonl-parser/target/release/c2-engine import-all

# 5. Build & chạy UI
cd app
python3 gen_icons.py        # sinh 4 PNG icon placeholder
cargo tauri dev             # mở UI dev mode
# hoặc: cargo tauri build   # tạo .app bundle (chưa code-signed)
```

### Gỡ cài đặt

```bash
# Stop daemon
launchctl unload ~/Library/LaunchAgents/club.microai.c2tracker.engine.plist
rm ~/Library/LaunchAgents/club.microai.c2tracker.engine.plist

# Gỡ hooks khỏi Claude Code settings (restore từ backup tự động)
./c2-engine uninstall-hooks

# Xoá dữ liệu (tuỳ chọn)
rm -rf ~/.c2-tracker  # DB, token, log
```

### Troubleshooting

| Triệu chứng | Nguyên nhân | Cách fix |
|---|---|---|
| `launchctl: Path not specified` | `install.sh` chưa patch path | Bản v0.0.1 nâng cấp đã fix, dùng commit mới nhất |
| Daemon không listen 9786 | Port bị chiếm | `lsof -i :9786` xem process nào, `kill` hoặc đổi port trong plist |
| UI hiển thị "Chưa có session" | DB rỗng | Chạy `c2-engine import-all` để populate |
| Hooks không fire | Claude Code chưa reload settings | Restart `claude` session hoặc gửi prompt mới |
| WebSocket disconnect liên tục | Token mismatch | `cat ~/.c2-tracker/env` so với `~/.c2-tracker/runtime.json` |

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
