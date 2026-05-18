mod db;
mod mock;

use db::{DbPool, EventDto, IssueDto, SessionDto, StatsDto};
use std::path::PathBuf;
use std::sync::OnceLock;

// Global pool: None nếu DB không tồn tại hoặc mở fail → fallback mock.
static DB_POOL: OnceLock<Option<DbPool>> = OnceLock::new();

fn init_pool() -> Option<DbPool> {
    // Env var override (dev/test)
    let db_path: PathBuf = if let Ok(p) = std::env::var("C2_DB_PATH") {
        PathBuf::from(p)
    } else {
        dirs::home_dir()
            .map(|h| h.join(".c2-tracker").join("c2.db"))
            .unwrap_or_else(|| PathBuf::from("/tmp/c2.db"))
    };

    match db::open_readonly(&db_path) {
        Ok(pool) => {
            eprintln!("[c2-tracker] DB opened: {}", db_path.display());
            Some(pool)
        }
        Err(e) => {
            eprintln!("[c2-tracker] DB unavailable ({}). Using mock data.", e);
            None
        }
    }
}

fn pool() -> Option<&'static DbPool> {
    DB_POOL.get_or_init(init_pool).as_ref()
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
fn get_sessions() -> Vec<SessionDto> {
    match pool() {
        Some(p) => match db::get_sessions_impl(p) {
            Ok(sessions) => sessions,
            Err(e) => {
                eprintln!("[c2-tracker] get_sessions error: {e}");
                mock_sessions_as_dto()
            }
        },
        None => mock_sessions_as_dto(),
    }
}

#[tauri::command]
fn get_events(session_id: String, limit: Option<u32>, before_id: Option<i64>) -> Vec<EventDto> {
    match pool() {
        Some(p) => match db::get_events_impl(p, &session_id, limit, before_id) {
            Ok(events) => events,
            Err(e) => {
                eprintln!("[c2-tracker] get_events error: {e}");
                // before_id phân trang không fallback mock (mock không có pagination)
                if before_id.is_some() { vec![] } else { mock_events_as_dto(&session_id) }
            }
        },
        None => if before_id.is_some() { vec![] } else { mock_events_as_dto(&session_id) },
    }
}

#[tauri::command]
fn get_issues(limit: Option<u32>) -> Vec<IssueDto> {
    match pool() {
        Some(p) => match db::get_issues_impl(p, limit) {
            Ok(issues) => issues,
            Err(e) => {
                eprintln!("[c2-tracker] get_issues error: {e}");
                vec![]
            }
        },
        None => vec![],
    }
}

#[tauri::command]
fn get_issue_count() -> u32 {
    match pool() {
        Some(p) => db::get_issue_count_impl(p).unwrap_or(0),
        None => 0,
    }
}

#[derive(serde::Serialize)]
struct RuntimeInfo {
    port: u16,
    token: String,
}

#[tauri::command]
fn get_runtime() -> Option<RuntimeInfo> {
    let path = dirs::home_dir()?.join(".c2-tracker").join("runtime.json");
    let raw = std::fs::read_to_string(&path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    Some(RuntimeInfo {
        port: v.get("port")?.as_u64()? as u16,
        token: v.get("token")?.as_str()?.to_string(),
    })
}

#[tauri::command]
fn get_stats() -> StatsDto {
    match pool() {
        Some(p) => match db::get_stats_impl(p) {
            Ok(stats) => stats,
            Err(e) => {
                eprintln!("[c2-tracker] get_stats error: {e}");
                mock_stats_as_dto()
            }
        },
        None => mock_stats_as_dto(),
    }
}

// ── Mock → DTO adapters (fallback) ────────────────────────────────────────────

fn mock_sessions_as_dto() -> Vec<SessionDto> {
    mock::sessions()
        .into_iter()
        .map(|s| SessionDto {
            id: s.id,
            project: s.project,
            cwd: s.cwd,
            status: s.status,
            started_at: s.started_at,
            last_event_at: s.last_event_at,
            duration_label: s.duration_label,
            event_count: s.event_count,
            token_count: s.token_count,
            model: s.model,
            ai_title: s.ai_title,
            git_branch: None,
            total_input_tokens: s.token_count as i64,
            total_output_tokens: 0,
        })
        .collect()
}

fn mock_events_as_dto(session_id: &str) -> Vec<EventDto> {
    mock::events_for(session_id)
        .into_iter()
        .enumerate()
        .map(|(i, e)| EventDto {
            id: i as i64,
            session_id: e.session_id,
            kind: e.kind,
            timestamp_label: e.timestamp_label,
            text: e.text,
            tool_name: e.tool_name,
            tool_input: e.tool_input,
            tool_output: e.tool_output,
            is_error: e.is_error,
            thinking_duration_ms: e.thinking_duration_ms,
            thinking_tokens: e.thinking_tokens,
            content: "{}".to_string(),
            category: "content".to_string(),
            content_kind: None,
            tool_use_id: None,
            timestamp: None,
        })
        .collect()
}

fn mock_stats_as_dto() -> StatsDto {
    let s = mock::quick_stats();
    StatsDto {
        sessions_today: s.sessions_today,
        tokens_today: s.tokens_today as u64,
        tool_calls_today: s.tool_calls_today,
        active_sessions_count: s.recording_count,
        recording_count: s.recording_count,
    }
}

// ── App entry ─────────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Khởi tạo pool sớm (trước khi UI load).
    let _ = pool();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_sessions,
            get_events,
            get_stats,
            get_runtime,
            get_issues,
            get_issue_count
        ])
        .run(tauri::generate_context!())
        .expect("error while running C2-Tracker");
}
