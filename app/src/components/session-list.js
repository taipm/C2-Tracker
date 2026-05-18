// Renders the session sidebar list using DOM APIs (no innerHTML).
// All text comes from Tauri backend mock or browser fallback — but we still
// use textContent everywhere to keep the surface XSS-safe.

const STATUS_LABEL = {
  live: "● live",
  idle: "  idle",
  ended: "  done",
};

export function renderSessionList(sessions, root, { activeId, onSelect }) {
  root.textContent = "";
  if (!sessions || sessions.length === 0) {
    const empty = document.createElement("li");
    empty.style.cssText = "padding:18px;color:var(--text-mute);font-size:12px;text-align:center;";
    empty.textContent = "Chưa có session";
    root.appendChild(empty);
    return;
  }

  sessions.forEach((s) => {
    const li = document.createElement("li");
    li.className = "session-item" + (s.id === activeId ? " active" : "");
    li.dataset.id = s.id;

    // Row 1: status dot · status label · project name
    const row1 = document.createElement("div");
    row1.className = "si-row1";

    const dot = document.createElement("span");
    dot.className = "si-dot " + s.status;
    row1.appendChild(dot);

    const status = document.createElement("span");
    status.className = "si-status " + s.status;
    status.textContent = (s.status || "").toUpperCase();
    row1.appendChild(status);

    const project = document.createElement("span");
    project.className = "si-project";
    project.textContent = s.project;
    row1.appendChild(project);

    // Row 2: time ago · events
    const row2 = document.createElement("div");
    row2.className = "si-meta";
    const time = document.createElement("span");
    time.textContent = s.duration_label + " ago";
    const events = document.createElement("span");
    events.textContent = s.event_count + "ev";
    row2.append(time, events);

    // Row 3: AI title (truncated by CSS)
    const title = document.createElement("div");
    title.className = "si-title";
    title.textContent = s.ai_title || "(untitled)";

    li.append(row1, row2, title);
    li.addEventListener("click", () => onSelect && onSelect(s.id));
    root.appendChild(li);
  });
}
