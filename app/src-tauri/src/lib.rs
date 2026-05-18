mod mock;

use mock::{Event, QuickStats, Session};

#[tauri::command]
fn get_sessions() -> Vec<Session> {
    mock::sessions()
}

#[tauri::command]
fn get_events(session_id: String) -> Vec<Event> {
    mock::events_for(&session_id)
}

#[tauri::command]
fn get_stats() -> QuickStats {
    mock::quick_stats()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_sessions,
            get_events,
            get_stats
        ])
        .run(tauri::generate_context!())
        .expect("error while running C2-Tracker");
}
