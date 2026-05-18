# C2-Tracker — Tài liệu Yêu cầu (REQ)

> **Phiên bản**: 0.4 · **Cập nhật**: 2026-05-18 · **Owner**: taipm
>
> Tài liệu này định nghĩa phạm vi, kiến trúc và tiêu chí nghiệm thu cho dự án C2-Tracker. Nguồn gốc: `c2-tracker.md` (đặc tả sơ khai) + 4 quyết định kiến trúc (xem mục §2.2).

---

## 1. Tổng quan

### 1.1. Vấn đề

Khi làm việc với Claude Code, mọi prompt, response, chuỗi reasoning (extended thinking), tool call và kết quả tool đều **được lưu sẵn vào file JSONL** tại `~/.claude/projects/<encoded-cwd>/<session-uuid>.jsonl` — nhưng:

- Người dùng **không có công cụ trực quan** để xem lại, so sánh, hoặc phân tích các session đó.
- Khi session kết thúc, tri thức trong đó (cách giải bug, prompt hay, sai lầm hay gặp) bị **lãng quên** — không được surface lại.
- Quá trình **reasoning của Claude** (`thinking` blocks) cực kỳ giá trị để học hỏi nhưng đang bị bỏ phí.

### 1.2. Giải pháp

**C2-Tracker** là một desktop app (Rust 1.95 + Tauri v2) chạy ngầm trên macOS/Linux, đóng vai trò **"recording studio"** cho mọi phiên Claude Code:

- **Capture realtime**: theo dõi mọi session đang chạy + import session cũ.
- **Visualize**: UI dạng chat 2 panel (sidebar sessions ↔ stream chính).
- **Analyze**: tính metrics (tokens, duration, tool usage, error rate) và surface insights.
- **Persist**: lưu local lâu dài, không phụ thuộc cloud.

### 1.3. Phi mục tiêu (Non-goals) cho MVP

| Phi mục tiêu | Lý do |
|---|---|
| Thay thế `claude-mem` (full-text / vector search) | claude-mem đã làm tốt — C2-Tracker bổ sung, không thay thế. Xem §3. |
| Edit / replay session về phía Claude Code | Out of scope: app **chỉ đọc**, không ghi vào `~/.claude`. |
| Cloud sync / multi-device | Local-first. Sync là phase 3. |
| Hỗ trợ Windows | macOS + Linux trước (môi trường chính của owner). |
| Phân tích cross-team (team analytics) | Single-user app. KPI dashboard là phase 3. |

---

## 2. Quyết định kiến trúc

### 2.1. Bảng quyết định

| # | Quyết định | Giá trị | Ghi chú |
|---|---|---|---|
| AD-1 | **Capture mode** | JSONL watcher (primary) + Hooks (trigger realtime) | §4.1 |
| AD-2 | **UI ratio** | Trái 4 (sessions) — Phải 8 (stream) | Đảo so với spec gốc |
| AD-3 | **MVP storage** | SQLite local (`~/.c2-tracker/c2.db`) | Redis + PG + dbpier dời sang Phase 2 |
| AD-4 | **Positioning** | Realtime recording + visualization (bổ sung claude-mem) | §3 |

### 2.2. Lý do (rationale)

- **AD-1**: JSONL là nguồn dữ liệu **chính thức** (Claude Code tự ghi). Hooks chỉ làm trigger để UI cập nhật nhanh hơn — không phải nguồn duy nhất. Nếu user không cài hooks, app vẫn hoạt động (chậm hơn vài trăm ms).
- **AD-2**: Stream chat là vùng đọc chính, cần rộng. Sidebar session list chỉ cần đủ hiện tiêu đề + thời gian.
- **AD-3**: SQLite đủ cho 1 user, **zero setup**, single-file backup. Khi nào dữ liệu vượt vài GB hoặc cần share team mới đổi sang PG.
- **AD-4**: Tránh ôm đồm. claude-mem mạnh ở search → C2-Tracker mạnh ở live monitoring + UX.

---

## 3. Định vị so với các công cụ hiện có

| Tool | Vai trò | Overlap với C2-Tracker |
|---|---|---|
| `claude-mem` (MCP) | Search/memory engine offline trên transcript đã chunk hoá | Trùng "đọc JSONL". Khác: claude-mem optimize cho retrieval; C2-Tracker optimize cho live viewing. |
| `TimelineReport` skill | Báo cáo timeline theo session | Trùng "thống kê". Khác: TimelineReport là 1 lần / on-demand; C2-Tracker là dashboard liên tục. |
| Claude Code native `--resume` | Liệt kê + nối lại session | Trùng "session list". Khác: native CLI không có UI / metrics / cross-session view. |

**USP của C2-Tracker**:
1. **Live recording indicator** — biết ngay có session nào đang chạy ở đâu.
2. **Side-by-side comparison** — xem 2 session song song.
3. **Tracker-style metrics** — token spend, tool error rate, thinking time, theo thời gian.
4. **GUI** — không cần CLI / không cần Claude Desktop.

---

## 4. Cơ chế thu thập dữ liệu

### 4.1. Kiến trúc capture

```
┌─────────────────────────────────────────────────────────┐
│  Claude Code (terminal)                                 │
│         │                                               │
│         ├─► Ghi JSONL ──► ~/.claude/projects/.../*.jsonl│
│         │                          ▲                    │
│         │                          │ (1) notify watcher │
│         │                          │                    │
│         └─► Hooks ──► POST http://127.0.0.1:PORT/event  │
│                                    │ (2) realtime trigger│
│                                    ▼                    │
│              ┌──────────────────────────────────┐       │
│              │   C2-Tracker (Tauri + Rust)      │       │
│              │   ├─ JSONL parser (incremental)  │       │
│              │   ├─ Hooks HTTP server (local)   │       │
│              │   ├─ Event bus                   │       │
│              │   ├─ SQLite (persist)            │       │
│              │   └─ WebView UI                  │       │
│              └──────────────────────────────────┘       │
└─────────────────────────────────────────────────────────┘
```

### 4.2. JSONL watcher (primary source)

- **Crate**: [`notify`](https://crates.io/crates/notify) (cross-platform fsevents/inotify).
- **Watch root**: `~/.claude/projects/` recursive, filter `*.jsonl`.
- **Incremental parse**:
  - Mỗi file giữ một con trỏ offset (last_byte) trong bảng `session_cursor`.
  - Khi file đổi: `seek(offset) → read_to_end() → parse từng dòng JSON`.
  - Append event mới vào DB + broadcast WebSocket sang UI.
- **Schema 1 dòng JSONL** (tóm tắt — xem chi tiết tại §5.1):
  - `type`: `user` | `assistant` | `system` | `summary`
  - `uuid`, `parentUuid`, `timestamp`, `cwd`, `sessionId`, `version`
  - `message.content[]`: `text` | `tool_use` | `tool_result` | `thinking`
- **Tolerance**:
  - File có thể bị truncate (rare) — phát hiện bằng `inode change` hoặc `size < offset` → reset offset, re-parse từ đầu.
  - File rất lớn (>50MB) — parse stream từng dòng, không load full vào RAM.

### 4.3. Hooks HTTP server (realtime trigger)

Claude Code hỗ trợ **native `http` hook type** với Bearer token — KHÔNG cần curl shim. Tham chiếu: https://code.claude.com/docs/en/hooks

#### 4.3.1. Bootstrap

- C2-Tracker bind `127.0.0.1:<port-cố-định-từ-config>` lúc start (default `9786`, fallback nếu bị chiếm).
- Sinh ngẫu nhiên Bearer token mỗi lần start, lưu vào `~/.c2-tracker/runtime.json` (chmod 600):
  ```json
  { "port": 9786, "token": "ct_<32-bytes-base64>" }
  ```
- Lệnh `c2-tracker install-hooks` ghi `~/.claude/settings.json` (merge, không overwrite):
  ```json
  {
    "hooks": {
      "UserPromptSubmit": [{"hooks":[{
        "type": "http",
        "url": "http://127.0.0.1:9786/hooks/user-prompt-submit",
        "headers": { "Authorization": "Bearer ${CT_TOKEN}" },
        "allowedEnvVars": ["CT_TOKEN"],
        "timeout": 5
      }]}]
    }
  }
  ```
  Token được tham chiếu qua biến môi trường `CT_TOKEN` (lưu trong `~/.c2-tracker/env` được Claude Code đọc qua `allowedEnvVars`).
- Server validate `Authorization: Bearer <token>` mọi request, reject 401 nếu sai.

#### 4.3.2. Ma trận hooks — priority cho C2-Tracker

Claude Code có **28+ hook events**. C2-Tracker không cần tất cả — phân theo tier:

| Tier | Event | Lý do dùng | MVP? |
|---|---|---|---|
| **T1** | `SessionStart` | Tạo record session sớm trước khi JSONL flush. Lấy `transcript_path` để watcher bind ngay. | ✅ |
| **T1** | `UserPromptSubmit` | Báo "session vừa active" → refresh UI realtime. Có `prompt` text full → preview ngay. | ✅ |
| **T1** | `Stop` | Mark turn kết thúc, finalize `stop_reason`, push notification "turn done". | ✅ |
| **T1** | `SessionEnd` | Set `ended_at`, đóng watcher cho file đó. Có `reason` (`logout`/`clear`/...). | ✅ |
| **T2** | `PostToolUse` | Tool result đến ngay, hiển thị trong stream nhanh hơn watcher ~500ms. | ⏳ Phase 2 |
| **T2** | `PostToolUseFailure` | Surface vào "Issues" panel với `error` field. | ⏳ Phase 2 |
| **T2** | `Notification` | Hiện badge khi Claude Code gửi notification (vd. need permission). | ⏳ Phase 2 |
| **T2** | `PreCompact` / `PostCompact` | Lưu mốc compact để render divider trong stream. | ⏳ Phase 2 |
| **T3** | `SubagentStart` / `SubagentStop` | Track sidechain hierarchy, link parent ↔ subagent session. | ⏳ Phase 3 |
| **T3** | `TaskCreated` / `TaskCompleted` | Render TodoList panel ngang stream. | ⏳ Phase 3 |
| **T3** | `StopFailure` | Đo rate limit / API error frequency. | ⏳ Phase 3 |
| **T4 — KHÔNG dùng** | `PreToolUse`, `PermissionRequest`, `PermissionDenied`, `PostToolBatch`, `UserPromptExpansion`, `Setup`, `WorktreeCreate/Remove`, `Elicitation*`, `InstructionsLoaded`, `ConfigChange`, `CwdChanged`, `FileChanged`, `TeammateIdle` | Hoặc đã có trong JSONL, hoặc high-frequency low-value, hoặc out of scope passive recorder. | — |

**Tổng kết**: MVP cần wire 4 endpoint (T1). Phase 2 thêm 5 endpoint (T2). Phase 3 thêm 5 endpoint (T3).

#### 4.3.3. Endpoint map (MVP)

| Method | Path | Hook event | Action |
|---|---|---|---|
| POST | `/hooks/session-start` | `SessionStart` | Upsert `sessions` row, bind watcher cho `transcript_path` ngay |
| POST | `/hooks/user-prompt-submit` | `UserPromptSubmit` | Trigger watcher.refresh, broadcast WS "new prompt" |
| POST | `/hooks/stop` | `Stop` | Mark turn done, update token totals, broadcast WS |
| POST | `/hooks/session-end` | `SessionEnd` | Set `ended_at`, `has_stop_event=1`, unbind watcher |

**Payload common** (Claude Code gửi):
```json
{
  "session_id": "...",
  "transcript_path": "/Users/.../session.jsonl",
  "cwd": "/...",
  "permission_mode": "default",
  "hook_event_name": "UserPromptSubmit",
  "effort": { "level": "medium" }
}
```
Plus event-specific fields (`source`, `prompt`, `reason`, ...).

**Response** từ C2-Tracker: luôn `{"continue": true, "suppressOutput": true}` (200 OK). C2-Tracker là **passive observer** — KHÔNG block, KHÔNG modify Claude Code behavior. Không bao giờ return `decision: block`.

#### 4.3.4. Resilience

- **Hook fail không được crash Claude Code**: timeout 5s, server return 200 ngay cả khi xử lý nội bộ lỗi (push event vào channel async, không sync DB write trong handler).
- **Server down**: Claude Code nhận connection refused → non-blocking error (theo docs). Không ảnh hưởng user.
- **Token rotate**: mỗi lần restart app, sinh token mới + update `~/.c2-tracker/env`. Hook tự đọc env lúc fire.
- **Disable nhanh**: user có thể set `"disableAllHooks": true` trong `settings.json` — app vẫn hoạt động qua watcher.
- **Uninstall**: lệnh `c2-tracker uninstall-hooks` remove các entry C2-Tracker đã ghi (dùng marker comment hoặc tag riêng).

#### 4.3.5. Fallback (không có hooks)

Nếu user không chạy `install-hooks`, watcher dùng polling 500ms (notify event spam, gộp debounce 100ms). Tất cả tính năng MVP vẫn hoạt động, chỉ chậm hơn ~500ms.

### 4.4. Event lifecycle

```
[Hook fired] ─► HTTP server ─► trigger watcher.refresh(session_id)
                                              │
[File change] ─► notify event ─────────────────┤
                                              ▼
                              ┌──────────────────────────────┐
                              │  Read offset → tail file     │
                              │  Parse new lines             │
                              │  Insert into events table    │
                              │  Update session metadata     │
                              │  Broadcast WS to UI          │
                              └──────────────────────────────┘
```

---

## 5. Data model

### 5.1. SQLite schema (MVP) — đã update theo prototype findings (§5.3)

```sql
CREATE TABLE sessions (
  id                    TEXT PRIMARY KEY,    -- sessionId từ JSONL
  cwd                   TEXT NOT NULL,
  project_name          TEXT,                -- derived: basename(cwd)
  ai_title              TEXT,                -- aiTitle mới nhất (event type=ai-title)
  summary               TEXT,                -- summary từ compact event (nếu có)
  started_at            INTEGER NOT NULL,    -- unix ms (timestamp event đầu tiên)
  ended_at              INTEGER,             -- NULL nếu chưa kết thúc
  last_event_at         INTEGER NOT NULL,
  jsonl_path            TEXT NOT NULL,
  file_offset           INTEGER NOT NULL DEFAULT 0,
  file_inode            INTEGER,
  total_events          INTEGER DEFAULT 0,
  total_input_tokens    INTEGER DEFAULT 0,
  total_output_tokens   INTEGER DEFAULT 0,
  total_cache_creation_tokens INTEGER DEFAULT 0,
  total_cache_read_tokens     INTEGER DEFAULT 0,
  model                 TEXT,
  cli_version           TEXT,                -- field `version` (Claude Code version)
  git_branch            TEXT,
  permission_mode       TEXT,                -- mode mới nhất
  has_stop_event        INTEGER DEFAULT 0,
  hook_success_count    INTEGER DEFAULT 0,
  hook_error_count      INTEGER DEFAULT 0
);

CREATE INDEX idx_sessions_last ON sessions(last_event_at DESC);
CREATE INDEX idx_sessions_cwd  ON sessions(cwd);

CREATE TABLE events (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id    TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  uuid          TEXT UNIQUE,                 -- NULL với meta events không có uuid
  parent_uuid   TEXT,
  type          TEXT NOT NULL,               -- user|assistant|system|summary|ai-title|attachment|...
  category      TEXT NOT NULL,               -- 'content' | 'meta' | 'attachment'
  role          TEXT,                        -- user | assistant
  timestamp     INTEGER,                     -- NULL với meta events không có ts
  content_kind  TEXT,                        -- text|tool_use|tool_result|thinking|(NULL nếu meta)
  tool_name     TEXT,                        -- chỉ khi tool_use
  tool_use_id   TEXT,                        -- link tool_use ↔ tool_result (toolUseID/id)
  stop_reason   TEXT,                        -- end_turn|tool_use|max_tokens|...
  duration_ms   INTEGER,                     -- chỉ khi có durationMs
  is_sidechain  INTEGER DEFAULT 0,           -- isSidechain (subagent)
  is_error      INTEGER DEFAULT 0,           -- tool_result.is_error hoặc attachment kiểu error
  attachment_kind TEXT,                      -- hook_success|hook_non_blocking_error|task_reminder|...
  content       TEXT NOT NULL,               -- raw JSON line nguyên gốc
  text_preview  TEXT,                        -- 200 char đầu cho search nhanh
  input_tokens  INTEGER,
  output_tokens INTEGER,
  cache_creation_tokens INTEGER,
  cache_read_tokens     INTEGER
);

CREATE INDEX idx_events_session  ON events(session_id, timestamp);
CREATE INDEX idx_events_tool     ON events(tool_name) WHERE tool_name IS NOT NULL;
CREATE INDEX idx_events_category ON events(category);
CREATE INDEX idx_events_tooluse  ON events(tool_use_id) WHERE tool_use_id IS NOT NULL;

CREATE TABLE app_config (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
```

**Idempotency**:
- Events có `uuid`: dùng `INSERT OR IGNORE` với UNIQUE constraint.
- Events không có `uuid` (vd. `permission-mode`, `last-prompt`, `ai-title`, `file-history-snapshot`): dedupe bằng hash SHA-1 của `content` (cột `uuid` lưu `meta:<hash>`).

### 5.2. JSONL line format — schema thực tế (đã verify với prototype)

Chia event thành **3 category** để xử lý khác nhau:

#### 5.2.1. Content events (`category='content'`)

| `type` | Ghi chú |
|---|---|
| `user` | Prompt từ user HOẶC kết quả tool gửi về model (có `toolUseResult`) |
| `assistant` | Response từ Claude — text, tool_use, thinking |
| `system` | Nội dung system message giữa phiên |
| `summary` | Compact summary (khi `/compact`) — có field `summary` + `leafUuid` |

**Field chuẩn của content event**:

| Field | Kiểu | Ghi chú |
|---|---|---|
| `uuid` | string | Bắt buộc, UNIQUE |
| `parentUuid` | string \| null | Tree linkage |
| `type` | enum trên | |
| `timestamp` | ISO-8601 | |
| `sessionId` | string | |
| `cwd` | string | |
| `version` | string | Claude Code version (vd. `2.1.143`) |
| `gitBranch` | string | |
| `userType` | string | `external` thường gặp |
| `isSidechain` | boolean | `true` = subagent event |
| `entrypoint` | string | |
| `requestId` | string | Có ở `assistant` |
| `promptId` | string | Có ở `user` |
| `permissionMode` | string | Có ở `user` (mode tại thời điểm submit) |
| `sourceToolAssistantUUID` | string | Có ở `user` khi là tool_result, link sang assistant uuid |
| `toolUseResult` | object | Chỉ ở `user`, kết quả tool gửi về |
| `message` | object | Xem dưới |

**`message` object**:
- `role`: `user` \| `assistant`
- `model`: vd. `claude-opus-4-7`
- `stop_reason`: `end_turn` \| `tool_use` \| `max_tokens` \| `pause_turn` (chỉ assistant)
- `usage`: `{ input_tokens, output_tokens, cache_creation_input_tokens, cache_read_input_tokens, ... }`
- `content[]`: array các block:
  - `{ type: "text", text: "..." }`
  - `{ type: "thinking", thinking: "...", signature: "..." }`
  - `{ type: "tool_use", id: "toolu_...", name: "Read", input: {...} }`
  - `{ type: "tool_result", tool_use_id: "toolu_...", is_error: bool, content: ... }`

#### 5.2.2. Meta events (`category='meta'`)

Không có `uuid`/`timestamp` chuẩn → dedupe bằng hash SHA-1 của raw line.

| `type` | Field đặc trưng | Mục đích |
|---|---|---|
| `ai-title` | `aiTitle: string` | Title do Claude tự sinh cho session. Xuất hiện nhiều lần khi đổi context — UI dùng cái **mới nhất**. |
| `permission-mode` | `permissionMode: string` | Mode mới (`default`, `acceptEdits`, ...) |
| `last-prompt` | `leafUuid: string`, `lastPrompt: ...` | Marker cho prompt cuối — dùng resume |
| `file-history-snapshot` | `snapshot`, `messageId`, `isSnapshotUpdate` | Snapshot files đã sửa — phục vụ undo |

#### 5.2.3. Attachment events (`category='attachment'`)

| `type` | Field đặc trưng |
|---|---|
| `attachment` | `attachment.type` (= `hook_success` \| `hook_non_blocking_error` \| `task_reminder` \| `deferred_tools_delta` \| `mcp_instructions_delta` \| `skill_listing` \| ...) |

Attachment thường có thêm `attachment.hookEvent`, `attachment.toolUseID`, `attachment.stderr`, `attachment.exitCode`.

**Phân loại nhanh**:
- `hook_success` → metric `hook_success_count` của session.
- `hook_non_blocking_error` → metric `hook_error_count` + surface trong "Issues" panel.
- Khác → store raw, hiển thị on-demand.

### 5.3. Findings từ prototype parser (2026-05-18)

Chạy parser trên session đang live (`729a9549-...jsonl`, 349KB, 188 dòng):

| Insight | Tác động |
|---|---|
| Có **8 type events** (vs spec gốc liệt kê 4) | Đã bổ sung §5.2 |
| `aiTitle` đổi 8 lần trong 20 phút | Lưu version mới nhất + có thể track history (Phase 2) |
| Cache hit ratio rất cao (`cache_read 2.5M` vs `input 79`) | Phải đo `cache_*_tokens` để cost analytics đúng |
| Hook errors **32% tổng hook events** (28/86) | Surface vào "Issues" panel trong UI |
| 0 parse error trên 188 dòng | Confirm tolerance: `serde_json::Value` + skip-on-error là đủ |
| Schema có `toolUseID`, `stop_reason`, `durationMs` | Đã thêm vào `events` table |
| File tăng size khi đang đọc | Confirm incremental parse cần `seek(offset)` + tail |

Source code prototype: `prototypes/jsonl-parser/`.

---

## 6. UI / UX

### 6.1. Layout tổng thể (AD-2: trái 4 — phải 8)

```
┌───────────────────────────────────────────────────────────────────────────┐
│  C2-Tracker     ● Recording (3 active)             ⚙ Settings   ? Help    │
├──────────────────────────┬────────────────────────────────────────────────┤
│  LEFT 4/12               │  RIGHT 8/12                                    │
│                          │                                                │
│  ┌─ Filter ─────────┐    │  ┌─ Session header ─────────────────────────┐ │
│  │ 🔍 Search...     │    │  │ project: c2-tracker · started 10:30 · 45m │ │
│  │ ▾ All projects   │    │  │ tokens 12.4k · 23 events · model: opus    │ │
│  │ ▾ Last 7 days    │    │  └──────────────────────────────────────────┘ │
│  └──────────────────┘    │                                                │
│                          │  ┌─ Stream (chat-style transcript) ─────────┐ │
│  ┌─ Sessions ───────┐    │  │ [user] 10:30  "viết REQ.md cho c2..."    │ │
│  │ ●live · c2-track │◀───┤  │                                          │ │
│  │   2m ago · 23ev  │    │  │ [thinking]  3.2s · 412 tok               │ │
│  │ ───────────────  │    │  │   ▸ click to expand reasoning            │ │
│  │   idle · llm-srv │    │  │                                          │ │
│  │   1h ago · 89ev  │    │  │ [assistant] 10:30  "Đã đọc xong..."       │ │
│  │ ───────────────  │    │  │                                          │ │
│  │   done · sage    │    │  │ [tool_use] Read /Volumes/.../c2.md       │ │
│  │   yest · 41ev    │    │  │ [tool_result] 80 lines                    │ │
│  │ ...              │    │  │                                          │ │
│  └──────────────────┘    │  │ [user] 10:32 "thêm phần Roadmap..."      │ │
│                          │  │ ...                                        │ │
│  ┌─ Quick stats ────┐    │  │                                          ▼ │ │
│  │ Today  4 sess    │    │  └──────────────────────────────────────────┘ │
│  │ Tokens 48.2k     │    │                                                │
│  │ Tools  127       │    │  ┌─ Action bar ─────────────────────────────┐ │
│  └──────────────────┘    │  │ [Copy] [Export MD] [Open in editor]      │ │
│                          │  └──────────────────────────────────────────┘ │
└──────────────────────────┴────────────────────────────────────────────────┘
```

### 6.2. Component breakdown

| Component | Vị trí | Mô tả |
|---|---|---|
| Header | Top | Logo + recording indicator (số session đang active) + settings |
| Filter | Left top | Search box + filter project/date/status |
| Session list | Left mid | Card mỗi session: status dot, project, thời gian, số event. Active nhấp nháy. |
| Quick stats | Left bottom | Tổng theo ngày: số session, tokens, tool calls |
| Session header | Right top | Metadata session đang chọn |
| Stream | Right mid | Transcript dạng chat, group theo turn (user → thinking → assistant → tools) |
| Action bar | Right bottom | Copy / Export / Open file gốc |

### 6.3. Tương tác

- **Auto-follow**: nếu user đang xem session active và stream scroll xuống cuối, event mới tự xuất hiện. Nếu user scroll lên trên thì "freeze" + hiện nút "↓ N new messages".
- **Thinking blocks**: collapsed mặc định, click để mở (giống Claude.ai).
- **Tool calls**: hiển thị tên tool + input ngắn gọn, click để xem full input/output.
- **Keyboard**: `↑↓` chọn session, `Cmd+K` mở search palette, `Cmd+E` export.
- **Theme**: Dark mặc định (terminal vibe). Light theme phase 2.

### 6.4. Empty / error states

| State | UI |
|---|---|
| Chưa có session nào | "Chưa có session. Mở Claude Code ở 1 thư mục bất kỳ để bắt đầu thu thập." + nút "Install hooks" |
| Watcher không chạy được (permission) | Banner đỏ "Không có quyền truy cập `~/.claude/projects/`" + hướng dẫn fix |
| JSONL parse error | Skip line đó, log vào `~/.c2-tracker/parse-errors.log`, không crash UI |

---

## 7. Tech stack chi tiết

| Layer | Công nghệ | Phiên bản |
|---|---|---|
| Desktop framework | Tauri | v2.x |
| Backend lang | Rust | 1.95+ |
| File watcher | `notify` | latest |
| HTTP server | `axum` (local hooks endpoint) | latest |
| DB driver | `rusqlite` + `r2d2` | latest |
| Serialization | `serde` + `serde_json` | latest |
| Frontend | Vanilla HTML/CSS/JS (không bundler) hoặc Vue 3 (cân nhắc) | — |
| Frontend ↔ Backend | Tauri IPC `invoke` + event channel | — |
| Realtime push | Tauri `emit` (qua WebView event) | — |

**Lý do không dùng React/bundler**: phù hợp pattern `tauri-desktop` của owner (skill `tauri-desktop`), build nhanh, không Node toolchain. Cân nhắc lại nếu UI phức tạp vượt mức.

---

## 8. Bảo mật & Privacy

- **Local-only**: không gửi data ra ngoài.
- **File permissions**: SQLite DB ở `~/.c2-tracker/c2.db` chmod 600.
- **Secrets**: prompt có thể chứa API key, token. **MVP không encrypt at rest** (rely on disk encryption). Phase 2 có thể thêm option encrypt DB qua SQLCipher.
- **Hooks endpoint**: bind `127.0.0.1` only, generate random token mỗi lần start, hooks phải gửi `Authorization: Bearer <token>` (token lưu cùng port file, chmod 600).
- **PII**: log app KHÔNG ghi nội dung event, chỉ ghi event count + error.
- **Export**: trước khi export markdown, app cảnh báo nếu phát hiện pattern giống secret (regex AWS keys, GH tokens) — không tự redact, chỉ warn.

---

## 9. Roadmap

### Phase 1 — MVP (mục tiêu: 2-3 tuần)

| ID | Hạng mục | Acceptance |
|---|---|---|
| M1.1 | Scaffold Tauri v2 + Rust 1.95 + SQLite | App start được, DB tự migrate |
| M1.2 | JSONL watcher | Detect file mới + file change trong `~/.claude/projects/**`, log ra console |
| M1.3 | JSONL parser incremental + idempotent | Re-parse 1 file 2 lần không sinh duplicate event trong DB |
| M1.4 | Schema sessions/events + write path | 1 session test → ≥10 event ghi đúng vào SQLite |
| M1.5 | UI khung 4:8, dark theme | Chạy `cargo tauri dev`, layout đúng tỉ lệ, responsive cửa sổ |
| M1.6 | Session list (left) + stream (right) hiển thị data thật | Mở app → thấy session hiện có, click → thấy transcript |
| M1.7 | Auto-follow + thinking collapse + tool call rendering | Tương tác đúng spec §6.3 |
| M1.8 | Hooks server (4 T1 endpoint) + `install-hooks` / `uninstall-hooks` | (1) `c2-tracker install-hooks` merge đúng vào `~/.claude/settings.json` (idempotent, không nhân đôi); (2) 4 endpoint trả 200 < 50ms cho payload bất kỳ; (3) sai/thiếu Bearer → 401; (4) sau cài, mở session mới: `sessions` row tạo trong < 200ms từ `SessionStart`; (5) prompt mới hiển thị UI < 300ms; (6) Stop event update `total_*_tokens` đúng; (7) kill server đột ngột, Claude Code vẫn chạy bình thường (non-blocking); (8) `uninstall-hooks` rollback sạch (diff settings.json = empty). |
| M1.9 | Quick stats + session metadata header | Đếm đúng tokens, events, duration |
| M1.10 | Export 1 session sang Markdown | File MD chứa đầy đủ user/assistant/tool, đọc được |

**Definition of done MVP**: dùng được app trên máy owner trong 1 tuần liên tục, capture ≥50 session, không mất event nào so với JSONL gốc.

### Phase 2 — Storage backends + analytics (sau MVP)

- Pluggable storage: SQLite (default) / PostgreSQL / dbpier HTTP.
- Redis cache layer (optional, cho lookup nhanh khi DB lớn).
- Insights panel: most-used tools, error rate, prompt length distribution, token cost estimate.
- FTS5 search trên SQLite (full-text trên `events.text_preview` + `content`).
- Cross-session diff/compare.

### Phase 3 — Mở rộng (tương lai)

- Multi-device sync (qua Gitea hoặc self-hosted server).
- Team mode (ẩn danh metrics đẩy về KPI dashboard — tích hợp `10x-metrics-dashboard`).
- Tích hợp với `claude-mem` qua MCP: nút "Search in claude-mem" trong UI.
- Plugin system: cho phép user viết Lua/Rhai script chạy trên event stream (vd. auto-export khi session > N tokens).
- Windows support.

---

## 10. Tiêu chí nghiệm thu (Acceptance Criteria)

### 10.1. Functional

- [ ] App detect được session mới trong **< 1s** kể từ khi Claude Code tạo file JSONL.
- [ ] Event mới hiển thị trong UI trong **< 500ms** kể từ khi Claude Code ghi vào file (có hooks).
- [ ] Có thể import lại toàn bộ session lịch sử (≥ 1000 session) không crash, không OOM.
- [ ] Re-parse 1 file 10 lần không tạo duplicate event.
- [ ] Export Markdown round-trip: copy MD → đọc lại → nội dung giống transcript gốc.

### 10.2. Performance

- [ ] App khi idle (3 session active): RAM < 200 MB, CPU < 2%.
- [ ] Mở session 50k tokens: render xong stream trong **< 2s**.
- [ ] DB size sau 1000 session ~ 200 MB là chấp nhận được.

### 10.3. Reliability

- [ ] Force-kill app khi đang parse: lần start sau parse lại từ `file_offset`, không mất / không trùng event.
- [ ] File JSONL bị truncate: app detect, reset offset, parse lại — không crash.
- [ ] Dòng JSONL malformed: log warning, skip, không crash.

### 10.4. UX

- [ ] Lần đầu mở app: có hướng dẫn 3 bước (install hooks / mở Claude Code / xem session).
- [ ] Recording indicator rõ ràng: ● đỏ khi có ≥1 session active.
- [ ] Keyboard shortcut `Cmd+K`, `↑↓`, `Cmd+E` hoạt động.

---

## 11. Rủi ro

| # | Rủi ro | Hướng xử lý |
|---|---|---|
| R1 | Claude Code thay đổi format JSONL ở version mới | Pin `version` field vào DB, graceful fallback khi gặp field lạ (skip + log, không crash) |
| R2 | Hook system thay đổi tên event | Versioned hook installer, tự kiểm tra `claude --version` trước khi `install-hooks` |
| R3 | File `.jsonl` lên vài chục MB → notify event spam | Debounce 100ms per file, batch parse |
| R4 | Multiple Claude Code instance cùng ghi nhiều file | `notify` xử lý OK; cần test với ≥5 instance song song trong M1.2 |
| R5 | dbpier auth chưa rõ → block Phase 2 storage abstraction | Đánh dấu blocker, owner cần làm việc với dbpier team trước Phase 2 |
| R6 | Native `http` hook type yêu cầu Claude Code 2.x (owner đang 2.1.143 — OK). User dùng version cũ (<2.0) sẽ fail vì chỉ hỗ trợ `command` type | `c2-tracker install-hooks` detect version qua `claude --version`. Nếu < 2.0 → tự rơi xuống fallback `command` shim với curl (lưu token trong file `~/.c2-tracker/env`, source trước khi gọi). Phát warning rõ ràng cho user upgrade. Không ảnh hưởng kiến trúc — chỉ là rendering khác trong installer. |

## 12. Quyết định đã chốt (kèm rationale)

Các điểm chi tiết được chốt sau khi review draft v0.1:

### 12.1. Summary event xử lý kép

JSONL có dòng `{"type":"summary","summary":"...","leafUuid":"..."}` (sinh ra khi compact / resume).

- **Ghi vào `events`** với `content_kind = 'summary'` → giữ vị trí thời gian, có thể render trong stream.
- **Đồng thời denormalize vào `sessions.summary`** → hiển thị tên session trong sidebar không cần JOIN.
- Nếu 1 session có nhiều summary (compact nhiều lần), `sessions.summary` lấy cái **mới nhất**.

### 12.2. Insights metrics — phase 1 chỉ tool_name

| Metric | Cách tính | Phase |
|---|---|---|
| Most used tools | `GROUP BY tool_name FROM events WHERE tool_name IS NOT NULL` | 1 |
| Tool error rate | `SUM(is_error) / COUNT(*) per tool` | 1 |
| Avg thinking time | Diff timestamp giữa user prompt → assistant text trong cùng turn, lọc turn có `thinking` block | 1 |
| Bash verb breakdown (`git`, `cargo`, `npm`...) | Regex trên `Bash` tool input | 2 |
| Prompt length distribution | Histogram `LENGTH(text)` cho user events | 2 |
| Estimated cost ($) | `input_tokens * price_in + output_tokens * price_out` theo `model` | 2 |

**Lý do KHÔNG đo slash commands / bash verbs trong phase 1**: cần parse `tool_input`/`text` thêm, giá trị thấp hơn so với tool name vốn đã có sẵn cột chỉ mục.

### 12.3. Session status — 2 chiều (Stop event × thời gian)

```
status =
  'ended' if has_stop_event OR (now - last_event_at >= 30 phút)
  'active' if NOT has_stop_event AND (now - last_event_at < 2 phút)
  'idle'   otherwise
```

| Ngưỡng | Giá trị | Lý do |
|---|---|---|
| Active cutoff | 2 phút | Claude Code suy nghĩ nặng (extended thinking, tool chain) thường ≤ 1-2 phút. < 1 phút quá nhạy, > 5 phút quá trễ. |
| Ended cutoff | 30 phút | User đi họp/ăn trưa rồi quay lại resume vẫn đếm là "cùng session". Mốc 1h quá dài, 15p quá ngắn. |
| Hard signal | Stop event từ hooks | Khi có thì luôn ưu tiên — không cần đợi timeout. |

Trạng thái được tính **on read** (subquery) trong MVP, không lưu cứng. Phase 2 có thể thêm background job re-compute mỗi 30s để giảm CPU UI.

### 12.4. dbpier — defer

Phase 2 sẽ chốt sau khi có:
- Tài liệu API dbpier (HTTP endpoints, auth scheme).
- Decision có dùng dbpier hay direct PG protocol (vì direct PG đã đủ cho single-user).

Đánh dấu R5 ở §11 làm blocker.

---

## 13. Tham khảo

- Tauri v2 docs: https://v2.tauri.app/
- Rust 1.95 release notes: https://blog.rust-lang.org/2026/04/16/Rust-1.95.0/
- Claude Code hooks reference: `~/.claude/settings.json` (xem `/help`)
- File `c2-tracker.md` — spec gốc của owner
- dbpier: https://dbpier.codereview.club/
- Skill `tauri-desktop` — pattern scaffold owner đang dùng

---

*Tài liệu này là draft v0.4 — schema đã verify bằng prototype parser (§5.3), hooks integration đã khảo sát đầy đủ 28 events và phân tier T1-T4 (§4.3). Phase 2 còn chờ thông tin dbpier (R5).*
