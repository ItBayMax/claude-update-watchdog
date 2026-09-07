//! Periodic status refresh. Doubles as the polling fallback for the
//! event-log subscription: if a recent 208 was not handled yet (e.g. the
//! subscription could not be established), the tick reacts to it.

use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{async_runtime, AppHandle, Emitter};
use tokio::time::{interval, MissedTickBehavior};
use tracing::{info, warn};

use crate::model::RepairTrigger;
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerState {
    Disabled,
    Paused,
    Running,
}

pub fn current_state(state: &AppState) -> SchedulerState {
    if state.config.lock().poll_seconds == 0 {
        return SchedulerState::Disabled;
    }
    if state.scheduler_paused.load(Ordering::SeqCst) {
        return SchedulerState::Paused;
    }
    if state.scheduler_handle.lock().is_some() {
        SchedulerState::Running
    } else {
        SchedulerState::Paused
    }
}

pub fn stop(state: &AppState) {
    if let Some(h) = state.scheduler_handle.lock().take() {
        h.abort();
        info!("scheduler stopped");
    }
}

pub fn pause(state: &AppState) {
    state.scheduler_paused.store(true, Ordering::SeqCst);
    stop(state);
}

pub fn resume(app: AppHandle, state: AppState) {
    state.scheduler_paused.store(false, Ordering::SeqCst);
    restart(app, state);
}

pub fn restart(app: AppHandle, state: AppState) {
    stop(&state);
    if state.scheduler_paused.load(Ordering::SeqCst) {
        info!("scheduler paused — leaving timer stopped");
        crate::tray::refresh_state(&app, &state);
        return;
    }
    let secs = state.config.lock().poll_seconds;
    if secs == 0 {
        info!("scheduler disabled (poll_seconds = 0)");
        crate::tray::refresh_state(&app, &state);
        return;
    }
    let secs = secs.max(2);
    let app2 = app.clone();
    let state2 = state.clone();
    let h = async_runtime::spawn(async move {
        let mut t = interval(Duration::from_secs(secs));
        t.set_missed_tick_behavior(MissedTickBehavior::Delay);
        t.tick().await; // skip the immediate first tick
        loop {
            t.tick().await;
            run_one(&app2, &state2).await;
        }
    });
    *state.scheduler_handle.lock() = Some(h);
    info!("scheduler started: every {secs}s");
    crate::tray::refresh_state(&app, &state);
}

async fn run_one(app: &AppHandle, state: &AppState) {
    let st = state.clone();
    let snapshot = match async_runtime::spawn_blocking(move || crate::status::collect(&st)).await {
        Ok(s) => s,
        Err(e) => {
            warn!("status collect failed: {e}");
            return;
        }
    };
    *state.status.lock() = Some(snapshot.clone());
    let _ = app.emit("claudewatchdog://status", &snapshot);
    crate::tray::refresh_state(app, state);

    if let Some(f) = snapshot.recent_failure.clone() {
        if !snapshot.recent_failure_handled && !snapshot.repairing {
            let app2 = app.clone();
            let st2 = state.clone();
            async_runtime::spawn_blocking(move || {
                crate::repair::handle_failure(&app2, &st2, f, RepairTrigger::Poll);
            });
        }
    }
}
