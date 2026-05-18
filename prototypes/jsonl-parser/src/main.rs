// JSONL parser prototype — verify schema thực tế trên file Claude Code transcript.
// Mục tiêu: confirm REQ.md §5.2, phát hiện field/type chưa lường trước.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    env,
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

#[derive(Default, Debug)]
struct Stats {
    total_lines: usize,
    parse_errors: usize,
    types: BTreeMap<String, usize>,
    content_kinds: BTreeMap<String, usize>,
    tools: BTreeMap<String, usize>,
    tool_errors: BTreeMap<String, usize>,
    models: BTreeMap<String, usize>,
    git_branches: BTreeMap<String, usize>,
    versions: BTreeMap<String, usize>,
    permission_modes: BTreeMap<String, usize>,
    attachment_kinds: BTreeMap<String, usize>,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    sidechain_events: usize,
    first_ts: Option<DateTime<Utc>>,
    last_ts: Option<DateTime<Utc>>,
    session_id: Option<String>,
    cwd: Option<String>,
    ai_title: Option<String>,
    summary: Option<String>,
    unknown_top_keys: BTreeMap<String, usize>,
}

const KNOWN_TOP_KEYS: &[&str] = &[
    "type", "uuid", "parentUuid", "timestamp", "sessionId", "cwd", "version",
    "gitBranch", "userType", "isSidechain", "message", "requestId", "entrypoint",
    "promptId", "permissionMode", "toolUseResult", "sourceToolAssistantUUID",
    "aiTitle", "leafUuid", "summary", "attachment", "isSnapshotUpdate", "messageId",
    "snapshot", "isCompactSummary", "isVisibleInTranscriptOnly",
];

fn record_ts(stats: &mut Stats, ts: &str) {
    if let Ok(t) = ts.parse::<DateTime<Utc>>() {
        stats.first_ts = Some(stats.first_ts.map_or(t, |x| x.min(t)));
        stats.last_ts = Some(stats.last_ts.map_or(t, |x| x.max(t)));
    }
}

fn walk_message_content(stats: &mut Stats, msg: &Value) {
    let Some(content) = msg.get("content") else { return };

    let blocks = match content {
        Value::Array(arr) => arr.iter().collect::<Vec<_>>(),
        Value::String(_) => {
            *stats.content_kinds.entry("text".into()).or_default() += 1;
            return;
        }
        _ => return,
    };

    for block in blocks {
        let kind = block.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");
        *stats.content_kinds.entry(kind.into()).or_default() += 1;

        match kind {
            "tool_use" => {
                if let Some(name) = block.get("name").and_then(|v| v.as_str()) {
                    *stats.tools.entry(name.into()).or_default() += 1;
                }
            }
            "tool_result" => {
                let is_err = block.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
                if is_err {
                    // tool_result không có name → đánh dấu chung "tool_result"
                    *stats.tool_errors.entry("(unnamed)".into()).or_default() += 1;
                }
            }
            _ => {}
        }
    }
}

fn process_line(stats: &mut Stats, line: &str) -> Result<()> {
    let v: Value = serde_json::from_str(line).context("parse JSON")?;
    stats.total_lines += 1;

    let obj = v.as_object().context("not an object")?;
    for k in obj.keys() {
        if !KNOWN_TOP_KEYS.contains(&k.as_str()) {
            *stats.unknown_top_keys.entry(k.clone()).or_default() += 1;
        }
    }

    let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("(no-type)");
    *stats.types.entry(ty.into()).or_default() += 1;

    if let Some(sid) = obj.get("sessionId").and_then(|v| v.as_str()) {
        if stats.session_id.is_none() {
            stats.session_id = Some(sid.to_string());
        }
    }
    if let Some(cwd) = obj.get("cwd").and_then(|v| v.as_str()) {
        if stats.cwd.is_none() {
            stats.cwd = Some(cwd.to_string());
        }
    }
    if let Some(ts) = obj.get("timestamp").and_then(|v| v.as_str()) {
        record_ts(stats, ts);
    }
    if let Some(branch) = obj.get("gitBranch").and_then(|v| v.as_str()) {
        if !branch.is_empty() {
            *stats.git_branches.entry(branch.into()).or_default() += 1;
        }
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
                    stats.input_tokens         += usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    stats.output_tokens        += usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    stats.cache_creation_tokens += usage.get("cache_creation_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    stats.cache_read_tokens    += usage.get("cache_read_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                }
                walk_message_content(stats, msg);
            }
        }
        _ => {}
    }

    Ok(())
}

fn print_section(title: &str) {
    println!("\n== {} ==", title);
}

fn print_map<V: std::fmt::Display>(map: &BTreeMap<String, V>) {
    if map.is_empty() {
        println!("  (none)");
        return;
    }
    // Sort by value desc bằng cách collect ra vec
    let mut entries: Vec<_> = map.iter().collect();
    entries.sort_by(|a, b| b.1.to_string().parse::<u64>().unwrap_or(0)
        .cmp(&a.1.to_string().parse::<u64>().unwrap_or(0)));
    for (k, v) in entries {
        println!("  {:>6}  {}", v.to_string(), k);
    }
}

fn run(path: &Path) -> Result<Stats> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut stats = Stats::default();

    for (idx, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("read line {}", idx + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        if let Err(e) = process_line(&mut stats, &line) {
            stats.parse_errors += 1;
            eprintln!("[warn] line {}: {}", idx + 1, e);
        }
    }
    Ok(stats)
}

fn main() -> Result<()> {
    let path = env::args().nth(1).context("usage: jsonl-parser <file.jsonl>")?;
    let path = Path::new(&path);
    let meta = std::fs::metadata(path)?;
    let stats = run(path)?;

    println!("File: {}", path.display());
    println!("Size: {} bytes", meta.len());
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

    Ok(())
}
