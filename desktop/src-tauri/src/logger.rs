use std::path::{Path, PathBuf};

use anyhow::Result;
use tracing_appender::{non_blocking::WorkerGuard, rolling};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

const LOG_PREFIX: &str = "claude-watchdog.log";

#[allow(dead_code)]
pub struct LogHandle {
    pub dir: PathBuf,
    _guard: WorkerGuard,
}

pub fn init(log_dir: &Path, level: &str) -> Result<LogHandle> {
    std::fs::create_dir_all(log_dir)?;
    let file_appender = rolling::daily(log_dir, LOG_PREFIX);
    let (nb, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(level))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let file_layer = fmt::layer().with_writer(nb).with_ansi(false).with_target(false);
    // Console copy goes to stderr so the CLI modes can keep stdout for JSON.
    let console_layer = fmt::layer().with_writer(std::io::stderr).with_target(false);

    let _ = tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .with(console_layer)
        .try_init();

    Ok(LogHandle {
        dir: log_dir.to_path_buf(),
        _guard: guard,
    })
}

/// Tail of today's log file (or the newest one), capped at `max_bytes`.
pub fn read_recent(dir: &Path, max_bytes: usize) -> String {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let candidate = dir.join(format!("{LOG_PREFIX}.{today}"));
    let path = if candidate.exists() {
        candidate
    } else {
        let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
        if let Ok(read) = std::fs::read_dir(dir) {
            for entry in read.flatten() {
                let Ok(meta) = entry.metadata() else { continue };
                if !meta.is_file() {
                    continue;
                }
                let Ok(mtime) = meta.modified() else { continue };
                let is_ours = entry.file_name().to_string_lossy().starts_with(LOG_PREFIX);
                if is_ours && newest.as_ref().map(|(t, _)| mtime > *t).unwrap_or(true) {
                    newest = Some((mtime, entry.path()));
                }
            }
        }
        match newest {
            Some((_, p)) => p,
            None => return String::new(),
        }
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return String::new();
    };
    if bytes.len() <= max_bytes {
        return String::from_utf8_lossy(&bytes).to_string();
    }
    let start = bytes.len() - max_bytes;
    String::from_utf8_lossy(&bytes[start..]).to_string()
}
