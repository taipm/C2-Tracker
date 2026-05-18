// Stream renderer — chat-style transcript.
// All dynamic strings go through textContent to prevent XSS.

function fmtNum(n) {
  if (n == null) return "—";
  if (n >= 1000) return (n / 1000).toFixed(1) + "k";
  return String(n);
}

export function renderSessionHeader(session, root) {
  root.textContent = "";
  if (!session) return;

  const row1 = document.createElement("div");
  row1.className = "sh-row1";

  const project = document.createElement("span");
  project.className = "sh-project";
  project.textContent = session.project;

  const dot = document.createElement("span");
  dot.className = "sh-dot";
  dot.textContent = "·";

  const title = document.createElement("span");
  title.className = "sh-title";
  title.textContent = session.ai_title || "(untitled)";

  row1.append(project, dot, title);

  const row2 = document.createElement("div");
  row2.className = "sh-row2";
  row2.id = "sh-meta";

  const parts = [
    `started ${session.duration_label} ago`,
    `${session.duration_label}`,
    `${fmtNum(session.token_count)} tokens`,
    `${session.event_count} events`,
    `model: ${session.model}`,
  ];

  parts.forEach((p, i) => {
    const span = document.createElement("span");
    span.textContent = p;
    row2.appendChild(span);
    if (i < parts.length - 1) {
      const d = document.createElement("span");
      d.className = "sh-dot";
      d.textContent = "·";
      row2.appendChild(d);
    }
  });

  root.append(row1, row2);
}

export function renderStream(events, root, _opts) {
  root.textContent = "";
  if (!events || events.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty";
    const icon = document.createElement("div");
    icon.className = "empty-icon";
    icon.textContent = "•";
    const title = document.createElement("div");
    title.className = "empty-title";
    title.textContent = "No events yet";
    const hint = document.createElement("div");
    hint.className = "empty-hint";
    hint.textContent = "Stream sẽ cập nhật khi Claude Code ghi event mới.";
    empty.append(icon, title, hint);
    root.appendChild(empty);
    return;
  }

  events.forEach((ev) => root.appendChild(renderEvent(ev)));
  // Auto-scroll to bottom
  requestAnimationFrame(() => {
    root.scrollTop = root.scrollHeight;
  });
}

function renderEvent(ev) {
  const wrap = document.createElement("div");
  wrap.className = "event";

  const gutter = document.createElement("div");
  gutter.className = "ev-gutter";
  const kind = document.createElement("span");
  kind.className = "ev-kind " + ev.kind;
  kind.textContent = "[" + ev.kind + "]";
  const time = document.createElement("span");
  time.className = "ev-time";
  time.textContent = ev.timestamp_label || "";
  gutter.append(kind, time);

  const body = document.createElement("div");
  body.className = "ev-body " + ev.kind;

  if (ev.kind === "thinking") {
    renderThinking(ev, body);
  } else if (ev.kind === "tool_use" || ev.kind === "tool_result") {
    renderTool(ev, body);
  } else {
    // user / assistant / summary: plain text
    body.textContent = ev.text || "";
  }

  wrap.append(gutter, body);
  return wrap;
}

function renderThinking(ev, body) {
  const header = document.createElement("div");
  header.className = "thinking-header";
  const arrow = document.createElement("span");
  arrow.className = "arrow";
  arrow.textContent = "▸";
  const label = document.createElement("span");
  const dur = ev.thinking_duration_ms ? (ev.thinking_duration_ms / 1000).toFixed(1) + "s" : "—";
  const tok = ev.thinking_tokens ? ev.thinking_tokens + " tok" : "";
  label.textContent = ` ${dur}${tok ? " · " + tok : ""} · click to expand`;
  header.append(arrow, label);

  const content = document.createElement("div");
  content.className = "thinking-content";
  const inner = document.createElement("div");
  inner.style.padding = "6px 10px";
  inner.textContent = ev.text || "";
  content.appendChild(inner);

  header.addEventListener("click", () => {
    header.classList.toggle("expanded");
    content.classList.toggle("open");
  });

  body.append(header, content);
}

function renderTool(ev, body) {
  const block = document.createElement("div");
  block.className = "tool-block" + (ev.is_error ? " is-error" : "");

  const header = document.createElement("div");
  header.className = "tool-header";

  const arrow = document.createElement("span");
  arrow.className = "tool-arrow";
  arrow.textContent = "▸";

  const name = document.createElement("span");
  name.className = "tool-name";
  name.textContent = ev.tool_name || ev.kind;

  const summary = document.createElement("span");
  summary.className = "tool-summary";
  summary.textContent = ev.text || "";

  header.append(arrow, name, summary);

  const bodyEl = document.createElement("div");
  bodyEl.className = "tool-body";

  if (ev.tool_input) {
    const lbl = document.createElement("span");
    lbl.className = "tool-section-label";
    lbl.textContent = "input";
    const pre = document.createElement("pre");
    pre.textContent = prettifyJson(ev.tool_input);
    bodyEl.append(lbl, pre);
  }
  if (ev.tool_output) {
    const lbl = document.createElement("span");
    lbl.className = "tool-section-label";
    lbl.textContent = ev.is_error ? "output (error)" : "output";
    const pre = document.createElement("pre");
    pre.textContent = prettifyJson(ev.tool_output);
    bodyEl.append(lbl, pre);
  }

  header.addEventListener("click", () => {
    header.classList.toggle("expanded");
    bodyEl.classList.toggle("open");
  });

  block.append(header, bodyEl);
  body.appendChild(block);
}

function prettifyJson(s) {
  if (!s) return "";
  try {
    return JSON.stringify(JSON.parse(s), null, 2);
  } catch (_) {
    return String(s);
  }
}
