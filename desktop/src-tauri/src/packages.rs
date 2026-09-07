//! Everything about the Claude MSIX package and its processes.
//!
//! - Registered versions come from the per-user AppModel repository cache in
//!   the registry (no WinRT needed):
//!   `HKCU\Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages\<PackageFullName>`
//! - Process ↔ package association uses `GetPackageFullName` on a
//!   `PROCESS_QUERY_LIMITED_INFORMATION` handle. Only processes that really
//!   run inside the package (Claude.exe + its Electron helpers) report a
//!   package; the Claude Code CLI and shells spawned from it do not, so they
//!   are never touched.
//! - Kill goes through OpenProcess/TerminateProcess so ACCESS_DENIED can be
//!   told apart from other failures.
//! - Relaunch activates the AUMID via `shell:AppsFolder\<AUMID>`.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};
use tracing::{debug, warn};
use winreg::enums::*;
use winreg::RegKey;

use crate::model::{ClaudeProcess, KillOutcome, RegisteredPackage, SimpleProcess};

const REPO_PACKAGES_KEY: &str = r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages";

pub fn windows_apps_dir() -> PathBuf {
    let pf = std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
    pf.join("WindowsApps")
}

/// "Claude_pzs8sxrjxfjjc" → ("Claude", "pzs8sxrjxfjjc")
pub fn family_parts(family: &str) -> (String, String) {
    match family.split_once('_') {
        Some((n, h)) => (n.to_string(), h.to_string()),
        None => (family.to_string(), String::new()),
    }
}

/// `Claude_1.46388.4.0_x64__pzs8sxrjxfjjc` belongs to family `Claude_pzs8sxrjxfjjc`.
pub fn is_family_member(full_name: &str, family: &str) -> bool {
    let (name, hash) = family_parts(family);
    full_name.starts_with(&format!("{name}_")) && full_name.ends_with(&format!("__{hash}"))
}

pub fn version_of(full_name: &str) -> String {
    full_name.split('_').nth(1).unwrap_or_default().to_string()
}

fn version_key(v: &str) -> Vec<u64> {
    v.split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect()
}

/// Packages of the family registered for the current user, newest first.
pub fn registered_packages(family: &str) -> Vec<RegisteredPackage> {
    let mut out = Vec::new();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = match hkcu.open_subkey_with_flags(REPO_PACKAGES_KEY, KEY_READ) {
        Ok(k) => k,
        Err(e) => {
            debug!("AppModel repository key not readable: {e}");
            return out;
        }
    };
    for name in key.enum_keys().flatten() {
        if !is_family_member(&name, family) {
            continue;
        }
        let root: Option<String> = key
            .open_subkey(&name)
            .ok()
            .and_then(|k| k.get_value::<String, _>("PackageRootFolder").ok());
        out.push(RegisteredPackage {
            version: version_of(&name),
            full_name: name,
            root_folder: root,
        });
    }
    out.sort_by(|a, b| version_key(&b.version).cmp(&version_key(&a.version)));
    out
}

/// Package full name of a running process, if it has package identity.
#[cfg(windows)]
pub fn package_full_name_of(pid: u32) -> Option<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Storage::Packaging::Appx::GetPackageFullName;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut len: u32 = 1024;
        let mut buf: Vec<u16> = vec![0; 1024];
        let rc = GetPackageFullName(handle, &mut len, buf.as_mut_ptr());
        let _ = CloseHandle(handle);
        if rc != 0 {
            // 15700 = APPMODEL_ERROR_NO_PACKAGE (plain process) — the common case.
            return None;
        }
        let n = (len as usize).saturating_sub(1).min(buf.len());
        Some(String::from_utf16_lossy(&buf[..n]))
    }
}

#[cfg(not(windows))]
pub fn package_full_name_of(_pid: u32) -> Option<String> {
    None
}

/// One pass over the process table.
pub struct ProcessSnapshot {
    /// Package-identity processes in the caller's own session — the user's
    /// Desktop AppX container. These are what a repair acts on.
    pub members: Vec<ClaudeProcess>,
    /// Package-identity processes outside the session: the packaged
    /// CoworkVMService (LocalSystem, session 0) or another user's Claude.
    /// Only visible when elevated. Never terminated.
    pub services: Vec<ClaudeProcess>,
    /// "claude*" processes without package identity (Claude Code CLI, shells).
    pub others: Vec<SimpleProcess>,
}

pub fn claude_processes(family: &str, current_full_name: Option<&str>) -> ProcessSnapshot {
    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let (name_part, _) = family_parts(family);
    let needle = name_part.to_lowercase();
    let me = std::process::id();
    let my_session: Option<u32> = sys
        .process(sysinfo::Pid::from_u32(me))
        .and_then(|p| p.session_id())
        .map(|s| s.as_u32());

    let mut members: Vec<ClaudeProcess> = Vec::new();
    let mut services: Vec<ClaudeProcess> = Vec::new();
    let mut others: Vec<SimpleProcess> = Vec::new();
    for (pid, proc) in sys.processes() {
        let pid_u = pid.as_u32();
        if pid_u == me {
            continue;
        }
        let pname = proc.name().to_string_lossy().to_string();
        let exe = proc.exe().map(|p| p.to_string_lossy().to_string());
        match package_full_name_of(pid_u) {
            Some(pkg) if is_family_member(&pkg, family) => {
                let started_at = DateTime::<Utc>::from_timestamp(proc.start_time() as i64, 0);
                let orphan = current_full_name
                    .map(|c| !c.eq_ignore_ascii_case(&pkg))
                    .unwrap_or(false);
                let session_id = proc.session_id().map(|s| s.as_u32());
                let is_service = match (session_id, my_session) {
                    (Some(0), _) => true,
                    (Some(s), Some(mine)) => s != mine,
                    _ => false,
                };
                let entry = ClaudeProcess {
                    pid: pid_u,
                    name: pname,
                    version: version_of(&pkg),
                    package_full_name: pkg,
                    exe,
                    started_at,
                    orphan,
                    session_id,
                    is_service,
                };
                if is_service {
                    services.push(entry);
                } else {
                    members.push(entry);
                }
            }
            _ => {
                if pname.to_lowercase().contains(&needle) {
                    others.push(SimpleProcess { pid: pid_u, name: pname, exe });
                }
            }
        }
    }
    let order = |a: &ClaudeProcess, b: &ClaudeProcess| {
        a.package_full_name
            .cmp(&b.package_full_name)
            .then(a.started_at.cmp(&b.started_at))
            .then(a.pid.cmp(&b.pid))
    };
    members.sort_by(order);
    services.sort_by(order);
    others.sort_by_key(|p| p.pid);
    ProcessSnapshot { members, services, others }
}

struct KillErr {
    denied: bool,
    reason: String,
}

#[cfg(windows)]
fn kill_one(pid: u32) -> Result<(), KillErr> {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ACCESS_DENIED};
    use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if handle.is_null() {
            let code = GetLastError();
            return Err(if code == ERROR_ACCESS_DENIED {
                KillErr { denied: true, reason: "拒绝访问（受保护进程）".into() }
            } else {
                KillErr { denied: false, reason: format!("OpenProcess 失败 (Win32 error {code})") }
            });
        }
        let ok = TerminateProcess(handle, 1);
        let err_code = if ok == 0 { GetLastError() } else { 0 };
        let _ = CloseHandle(handle);
        if ok == 0 {
            return Err(if err_code == ERROR_ACCESS_DENIED {
                KillErr { denied: true, reason: "拒绝访问（受保护进程）".into() }
            } else {
                KillErr { denied: false, reason: format!("TerminateProcess 失败 (Win32 error {err_code})") }
            });
        }
        Ok(())
    }
}

#[cfg(not(windows))]
fn kill_one(_pid: u32) -> Result<(), KillErr> {
    Err(KillErr { denied: false, reason: "not supported on this platform".into() })
}

/// Terminate the given PIDs with per-PID outcomes.
pub fn kill(pids: &[u32]) -> Vec<KillOutcome> {
    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let mut out = Vec::with_capacity(pids.len());
    for &pid in pids {
        let spid = sysinfo::Pid::from_u32(pid);
        let name = sys
            .process(spid)
            .map(|p| p.name().to_string_lossy().to_string())
            .unwrap_or_default();
        let package_full_name = package_full_name_of(pid);
        if sys.process(spid).is_none() {
            out.push(KillOutcome {
                pid,
                name,
                package_full_name,
                killed: false,
                error: Some("进程已不存在".into()),
                protected: true,
            });
            continue;
        }
        match kill_one(pid) {
            Ok(()) => out.push(KillOutcome {
                pid,
                name,
                package_full_name,
                killed: true,
                error: None,
                protected: false,
            }),
            Err(e) => {
                warn!(pid, reason = %e.reason, "kill failed");
                out.push(KillOutcome {
                    pid,
                    name,
                    package_full_name,
                    killed: false,
                    error: Some(e.reason),
                    protected: e.denied,
                })
            }
        }
    }
    out
}

/// Activate the packaged app through the shell (same as the Start menu).
#[cfg(windows)]
pub fn launch_aumid(aumid: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let target = format!("shell:AppsFolder\\{aumid}");
    let file: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            std::ptr::null(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if result > 32 {
        return Ok(());
    }
    // Fallback: hand the URI to the running shell.
    std::process::Command::new("explorer.exe")
        .arg(&target)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("ShellExecuteW returned {result}; explorer fallback failed: {e}"))
}

#[cfg(not(windows))]
pub fn launch_aumid(_aumid: &str) -> Result<(), String> {
    Err("not supported on this platform".into())
}

/// `HKLM|HKCU\SOFTWARE\Policies\Claude\disableAutoUpdates` — the official
/// managed setting that turns Claude's updater off. `None` = not set.
pub fn policy_disable_auto_updates() -> Option<bool> {
    for hive in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        let Ok(key) = RegKey::predef(hive).open_subkey_with_flags(r"SOFTWARE\Policies\Claude", KEY_READ)
        else {
            continue;
        };
        if let Ok(v) = key.get_value::<u32, _>("disableAutoUpdates") {
            return Some(v != 0);
        }
        if let Ok(v) = key.get_value::<String, _>("disableAutoUpdates") {
            let s = v.trim().to_lowercase();
            return Some(s == "true" || s == "1");
        }
    }
    None
}
