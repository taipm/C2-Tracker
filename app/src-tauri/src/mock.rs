use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub project: String,
    pub cwd: String,
    pub status: String, // "live" | "idle" | "ended"
    pub started_at: i64,
    pub last_event_at: i64,
    pub duration_label: String,
    pub event_count: u32,
    pub token_count: u32,
    pub model: String,
    pub ai_title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub session_id: String,
    pub kind: String, // "user" | "thinking" | "assistant" | "tool_use" | "tool_result" | "summary"
    pub timestamp_label: String,
    pub text: String,
    pub tool_name: Option<String>,
    pub tool_input: Option<String>,
    pub tool_output: Option<String>,
    pub is_error: bool,
    pub thinking_duration_ms: Option<u32>,
    pub thinking_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickStats {
    pub sessions_today: u32,
    pub tokens_today: u32,
    pub tool_calls_today: u32,
    pub recording_count: u32,
}

pub fn sessions() -> Vec<Session> {
    vec![
        Session {
            id: "sess-c2tracker-001".into(),
            project: "c2-tracker".into(),
            cwd: "/Volumes/OWC-taipm-ssd/gitea/C2-Tracker".into(),
            status: "live".into(),
            started_at: 0,
            last_event_at: 0,
            duration_label: "45m".into(),
            event_count: 23,
            token_count: 12_400,
            model: "claude-opus-4-7".into(),
            ai_title: "Scaffold Tauri app + UI mock".into(),
        },
        Session {
            id: "sess-llmserver-002".into(),
            project: "llm-server".into(),
            cwd: "/Volumes/OWC-taipm-ssd/gitea/llm-server".into(),
            status: "idle".into(),
            started_at: 0,
            last_event_at: 0,
            duration_label: "1h 12m".into(),
            event_count: 89,
            token_count: 31_800,
            model: "claude-opus-4-7".into(),
            ai_title: "Rate limit silent fallback bug".into(),
        },
        Session {
            id: "sess-sage-003".into(),
            project: "sage".into(),
            cwd: "/Volumes/OWC-taipm-ssd/gitea/sage".into(),
            status: "ended".into(),
            started_at: 0,
            last_event_at: 0,
            duration_label: "2h 04m".into(),
            event_count: 41,
            token_count: 18_700,
            model: "claude-sonnet-4-7".into(),
            ai_title: "Embed version metadata vào binary".into(),
        },
    ]
}

pub fn events_for(session_id: &str) -> Vec<Event> {
    match session_id {
        "sess-c2tracker-001" => c2_tracker_events(),
        "sess-llmserver-002" => llm_server_events(),
        "sess-sage-003" => sage_events(),
        _ => vec![],
    }
}

pub fn quick_stats() -> QuickStats {
    QuickStats {
        sessions_today: 4,
        tokens_today: 48_200,
        tool_calls_today: 127,
        recording_count: 1,
    }
}

fn c2_tracker_events() -> Vec<Event> {
    let sid = "sess-c2tracker-001";
    vec![
        Event {
            id: "e1".into(), session_id: sid.into(), kind: "user".into(),
            timestamp_label: "10:30:12".into(),
            text: "Scaffold Tauri v2 desktop app cho C2-Tracker. Đọc REQ.md §6 để biết layout 4:8.".into(),
            tool_name: None, tool_input: None, tool_output: None, is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "e2".into(), session_id: sid.into(), kind: "thinking".into(),
            timestamp_label: "10:30:14".into(),
            text: "Owner muốn vanilla HTML/CSS/JS — không React/Vue. Cần custom titlebar, frosted glass dark theme. Mock data cho 3 session. Layout 4:8 ratio. Stream chat-style với thinking collapsible, tool_use expandable. Auto-follow checkbox + Cmd+K search palette.".into(),
            tool_name: None, tool_input: None, tool_output: None, is_error: false,
            thinking_duration_ms: Some(3_200), thinking_tokens: Some(412),
        },
        Event {
            id: "e3".into(), session_id: sid.into(), kind: "assistant".into(),
            timestamp_label: "10:30:17".into(),
            text: "Đã đọc xong REQ.md. Bắt đầu scaffold cấu trúc `app/`. Sẽ tạo Tauri v2 project với window 1280x820, custom titlebar, decorations: false.".into(),
            tool_name: None, tool_input: None, tool_output: None, is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "e4".into(), session_id: sid.into(), kind: "tool_use".into(),
            timestamp_label: "10:30:19".into(),
            text: "Read REQ.md".into(),
            tool_name: Some("Read".into()),
            tool_input: Some("{\n  \"file_path\": \"/Volumes/OWC-taipm-ssd/gitea/C2-Tracker/REQ.md\"\n}".into()),
            tool_output: None, is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "e5".into(), session_id: sid.into(), kind: "tool_result".into(),
            timestamp_label: "10:30:19".into(),
            text: "620 lines read".into(),
            tool_name: Some("Read".into()),
            tool_input: None,
            tool_output: Some("# C2-Tracker — Tài liệu Yêu cầu (REQ)\n\n> Phiên bản: 0.4 · Cập nhật: 2026-05-18 · Owner: taipm\n...\n[620 lines total]".into()),
            is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "e6".into(), session_id: sid.into(), kind: "user".into(),
            timestamp_label: "10:32:45".into(),
            text: "Thêm phần Roadmap chi tiết cho Phase 2 và 3, mỗi phase 5 hạng mục.".into(),
            tool_name: None, tool_input: None, tool_output: None, is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "e7".into(), session_id: sid.into(), kind: "thinking".into(),
            timestamp_label: "10:32:46".into(),
            text: "Phase 2 nên tập trung analytics + storage backend (đã có sẵn trong REQ §9). Phase 3 là multi-device sync, team mode, plugin system, Windows support. Cần thêm chi tiết acceptance.".into(),
            tool_name: None, tool_input: None, tool_output: None, is_error: false,
            thinking_duration_ms: Some(1_840), thinking_tokens: Some(218),
        },
        Event {
            id: "e8".into(), session_id: sid.into(), kind: "tool_use".into(),
            timestamp_label: "10:32:50".into(),
            text: "Edit REQ.md (Roadmap section)".into(),
            tool_name: Some("Edit".into()),
            tool_input: Some("{\n  \"file_path\": \"/Volumes/.../REQ.md\",\n  \"old_string\": \"### Phase 2 — Storage backends + analytics\",\n  \"new_string\": \"### Phase 2 — Storage backends + analytics (chi tiết)\"\n}".into()),
            tool_output: None, is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "e9".into(), session_id: sid.into(), kind: "tool_result".into(),
            timestamp_label: "10:32:50".into(),
            text: "File edited successfully".into(),
            tool_name: Some("Edit".into()),
            tool_input: None,
            tool_output: Some("{\n  \"status\": \"ok\",\n  \"replaced\": 1\n}".into()),
            is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "e10".into(), session_id: sid.into(), kind: "tool_use".into(),
            timestamp_label: "10:33:02".into(),
            text: "Bash: cargo tauri dev".into(),
            tool_name: Some("Bash".into()),
            tool_input: Some("{\n  \"command\": \"cd app && cargo tauri dev\",\n  \"description\": \"Smoke test Tauri app\"\n}".into()),
            tool_output: None, is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "e11".into(), session_id: sid.into(), kind: "tool_result".into(),
            timestamp_label: "10:33:18".into(),
            text: "Compiled successfully — window opened".into(),
            tool_name: Some("Bash".into()),
            tool_input: None,
            tool_output: Some("Compiling c2-tracker v0.1.0\nFinished `dev` profile in 14.32s\nRunning app on http://localhost:1420".into()),
            is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "e12".into(), session_id: sid.into(), kind: "assistant".into(),
            timestamp_label: "10:33:22".into(),
            text: "Scaffold xong. App mở window, hiển thị 3 session mock, click vào từng session load stream tương ứng. Custom titlebar drag được, dark theme + frosted glass đã áp dụng.".into(),
            tool_name: None, tool_input: None, tool_output: None, is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
    ]
}

fn llm_server_events() -> Vec<Event> {
    let sid = "sess-llmserver-002";
    vec![
        Event {
            id: "l1".into(), session_id: sid.into(), kind: "user".into(),
            timestamp_label: "09:14:02".into(),
            text: "Rate limit của Ollama đang fallback silent — không log gì. Tìm root cause.".into(),
            tool_name: None, tool_input: None, tool_output: None, is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "l2".into(), session_id: sid.into(), kind: "thinking".into(),
            timestamp_label: "09:14:04".into(),
            text: "Silent fallback nghĩa là code catch error rồi swallow. Cần tìm chỗ catch unwrap_or hoặc match Err(_) => default. Bắt đầu grep.".into(),
            tool_name: None, tool_input: None, tool_output: None, is_error: false,
            thinking_duration_ms: Some(2_140), thinking_tokens: Some(287),
        },
        Event {
            id: "l3".into(), session_id: sid.into(), kind: "tool_use".into(),
            timestamp_label: "09:14:08".into(),
            text: "Grep: unwrap_or in src/".into(),
            tool_name: Some("Grep".into()),
            tool_input: Some("{\n  \"pattern\": \"unwrap_or|unwrap_or_default\",\n  \"path\": \"src/\",\n  \"glob\": \"*.rs\"\n}".into()),
            tool_output: None, is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "l4".into(), session_id: sid.into(), kind: "tool_result".into(),
            timestamp_label: "09:14:08".into(),
            text: "14 matches in 6 files".into(),
            tool_name: Some("Grep".into()),
            tool_input: None,
            tool_output: Some("src/limiter.rs:42:    let limit = config.rate_limit.unwrap_or(DEFAULT_RATE);\nsrc/limiter.rs:78:    cache.get(&key).unwrap_or_default();\nsrc/handler.rs:103:    response.status().unwrap_or(500);\n... [11 more]".into()),
            is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "l5".into(), session_id: sid.into(), kind: "tool_use".into(),
            timestamp_label: "09:14:21".into(),
            text: "Read src/limiter.rs lines 30-90".into(),
            tool_name: Some("Read".into()),
            tool_input: Some("{\n  \"file_path\": \"src/limiter.rs\",\n  \"offset\": 30,\n  \"limit\": 60\n}".into()),
            tool_output: None, is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "l6".into(), session_id: sid.into(), kind: "tool_result".into(),
            timestamp_label: "09:14:21".into(),
            text: "60 lines".into(),
            tool_name: Some("Read".into()),
            tool_input: None,
            tool_output: Some("fn apply_limit(config: &Config) -> u32 {\n    let limit = config.rate_limit.unwrap_or(DEFAULT_RATE);\n    // BUG: silent fallback - no log when config missing\n    limit\n}".into()),
            is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "l7".into(), session_id: sid.into(), kind: "assistant".into(),
            timestamp_label: "09:14:34".into(),
            text: "Tìm thấy. `src/limiter.rs:42` dùng `unwrap_or(DEFAULT_RATE)` khi config thiếu — không log warning. User không biết config bị missing.".into(),
            tool_name: None, tool_input: None, tool_output: None, is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "l8".into(), session_id: sid.into(), kind: "tool_use".into(),
            timestamp_label: "09:15:02".into(),
            text: "Edit src/limiter.rs".into(),
            tool_name: Some("Edit".into()),
            tool_input: Some("{\n  \"file_path\": \"src/limiter.rs\",\n  \"old_string\": \"let limit = config.rate_limit.unwrap_or(DEFAULT_RATE);\",\n  \"new_string\": \"let limit = config.rate_limit.unwrap_or_else(|| { warn!(\\\"rate_limit missing, using DEFAULT_RATE={}\\\", DEFAULT_RATE); DEFAULT_RATE });\"\n}".into()),
            tool_output: None, is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "l9".into(), session_id: sid.into(), kind: "tool_result".into(),
            timestamp_label: "09:15:02".into(),
            text: "Edit applied".into(),
            tool_name: Some("Edit".into()),
            tool_input: None,
            tool_output: Some("{\"replaced\": 1}".into()),
            is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "l10".into(), session_id: sid.into(), kind: "tool_use".into(),
            timestamp_label: "09:15:08".into(),
            text: "Bash: cargo test limiter".into(),
            tool_name: Some("Bash".into()),
            tool_input: Some("{\n  \"command\": \"cargo test limiter -- --nocapture\"\n}".into()),
            tool_output: None, is_error: true,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "l11".into(), session_id: sid.into(), kind: "tool_result".into(),
            timestamp_label: "09:15:23".into(),
            text: "1 test failed (expected)".into(),
            tool_name: Some("Bash".into()),
            tool_input: None,
            tool_output: Some("running 4 tests\ntest limiter::tests::fallback_logs_warning ... FAILED\n\nfailures:\n----\nthread 'limiter::tests::fallback_logs_warning' panicked at 'warning not captured'\n\ntest result: FAILED. 3 passed; 1 failed".into()),
            is_error: true,
            thinking_duration_ms: None, thinking_tokens: None,
        },
    ]
}

fn sage_events() -> Vec<Event> {
    let sid = "sess-sage-003";
    vec![
        Event {
            id: "s1".into(), session_id: sid.into(), kind: "user".into(),
            timestamp_label: "Yesterday 16:02".into(),
            text: "Embed git commit + build timestamp vào sage binary, expose qua --version flag.".into(),
            tool_name: None, tool_input: None, tool_output: None, is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "s2".into(), session_id: sid.into(), kind: "assistant".into(),
            timestamp_label: "Yesterday 16:02".into(),
            text: "Sẽ tạo build.rs đọc git commit qua `git rev-parse HEAD`, build timestamp qua `chrono::Utc::now()`, expose qua env! macro.".into(),
            tool_name: None, tool_input: None, tool_output: None, is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "s3".into(), session_id: sid.into(), kind: "tool_use".into(),
            timestamp_label: "Yesterday 16:03".into(),
            text: "Write build.rs".into(),
            tool_name: Some("Write".into()),
            tool_input: Some("{\n  \"file_path\": \"build.rs\",\n  \"content\": \"use std::process::Command;\\n\\nfn main() {\\n    let git = Command::new(\\\"git\\\").args([\\\"rev-parse\\\", \\\"--short\\\", \\\"HEAD\\\"]).output().unwrap();\\n    let commit = String::from_utf8_lossy(&git.stdout);\\n    println!(\\\"cargo:rustc-env=GIT_COMMIT={}\\\", commit.trim());\\n}\\n\"\n}".into()),
            tool_output: None, is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "s4".into(), session_id: sid.into(), kind: "tool_result".into(),
            timestamp_label: "Yesterday 16:03".into(),
            text: "Created build.rs".into(),
            tool_name: Some("Write".into()),
            tool_input: None,
            tool_output: Some("File created (12 lines)".into()),
            is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "s5".into(), session_id: sid.into(), kind: "thinking".into(),
            timestamp_label: "Yesterday 16:04".into(),
            text: "Cần handle case không có .git dir (vd. install qua cargo install). Fallback `unknown`.".into(),
            tool_name: None, tool_input: None, tool_output: None, is_error: false,
            thinking_duration_ms: Some(890), thinking_tokens: Some(98),
        },
        Event {
            id: "s6".into(), session_id: sid.into(), kind: "tool_use".into(),
            timestamp_label: "Yesterday 16:04".into(),
            text: "Edit src/main.rs add --version".into(),
            tool_name: Some("Edit".into()),
            tool_input: Some("{\n  \"file_path\": \"src/main.rs\",\n  \"old_string\": \"#[derive(Parser)]\",\n  \"new_string\": \"const VERSION: &str = concat!(env!(\\\"CARGO_PKG_VERSION\\\"), \\\" (\\\", env!(\\\"GIT_COMMIT\\\"), \\\")\\\" );\\n\\n#[derive(Parser)]\"\n}".into()),
            tool_output: None, is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "s7".into(), session_id: sid.into(), kind: "tool_result".into(),
            timestamp_label: "Yesterday 16:04".into(),
            text: "Edited".into(),
            tool_name: Some("Edit".into()),
            tool_input: None,
            tool_output: Some("{\"replaced\": 1}".into()),
            is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "s8".into(), session_id: sid.into(), kind: "tool_use".into(),
            timestamp_label: "Yesterday 16:05".into(),
            text: "Bash: cargo build && ./target/debug/sage --version".into(),
            tool_name: Some("Bash".into()),
            tool_input: Some("{\n  \"command\": \"cargo build && ./target/debug/sage --version\"\n}".into()),
            tool_output: None, is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "s9".into(), session_id: sid.into(), kind: "tool_result".into(),
            timestamp_label: "Yesterday 16:05".into(),
            text: "sage 0.3.1 (a1b2c3d)".into(),
            tool_name: Some("Bash".into()),
            tool_input: None,
            tool_output: Some("Compiling sage v0.3.1\nFinished `dev` profile in 8.4s\nsage 0.3.1 (a1b2c3d)".into()),
            is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
        Event {
            id: "s10".into(), session_id: sid.into(), kind: "assistant".into(),
            timestamp_label: "Yesterday 16:05".into(),
            text: "Done. `--version` flag in ra version + git commit ngắn. Đã verify build chạy đúng.".into(),
            tool_name: None, tool_input: None, tool_output: None, is_error: false,
            thinking_duration_ms: None, thinking_tokens: None,
        },
    ]
}
