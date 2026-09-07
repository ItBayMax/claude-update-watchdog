use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::config::AppConfig;
use crate::model::{RepairRecord, WatchStatus};

const HISTORY_FILE: &str = "history.json";
const HISTORY_KEEP: usize = 100;

#[derive(Clone)]
pub struct AppState {
    pub app_dir: PathBuf,
    pub log_dir: PathBuf,
    pub config: Arc<Mutex<AppConfig>>,
    /// Latest snapshot produced by `status::collect`.
    pub status: Arc<Mutex<Option<WatchStatus>>>,
    /// Repair history, newest last. Persisted to history.json.
    pub history: Arc<Mutex<Vec<RepairRecord>>>,
    /// True while a repair pipeline is running (single-flight guard).
    pub repairing: Arc<AtomicBool>,
    /// EventRecordIDs of launch failures we already reacted to, so the
    /// subscription and the poll fallback never double-handle one event.
    pub handled_failures: Arc<Mutex<HashSet<u64>>>,
    pub scheduler_handle: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    pub scheduler_paused: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(app_dir: PathBuf, log_dir: PathBuf, cfg: AppConfig) -> Self {
        Self {
            app_dir,
            log_dir,
            config: Arc::new(Mutex::new(cfg)),
            status: Arc::new(Mutex::new(None)),
            history: Arc::new(Mutex::new(Vec::new())),
            repairing: Arc::new(AtomicBool::new(false)),
            handled_failures: Arc::new(Mutex::new(HashSet::new())),
            scheduler_handle: Arc::new(Mutex::new(None)),
            scheduler_paused: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn load_history(&self) {
        let path = self.app_dir.join(HISTORY_FILE);
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(list) = serde_json::from_slice::<Vec<RepairRecord>>(&bytes) {
                *self.history.lock() = list;
            }
        }
    }

    pub fn push_history(&self, record: RepairRecord) {
        let snapshot = {
            let mut h = self.history.lock();
            h.push(record);
            if h.len() > HISTORY_KEEP {
                let excess = h.len() - HISTORY_KEEP;
                h.drain(0..excess);
            }
            h.clone()
        };
        self.save_history(&snapshot);
    }

    pub fn clear_history(&self) {
        self.history.lock().clear();
        self.save_history(&[]);
    }

    fn save_history(&self, list: &[RepairRecord]) {
        let _ = std::fs::create_dir_all(&self.app_dir);
        let path = self.app_dir.join(HISTORY_FILE);
        let tmp = path.with_extension("json.tmp");
        if let Ok(bytes) = serde_json::to_vec_pretty(list) {
            if std::fs::write(&tmp, bytes).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
}
