mod db;
mod parser;
mod server;
mod types;
mod watcher;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::{path::PathBuf, sync::Arc};
use tokio_util::sync::CancellationToken;
use tracing::info;

#[derive(Parser)]
#[command(name = "c2-engine", about = "C2-Tracker engine — JSONL parser + watcher + HTTP server")]
struct Cli {
    /// SQLite DB path (default: ~/.c2-tracker/c2.db)
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Parse một file JSONL rồi in stats (+ insert vào DB)
    ParseOnce {
        file: PathBuf,
    },
    /// Import toàn bộ session lịch sử từ root
    ImportAll {
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Chạy file watcher (không có HTTP server)
    Watch {
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Chạy watcher + HTTP server
    Serve {
        #[arg(long, default_value = "9786")]
        port: u16,
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// In bearer token hiện tại
    Token,
}

fn default_db() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".c2-tracker")
        .join("c2.db")
}

fn default_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
        .join("projects")
}

fn runtime_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".c2-tracker")
        .join("runtime.json")
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let cli = Cli::parse();
    let db_path = cli.db.unwrap_or_else(default_db);

    match cli.cmd {
        Cmd::ParseOnce { file } => cmd_parse_once(&db_path, &file),

        Cmd::ImportAll { root } => {
            let root = root.unwrap_or_else(default_root);
            let pool = Arc::new(db::open(&db_path)?);
            let count = watcher::import_all(&pool, &root)?;
            println!("Imported {} files", count);
            Ok(())
        }

        Cmd::Watch { root } => {
            let root = root.unwrap_or_else(default_root);
            let pool = Arc::new(db::open(&db_path)?);
            let cancel = CancellationToken::new();
            let cancel_clone = cancel.clone();

            // trigger_tx phải sống suốt vòng đời của watcher
            let (trigger_tx, trigger_rx) = tokio::sync::mpsc::channel::<std::path::PathBuf>(128);
            let _trigger_tx_keep = trigger_tx; // giữ sender để channel không close

            // Ctrl+C handler
            tokio::spawn(async move {
                tokio::signal::ctrl_c().await.ok();
                info!("Received Ctrl+C, shutting down");
                cancel_clone.cancel();
            });

            watcher::run_watcher(pool, root, trigger_rx, cancel).await?;
            Ok(())
        }

        Cmd::Serve { port, root } => {
            let root = root.unwrap_or_else(default_root);
            let pool = Arc::new(db::open(&db_path)?);
            let cancel = CancellationToken::new();
            let cancel_c1 = cancel.clone();
            let cancel_c2 = cancel.clone();

            // Load hoặc sinh token
            let rt_path = runtime_path();
            let token = if rt_path.exists() {
                match db::read_runtime_token(&rt_path) {
                    Ok((_, t)) => t,
                    Err(_) => db::write_runtime_token(&rt_path, port)?,
                }
            } else {
                db::write_runtime_token(&rt_path, port)?
            };

            info!("Token loaded from {}", rt_path.display());

            // Watcher
            let (trigger_tx, trigger_rx) = tokio::sync::mpsc::channel::<std::path::PathBuf>(128);
            let watcher_tx = trigger_tx.clone();
            let _trigger_tx_keep = trigger_tx; // giữ để channel open
            let pool_w = Arc::clone(&pool);

            let watcher_handle = tokio::spawn(async move {
                if let Err(e) = watcher::run_watcher(pool_w, root, trigger_rx, cancel_c1).await {
                    tracing::error!("watcher error: {}", e);
                }
            });

            // HTTP server
            let state = server::AppState {
                pool: Arc::clone(&pool),
                token,
                watcher_tx,
            };

            let server_handle = tokio::spawn(async move {
                if let Err(e) = server::run_server(state, port, cancel_c2).await {
                    tracing::error!("server error: {}", e);
                }
            });

            // Ctrl+C
            tokio::signal::ctrl_c().await.ok();
            info!("Received Ctrl+C, shutting down");
            cancel.cancel();

            let _ = tokio::join!(watcher_handle, server_handle);
            info!("Shutdown complete");
            Ok(())
        }

        Cmd::Token => {
            let rt_path = runtime_path();
            if rt_path.exists() {
                let (port, token) = db::read_runtime_token(&rt_path)?;
                println!("Port : {}", port);
                println!("Token: {}", token);
            } else {
                println!("No runtime.json found at {}", rt_path.display());
                println!("Run `c2-engine serve` to generate a token.");
            }
            Ok(())
        }
    }
}

fn cmd_parse_once(db_path: &PathBuf, file: &PathBuf) -> Result<()> {
    let file_meta = std::fs::metadata(file)
        .with_context(|| format!("stat {}", file.display()))?;

    let (stats, batch) = parser::parse_file(file)?;
    parser::print_stats(file, file_meta.len(), &stats);

    // Insert vào DB
    let pool = db::open(db_path)?;
    let path_str = file.to_string_lossy().to_string();

    let session_id = file.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Upsert session trước để có FK target cho events
    db::upsert_session(&pool, &batch.meta, &path_str, &session_id)?;
    // insert_events dùng INSERT OR IGNORE → events đã có uuid sẽ bị skip (idempotent)
    // tokens trong session có thể bị double-count nếu parse_once chạy 2 lần — OK cho diagnostic cmd
    let (inserted, dups) = db::insert_events(&pool, &session_id, &batch.events)?;

    // Lưu offset cuối
    use std::os::unix::fs::MetadataExt;
    db::update_cursor(&pool, &session_id, batch.new_offset as i64, Some(file_meta.ino()))?;

    println!("\n== DB insert ==");
    println!("  inserted  : {}", inserted);
    println!("  duplicates: {}", dups);
    println!("  db path   : {}", db_path.display());

    Ok(())
}
