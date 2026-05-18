# C2-Tracker

## Yêu cầu

> Hiện tại khi sử dụng claude code, chúng ta đang prompt và nhận response một cách tự nhiên, dữ liệu này mất đi khi kết thúc session rất lãng phí, kể cả quá trình suy luận của claude code cũng bị mất đi mà không thu thập để khai thác được, đây là nguồn dữ liệu rất có ý nghĩa. Tôi dự định xây dựng một ứng dụng Desktop (Rust 1.95, Tauri V2) để:
>
> * \[1\] Theo dõi và thu thập realtime toàn bộ nội dung trong quá trình làm việc với claude code
> * \[2\] Dữ liệu thu thập realtime sẽ được tổ chức lưu ở (a) redis (cache nhằm tra cứu nhanh), (b) postgresql (nhằm lưu trữ lâu dài). Kết nối với db thông qua dbpier (<https://dbpier.codereview.club/>) hoặc trực tiếp, cho phép cấu hình
> * \[3\] UI gồm 02 phần, bên trái - bên phải (8:4), bên phải là stream (coi như là recording quá trình làm việc) giao tiếp giữa người dùng với claude code (giống như một cửa sổ chat giữa người dùng và claude code), bên trái chúng ta sẽ có danh sách các session đang làm việc với claude code và một số thông tin khác (bạn tự đề xuất)
>
> https://v2.tauri.app/
>
> https://blog.rust-lang.org/2026/04/16/Rust-1.95.0/


---

## UI Layout (Proposed)

### Split Layout: Left (8/12) — Right (4/12)

```
┌─────────────────────────────────────────────────────────────────┐
│  HEADER: Logo "C2-Tracker"    [● Recording]   [⚙ Settings]      │
├───────────────────────────┬─────────────────────────────────────┤
│  LEFT (8/12)              │  RIGHT (4/12)                       │
│                           │                                     │
│  ┌─ SESSIONS ──────────┐  │  ┌─ STREAM ──────────────────────┐ │
│  │ ▶ Session 001       │  │  │                               │ │
│  │   Session 002  ◀──  │  │  │  [User]: lệnh gì đó...       │ │
│  │   Session 003       │  │  │                               │ │
│  │   Session 004       │  │  │  [Claude]: response...        │ │
│  │   ...               │  │  │                               │ │
│  └─────────────────────┘  │  │  [User]: tiếp...              │ │
│                           │  │                               │ │
│  ┌─ METADATA ──────────┐  │  │  [Claude]: ...                │ │
│  │ Started: 10:30 AM   │  │  │                               │ │
│  │ Duration: 45m       │  │  │                               │ │
│  │ Prompts: 12        │  │  │                               │ │
│  │ Tokens: ~8,240     │  │  │                               │ │
│  └─────────────────────┘  │  └───────────────────────────────┘ │
│                           │                                     │
│  ┌─ INSIGHTS ──────────┐  │  ┌─ INPUT ────────────────────────┐ │
│  │ Most used commands  │  │  │  > Type here...        [Send]  │ │
│  │ Error rate          │  │  └───────────────────────────────┘ │
│  │ Code patterns       │  │                                     │
│  └─────────────────────┘  │                                     │
└───────────────────────────┴─────────────────────────────────────┘
```

### Component Details

#### Left Panel (8/12 width)

| Component | Mô tả |
|-----------|-------|
| **Session List** | Danh sách các Claude Code session, click để chọn xem lại |
| **Metadata** | Thông tin session: thời gian bắt đầu, duration, số prompts, token count |
| **Insights** | Phân tích: commands hay dùng, error rate, code patterns |

#### Right Panel (4/12 width)

| Component | Mô tả |
|-----------|-------|
| **Stream** | Realtime transcript giữa user ↔ Claude Code (terminal-style output) |
| **Input** | Text input để prompt trực tiếp (hoặc capture từ Claude Code) |

### Design Notes

* **Recording indicator** (●) — nhấp nháy đỏ khi đang thu thập data
* **Dark theme** — phù hợp với terminal/IDE vibe
* **Split ratio 8:4** — left panel đủ rộng để xem session list + metadata
* **Terminal-style font** cho stream — monospace, dễ đọc log


---

## Tech Stack

* **Desktop**: Rust 1.95 + Tauri V2
* **Cache**: Redis (realtime lookup)
* **Storage**: PostgreSQL (long-term storage)
* **Frontend**: WebView (Tauri built-in)