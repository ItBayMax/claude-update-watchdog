use tauri::{AppHandle, Emitter};
use tauri_plugin_autostart::ManagerExt;
use tracing::warn;

use crate::config::{self, AppConfig};
use crate::model::{
    ClaudeProcess, ContainerEvent, KillOutcome, RemoveOutcome, RepairRecord, RepairTrigger,
    StaleScan, TaskStatus, WatchStatus,
};
use crate::scheduler::SchedulerState;
use crate::state::AppState;

fn join_err(e: tauri::Error) -> String {
    format!("join error: {e}")
}

#[tauri::command]
pub fn get_config(state: tauri::State<'_, AppState>) -> AppConfig {
    state.config.lock().clone()
}

#[tauri::command]
pub async fn save_config(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    cfg: AppConfig,
) -> Result<(), String> {
    let old = state.config.lock().clone();
    config::save(&state.app_dir, &cfg).map_err(|e| e.to_string())?;
    *state.config.lock() = cfg.clone();

    if let Ok(enabled) = app.autolaunch().is_enabled() {
        if cfg.autostart && !enabled {
            let _ = app.autolaunch().enable();
        } else if !cfg.autostart && enabled {
            let _ = app.autolaunch().disable();
        }
    }
    if old.poll_seconds != cfg.poll_seconds {
        crate::scheduler::restart(app.clone(), state.inner().clone());
    }
    crate::tray::refresh_state(&app, state.inner());
    Ok(())
}

/// Fresh snapshot (event log + processes + registry). Also pushed to the UI.
#[tauri::command]
pub async fn get_status(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<WatchStatus, String> {
    let st = state.inner().clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || crate::status::collect(&st))
        .await
        .map_err(join_err)?;
    *state.status.lock() = Some(snapshot.clone());
    let _ = app.emit("claudewatchdog://status", &snapshot);
    crate::tray::refresh_state(&app, state.inner());
    Ok(snapshot)
}

#[tauri::command]
pub fn last_status(state: tauri::State<'_, AppState>) -> Option<WatchStatus> {
    state.status.lock().clone()
}

#[tauri::command]
pub async fn list_processes(state: tauri::State<'_, AppState>) -> Result<Vec<ClaudeProcess>, String> {
    let cfg = state.config.lock().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let registered = crate::packages::registered_packages(&cfg.package_family);
        let current = registered.first().map(|r| r.full_name.clone());
        crate::packages::claude_processes(&cfg.package_family, current.as_deref()).members
    })
    .await
    .map_err(join_err)
}

#[tauri::command]
pub async fn kill_processes(pids: Vec<u32>) -> Result<Vec<KillOutcome>, String> {
    tauri::async_runtime::spawn_blocking(move || crate::packages::kill(&pids))
        .await
        .map_err(join_err)
}

/// Manual repair (or a dry-run plan). Runs the blocking pipeline off the
/// async runtime; the UI gets progress through the repair-* events.
#[tauri::command]
pub async fn repair_now(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    dry_run: bool,
) -> Result<RepairRecord, String> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::repair::run_blocking(Some(&app), &st, RepairTrigger::Manual, dry_run, None)
    })
    .await
    .map_err(join_err)
}

#[tauri::command]
pub async fn launch_claude(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let aumid = state.config.lock().aumid();
    tauri::async_runtime::spawn_blocking(move || crate::packages::launch_aumid(&aumid))
        .await
        .map_err(join_err)?
}

#[tauri::command]
pub fn get_history(state: tauri::State<'_, AppState>) -> Vec<RepairRecord> {
    let mut h = state.history.lock().clone();
    h.reverse();
    h
}

#[tauri::command]
pub fn clear_history(state: tauri::State<'_, AppState>) {
    state.clear_history();
}

#[tauri::command]
pub async fn recent_events(
    state: tauri::State<'_, AppState>,
    max: Option<usize>,
) -> Result<Vec<ContainerEvent>, String> {
    let family = state.config.lock().package_family.clone();
    let n = max.unwrap_or(60).clamp(1, 300);
    tauri::async_runtime::spawn_blocking(move || crate::eventlog::container_events(&family, n))
        .await
        .map_err(join_err)
}

#[tauri::command]
pub async fn scan_stale(
    state: tauri::State<'_, AppState>,
    include_deleted: bool,
) -> Result<StaleScan, String> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || crate::stale::scan(&st, include_deleted))
        .await
        .map_err(join_err)
}

#[tauri::command]
pub async fn remove_stale(
    state: tauri::State<'_, AppState>,
    names: Vec<String>,
) -> Result<Vec<RemoveOutcome>, String> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || crate::stale::remove(&st, &names))
        .await
        .map_err(join_err)
}

#[tauri::command]
pub async fn task_status() -> Result<TaskStatus, String> {
    tauri::async_runtime::spawn_blocking(crate::task::status)
        .await
        .map_err(join_err)
}

#[tauri::command]
pub async fn install_task(state: tauri::State<'_, AppState>) -> Result<TaskStatus, String> {
    let aumid = state.config.lock().aumid();
    tauri::async_runtime::spawn_blocking(move || crate::task::install(&aumid))
        .await
        .map_err(join_err)?
}

#[tauri::command]
pub async fn uninstall_task() -> Result<TaskStatus, String> {
    tauri::async_runtime::spawn_blocking(crate::task::uninstall)
        .await
        .map_err(join_err)?
}

#[tauri::command]
pub async fn run_task_now() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(crate::task::run_now)
        .await
        .map_err(join_err)?
}

#[tauri::command]
pub fn read_logs(state: tauri::State<'_, AppState>, max_kb: Option<usize>) -> String {
    let cap = max_kb.unwrap_or(256).clamp(8, 4096) * 1024;
    crate::logger::read_recent(&state.log_dir, cap)
}

#[tauri::command]
pub fn open_log_dir(state: tauri::State<'_, AppState>) -> Result<(), String> {
    open_dir(&state.log_dir)
}

#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    open_dir(std::path::Path::new(&path))
}

fn open_dir(p: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("explorer.exe")
        .arg(p)
        .spawn()
        .map(|_| ())
        .map_err(|e| {
            warn!("open_dir failed: {e}");
            e.to_string()
        })
}

#[tauri::command]
pub fn autostart_status(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        app.autolaunch().enable().map_err(|e| e.to_string())
    } else {
        app.autolaunch().disable().map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) {
    crate::tray::show_main_window(&app);
}

#[tauri::command]
pub fn scheduler_status(state: tauri::State<'_, AppState>) -> SchedulerState {
    crate::scheduler::current_state(state.inner())
}

#[tauri::command]
pub async fn pause_scheduler(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<SchedulerState, String> {
    crate::scheduler::pause(state.inner());
    crate::tray::refresh_state(&app, state.inner());
    let _ = app.emit("claudewatchdog://scheduler-changed", ());
    Ok(crate::scheduler::current_state(state.inner()))
}

#[tauri::command]
pub async fn resume_scheduler(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<SchedulerState, String> {
    crate::scheduler::resume(app.clone(), state.inner().clone());
    let _ = app.emit("claudewatchdog://scheduler-changed", ());
    Ok(crate::scheduler::current_state(state.inner()))
}

#[tauri::command]
pub fn is_elevated() -> bool {
    crate::admin::is_elevated()
}

#[tauri::command]
pub async fn relaunch_as_admin() -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(crate::admin::relaunch_as_admin)
        .await
        .map_err(join_err)?
}

#[tauri::command]
pub fn get_platform() -> String {
    std::env::consts::OS.to_string()
}

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}
