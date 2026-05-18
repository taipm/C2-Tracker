// C2-Tracker frontend — vanilla JS, no bundler.
// SAFETY: tất cả string động được bọc escapeHtml() trước khi đưa vào innerHTML.
// Data nguồn chỉ đến từ Tauri commands (Rust mock) hoặc BROWSER_MOCK hard-coded.

import { renderSessionList } from "./components/session-list.js";
import { renderStream, renderSessionHeader } from "./components/stream.js";

async function invoke(cmd, args) {
  if (window.__TAURI__?.core?.invoke) {
    return window.__TAURI__.core.invoke(cmd, args);
  }
  if (window.__TAURI_INTERNALS__?.invoke) {
    return window.__TAURI_INTERNALS__.invoke(cmd, args);
  }
  return browserFallback(cmd, args);
}

function browserFallback(cmd, args) {
  if (cmd === "get_sessions") return Promise.resolve(BROWSER_MOCK.sessions);
  if (cmd === "get_events") return Promise.resolve(BROWSER_MOCK.events[args.sessionId] || []);
  if (cmd === "get_stats") return Promise.resolve(BROWSER_MOCK.stats);
  if (cmd === "get_issues") return Promise.resolve([]);
  if (cmd === "get_issue_count") return Promise.resolve(0);
  return Promise.reject(new Error("unknown command: " + cmd));
}

const BROWSER_MOCK = {
  sessions: [
    { id: "sess-c2tracker-001", project: "c2-tracker", status: "live", duration_label: "45m", event_count: 23, token_count: 12400, model: "claude-opus-4-7", ai_title: "Scaffold Tauri app", cwd: "" },
    { id: "sess-llmserver-002", project: "llm-server", status: "idle", duration_label: "1h 12m", event_count: 89, token_count: 31800, model: "claude-opus-4-7", ai_title: "Rate limit silent fallback bug", cwd: "" },
    { id: "sess-sage-003", project: "sage", status: "ended", duration_label: "2h 04m", event_count: 41, token_count: 18700, model: "claude-sonnet-4-7", ai_title: "Embed version into binary", cwd: "" },
  ],
  events: {
    "sess-c2tracker-001": [
      { id: "e1", kind: "user", timestamp_label: "10:30:12", text: "Scaffold Tauri v2 desktop app cho C2-Tracker." },
      { id: "e2", kind: "thinking", timestamp_label: "10:30:14", text: "Owner muốn vanilla HTML/CSS/JS.", thinking_duration_ms: 3200, thinking_tokens: 412 },
      { id: "e3", kind: "assistant", timestamp_label: "10:30:17", text: "Đã đọc xong REQ.md." },
    ],
  },
  stats: { sessions_today: 4, tokens_today: 48200, tool_calls_today: 127, recording_count: 1 },
};

const state = {
  sessions: [],
  stats: null,
  activeSessionId: null,
  events: [],
  autoFollow: true,
  paletteOpen: false,
  paletteResults: [],
  paletteIndex: 0,
  // Feature 2: scroll-up pagination
  loadingOlder: false,
  reachedOldest: false,
  // Feature 3: issues panel
  activePanel: "stream", // "stream" | "issues"
  issues: [],
  issueCount: 0,
};

async function init() {
  try {
    state.sessions = await invoke("get_sessions");
    state.stats = await invoke("get_stats");
  } catch (err) {
    console.error("Failed to load initial data:", err);
    state.sessions = [];
    state.stats = { sessions_today: 0, tokens_today: 0, tool_calls_today: 0, recording_count: 0 };
  }

  renderRecordingIndicator();
  renderQuickStats();
  renderSessionList(state.sessions, document.getElementById("session-list"), {
    activeId: state.activeSessionId,
    onSelect: (id) => selectSession(id),
  });
  document.getElementById("sessions-count").textContent = String(state.sessions.length);

  if (state.sessions.length > 0) {
    selectSession(state.sessions[0].id);
  } else {
    renderEmptyStream();
  }

  attachHandlers();
  startWebSocket();
  setupVisibilityPause();
  attachScrollPagination();
}

let lastEventMaxId = 0;
let ws = null;
let wsReconnectTimer = null;
let wsBackoffMs = 1000;

// ── Feature 1: Page Visibility ────────────────────────────────────────────────

function setupVisibilityPause() {
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") {
      // Đóng WS sạch, huỷ reconnect timer (tiết kiệm pin)
      if (wsReconnectTimer) {
        clearTimeout(wsReconnectTimer);
        wsReconnectTimer = null;
      }
      if (ws) {
        ws.onclose = null; // prevent scheduleReconnect from firing
        ws.close(1000, "tab hidden");
        ws = null;
      }
      setHealthDot(false);
    } else {
      // Tab trở lại visible: reconnect + refresh full data
      startWebSocket();
      if (state.activeSessionId) {
        invoke("get_events", { sessionId: state.activeSessionId })
          .then((evs) => {
            state.events = evs;
            state.reachedOldest = false;
            lastEventMaxId = evs.length ? evs[evs.length - 1].id : 0;
            renderCurrentPanel();
          })
          .catch((e) => console.warn("visibility refresh events failed:", e));
      }
      refreshSessionsAndStats();
    }
  });
}

// ── Feature 2: Scroll-up pagination ──────────────────────────────────────────

let scrollPaginationThrottle = null;

function attachScrollPagination() {
  const streamEl = document.getElementById("stream");
  if (!streamEl) return;
  streamEl.addEventListener("scroll", () => {
    if (state.activePanel !== "stream") return;
    if (state.loadingOlder || state.reachedOldest) return;
    if (streamEl.scrollTop > 80) return;
    if (scrollPaginationThrottle) return;

    scrollPaginationThrottle = setTimeout(() => {
      scrollPaginationThrottle = null;
    }, 500);

    loadOlderEvents(streamEl);
  });
}

async function loadOlderEvents(streamEl) {
  if (!state.activeSessionId || state.events.length === 0) return;
  if (state.loadingOlder || state.reachedOldest) return;

  state.loadingOlder = true;
  const oldestId = state.events[0].id;

  try {
    const older = await invoke("get_events", {
      sessionId: state.activeSessionId,
      beforeId: oldestId,
    });

    if (older.length === 0) {
      state.reachedOldest = true;
      state.loadingOlder = false;
      return;
    }

    // Prepend và giữ scroll position
    const prevHeight = streamEl.scrollHeight;
    state.events = [...older, ...state.events];
    renderCurrentPanel();
    // Adjust scrollTop để user không thấy giật
    requestAnimationFrame(() => {
      streamEl.scrollTop += streamEl.scrollHeight - prevHeight;
    });

    if (older.length < 500) {
      state.reachedOldest = true;
    }
  } catch (e) {
    console.warn("loadOlderEvents failed:", e);
  } finally {
    state.loadingOlder = false;
  }
}

async function startWebSocket() {
  let runtime;
  try {
    runtime = await invoke("get_runtime");
  } catch (err) {
    console.warn("get_runtime failed, fallback polling:", err);
    return startPollingFallback();
  }
  if (!runtime || !runtime.port || !runtime.token) {
    console.warn("runtime.json không có — fallback polling");
    return startPollingFallback();
  }
  const url = `ws://127.0.0.1:${runtime.port}/ws?token=${encodeURIComponent(runtime.token)}`;

  try {
    ws = new WebSocket(url);
  } catch (err) {
    console.warn("WS construct failed:", err);
    return scheduleReconnect();
  }

  ws.addEventListener("open", () => {
    wsBackoffMs = 1000;
    setHealthDot(true);
    console.log("[WS] connected");
  });
  ws.addEventListener("message", (ev) => handleWsMessage(ev.data));
  ws.addEventListener("close", () => {
    setHealthDot(false);
    console.warn("[WS] closed, reconnecting...");
    scheduleReconnect();
  });
  ws.addEventListener("error", (e) => {
    console.warn("[WS] error", e);
    // close handler sẽ trigger reconnect
  });
}

function scheduleReconnect() {
  if (wsReconnectTimer) return;
  const delay = wsBackoffMs;
  wsReconnectTimer = setTimeout(() => {
    wsReconnectTimer = null;
    startWebSocket();
  }, delay);
  wsBackoffMs = Math.min(wsBackoffMs * 2, 30000);
}

function setHealthDot(alive) {
  const dot = document.querySelector(".rec-dot");
  if (dot) dot.style.opacity = alive ? "1" : "0.35";
}

async function handleWsMessage(raw) {
  let msg;
  try {
    msg = JSON.parse(raw);
  } catch {
    return;
  }
  if (msg.kind === "hello") return;

  if (msg.kind === "session-upsert") {
    await refreshSessionsAndStats();
    return;
  }

  if (msg.kind === "event-batch") {
    // Chỉ refetch nếu session đang xem
    if (msg.session_id === state.activeSessionId) {
      try {
        const evs = await invoke("get_events", { sessionId: state.activeSessionId });
        state.events = evs;
        state.reachedOldest = false;
        lastEventMaxId = evs.length ? evs[evs.length - 1].id : 0;
        renderCurrentPanel();
      } catch (e) {
        console.warn("get_events failed:", e);
      }
    }
    // Vẫn refresh stats + sidebar (last_event_at, token_count tăng)
    await refreshSessionsAndStats({ skipReorderActive: true });
    // Cập nhật issue count badge
    refreshIssueCount();
  }
}

async function refreshSessionsAndStats(_opts = {}) {
  try {
    const fresh = await invoke("get_sessions");
    state.sessions = fresh;
    renderSessionList(state.sessions, document.getElementById("session-list"), {
      activeId: state.activeSessionId,
      onSelect: (id) => selectSession(id),
    });
    document.getElementById("sessions-count").textContent = String(state.sessions.length);
    renderRecordingIndicator();

    const newStats = await invoke("get_stats");
    state.stats = newStats;
    renderQuickStats();

    // Cập nhật session header (token_count, event_count thay đổi)
    if (state.activeSessionId) {
      const session = state.sessions.find((s) => s.id === state.activeSessionId);
      if (session) renderSessionHeader(session, document.getElementById("session-header"));
    }
  } catch (err) {
    console.warn("refresh failed:", err);
  }
}

// ── Feature 3: Issues panel helpers ──────────────────────────────────────────

async function refreshIssueCount() {
  try {
    state.issueCount = await invoke("get_issue_count");
    updateIssuesBadge();
  } catch (e) {
    // ignore
  }
}

function updateIssuesBadge() {
  const btn = document.getElementById("btn-panel-issues");
  if (!btn) return;
  const n = state.issueCount;
  btn.textContent = n > 0 ? `Issues (${n})` : "Issues";
}

async function switchPanel(panel) {
  state.activePanel = panel;
  // Update button active states
  document.getElementById("btn-panel-stream")?.classList.toggle("active", panel === "stream");
  document.getElementById("btn-panel-issues")?.classList.toggle("active", panel === "issues");

  if (panel === "issues") {
    try {
      state.issues = await invoke("get_issues", { limit: 50 });
    } catch (e) {
      state.issues = [];
      console.warn("get_issues failed:", e);
    }
  }
  renderCurrentPanel();
}

function renderCurrentPanel() {
  if (state.activePanel === "issues") {
    renderIssuesPanel(state.issues, document.getElementById("stream"));
  } else {
    renderStream(state.events, document.getElementById("stream"), { autoFollow: state.autoFollow });
  }
}

function renderIssuesPanel(issues, root) {
  root.textContent = "";
  if (!issues || issues.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty";
    const icon = document.createElement("div");
    icon.className = "empty-icon";
    icon.textContent = "•";
    const title = document.createElement("div");
    title.className = "empty-title";
    title.textContent = "Khong co loi hook nao";
    const hint = document.createElement("div");
    hint.className = "empty-hint";
    hint.textContent = "hook_non_blocking_error se hien thi o day.";
    empty.append(icon, title, hint);
    root.appendChild(empty);
    return;
  }

  issues.forEach((issue) => {
    const card = document.createElement("div");
    card.className = "event issue-card";

    const gutter = document.createElement("div");
    gutter.className = "ev-gutter";

    const kindBadge = document.createElement("span");
    kindBadge.className = "ev-kind tool_result is-error";
    kindBadge.textContent = "[hook_error]";

    const time = document.createElement("span");
    time.className = "ev-time";
    time.textContent = issue.timestamp_label;

    gutter.append(kindBadge, time);

    const body = document.createElement("div");
    body.className = "ev-body";

    const sessionLine = document.createElement("div");
    sessionLine.style.cssText = "font-size:11px;color:var(--text-mute);margin-bottom:4px;";
    const sessionLink = document.createElement("span");
    sessionLink.style.cssText = "cursor:pointer;text-decoration:underline;color:var(--accent);";
    sessionLink.textContent = issue.session_title || issue.session_id;
    sessionLink.addEventListener("click", () => selectSession(issue.session_id));
    sessionLine.append(sessionLink);

    const kindLine = document.createElement("div");
    kindLine.style.cssText = "font-size:11px;color:var(--text-mute);margin-bottom:6px;";
    kindLine.textContent = issue.attachment_kind;

    const snippet = document.createElement("pre");
    snippet.style.cssText = "font-family:var(--font-mono,monospace);font-size:11px;white-space:pre-wrap;word-break:break-all;background:var(--bg-code,#1a1a1a);padding:6px 8px;border-radius:4px;margin:0;max-height:120px;overflow:auto;";
    snippet.textContent = issue.snippet || "(no content)";

    body.append(sessionLine, kindLine, snippet);
    card.append(gutter, body);
    root.appendChild(card);
  });
}

// Fallback: polling 5s nếu WS không kết nối được
let pollHandle = null;
function startPollingFallback() {
  if (pollHandle) return;
  console.log("[fallback] polling 5s");
  pollHandle = setInterval(async () => {
    await refreshSessionsAndStats();
    if (state.activeSessionId) {
      try {
        const evs = await invoke("get_events", { sessionId: state.activeSessionId });
        const newMax = evs.length ? evs[evs.length - 1].id : 0;
        if (newMax !== lastEventMaxId) {
          state.events = evs;
          lastEventMaxId = newMax;
          state.reachedOldest = false;
          renderCurrentPanel();
        }
      } catch {}
    }
  }, 5000);
}

function renderRecordingIndicator() {
  const active = state.sessions.filter((s) => s.status === "live").length;
  const label = document.getElementById("rec-label");
  if (label) label.textContent = `Recording (${active} active)`;
}

function renderQuickStats() {
  const fmt = (n) => (n >= 1000 ? (n / 1000).toFixed(1) + "k" : String(n));
  document.getElementById("stat-sessions").textContent = String(state.stats.sessions_today);
  document.getElementById("stat-tokens").textContent = fmt(state.stats.tokens_today);
  document.getElementById("stat-tools").textContent = String(state.stats.tool_calls_today);
}

async function selectSession(id) {
  state.activeSessionId = id;
  state.reachedOldest = false;
  state.loadingOlder = false;
  // Khi chọn session mới, switch về stream panel
  state.activePanel = "stream";
  document.getElementById("btn-panel-stream")?.classList.add("active");
  document.getElementById("btn-panel-issues")?.classList.remove("active");

  renderSessionList(state.sessions, document.getElementById("session-list"), {
    activeId: id,
    onSelect: (sid) => selectSession(sid),
  });

  try {
    state.events = await invoke("get_events", { sessionId: id });
    lastEventMaxId = state.events.length ? state.events[state.events.length - 1].id : 0;
  } catch (err) {
    console.error("Failed to load events:", err);
    state.events = [];
    lastEventMaxId = 0;
  }

  const session = state.sessions.find((s) => s.id === id);
  renderSessionHeader(session, document.getElementById("session-header"));
  renderCurrentPanel();
}

function renderEmptyStream() {
  const stream = document.getElementById("stream");
  // Static strings only - safe to use textContent-based DOM construction.
  stream.textContent = "";
  const wrap = document.createElement("div");
  wrap.className = "empty";
  const icon = document.createElement("div");
  icon.className = "empty-icon";
  icon.textContent = "•";
  const title = document.createElement("div");
  title.className = "empty-title";
  title.textContent = "Chưa có session nào";
  const hint = document.createElement("div");
  hint.className = "empty-hint";
  hint.textContent = "Mở Claude Code ở 1 thư mục bất kỳ để bắt đầu thu thập.";
  wrap.append(icon, title, hint);
  stream.appendChild(wrap);
}

function attachHandlers() {
  document.getElementById("auto-follow").addEventListener("change", (e) => {
    state.autoFollow = e.target.checked;
  });

  document.getElementById("btn-panel-stream").addEventListener("click", () => switchPanel("stream"));
  document.getElementById("btn-panel-issues").addEventListener("click", () => switchPanel("issues"));
  document.getElementById("btn-copy").addEventListener("click", () => flash("Copied to clipboard (mock)"));
  document.getElementById("btn-export").addEventListener("click", () => flash("Exported to /tmp/session.md (mock)"));
  document.getElementById("btn-open").addEventListener("click", () => flash("Open file in editor (mock)"));
  document.getElementById("btn-settings").addEventListener("click", () => flash("Settings (coming soon)"));
  document.getElementById("btn-help").addEventListener("click", () => flash("Help (coming soon)"));

  window.addEventListener("keydown", (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      openPalette();
    } else if (e.key === "Escape" && state.paletteOpen) {
      closePalette();
    } else if (state.paletteOpen) {
      handlePaletteKey(e);
    } else if (!isTypingInInput(e.target) && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      e.preventDefault();
      navigateSessions(e.key === "ArrowDown" ? 1 : -1);
    }
  });

  document.getElementById("palette-backdrop").addEventListener("click", (e) => {
    if (e.target.id === "palette-backdrop") closePalette();
  });

  document.getElementById("palette-input").addEventListener("input", (e) => {
    updatePaletteResults(e.target.value);
  });
}

function isTypingInInput(el) {
  return el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable);
}

function navigateSessions(delta) {
  if (state.sessions.length === 0) return;
  const idx = state.sessions.findIndex((s) => s.id === state.activeSessionId);
  let next = idx + delta;
  if (next < 0) next = 0;
  if (next >= state.sessions.length) next = state.sessions.length - 1;
  selectSession(state.sessions[next].id);
}

function openPalette() {
  state.paletteOpen = true;
  state.paletteIndex = 0;
  document.getElementById("palette-backdrop").classList.remove("hidden");
  const input = document.getElementById("palette-input");
  input.value = "";
  updatePaletteResults("");
  setTimeout(() => input.focus(), 30);
}

function closePalette() {
  state.paletteOpen = false;
  document.getElementById("palette-backdrop").classList.add("hidden");
}

function updatePaletteResults(query) {
  const q = query.trim().toLowerCase();
  const items = [];
  state.sessions.forEach((s) => {
    const haystack = `${s.project} ${s.ai_title} ${s.status} ${s.model}`.toLowerCase();
    if (!q || haystack.includes(q)) {
      items.push({
        type: "session",
        id: s.id,
        title: s.project,
        subtitle: s.ai_title,
        meta: `${s.status} · ${s.event_count} ev · ${s.duration_label}`,
      });
    }
  });
  state.paletteResults = items.slice(0, 12);
  state.paletteIndex = 0;
  renderPaletteResults();
}

function renderPaletteResults() {
  const ul = document.getElementById("palette-results");
  // Rebuild safely using textContent for all user-derived strings.
  ul.textContent = "";
  if (state.paletteResults.length === 0) {
    const li = document.createElement("li");
    li.className = "empty";
    li.style.cssText = "padding:14px;color:var(--text-mute);";
    li.textContent = "Không tìm thấy.";
    ul.appendChild(li);
    return;
  }
  state.paletteResults.forEach((r, i) => {
    const li = document.createElement("li");
    li.dataset.idx = String(i);
    if (i === state.paletteIndex) li.className = "active";
    const head = document.createElement("div");
    const titleSpan = document.createElement("span");
    titleSpan.textContent = r.title;
    const dash = document.createElement("span");
    dash.style.color = "var(--text-mute)";
    dash.textContent = " — " + r.subtitle;
    head.append(titleSpan, dash);
    const meta = document.createElement("div");
    meta.className = "pal-meta";
    meta.textContent = r.meta;
    li.append(head, meta);
    li.addEventListener("click", () => {
      state.paletteIndex = i;
      selectPaletteItem();
    });
    ul.appendChild(li);
  });
}

function handlePaletteKey(e) {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    state.paletteIndex = Math.min(state.paletteIndex + 1, state.paletteResults.length - 1);
    renderPaletteResults();
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    state.paletteIndex = Math.max(state.paletteIndex - 1, 0);
    renderPaletteResults();
  } else if (e.key === "Enter") {
    e.preventDefault();
    selectPaletteItem();
  }
}

function selectPaletteItem() {
  const item = state.paletteResults[state.paletteIndex];
  if (!item) return;
  if (item.type === "session") selectSession(item.id);
  closePalette();
}

let flashTimeout = null;
function flash(msg) {
  let el = document.getElementById("flash-toast");
  if (!el) {
    el = document.createElement("div");
    el.id = "flash-toast";
    el.style.cssText =
      "position:fixed;bottom:20px;left:50%;transform:translateX(-50%);background:var(--bg-elev);color:var(--text-0);padding:8px 16px;border-radius:8px;border:1px solid var(--border-strong);font-size:12px;z-index:200;opacity:0;transition:opacity 140ms;";
    document.body.appendChild(el);
  }
  el.textContent = msg;
  requestAnimationFrame(() => (el.style.opacity = "1"));
  clearTimeout(flashTimeout);
  flashTimeout = setTimeout(() => (el.style.opacity = "0"), 1600);
}

window.addEventListener("DOMContentLoaded", init);
