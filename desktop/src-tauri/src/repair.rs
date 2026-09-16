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

/// Launch the AUMID once and wait up to `WAIT_UP_SECS` for a process of the
/// current version to appear. Returns whether Claude came up.
fn launch_and_wait(
    cfg: &crate::config::AppConfig,
    aumid: &str,
    current: Option<&str>,
    record: &mut RepairRecord,
    phase: &str,
) -> bool {
    match crate::packages::launch_aumid(aumid) {
        Ok(()) => {
            record.relaunched = true;
            info!(phase, "repair: launched {aumid}");
        }
        Err(e) => {
            warn!(phase, "repair: launch call failed: {e}");
            return false;
        }
    }
    let deadline = Instant::now() + Duration::from_secs(WAIT_UP_SECS);
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let now = crate::packages::claude_processes(&cfg.package_family, current).members;
        let alive = now.iter().any(|p| {
            current
                .map(|c| c.eq_ignore_ascii_case(&p.package_full_name))
                .unwrap_or(true)
        });
        if alive {
            return true;
        }
        if Instant::now() >= deadline {
            warn!(phase, "repair: no Claude process appeared within {WAIT_UP_SECS}s");
            return false;
        }
    }
}

/// Restart the AppX Deployment Service.
///
/// When the per-user container job is wedged with no process inside it, the
/// object is kept alive by a leaked handle. Enumeration shows the remaining
/// handle holders are processes an unprivileged caller cannot even open, and
/// AppXSvc (inside a shared svchost) is the service that owns container
/// lifetime. Restarting it drops those handles without killing svchost itself.
fn restart_appxsvc() -> Result<String, String> {
    // -Force also stops dependents (WSAIFabricSvc on machines with the Android
    // subsystem), and Start-Service does not bring them back, so restart the
    // ones that were running.
    let script = "\
$ErrorActionPreference = 'Stop'
$svc = Get-Service -Name AppXSvc
$deps = @($svc.DependentServices | Where-Object { $_.Status -eq 'Running' } | ForEach-Object { $_.Name })
if ($svc.Status -eq 'Running') { Stop-Service -Name AppXSvc -Force }
$deadline = (Get-Date).AddSeconds(20)
while ((Get-Service -Name AppXSvc).Status -ne 'Stopped' -and (Get-Date) -lt $deadline) {
  Start-Sleep -Milliseconds 300
}
Start-Service -Name AppXSvc
foreach ($d in $deps) {
  try { Start-Service -Name $d -ErrorAction Stop } catch { Write-Output \"dependent $d not restarted: $($_.Exception.Message)\" }
}
'AppXSvc=' + (Get-Service -Name AppXSvc).Status + ' restored_dependents=' + ($deps -join ',')";
    crate::task::run_powershell(script)
}

/// Re-register the package for the current user.
///
/// `Add-AppxPackage -Register` rebuilds the per-user registration from the
/// manifest that is already on disk. It does not touch `%APPDATA%\\Claude`,
/// the package's LocalCache, or `%USERPROFILE%\\.claude`, so login, sessions
/// and settings survive. This is deliberately NOT `Reset-AppxPackage`, which
/// would wipe app data and force a fresh login.
fn reregister_package(full_name: &str) -> Result<String, String> {
    if !full_name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(format!("refusing to re-register suspicious name: {full_name}"));
    }
    let script = format!(
        "\
$ErrorActionPreference = 'Stop'
$m = Join-Path $env:ProgramFiles 'WindowsApps\\{full_name}\\AppxManifest.xml'
if (-not (Test-Path -LiteralPath $m)) {{ throw \"manifest not found: $m\" }}
Add-AppxPackage -Register $m -DisableDevelopmentMode
'registered'"
    );
    crate::task::run_powershell(&script)
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

    // 3. relaunch, escalating through container-level remedies when a plain
    //    relaunch is not enough.
    //
    //    Two distinct failure classes produce the same 0x80070020:
    //      A. a process of the old version survived  -> stopping it is the fix
    //      B. the per-user container job is wedged with no process left in it
    //         -> there is nothing to stop, and relaunching just fails again.
    //    `procs.is_empty()` separates them: no package-identity process existed
    //    when the snapshot was taken, so class B.
    let container_wedged = procs.is_empty();
    let stale_containers =
        crate::eventlog::stale_containers(&cfg.package_family, current.as_deref(), 400);
    if container_wedged {
        warn!(
            stale_containers = ?stale_containers,
            "repair: no Claude process to stop -> container-level failure; \
             terminating processes cannot fix this class"
        );
    }
    for (pkg, id) in &stale_containers {
        warn!(
            container_id = %id,
            "repair: container still open for {pkg} — this blocks the current version"
        );
    }

    let mut running = launch_and_wait(&cfg, &aumid, current.as_deref(), &mut record, "relaunch");
    let mut remedy: Option<&str> = None;

    if !running && container_wedged && cfg.restart_appxsvc_on_container_failure {
        if crate::admin::is_elevated() {
            match restart_appxsvc() {
                Ok(out) => {
                    info!(status = %out.trim(), "repair: restarted AppXSvc");
                    running =
                        launch_and_wait(&cfg, &aumid, current.as_deref(), &mut record, "after-appxsvc");
                    if running {
                        remedy = Some("重启 AppXSvc");
                    }
                }
                Err(e) => warn!("repair: could not restart AppXSvc: {e}"),
            }
        } else {
            warn!("repair: AppXSvc remedy skipped, not running as administrator");
        }
    }

    if !running && container_wedged && cfg.reregister_on_container_failure {
        if let Some(full) = current.as_deref() {
            match reregister_package(full) {
                Ok(_) => {
                    info!(package = full, "repair: re-registered package for current user");
                    running = launch_and_wait(
                        &cfg,
                        &aumid,
                        current.as_deref(),
                        &mut record,
                        "after-reregister",
                    );
                    if running {
                        remedy = Some("重新注册程序包");
                    }
                }
                Err(e) => warn!("repair: re-register failed: {e}"),
            }
        }
    }

    // One last plain retry for class A, where the first launch can race the
    // container teardown.
    if !running && !container_wedged {
        std::thread::sleep(Duration::from_secs(5));
        running = launch_and_wait(&cfg, &aumid, current.as_deref(), &mut record, "retry");
    }

    record.success = running;
    record.finished_at = Utc::now();
    record.message = match (running, remedy, container_wedged) {
        (true, Some(r), _) => format!(
            "容器级故障，已通过{}恢复并启动 Claude {}",
            r,
            record.target_version.clone().unwrap_or_default()
        ),
        (true, None, _) => format!(
            "已结束 {} 个进程并重新启动 Claude {}",
            killed_count,
            record.target_version.clone().unwrap_or_default()
        ),
        (false, _, true) => {
            let blocker = if stale_containers.is_empty() {
                "没有可结束的 Claude 进程，杀进程无效".to_string()
            } else {
                format!(
                    "旧版本 {} 的容器仍未销毁，且没有可结束的进程",
                    stale_containers
                        .iter()
                        .map(|(p, _)| crate::packages::version_of(p))
                        .collect::<Vec<_>>()
                        .join("、")
                )
            };
            format!(
                "容器级故障：{blocker}。补救手段均未生效，需要注销后重新登录或重启电脑"
            )
        }
        (false, _, false) => "重新启动失败两次，请注销后重新登录或重启电脑".to_string(),
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
