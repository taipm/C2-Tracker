use serde::{Deserialize, Serialize};

/// Một event đã được parse từ JSONL, sẵn sàng để insert vào DB.
#[derive(Debug, Clone)]
pub struct ParsedEvent {
    /// uuid từ JSONL, hoặc "meta:<sha1>" cho meta events.
    pub uuid: String,
    pub parent_uuid: Option<String>,
    pub event_type: String,
    pub category: EventCategory,
    pub role: Option<String>,
    pub timestamp_ms: Option<i64>,
    pub content_kind: Option<String>,
    pub tool_name: Option<String>,
    pub tool_use_id: Option<String>,
    pub stop_reason: Option<String>,
    pub duration_ms: Option<i64>,
    pub is_sidechain: bool,
    pub is_error: bool,
    pub attachment_kind: Option<String>,
    /// Raw JSON line gốc.
    pub content: String,
    /// 200 ký tự đầu của text content (nếu có).
    pub text_preview: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_creation_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventCategory {
    Content,
    Meta,
    Attachment,
}

impl EventCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::Meta => "meta",
            Self::Attachment => "attachment",
        }
    }
}

/// Metadata cần upsert vào bảng sessions sau khi parse batch.
#[derive(Debug, Default, Clone)]
pub struct SessionMeta {
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub project_name: Option<String>,
    pub ai_title: Option<String>,
    pub summary: Option<String>,
    pub started_at: Option<i64>,
    pub last_event_at: Option<i64>,
    pub model: Option<String>,
    pub cli_version: Option<String>,
    pub git_branch: Option<String>,
    pub permission_mode: Option<String>,
    pub input_tokens_delta: i64,
    pub output_tokens_delta: i64,
    pub cache_creation_delta: i64,
    pub cache_read_delta: i64,
    pub event_count: usize,
    pub hook_success_delta: i64,
    pub hook_error_delta: i64,
}

/// Hook request payload chung từ Claude Code.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HookPayload {
    pub session_id: Option<String>,
    pub transcript_path: Option<String>,
    pub cwd: Option<String>,
    pub permission_mode: Option<String>,
    pub hook_event_name: Option<String>,
    // UserPromptSubmit
    pub prompt: Option<String>,
    // SessionEnd
    pub reason: Option<String>,
    // Stop
    pub stop_reason: Option<String>,
    // Catch-all extra fields
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Response chuẩn trả về cho Claude Code hooks.
#[derive(Debug, Serialize)]
pub struct HookResponse {
    pub r#continue: bool,
    #[serde(rename = "suppressOutput")]
    pub suppress_output: bool,
}

impl Default for HookResponse {
    fn default() -> Self {
        Self { r#continue: true, suppress_output: true }
    }
}

/// Message phát broadcast cho WebSocket clients (UI realtime).
/// Tag "kind" tương ứng case kebab-case.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WsMessage {
    /// Một session vừa được upsert/update — frontend nên refresh session list.
    SessionUpsert { session_id: String },
    /// Một batch events vừa append cho session — frontend nên refresh events
    /// nếu đang xem session đó. Không carry data để tránh đua giữa watcher đang
    /// commit và frontend đọc — frontend tự fetch lại để có view nhất quán.
    EventBatch { session_id: String, inserted: usize },
}
