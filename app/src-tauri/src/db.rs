//! DB query layer — read-only access to c2.db written by c2-engine.
//!
//! Engine là writer duy nhất; app chỉ đọc. Pool mở với READONLY flag để
//! tránh lock conflict và bảo vệ DB.

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{OpenFlags, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub type DbPool = Pool<SqliteConnectionManager>;

// ── DTOs ──────────────────────────────────────────────────────────────────────

/// Khớp với shape frontend đang expect (session-list.js, stream.js, app.js).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDto {
    pub id: String,
    /// basename(cwd) — frontend dùng field "project"
    pub project: String,
    pub cwd: String,
    /// "live" | "idle" | "ended" — khớp STATUS_LABEL trong session-list.js
    pub status: String,
    /// Thời gian bắt đầu (unix ms) — frontend dùng để tính duration_label
    pub started_at: i64,
    pub last_event_at: i64,
    /// Human-readable duration, tính ở backend
    pub duration_label: String,
    pub event_count: u32,
    /// tổng input + output tokens
    pub token_count: u32,
    pub model: String,
    pub ai_title: String,
    pub git_branch: Option<String>,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
}

/// Event DTO. Frontend dùng: kind, timestamp_label, text, tool_name,
/// tool_input, tool_output, is_error, thinking_duration_ms, thinking_tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDto {
    pub id: i64,
    pub session_id: String,
    /// "user" | "assistant" | "thinking" | "tool_use" | "tool_result" | "summary" | ...
    pub kind: String,
    pub timestamp_label: String,
    /// text_preview cho display nhanh
    pub text: String,
    pub tool_name: Option<String>,
    /// raw JSON input (từ content column) — frontend prettify khi cần
    pub tool_input: Option<String>,
    /// raw JSON output — chỉ có với tool_result
    pub tool_output: Option<String>,
    pub is_error: bool,
    pub thinking_duration_ms: Option<u32>,
    pub thinking_tokens: Option<u32>,
    /// raw JSON line gốc — frontend dùng khi cần full data
    pub content: String,
    pub category: String,
    pub content_kind: Option<String>,
    pub tool_use_id: Option<String>,
    pub timestamp: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsDto {
    pub sessions_today: u32,
    pub tokens_today: u64,
    pub tool_calls_today: u32,
    /// active theo §12.3 logic
    pub active_sessions_count: u32,
    // alias cho frontend compat (app.js dùng sessions_today, tokens_today, tool_calls_today)
    pub recording_count: u32,
}

// ── Pool init ─────────────────────────────────────────────────────────────────

/// Mở read-only pool. Trả Err nếu file không tồn tại hoặc không phải SQLite.
pub fn open_readonly(db_path: &Path) -> Result<DbPool, String> {
    if !db_path.exists() {
        return Err(format!("DB not found: {}", db_path.display()));
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI;
    let manager = SqliteConnectionManager::file(db_path).with_flags(flags);
    Pool::builder()
        .max_size(4)
        .build(manager)
        .map_err(|e| format!("open pool {}: {e}", db_path.display()))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Tính session status theo §12.3.
fn compute_status(has_stop_event: i64, last_event_at: i64, now_ms: i64) -> &'static str {
    let age_minutes = (now_ms - last_event_at) / 60_000;
    if has_stop_event != 0 || age_minutes >= 30 {
        "ended"
    } else if age_minutes < 2 {
        "live" // frontend dùng "live" không phải "active"
    } else {
        "idle"
    }
}

/// Format duration từ ms thành "Xm" hoặc "Xh Ym".
fn fmt_duration(started_at: i64, last_event_at: i64) -> String {
    let secs = ((last_event_at - started_at).max(0)) / 1000;
    let mins = secs / 60;
    if mins < 60 {
        format!("{mins}m")
    } else {
        format!("{}h {}m", mins / 60, mins % 60)
    }
}

/// Format unix ms timestamp thành "HH:MM:SS".
fn fmt_time(ts_ms: Option<i64>) -> String {
    match ts_ms {
        None => "—".to_string(),
        Some(ms) => {
            use chrono::{Local, TimeZone};
            match Local.timestamp_millis_opt(ms) {
                chrono::LocalResult::Single(dt) => {
                    dt.format("%H:%M:%S").to_string()
                }
                _ => "—".to_string(),
            }
        }
    }
}

/// Start-of-day unix ms (local timezone).
fn today_start_ms() -> i64 {
    use chrono::{Local, TimeZone};
    let now = Local::now();
    // naive date at midnight in local tz
    let naive_sod = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
    Local.from_local_datetime(&naive_sod)
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or_else(|| now.timestamp_millis())
}

// ── Query functions ───────────────────────────────────────────────────────────

pub fn get_sessions_impl(pool: &DbPool) -> Result<Vec<SessionDto>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let now_ms = chrono::Utc::now().timestamp_millis();

    let mut stmt = conn
        .prepare(
            "SELECT id, COALESCE(project_name, ''), COALESCE(cwd, ''),
                    started_at, last_event_at, total_events,
                    total_input_tokens, total_output_tokens,
                    COALESCE(model, ''), COALESCE(ai_title, ''),
                    has_stop_event, git_branch
             FROM sessions
             ORDER BY last_event_at DESC
             LIMIT 100",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,   // id
                row.get::<_, String>(1)?,   // project_name
                row.get::<_, String>(2)?,   // cwd
                row.get::<_, i64>(3)?,      // started_at
                row.get::<_, i64>(4)?,      // last_event_at
                row.get::<_, i64>(5)?,      // total_events
                row.get::<_, i64>(6)?,      // total_input_tokens
                row.get::<_, i64>(7)?,      // total_output_tokens
                row.get::<_, String>(8)?,   // model
                row.get::<_, String>(9)?,   // ai_title
                row.get::<_, i64>(10)?,     // has_stop_event
                row.get::<_, Option<String>>(11)?, // git_branch
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut sessions = Vec::new();
    for row in rows {
        let (id, project, cwd, started_at, last_event_at, total_events,
             total_input, total_output, model, ai_title, has_stop, git_branch) =
            row.map_err(|e| e.to_string())?;

        let status = compute_status(has_stop, last_event_at, now_ms);
        let token_count = (total_input + total_output).min(u32::MAX as i64) as u32;
        let duration_label = fmt_duration(started_at, last_event_at);

        sessions.push(SessionDto {
            id,
            project,
            cwd,
            status: status.to_string(),
            started_at,
            last_event_at,
            duration_label,
            event_count: total_events.min(u32::MAX as i64) as u32,
            token_count,
            model,
            ai_title,
            git_branch,
            total_input_tokens: total_input,
            total_output_tokens: total_output,
        });
    }
    Ok(sessions)
}

pub fn get_events_impl(pool: &DbPool, session_id: &str, limit: Option<u32>) -> Result<Vec<EventDto>, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let lim = limit.unwrap_or(500) as i64;

    // Filter: chỉ lấy content category (MVP). Meta (last-prompt, file-history-snapshot,
    // permission-mode, ai-title) là metadata, không phải nội dung chat — bỏ khỏi stream.
    // Attachment (hook events) defer Phase 2.
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, \"type\", category, content_kind,
                    tool_name, tool_use_id, is_error, text_preview,
                    content, timestamp, duration_ms,
                    input_tokens, output_tokens
             FROM events
             WHERE session_id = ?1
               AND category = 'content'
             ORDER BY timestamp ASC, id ASC
             LIMIT ?2",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![session_id, lim], |row| {
            Ok((
                row.get::<_, i64>(0)?,               // id
                row.get::<_, String>(1)?,             // session_id
                row.get::<_, String>(2)?,             // type
                row.get::<_, String>(3)?,             // category
                row.get::<_, Option<String>>(4)?,     // content_kind
                row.get::<_, Option<String>>(5)?,     // tool_name
                row.get::<_, Option<String>>(6)?,     // tool_use_id
                row.get::<_, i64>(7)?,                // is_error
                row.get::<_, Option<String>>(8)?,     // text_preview
                row.get::<_, String>(9)?,             // content
                row.get::<_, Option<i64>>(10)?,       // timestamp
                row.get::<_, Option<i64>>(11)?,       // duration_ms
                row.get::<_, Option<i64>>(12)?,       // input_tokens
                row.get::<_, Option<i64>>(13)?,       // output_tokens
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut events = Vec::new();
    for row in rows {
        let (id, sid, ev_type, category, content_kind, tool_name, tool_use_id,
             is_error_int, text_preview, content, timestamp, duration_ms,
             input_tokens, output_tokens) =
            row.map_err(|e| e.to_string())?;

        // Map event type + content_kind → frontend "kind"
        let kind = map_kind(&ev_type, content_kind.as_deref());

        let text = text_preview.unwrap_or_default();
        let timestamp_label = fmt_time(timestamp);

        // Thiết lập tool_input / tool_output từ raw content JSON
        let (tool_input, tool_output) = extract_tool_io(&kind, &content);

        // thinking tokens từ output_tokens (content_kind = "thinking")
        let (thinking_duration_ms, thinking_tokens) = if kind == "thinking" {
            (
                duration_ms.map(|d| d.min(u32::MAX as i64) as u32),
                output_tokens.map(|t| t.min(u32::MAX as i64) as u32),
            )
        } else {
            (None, None)
        };

        events.push(EventDto {
            id,
            session_id: sid,
            kind,
            timestamp_label,
            text,
            tool_name,
            tool_input,
            tool_output,
            is_error: is_error_int != 0,
            thinking_duration_ms,
            thinking_tokens,
            content,
            category,
            content_kind,
            tool_use_id,
            timestamp,
        });
    }
    Ok(events)
}

pub fn get_stats_impl(pool: &DbPool) -> Result<StatsDto, String> {
    let conn = pool.get().map_err(|e| e.to_string())?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let today_ms = today_start_ms();

    // sessions today
    let sessions_today: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE started_at >= ?1",
            params![today_ms],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    // tokens today
    let tokens_today: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(total_input_tokens + total_output_tokens), 0)
             FROM sessions WHERE started_at >= ?1",
            params![today_ms],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    // tool calls today (events có tool_name IS NOT NULL trong sessions hôm nay)
    let tool_calls_today: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM events e
             JOIN sessions s ON e.session_id = s.id
             WHERE e.tool_name IS NOT NULL
               AND s.started_at >= ?1",
            params![today_ms],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    // active sessions count theo §12.3: no stop event AND last_event_at < 2 phút ago
    let active_count: u32 = {
        let cutoff_active = now_ms - 2 * 60_000; // last event phải trong 2 phút gần đây
        conn.query_row(
            "SELECT COUNT(*) FROM sessions
             WHERE has_stop_event = 0
               AND last_event_at >= ?1",
            params![cutoff_active],
            |row| row.get(0),
        )
        .unwrap_or(0)
    };

    Ok(StatsDto {
        sessions_today,
        tokens_today: tokens_today.max(0) as u64,
        tool_calls_today,
        active_sessions_count: active_count,
        recording_count: active_count,
    })
}

// ── Mapping helpers ───────────────────────────────────────────────────────────

/// Map DB event type + content_kind → frontend "kind" string.
fn map_kind(ev_type: &str, content_kind: Option<&str>) -> String {
    match (ev_type, content_kind) {
        ("assistant", Some("thinking")) => "thinking".to_string(),
        ("assistant", Some("tool_use")) => "tool_use".to_string(),
        ("user", Some("tool_result")) => "tool_result".to_string(),
        _ => ev_type.to_string(),
    }
}

/// Trích tool_input / tool_output từ raw JSON content.
/// tool_use → lấy "input" field; tool_result → lấy "content" field.
fn extract_tool_io(kind: &str, content: &str) -> (Option<String>, Option<String>) {
    let v: serde_json::Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(_) => return (None, None),
    };

    match kind {
        "tool_use" => {
            // content là raw JSONL line; tool_use block ở message.content[] hoặc có thể là block trực tiếp
            let input = find_in_message_content(&v, "tool_use", "input")
                .or_else(|| v.get("input").cloned());
            let s = input.map(|x| {
                if x.is_string() { x.as_str().unwrap_or("").to_string() }
                else { x.to_string() }
            });
            (s, None)
        }
        "tool_result" => {
            // tool_result content ở toolUseResult.content hoặc message.content[].content
            let from_ptr = v.pointer("/toolUseResult/content").cloned();
            let from_msg = find_in_message_content(&v, "tool_result", "content");
            let output = from_ptr.or(from_msg);
            let s = output.map(|x| {
                if x.is_string() { x.as_str().unwrap_or("").to_string() }
                else { x.to_string() }
            });
            (None, s)
        }
        _ => (None, None),
    }
}

/// Tìm field trong message.content[] theo type filter.
fn find_in_message_content(v: &serde_json::Value, block_type: &str, field: &str) -> Option<serde_json::Value> {
    let blocks = v.pointer("/message/content")?.as_array()?;
    for block in blocks {
        if block.get("type").and_then(|t| t.as_str()) == Some(block_type) {
            return block.get(field).cloned();
        }
    }
    None
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_db_path() -> PathBuf {
        // Ưu tiên env var override, sau đó thử /tmp/wire-test.db
        if let Ok(p) = std::env::var("C2_TEST_DB") {
            return PathBuf::from(p);
        }
        PathBuf::from("/tmp/wire-test.db")
    }

    #[test]
    fn test_get_sessions_real_db() {
        let db_path = test_db_path();
        if !db_path.exists() {
            eprintln!("SKIP: DB not found at {}. Run c2-engine parse-once first.", db_path.display());
            return;
        }
        let pool = open_readonly(&db_path).expect("open pool");
        let sessions = get_sessions_impl(&pool).expect("get_sessions");
        assert!(!sessions.is_empty(), "expected >= 1 session in DB");
        let s = &sessions[0];
        assert!(!s.id.is_empty(), "session id must not be empty");
        assert!(
            matches!(s.status.as_str(), "live" | "idle" | "ended"),
            "unexpected status: {}",
            s.status
        );
        eprintln!("sessions[0]: id={} project={} status={} events={} tokens={}",
            s.id, s.project, s.status, s.event_count, s.token_count);
    }

    #[test]
    fn test_get_events_real_db() {
        let db_path = test_db_path();
        if !db_path.exists() {
            eprintln!("SKIP: DB not found.");
            return;
        }
        let pool = open_readonly(&db_path).expect("open pool");
        let sessions = get_sessions_impl(&pool).expect("get_sessions");
        if sessions.is_empty() {
            eprintln!("SKIP: no sessions in DB.");
            return;
        }
        let session_id = &sessions[0].id;
        let events = get_events_impl(&pool, session_id, Some(50)).expect("get_events");
        eprintln!("session {} → {} events (limit 50)", session_id, events.len());
        for ev in &events {
            assert!(!ev.kind.is_empty(), "kind must not be empty");
        }
    }

    #[test]
    fn test_get_stats_real_db() {
        let db_path = test_db_path();
        if !db_path.exists() {
            eprintln!("SKIP: DB not found.");
            return;
        }
        let pool = open_readonly(&db_path).expect("open pool");
        let stats = get_stats_impl(&pool).expect("get_stats");
        eprintln!("stats: sessions_today={} tokens_today={} tools_today={} active={}",
            stats.sessions_today, stats.tokens_today, stats.tool_calls_today, stats.active_sessions_count);
        // Chỉ assert không panic, không assert giá trị cụ thể vì tuỳ DB
    }

    #[test]
    fn test_compute_status_logic() {
        let now = 1_000_000_000_000i64; // arbitrary ms
        // has_stop_event → ended
        assert_eq!(compute_status(1, now - 1000, now), "ended");
        // >= 30 phút → ended
        assert_eq!(compute_status(0, now - 31 * 60_000, now), "ended");
        // < 2 phút, no stop → live
        assert_eq!(compute_status(0, now - 60_000, now), "live");
        // 2-30 phút → idle
        assert_eq!(compute_status(0, now - 5 * 60_000, now), "idle");
    }

    #[test]
    fn test_fmt_duration() {
        assert_eq!(fmt_duration(0, 45 * 60_000), "45m");
        assert_eq!(fmt_duration(0, 72 * 60_000), "1h 12m");
        assert_eq!(fmt_duration(0, 0), "0m");
    }

    #[test]
    fn test_map_kind() {
        assert_eq!(map_kind("assistant", Some("thinking")), "thinking");
        assert_eq!(map_kind("assistant", Some("tool_use")), "tool_use");
        assert_eq!(map_kind("user", Some("tool_result")), "tool_result");
        assert_eq!(map_kind("user", None), "user");
        assert_eq!(map_kind("assistant", Some("text")), "assistant");
    }
}
