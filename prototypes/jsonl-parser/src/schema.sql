CREATE TABLE IF NOT EXISTS sessions (
  id                          TEXT PRIMARY KEY,
  cwd                         TEXT NOT NULL,
  project_name                TEXT,
  ai_title                    TEXT,
  summary                     TEXT,
  started_at                  INTEGER NOT NULL,
  ended_at                    INTEGER,
  last_event_at               INTEGER NOT NULL,
  jsonl_path                  TEXT NOT NULL,
  file_offset                 INTEGER NOT NULL DEFAULT 0,
  file_inode                  INTEGER,
  total_events                INTEGER DEFAULT 0,
  total_input_tokens          INTEGER DEFAULT 0,
  total_output_tokens         INTEGER DEFAULT 0,
  total_cache_creation_tokens INTEGER DEFAULT 0,
  total_cache_read_tokens     INTEGER DEFAULT 0,
  model                       TEXT,
  cli_version                 TEXT,
  git_branch                  TEXT,
  permission_mode             TEXT,
  has_stop_event              INTEGER DEFAULT 0,
  hook_success_count          INTEGER DEFAULT 0,
  hook_error_count            INTEGER DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_sessions_last ON sessions(last_event_at DESC);
CREATE INDEX IF NOT EXISTS idx_sessions_cwd  ON sessions(cwd);

CREATE TABLE IF NOT EXISTS events (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id       TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  uuid             TEXT UNIQUE,
  parent_uuid      TEXT,
  "type"           TEXT NOT NULL,
  category         TEXT NOT NULL,
  role             TEXT,
  timestamp        INTEGER,
  content_kind     TEXT,
  tool_name        TEXT,
  tool_use_id      TEXT,
  stop_reason      TEXT,
  duration_ms      INTEGER,
  is_sidechain     INTEGER DEFAULT 0,
  is_error         INTEGER DEFAULT 0,
  attachment_kind  TEXT,
  content          TEXT NOT NULL,
  text_preview     TEXT,
  input_tokens     INTEGER,
  output_tokens    INTEGER,
  cache_creation_tokens INTEGER,
  cache_read_tokens     INTEGER
);

CREATE INDEX IF NOT EXISTS idx_events_session  ON events(session_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_events_tool     ON events(tool_name) WHERE tool_name IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_events_category ON events(category);
CREATE INDEX IF NOT EXISTS idx_events_tooluse  ON events(tool_use_id) WHERE tool_use_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS app_config (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
