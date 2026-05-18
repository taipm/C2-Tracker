use anyhow::{Context, Result};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::{
    collections::HashMap,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::{broadcast, mpsc}, time::sleep};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{db::DbPool, parser, types::WsMessage};

/// Chạy watcher loop. Kết thúc khi cancel token được trigger.
/// `ws_tx`: optional broadcast sender — None khi import-all CLI (không có ai listen).
pub async fn run_watcher(
    pool: Arc<DbPool>,
    root: PathBuf,
    mut trigger_rx: mpsc::Receiver<PathBuf>,
    cancel: CancellationToken,
    ws_tx: Option<broadcast::Sender<WsMessage>>,
) -> Result<()> {
    info!("Watcher starting, root = {}", root.display());

    // notify channel để bridge FSEvents callback (sync) sang async loop
    let (notify_tx, mut notify_rx) = mpsc::channel::<PathBuf>(256);
    let notify_tx_clone = notify_tx; // move vào closure bên dưới
    let root_clone = root.clone();

    // Spawn notify watcher trên blocking thread
    let mut watcher = notify::recommended_watcher(
        move |res: notify::Result<Event>| {
            match res {
                Ok(event) => {
                    if matches!(
                        event.kind,
                        EventKind::Modify(_) | EventKind::Create(_)
                    ) {
                        for path in event.paths {
                            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                                let _ = notify_tx_clone.try_send(path);
                            }
                        }
                    }
                }
                Err(e) => error!("notify error: {}", e),
            }
        },
    ).context("create notify watcher")?;

    watcher.watch(&root_clone, RecursiveMode::Recursive)
        .with_context(|| format!("watch {}", root_clone.display()))?;

    // Debounce map: path → debounce_until
    let mut debounce: HashMap<PathBuf, Instant> = HashMap::new();
    const DEBOUNCE: Duration = Duration::from_millis(100);
    const POLL: Duration = Duration::from_millis(50);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("Watcher shutting down");
                break;
            }

            // notify events
            maybe_path = notify_rx.recv() => {
                if let Some(path) = maybe_path {
                    debounce.insert(path, Instant::now() + DEBOUNCE);
                }
            }

            // manual trigger từ hook
            maybe_path = trigger_rx.recv() => {
                if let Some(path) = maybe_path {
                    debounce.insert(path, Instant::now()); // immediate
                }
            }

            // poll debounce queue
            _ = sleep(POLL) => {}
        }

        // Process bất kỳ file nào đã hết debounce
        let now = Instant::now();
        let ready: Vec<PathBuf> = debounce
            .iter()
            .filter(|(_, until)| now >= **until)
            .map(|(p, _)| p.clone())
            .collect();

        for path in ready {
            debounce.remove(&path);
            let pool_ref = Arc::clone(&pool);
            let path_clone = path.clone();
            let ws_tx_clone = ws_tx.clone();
            tokio::task::spawn_blocking(move || {
                match process_file(&pool_ref, &path_clone) {
                    Ok(Some(report)) => {
                        if let Some(tx) = &ws_tx_clone {
                            let _ = tx.send(WsMessage::SessionUpsert {
                                session_id: report.session_id.clone(),
                            });
                            if report.inserted > 0 {
                                let _ = tx.send(WsMessage::EventBatch {
                                    session_id: report.session_id,
                                    inserted: report.inserted,
                                });
                            }
                        }
                    }
                    Ok(None) => {} // không có gì mới
                    Err(e) => warn!("process_file error {}: {}", path_clone.display(), e),
                }
            });
        }
    }

    Ok(())
}

/// Báo cáo gọn từ một lần process_file — dùng để broadcast WS.
#[derive(Debug, Clone)]
pub struct ProcessReport {
    pub session_id: String,
    pub inserted: usize,
}

/// Parse file incremental và insert vào DB.
/// Return Ok(None) nếu file không có gì mới (cursor đã caught up), Ok(Some(report)) nếu đã insert.
pub fn process_file(pool: &DbPool, path: &Path) -> Result<Option<ProcessReport>> {
    let file_meta = std::fs::metadata(path)
        .with_context(|| format!("stat {}", path.display()))?;
    let current_inode = file_meta.ino();
    let current_size = file_meta.len();

    // Xác định session_id từ filename (UUID trước .jsonl)
    let session_id = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Lấy cursor từ DB
    let (mut offset, saved_inode) = crate::db::get_cursor(pool, &session_id)?;

    // Detect truncate: inode đổi hoặc size < offset
    let inode_changed = saved_inode.map_or(false, |i| i as u64 != current_inode);
    if inode_changed || current_size < offset as u64 {
        warn!(
            session_id = %session_id,
            "file truncated or inode changed — resetting offset to 0"
        );
        offset = 0;
    }

    // Không có gì mới
    if current_size <= offset as u64 {
        return Ok(None);
    }

    debug!(session_id = %session_id, offset, "parsing from offset");

    let batch = parser::parse_from_offset(path, offset as u64)?;

    if batch.events.is_empty() {
        return Ok(None);
    }

    let path_str = path.to_string_lossy().to_string();

    // Upsert session — session_id từ filename takes priority
    crate::db::upsert_session(pool, &batch.meta, &path_str, &session_id)?;

    // Insert events
    let (inserted, dups) = crate::db::insert_events(pool, &session_id, &batch.events)?;
    info!(
        session_id = %session_id,
        inserted,
        dups,
        parse_errors = batch.parse_errors,
        "file processed"
    );

    // Update cursor
    crate::db::update_cursor(pool, &session_id, batch.new_offset as i64, Some(current_inode))?;

    Ok(Some(ProcessReport { session_id, inserted }))
}

/// Import tất cả file JSONL trong root (scan một lần).
pub fn import_all(pool: &DbPool, root: &Path) -> Result<usize> {
    let mut count = 0usize;
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            info!("importing {}", path.display());
            match process_file(pool, path) {
                Ok(_) => count += 1,
                Err(e) => warn!("import error {}: {}", path.display(), e),
            }
        }
    }
    Ok(count)
}
