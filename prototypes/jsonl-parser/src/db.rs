use anyhow::{Context, Result};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use std::path::Path;
use tracing::{debug, warn};

use crate::types::{EventCategory, ParsedEvent, SessionMeta};

pub type DbPool = Pool<SqliteConnectionManager>;

const SCHEMA: &str = include_str!("schema.sql");

pub fn open(db_path: &Path) -> Result<DbPool> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create dir {}", parent.display()))?;
    }
    let manager = SqliteConnectionManager::file(db_path);
    let pool = Pool::builder()
        .max_size(4)
        .build(manager)
        .with_context(|| format!("open db {}", db_path.display()))?;

    // migrate
    let conn = pool.get()?;
    conn.execute_batch(SCHEMA).context("run schema migration")?;
    // enable WAL for concurrent reads
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;

    Ok(pool)
}

/// Upsert session row. `session_id_override` takes priority over meta.session_id
/// (dùng filename UUID khi meta không có sessionId field).
pub fn upsert_session(pool: &DbPool, meta: &SessionMeta, jsonl_path: &str, session_id_override: &str) -> Result<()> {
    let conn = pool.get()?;
    let sid = if !session_id_override.is_empty() && session_id_override != "unknown" {
        session_id_override
    } else {
        meta.session_id.as_deref().unwrap_or("unknown")
    };
    let cwd = meta.cwd.as_deref().unwrap_or("");
    let project_name = meta.project_name.as_deref()
        .or_else(|| Path::new(cwd).file_name().and_then(|n| n.to_str()))
        .unwrap_or("");
    let now_ms = chrono::Utc::now().timestamp_millis();
    let started_at = meta.started_at.unwrap_or(now_ms);
    let last_event_at = meta.last_event_at.unwrap_or(now_ms);

    conn.execute(
        "INSERT INTO sessions (
            id, cwd, project_name, ai_title, summary,
            started_at, last_event_at, jsonl_path,
            total_input_tokens, total_output_tokens,
            total_cache_creation_tokens, total_cache_read_tokens,
            model, cli_version, git_branch, permission_mode,
            total_events, hook_success_count, hook_error_count
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)
         ON CONFLICT(id) DO UPDATE SET
            ai_title     = COALESCE(?4, ai_title),
            summary      = COALESCE(?5, summary),
            last_event_at = MAX(last_event_at, ?7),
            total_events  = total_events + ?17,
            total_input_tokens          = total_input_tokens + ?9,
            total_output_tokens         = total_output_tokens + ?10,
            total_cache_creation_tokens = total_cache_creation_tokens + ?11,
            total_cache_read_tokens     = total_cache_read_tokens + ?12,
            model           = COALESCE(?13, model),
            cli_version     = COALESCE(?14, cli_version),
            git_branch      = COALESCE(?15, git_branch),
            permission_mode = COALESCE(?16, permission_mode),
            hook_success_count = hook_success_count + ?18,
            hook_error_count   = hook_error_count + ?19",
        params![
            sid,
            cwd,
            project_name,
            meta.ai_title.as_deref(),
            meta.summary.as_deref(),
            started_at,
            last_event_at,
            jsonl_path,
            meta.input_tokens_delta,
            meta.output_tokens_delta,
            meta.cache_creation_delta,
            meta.cache_read_delta,
            meta.model.as_deref(),
            meta.cli_version.as_deref(),
            meta.git_branch.as_deref(),
            meta.permission_mode.as_deref(),
            meta.event_count as i64,
            meta.hook_success_delta,
            meta.hook_error_delta,
        ],
    ).context("upsert session")?;

    Ok(())
}

/// Upsert session từ hook (chỉ có session_id + path + cwd, không có token).
pub fn upsert_session_from_hook(
    pool: &DbPool,
    session_id: &str,
    jsonl_path: &str,
    cwd: &str,
) -> Result<()> {
    let conn = pool.get()?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let project_name = Path::new(cwd).file_name().and_then(|n| n.to_str()).unwrap_or("");

    conn.execute(
        "INSERT INTO sessions (id, cwd, project_name, started_at, last_event_at, jsonl_path)
         VALUES (?1, ?2, ?3, ?4, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
            last_event_at = MAX(last_event_at, ?4),
            jsonl_path = COALESCE(?5, jsonl_path)",
        params![session_id, cwd, project_name, now_ms, jsonl_path],
    ).context("upsert_session_from_hook")?;

    Ok(())
}

/// Lấy file_offset + file_inode hiện tại của session.
pub fn get_cursor(pool: &DbPool, session_id: &str) -> Result<(i64, Option<i64>)> {
    let conn = pool.get()?;
    let result = conn.query_row(
        "SELECT file_offset, file_inode FROM sessions WHERE id = ?1",
        params![session_id],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?)),
    );
    match result {
        Ok(v) => Ok(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok((0, None)),
        Err(e) => Err(e.into()),
    }
}

/// Cập nhật file_offset + file_inode sau khi parse.
pub fn update_cursor(pool: &DbPool, session_id: &str, offset: i64, inode: Option<u64>) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE sessions SET file_offset = ?1, file_inode = ?2 WHERE id = ?3",
        params![offset, inode.map(|i| i as i64), session_id],
    ).context("update_cursor")?;
    Ok(())
}

/// Insert batch events. Trả về (inserted, duplicates).
pub fn insert_events(pool: &DbPool, session_id: &str, events: &[ParsedEvent]) -> Result<(usize, usize)> {
    let mut conn = pool.get()?;
    let tx = conn.transaction()?;
    let mut inserted = 0usize;
    let mut dups = 0usize;

    for ev in events {
        let rows = tx.execute(
            "INSERT OR IGNORE INTO events (
                session_id, uuid, parent_uuid, \"type\", category, role,
                timestamp, content_kind, tool_name, tool_use_id,
                stop_reason, duration_ms, is_sidechain, is_error,
                attachment_kind, content, text_preview,
                input_tokens, output_tokens, cache_creation_tokens, cache_read_tokens
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
            params![
                session_id,
                ev.uuid,
                ev.parent_uuid,
                ev.event_type,
                ev.category.as_str(),
                ev.role,
                ev.timestamp_ms,
                ev.content_kind,
                ev.tool_name,
                ev.tool_use_id,
                ev.stop_reason,
                ev.duration_ms,
                ev.is_sidechain as i64,
                ev.is_error as i64,
                ev.attachment_kind,
                ev.content,
                ev.text_preview,
                ev.input_tokens,
                ev.output_tokens,
                ev.cache_creation_tokens,
                ev.cache_read_tokens,
            ],
        )?;
        if rows == 1 {
            inserted += 1;
            debug!(uuid = %ev.uuid, event_type = %ev.event_type, "inserted");
        } else {
            dups += 1;
            debug!(uuid = %ev.uuid, "duplicate, skipped");
        }
    }

    tx.commit()?;
    Ok((inserted, dups))
}

/// Mark session ended_at + has_stop_event.
pub fn mark_session_ended(pool: &DbPool, session_id: &str) -> Result<()> {
    let conn = pool.get()?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    conn.execute(
        "UPDATE sessions SET ended_at = ?1, has_stop_event = 1 WHERE id = ?2",
        params![now_ms, session_id],
    ).context("mark_session_ended")?;
    Ok(())
}

/// Update token totals từ JSONL (dùng cho Stop hook — reserved cho Tauri app).
#[allow(dead_code)]
pub fn add_token_totals(
    pool: &DbPool,
    session_id: &str,
    input: i64,
    output: i64,
    cache_create: i64,
    cache_read: i64,
) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE sessions SET
            total_input_tokens          = total_input_tokens + ?1,
            total_output_tokens         = total_output_tokens + ?2,
            total_cache_creation_tokens = total_cache_creation_tokens + ?3,
            total_cache_read_tokens     = total_cache_read_tokens + ?4
         WHERE id = ?5",
        params![input, output, cache_create, cache_read, session_id],
    ).context("add_token_totals")?;
    Ok(())
}

/// Đọc token từ runtime.json. Trả về (port, token).
pub fn read_runtime_token(runtime_path: &Path) -> Result<(u16, String)> {
    let raw = std::fs::read_to_string(runtime_path)
        .with_context(|| format!("read {}", runtime_path.display()))?;
    let v: serde_json::Value = serde_json::from_str(&raw)?;
    let port = v["port"].as_u64().unwrap_or(9786) as u16;
    let token = v["token"].as_str().context("missing token field")?.to_string();
    Ok((port, token))
}

/// Update port trong runtime.json mà giữ nguyên token. Dùng khi serve start với
/// --port khác với port đã lưu (vd. user đổi port hoặc lần đầu lưu sai).
pub fn update_runtime_port(runtime_path: &Path, port: u16, token: &str) -> Result<()> {
    let payload = serde_json::json!({ "port": port, "token": token });
    std::fs::write(runtime_path, payload.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(runtime_path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Sinh token mới, ghi vào runtime.json (chmod 600).
pub fn write_runtime_token(runtime_path: &Path, port: u16) -> Result<String> {
    use rand::RngCore;
    use base64::Engine;

    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = format!("ct_{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes));

    if let Some(parent) = runtime_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let payload = serde_json::json!({ "port": port, "token": token });
    std::fs::write(runtime_path, payload.to_string())?;

    // chmod 600
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(runtime_path, std::fs::Permissions::from_mode(0o600))?;
    }

    warn!("Generated new bearer token — saved to {}", runtime_path.display());
    Ok(token)
}
