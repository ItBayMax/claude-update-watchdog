//! Task Scheduler fallback.
//!
//! The GUI repairs failures only while it runs. The scheduled task covers the
//! rest: on AppModel-Runtime event 208 for Claude it runs
//! `claude-watchdog.exe --repair-from-event` (headless), 3 seconds later.
//!
//! Everything goes through PowerShell's ScheduledTasks module, which is
//! locale-independent (schtasks.exe prints localized text).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use serde::Deserialize;
use tracing::{info, warn};

use crate::model::TaskStatus;

pub const TASK_NAME: &str = "Claude Update Watchdog";
const CACHE_TTL: Duration = Duration::from_secs(60);

static CACHE: Lazy<Mutex<Option<(Instant, TaskStatus)>>> = Lazy::new(|| Mutex::new(None));

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Run a PowerShell snippet hidden and return its stdout (UTF-8).
pub fn run_powershell(script: &str) -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .output()
            .map_err(|e| format!("powershell: {e}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if stderr.is_empty() { format!("powershell exited with {}", output.status) } else { stderr });
        }
        Ok(stdout)
    }
    #[cfg(not(windows))]
    {
        let _ = script;
        Err("PowerShell is only available on Windows".into())
    }
}

#[derive(Deserialize)]
struct RawStatus {
    installed: bool,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    last_run_time: Option<String>,
    #[serde(default)]
    last_result: Option<u32>,
}

fn status_script() -> String {
    format!(
        "[Console]::OutputEncoding=[Text.Encoding]::UTF8; \
         $n={n}; \
         $t=Get-ScheduledTask -TaskName $n -ErrorAction SilentlyContinue; \
         if(-not $t){{ [pscustomobject]@{{ installed=$false }} | ConvertTo-Json -Compress; exit 0 }}; \
         $i=Get-ScheduledTaskInfo -TaskName $n -ErrorAction SilentlyContinue; \
         $a=$t.Actions | Select-Object -First 1; \
         $lr=$null; if($i -and $i.LastRunTime -and $i.LastRunTime.Year -gt 2000){{ $lr=$i.LastRunTime.ToString('yyyy-MM-dd HH:mm:ss') }}; \
         $res=$null; if($i){{ $res=[uint32]$i.LastTaskResult }}; \
         [pscustomobject]@{{ installed=$true; state=[string]$t.State; action=(($a.Execute + ' ' + $a.Arguments).Trim()); last_run_time=$lr; last_result=$res }} | ConvertTo-Json -Compress",
        n = ps_quote(TASK_NAME)
    )
}

/// Fresh query (≈0.5 s: spawns PowerShell). Updates the cache.
pub fn status() -> TaskStatus {
    let result = match run_powershell(&status_script()) {
        Ok(out) => match serde_json::from_str::<RawStatus>(out.trim()) {
            Ok(raw) => TaskStatus {
                installed: raw.installed,
                state: raw.state,
                action: raw.action,
                last_run_time: raw.last_run_time,
                last_result: raw.last_result,
                error: None,
            },
            Err(e) => TaskStatus { error: Some(format!("parse: {e}")), ..Default::default() },
        },
        Err(e) => TaskStatus { error: Some(e), ..Default::default() },
    };
    if let Ok(mut c) = CACHE.lock() {
        *c = Some((Instant::now(), result.clone()));
    }
    result
}

/// Cached status, refreshed at most once per minute (used by the poll).
pub fn cached_status() -> TaskStatus {
    if let Ok(c) = CACHE.lock() {
        if let Some((at, s)) = c.as_ref() {
            if at.elapsed() < CACHE_TTL {
                return s.clone();
            }
        }
    }
    status()
}

pub fn install(aumid: &str) -> Result<TaskStatus, String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let exe_s = exe.to_string_lossy().to_string();
    let script = format!(
        "[Console]::OutputEncoding=[Text.Encoding]::UTF8; \
         $n={n}; $exe={exe}; $aumid={aumid}; \
         $xpath = '*[System[Provider[@Name=''Microsoft-Windows-AppModel-Runtime''] and EventID=208]] and *[EventData[Data[@Name=''ApplicationName'']=''' + $aumid + ''']]'; \
         $sub = '<QueryList><Query Id=''0'' Path=''Microsoft-Windows-AppModel-Runtime/Admin''><Select Path=''Microsoft-Windows-AppModel-Runtime/Admin''>' + $xpath + '</Select></Query></QueryList>'; \
         $c = Get-CimClass -ClassName MSFT_TaskEventTrigger -Namespace Root/Microsoft/Windows/TaskScheduler; \
         $tr = New-CimInstance -CimClass $c -ClientOnly; $tr.Subscription = $sub; $tr.Enabled = $true; $tr.Delay = 'PT3S'; \
         $act = New-ScheduledTaskAction -Execute $exe -Argument '--repair-from-event'; \
         $set = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Minutes 5); \
         $pr = New-ScheduledTaskPrincipal -UserId ([Security.Principal.WindowsIdentity]::GetCurrent().Name) -LogonType Interactive -RunLevel Limited; \
         Register-ScheduledTask -TaskName $n -TaskPath '\\' -Action $act -Trigger $tr -Settings $set -Principal $pr -Description 'Claude Watchdog fallback: on AppModel-Runtime event 208 for Claude, run claude-watchdog --repair-from-event' -Force | Out-Null; \
         'ok'",
        n = ps_quote(TASK_NAME),
        exe = ps_quote(&exe_s),
        aumid = ps_quote(aumid),
    );
    let out = run_powershell(&script)?;
    if !out.contains("ok") {
        warn!("install task: unexpected output {out:?}");
    }
    info!(exe = %exe_s, "scheduled task installed");
    Ok(status())
}

pub fn uninstall() -> Result<TaskStatus, String> {
    let script = format!(
        "Unregister-ScheduledTask -TaskName {n} -Confirm:$false -ErrorAction Stop; 'ok'",
        n = ps_quote(TASK_NAME)
    );
    run_powershell(&script)?;
    info!("scheduled task removed");
    Ok(status())
}

pub fn run_now() -> Result<(), String> {
    let script = format!(
        "Start-ScheduledTask -TaskName {n} -ErrorAction Stop; 'ok'",
        n = ps_quote(TASK_NAME)
    );
    run_powershell(&script).map(|_| ())
}
