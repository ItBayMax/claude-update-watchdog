//! Event-driven reaction thread (analog of ip-killswitch's process_watcher).
//!
//! Subscribes to AppModel-Runtime event 208 for the Claude AUMID and hands
//! every delivery to `repair::handle_failure`. If the subscription cannot be
//! created the thread exits and the scheduler's poll covers the gap.

use tauri::AppHandle;
use tracing::{info, warn};

use crate::model::RepairTrigger;
use crate::state::AppState;

pub fn spawn(app: AppHandle, state: AppState) {
    std::thread::Builder::new()
        .name("claude-watchdog-eventlog".into())
        .spawn(move || run_blocking(app, state))
        .expect("failed to spawn event-log watcher thread");
}

fn run_blocking(app: AppHandle, state: AppState) {
    let aumid = state.config.lock().aumid();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    if let Err(e) = crate::eventlog::subscribe_launch_failures(&aumid, tx) {
        warn!("event-log subscription unavailable ({e}); relying on the periodic poll");
        return;
    }
    info!("event watcher running");
    for xml in rx {
        match crate::eventlog::parse_failure(&xml) {
            Some(failure) => crate::repair::handle_failure(&app, &state, failure, RepairTrigger::Event),
            None => warn!("received an event that could not be parsed"),
        }
    }
    warn!("event watcher channel closed");
}
