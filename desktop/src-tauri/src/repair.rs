//! The repair pipeline.
//!
//! Why it works: the new Claude version cannot create its Desktop AppX
//! container while a process of the *old* version is still alive (Windows
//! reports 0x80070020). Stopping every process that carries Claude package
//! identity tears the old container down; relaunching the AUMID then starts
//! the registered (new) version normally.
//!
//! Steps: single-flight guard → rate limit → snapshot → stop processes →
//! wait until gone → settle → launch AUMID → wait for the new version →
//! (retry once) → record + notify.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use chrono::Utc;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;
use tracing::{info, warn};

use crate::eventlog::ERROR_SHARING_VIOLATION_HRESULT;
use crate::model::{LaunchFailure, RepairRecord, RepairTrigger};
use crate::state::AppState;

const RATE_WINDOW_MINUTES: i64 = 15;
const WAIT_GONE_SECS: u64 = 20;
const SETTLE_SECS: u64 = 2;
const WAIT_UP_SECS: u64 = 25;

struct RepairingGuard<'a>(&'a AppState);
impl Drop for RepairingGuard<'_> {
    fn drop(&mut self) {
        self.0.repairing.store(false, Ordering::SeqCst);
    }
}

pub fn notify(app: Option<&AppHandle>, state: &AppState, title: &str, body: &str) {
    if !state.config.lock().notify {
        return;
    }
    if let Some(app) = app {
        let _ = app.notification().builder().title(title).body(body).show();
    }
}

fn emit<T: serde::Serialize + Clone>(app: Option<&AppHandle>, name: &str, payload: &T) {
    if let Some(app) = app {
        let _ = app.emit(name, payload);
    }
}

/// Non-dry-run attempts within the rate-limit window.
pub fn recent_attempts(state: &AppState) -> usize {
    let cut = Utc::now() - chrono::Duration::minutes(RATE_WINDOW_MINUTES);
    state
        .history
        .lock()
        .iter()
        .filter(|r| !r.dry_run && r.started_at > cut)
        .count()
}

/// Run the pipeline on the current thread (blocking; up to ~90 s).
/// `app` is `None` in the headless CLI / scheduled-task modes.
pub fn run_blocking(
    app: Option<&AppHandle>,
    state: &AppState,
    trigger: RepairTrigger,
    dry_run: bool,
    failure: Option<LaunchFailure>,
) -> RepairRecord {
    let started_at = Utc::now();
    let cfg = state.config.lock().clone();
    let aumid = cfg.aumid();
    let mut record = RepairRecord {
        id: uuid::Uuid::new_v4().to_string(),
        started_at,
        finished_at: started_at,
        trigger,
        dry_run,
        target_version: None,
        planned: Vec::new(),
        killed: Vec::new(),
        relaunched: false,
        success: false,
        message: String::new(),
        failure: failure.clone(),
    };

    if state
        .repairing
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        record.message = "已有修复在进行中，本次跳过".into();
        record.finished_at = Utc::now();
        return record;
    }
    let _guard = RepairingGuard(state);

    if !dry_run {
        let n = recent_attempts(state);
        if n as u32 >= cfg.max_attempts_per_15min {
            record.message = format!(
                "rate limit: 最近 15 分钟内已有 {n} 次修复尝试，本次不再执行"
            );
            record.finished_at = Utc::now();
            warn!("{}", record.message);
            state.push_history(record.clone());
            emit(app, "claudewatchdog://repair-finished", &record);
            return record;
        }
    }

    emit(app, "claudewatchdog://repair-started", &record);
    if let Some(a) = app {
        crate::tray::refresh_state(a, state);
    }

    let registered = crate::packages::registered_packages(&cfg.package_family);
    let current = registered.first().map(|r| r.full_name.clone());
    record.target_version = current.as_deref().map(crate::packages::version_of);
    let snap = crate::packages::claude_processes(&cfg.package_family, current.as_deref());
    let procs = snap.members;
    record.planned = procs.clone();
    info!(
        ?trigger,
        dry_run,
        processes = procs.len(),
        orphans = procs.iter().filter(|p| p.orphan).count(),
        skipped_services = snap.services.len(),
        current = ?current,
        "repair: snapshot taken"
    );
    for s in &snap.services {
        info!(
            pid = s.pid,
            name = %s.name,
            session = ?s.session_id,
            "repair: leaving packaged service / other-session process alone (managed by Windows)"
        );
    }

    if dry_run {
        record.success = true;
        record.message = format!("预演：将结束 {} 个进程并重新启动 {}", procs.len(), aumid);
        record.finished_at = Utc::now();
        emit(app, "claudewatchdog://repair-finished", &record);
        if let Some(a) = app {
            crate::tray::refresh_state(a, state);
        }
        return record;
    }

    // 1. stop every process with Claude package identity (old and new version)
    let pids: Vec<u32> = procs.iter().map(|p| p.pid).collect();
    record.killed = crate::packages::kill(&pids);
    let killed_count = record.killed.iter().filter(|k| k.killed).count();
    info!(killed = killed_count, of = pids.len(), "repair: processes stopped");

    // 2. wait for the containers to go away
    let deadline = Instant::now() + Duration::from_secs(WAIT_GONE_SECS);
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let left = crate::packages::claude_processes(&cfg.package_family, current.as_deref()).members;
        if left.is_empty() {
            break;
        }
        if Instant::now() >= deadline {
            warn!(left = left.len(), "repair: processes still alive after {WAIT_GONE_SECS}s");
            break;
        }
    }
    std::thread::sleep(Duration::from_secs(SETTLE_SECS));

    // 3. relaunch, verify, retry once
    let mut running = false;
    for attempt in 1..=2u32 {
        match crate::packages::launch_aumid(&aumid) {
            Ok(()) => {
                record.relaunched = true;
                info!(attempt, "repair: launched {aumid}");
            }
            Err(e) => {
                warn!(attempt, "repair: launch call failed: {e}");
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }
        }
        let deadline = Instant::now() + Duration::from_secs(WAIT_UP_SECS);
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let now =
                crate::packages::claude_processes(&cfg.package_family, current.as_deref()).members;
            let alive = now.iter().any(|p| {
                current
                    .as_deref()
                    .map(|c| c.eq_ignore_ascii_case(&p.package_full_name))
                    .unwrap_or(true)
            });
            if alive {
                running = true;
                break;
            }
            if Instant::now() >= deadline {
                break;
            }
        }
        if running {
            break;
        }
        warn!(attempt, "repair: no Claude process appeared within {WAIT_UP_SECS}s");
        std::thread::sleep(Duration::from_secs(5));
    }

    record.success = running;
    record.finished_at = Utc::now();
    record.message = if running {
        format!(
            "已结束 {} 个进程并重新启动 Claude {}",
            killed_count,
            record.target_version.clone().unwrap_or_default()
        )
    } else {
        "重新启动失败两次，请注销后重新登录或重启电脑".to_string()
    };
    if running {
        if let Some(f) = &failure {
            state.handled_failures.lock().insert(f.record_id);
        }
    }
    info!(success = running, "repair: {}", record.message);

    state.push_history(record.clone());
    emit(app, "claudewatchdog://repair-finished", &record);
    notify(
        app,
        state,
        if running { "Claude 已自动修复" } else { "Claude 修复失败" },
        &record.message,
    );
    if let Some(a) = app {
        crate::tray::refresh_state(a, state);
    }
    record
}

/// Shared reaction to a launch failure, used by the event subscription and
/// by the poll fallback. Each EventRecordID is handled at most once.
pub fn handle_failure(
    app: &AppHandle,
    state: &AppState,
    failure: LaunchFailure,
    trigger: RepairTrigger,
) {
    {
        let mut handled = state.handled_failures.lock();
        if handled.contains(&failure.record_id) {
            return;
        }
        handled.insert(failure.record_id);
    }
    if failure.error_code != ERROR_SHARING_VIOLATION_HRESULT {
        info!(
            record_id = failure.record_id,
            error = %failure.error_hex,
            "launch failure with a different error code; not a container conflict, ignoring"
        );
        return;
    }
    let cfg = state.config.lock().clone();
    info!(
        record_id = failure.record_id,
        package = %failure.package_full_name,
        ?trigger,
        "Claude launch failure 0x80070020 detected"
    );
    let _ = app.emit("claudewatchdog://failure", &failure);
    crate::tray::refresh_state(app, state);

    if cfg.auto_repair {
        if cfg.repair_delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(cfg.repair_delay_ms));
        }
        let _ = run_blocking(Some(app), state, trigger, false, Some(failure));
    } else {
        notify(
            Some(app),
            state,
            "Claude 启动失败",
            "检测到 0x80070020（旧版本进程残留）。自动修复已关闭，请在 Claude Watchdog 中确认修复。",
        );
        let _ = app.emit("claudewatchdog://prompt-repair", &failure);
        crate::tray::show_main_window(app);
    }
}
