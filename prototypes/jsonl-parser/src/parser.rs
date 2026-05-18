// JSONL incremental parser — logic giữ lại từ prototype, mở rộng để trả về
// ParsedEvent + SessionMeta để insert vào DB.

use anyhow::{Context, Result};
use chrono::DateTime;
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::Path,
};
use tracing::warn;

use crate::types::{EventCategory, ParsedEvent, SessionMeta};

const KNOWN_TOP_KEYS: &[&str] = &[
    "type", "uuid", "parentUuid", "timestamp", "sessionId", "cwd", "version",
    "gitBranch", "userType", "isSidechain", "message", "requestId", "entrypoint",
    "promptId", "permissionMode", "toolUseResult", "sourceToolAssistantUUID",
    "aiTitle", "leafUuid", "summary", "attachment", "isSnapshotUpdate", "messageId",
    "snapshot", "isCompactSummary", "isVisibleInTranscriptOnly",
];

/// Kết quả parse một batch (từ offset đến EOF).
pub struct ParseBatch {
    pub events: Vec<ParsedEvent>,
    pub meta: SessionMeta,
    /// Offset mới sau khi đọc xong (= vị trí EOF).
    pub new_offset: u64,
    pub parse_errors: usize,
}

/// Parse stats để in ra CLI (giữ lại output cũ của prototype).
#[derive(Default, Debug)]
pub struct Stats {
    pub total_lines: usize,
    pub parse_errors: usize,
    pub types: BTreeMap<String, usize>,
    pub content_kinds: BTreeMap<String, usize>,
    pub tools: BTreeMap<String, usize>,
    pub tool_errors: BTreeMap<String, usize>,
    pub models: BTreeMap<String, usize>,
    pub git_branches: BTreeMap<String, usize>,
    pub versions: BTreeMap<String, usize>,
    pub permission_modes: BTreeMap<String, usize>,
    pub attachment_kinds: BTreeMap<String, usize>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub sidechain_events: usize,
    pub first_ts: Option<DateTime<chrono::Utc>>,
    pub last_ts: Option<DateTime<chrono::Utc>>,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub ai_title: Option<String>,
    pub summary: Option<String>,
    pub unknown_top_keys: BTreeMap<String, usize>,
}

fn ts_to_ms(ts: &str) -> Option<i64> {
    ts.parse::<DateTime<chrono::Utc>>().ok().map(|t| t.timestamp_millis())
}

fn sha1_hex(s: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(s.as_bytes());
    hex::encode(hasher.finalize())
}

fn text_preview(text: &str) -> String {
    text.chars().take(200).collect()
}

/// Lấy text content đầu tiên trong message.content[].
fn first_text(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => Some(s.clone()),
        Value::Array(arr) => {
            for block in arr {
                if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                    if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                        return Some(t.to_string());
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn parse_line(line: &str) -> Result<(ParsedEvent, SessionMeta)> {
    let v: Value = serde_json::from_str(line).context("parse JSON")?;
    let obj = v.as_object().context("not an object")?;

    let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("(no-type)").to_string();
    let raw_uuid = obj.get("uuid").and_then(|v| v.as_str()).map(|s| s.to_string());
    let parent_uuid = obj.get("parentUuid").and_then(|v| v.as_str()).map(|s| s.to_string());
    let timestamp_ms = obj.get("timestamp").and_then(|v| v.as_str()).and_then(ts_to_ms);
    let is_sidechain = obj.get("isSidechain").and_then(|v| v.as_bool()).unwrap_or(false);

    // Xác định category
    let category = match ty.as_str() {
        "user" | "assistant" | "system" | "summary" => EventCategory::Content,
        "attachment" => EventCategory::Attachment,
        _ => EventCategory::Meta,
    };

    // uuid hoặc "meta:<sha1>"
    let uuid = raw_uuid.unwrap_or_else(|| format!("meta:{}", sha1_hex(line)));

    let mut meta = SessionMeta::default();

    // Session-level fields
    if let Some(sid) = obj.get("sessionId").and_then(|v| v.as_str()) {
        meta.session_id = Some(sid.to_string());
    }
    if let Some(cwd) = obj.get("cwd").and_then(|v| v.as_str()) {
        meta.cwd = Some(cwd.to_string());
        meta.project_name = Path::new(cwd).file_name()
            .and_then(|n| n.to_str()).map(|s| s.to_string());
    }
    if let Some(ver) = obj.get("version").and_then(|v| v.as_str()) {
        meta.cli_version = Some(ver.to_string());
    }
    if let Some(branch) = obj.get("gitBranch").and_then(|v| v.as_str()) {
        if !branch.is_empty() {
            meta.git_branch = Some(branch.to_string());
        }
    }
    if let Some(pm) = obj.get("permissionMode").and_then(|v| v.as_str()) {
        meta.permission_mode = Some(pm.to_string());
    }
    if let Some(ms) = timestamp_ms {
        meta.started_at = Some(ms);
        meta.last_event_at = Some(ms);
    }

    // Event-specific fields
    let mut role: Option<String> = None;
    let mut content_kind: Option<String> = None;
    let mut tool_name: Option<String> = None;
    let mut tool_use_id: Option<String> = None;
    let mut stop_reason: Option<String> = None;
    let mut duration_ms: Option<i64> = None;
    let mut is_error = false;
    let mut attachment_kind: Option<String> = None;
    let mut text_prev: Option<String> = None;
    let mut input_tokens: Option<i64> = None;
    let mut output_tokens: Option<i64> = None;
    let mut cache_creation_tokens: Option<i64> = None;
    let mut cache_read_tokens: Option<i64> = None;

    match ty.as_str() {
        "ai-title" => {
            if let Some(t) = obj.get("aiTitle").and_then(|v| v.as_str()) {
                meta.ai_title = Some(t.to_string());
            }
        }
        "summary" => {
            if let Some(s) = obj.get("summary").and_then(|v| v.as_str()) {
                meta.summary = Some(s.to_string());
                text_prev = Some(text_preview(s));
            }
            content_kind = Some("summary".to_string());
        }
        "permission-mode" => {
            if let Some(m) = obj.get("permissionMode").and_then(|v| v.as_str()) {
                meta.permission_mode = Some(m.to_string());
            }
        }
        "attachment" => {
            if let Some(a) = obj.get("attachment").and_then(|v| v.as_object()) {
                let kind = a.get("type").and_then(|v| v.as_str()).unwrap_or("?").to_string();
                attachment_kind = Some(kind.clone());
                match kind.as_str() {
                    "hook_success" => meta.hook_success_delta += 1,
                    "hook_non_blocking_error" => {
                        meta.hook_error_delta += 1;
                        is_error = true;
                    }
                    _ => {}
                }
            }
        }
        "assistant" | "user" => {
            if let Some(msg) = obj.get("message") {
                if let Some(r) = msg.get("role").and_then(|v| v.as_str()) {
                    role = Some(r.to_string());
                }
                if let Some(m) = msg.get("model").and_then(|v| v.as_str()) {
                    meta.model = Some(m.to_string());
                }
                if let Some(sr) = msg.get("stop_reason").and_then(|v| v.as_str()) {
                    stop_reason = Some(sr.to_string());
                }
                if let Some(dm) = obj.get("durationMs").and_then(|v| v.as_i64()) {
                    duration_ms = Some(dm);
                }
                if let Some(usage) = msg.get("usage").and_then(|v| v.as_object()) {
                    let it = usage.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                    let ot = usage.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                    let ct = usage.get("cache_creation_input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                    let rt = usage.get("cache_read_input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                    if it > 0 { input_tokens = Some(it); meta.input_tokens_delta += it; }
                    if ot > 0 { output_tokens = Some(ot); meta.output_tokens_delta += ot; }
                    if ct > 0 { cache_creation_tokens = Some(ct); meta.cache_creation_delta += ct; }
                    if rt > 0 { cache_read_tokens = Some(rt); meta.cache_read_delta += rt; }
                }

                // Walk content blocks
                if let Some(content_arr) = msg.get("content").and_then(|v| v.as_array()) {
                    if let Some(first) = content_arr.first() {
                        let kind = first.get("type").and_then(|v| v.as_str()).unwrap_or("text").to_string();
                        content_kind = Some(kind.clone());
                        match kind.as_str() {
                            "text" => {
                                if let Some(t) = first.get("text").and_then(|v| v.as_str()) {
                                    text_prev = Some(text_preview(t));
                                }
                            }
                            "tool_use" => {
                                tool_name = first.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
                                tool_use_id = first.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
                            }
                            "tool_result" => {
                                tool_use_id = first.get("tool_use_id").and_then(|v| v.as_str()).map(|s| s.to_string());
                                is_error = first.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
                                if let Some(c) = first.get("content") {
                                    text_prev = first_text(c).map(|t| text_preview(&t));
                                }
                            }
                            "thinking" => {
                                if let Some(t) = first.get("thinking").and_then(|v| v.as_str()) {
                                    text_prev = Some(text_preview(t));
                                }
                            }
                            _ => {}
                        }
                    }
                } else if let Some(content_str) = msg.get("content").and_then(|v| v.as_str()) {
                    content_kind = Some("text".to_string());
                    text_prev = Some(text_preview(content_str));
                }
            }
        }
        _ => {}
    }

    meta.event_count = 1;

    let ev = ParsedEvent {
        uuid,
        parent_uuid,
        event_type: ty,
        category,
        role,
        timestamp_ms,
        content_kind,
        tool_name,
        tool_use_id,
        stop_reason,
        duration_ms,
        is_sidechain,
        is_error,
        attachment_kind,
        content: line.to_string(),
        text_preview: text_prev,
        input_tokens,
        output_tokens,
        cache_creation_tokens,
        cache_read_tokens,
    };

    Ok((ev, meta))
}

/// Parse từ offset đến EOF. Trả về ParseBatch.
pub fn parse_from_offset(path: &Path, offset: u64) -> Result<ParseBatch> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    file.seek(SeekFrom::Start(offset))?;

    let mut reader = BufReader::new(file);
    let mut events = Vec::new();
    let mut merged_meta = SessionMeta::default();
    let mut parse_errors = 0usize;
    let mut buf = String::new();

    loop {
        buf.clear();
        let n = reader.read_line(&mut buf)?;
        if n == 0 {
            break;
        }
        let line = buf.trim();
        if line.is_empty() {
            continue;
        }
        match parse_line(line) {
            Ok((ev, line_meta)) => {
                merge_meta(&mut merged_meta, line_meta);
                events.push(ev);
            }
            Err(e) => {
                parse_errors += 1;
                warn!("parse error: {}", e);
            }
        }
    }

    // Lấy vị trí hiện tại = new offset
    let new_offset = reader.into_inner().stream_position()?;

    Ok(ParseBatch { events, meta: merged_meta, new_offset, parse_errors })
}

/// Parse toàn file từ đầu (dùng cho parse-once và import-all).
pub fn parse_file(path: &Path) -> Result<(Stats, ParseBatch)> {
    // Stats cho print (giữ nguyên output của prototype)
    let meta_file = std::fs::metadata(path)?;
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut stats = Stats::default();
    let mut events = Vec::new();
    let mut merged_meta = SessionMeta::default();
    let mut parse_errors = 0usize;

    for (idx, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("read line {}", idx + 1))?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Update stats (prototype logic)
        update_stats(&mut stats, line);

        // Parse for DB
        match parse_line(line) {
            Ok((ev, line_meta)) => {
                merge_meta(&mut merged_meta, line_meta);
                events.push(ev);
            }
            Err(e) => {
                parse_errors += 1;
                warn!("parse error line {}: {}", idx + 1, e);
            }
        }
    }

    stats.total_lines = events.len() + parse_errors;
    stats.parse_errors = parse_errors;

    let batch = ParseBatch {
        events,
        meta: merged_meta,
        new_offset: meta_file.len(),
        parse_errors,
    };

    Ok((stats, batch))
}

fn merge_meta(dst: &mut SessionMeta, src: SessionMeta) {
    if dst.session_id.is_none() { dst.session_id = src.session_id; }
    if dst.cwd.is_none() { dst.cwd = src.cwd; }
    if dst.project_name.is_none() { dst.project_name = src.project_name; }
    if src.ai_title.is_some() { dst.ai_title = src.ai_title; } // luôn lấy cái mới nhất
    if src.summary.is_some() { dst.summary = src.summary; }
    if dst.started_at.is_none() { dst.started_at = src.started_at; }
    if let Some(t) = src.last_event_at {
        dst.last_event_at = Some(dst.last_event_at.map_or(t, |x: i64| x.max(t)));
    }
    if src.model.is_some() { dst.model = src.model; }
    if dst.cli_version.is_none() { dst.cli_version = src.cli_version; }
    if dst.git_branch.is_none() { dst.git_branch = src.git_branch; }
    if src.permission_mode.is_some() { dst.permission_mode = src.permission_mode; }
    dst.input_tokens_delta += src.input_tokens_delta;
    dst.output_tokens_delta += src.output_tokens_delta;
    dst.cache_creation_delta += src.cache_creation_delta;
    dst.cache_read_delta += src.cache_read_delta;
    dst.event_count += src.event_count;
    dst.hook_success_delta += src.hook_success_delta;
    dst.hook_error_delta += src.hook_error_delta;
}

// --- Stats update (giữ lại logic prototype) ---

fn update_stats(stats: &mut Stats, line: &str) {
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => { stats.parse_errors += 1; eprintln!("[warn] {}", e); return; }
    };
    stats.total_lines += 1;
    let Some(obj) = v.as_object() else { return };

    for k in obj.keys() {
        if !KNOWN_TOP_KEYS.contains(&k.as_str()) {
            *stats.unknown_top_keys.entry(k.clone()).or_default() += 1;
        }
    }

    let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("(no-type)");
    *stats.types.entry(ty.into()).or_default() += 1;

    if stats.session_id.is_none() {
        stats.session_id = obj.get("sessionId").and_then(|v| v.as_str()).map(|s| s.to_string());
    }
    if stats.cwd.is_none() {
        stats.cwd = obj.get("cwd").and_then(|v| v.as_str()).map(|s| s.to_string());
    }
    if let Some(ts) = obj.get("timestamp").and_then(|v| v.as_str()) {
        if let Ok(t) = ts.parse::<DateTime<chrono::Utc>>() {
            stats.first_ts = Some(stats.first_ts.map_or(t, |x| x.min(t)));
            stats.last_ts = Some(stats.last_ts.map_or(t, |x| x.max(t)));
        }
    }
    if let Some(branch) = obj.get("gitBranch").and_then(|v| v.as_str()) {
        if !branch.is_empty() { *stats.git_branches.entry(branch.into()).or_default() += 1; }
    }
    if let Some(ver) = obj.get("version").and_then(|v| v.as_str()) {
        *stats.versions.entry(ver.into()).or_default() += 1;
    }
    if obj.get("isSidechain").and_then(|v| v.as_bool()).unwrap_or(false) {
        stats.sidechain_events += 1;
    }

    match ty {
        "ai-title" => {
            if let Some(t) = obj.get("aiTitle").and_then(|v| v.as_str()) {
                stats.ai_title = Some(t.to_string());
            }
        }
        "summary" => {
            if let Some(s) = obj.get("summary").and_then(|v| v.as_str()) {
                stats.summary = Some(s.to_string());
            }
        }
        "permission-mode" => {
            if let Some(m) = obj.get("permissionMode").and_then(|v| v.as_str()) {
                *stats.permission_modes.entry(m.into()).or_default() += 1;
            }
        }
        "attachment" => {
            if let Some(a) = obj.get("attachment").and_then(|v| v.as_object()) {
                let kind = a.get("type").and_then(|v| v.as_str()).unwrap_or("?");
                *stats.attachment_kinds.entry(kind.into()).or_default() += 1;
            }
        }
        "assistant" | "user" => {
            if let Some(msg) = obj.get("message") {
                if let Some(model) = msg.get("model").and_then(|v| v.as_str()) {
                    *stats.models.entry(model.into()).or_default() += 1;
                }
                if let Some(usage) = msg.get("usage").and_then(|v| v.as_object()) {
                    stats.input_tokens += usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    stats.output_tokens += usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    stats.cache_creation_tokens += usage.get("cache_creation_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    stats.cache_read_tokens += usage.get("cache_read_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                }
                walk_content_stats(stats, msg);
            }
        }
        _ => {}
    }
}

fn walk_content_stats(stats: &mut Stats, msg: &Value) {
    let Some(content) = msg.get("content") else { return };
    let blocks = match content {
        Value::Array(arr) => arr.iter().collect::<Vec<_>>(),
        Value::String(_) => { *stats.content_kinds.entry("text".into()).or_default() += 1; return; }
        _ => return,
    };
    for block in blocks {
        let kind = block.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");
        *stats.content_kinds.entry(kind.into()).or_default() += 1;
        if kind == "tool_use" {
            if let Some(name) = block.get("name").and_then(|v| v.as_str()) {
                *stats.tools.entry(name.into()).or_default() += 1;
            }
        } else if kind == "tool_result" {
            if block.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false) {
                *stats.tool_errors.entry("(unnamed)".into()).or_default() += 1;
            }
        }
    }
}

pub fn print_stats(path: &Path, file_size: u64, stats: &Stats) {
    fn print_section(title: &str) { println!("\n== {} ==", title); }
    fn print_map(map: &BTreeMap<String, usize>) {
        if map.is_empty() { println!("  (none)"); return; }
        let mut entries: Vec<_> = map.iter().collect();
        entries.sort_by(|a, b| b.1.cmp(a.1));
        for (k, v) in entries { println!("  {:>6}  {}", v, k); }
    }

    println!("File: {}", path.display());
    println!("Size: {} bytes", file_size);
    println!("Lines parsed: {} (errors: {})", stats.total_lines, stats.parse_errors);

    print_section("Session");
    println!("  sessionId : {:?}", stats.session_id);
    println!("  cwd       : {:?}", stats.cwd);
    println!("  aiTitle   : {:?}", stats.ai_title);
    println!("  summary   : {:?}", stats.summary);
    if let (Some(a), Some(b)) = (stats.first_ts, stats.last_ts) {
        let dur = b - a;
        println!("  first_ts  : {}", a);
        println!("  last_ts   : {}", b);
        println!("  duration  : {} min", dur.num_minutes());
    }
    println!("  sidechain : {} event(s)", stats.sidechain_events);

    print_section("Event types");
    print_map(&stats.types);
    print_section("Content kinds (in messages)");
    print_map(&stats.content_kinds);
    print_section("Tools used");
    print_map(&stats.tools);
    print_section("Models");
    print_map(&stats.models);
    print_section("Git branches");
    print_map(&stats.git_branches);
    print_section("Claude Code versions");
    print_map(&stats.versions);
    print_section("Permission modes");
    print_map(&stats.permission_modes);
    print_section("Attachment kinds");
    print_map(&stats.attachment_kinds);
    print_section("Tokens (sum)");
    println!("  input         : {}", stats.input_tokens);
    println!("  output        : {}", stats.output_tokens);
    println!("  cache_creation: {}", stats.cache_creation_tokens);
    println!("  cache_read    : {}", stats.cache_read_tokens);
    print_section("Unknown top-level keys (cần bổ sung schema)");
    print_map(&stats.unknown_top_keys);
}
