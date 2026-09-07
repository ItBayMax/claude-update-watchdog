//! Headless modes. None of these create a Tauri app or a window.
//!
//!   claude-watchdog.exe --check [--out <file>]           print the current WatchStatus as JSON
//!   claude-watchdog.exe --repair [--dry-run] [--out <file>]  manual repair (or a plan) as JSON
//!   claude-watchdog.exe --repair-from-event [--out <file>]   what the scheduled task runs: act only
//!                                                        if a recent 208/0x80070020 exists
//!   claude-watchdog.exe --minimized                      (GUI) start hidden in the tray
//!
//! `--out <file>` also writes the JSON to a file. Release builds use the GUI
//! subsystem, so shells do not wait for them and may not capture stdout;
//! scripts should read the file (or use `Start-Process -Wait -RedirectStandardOutput`).
//!
//! Exit codes: 0 ok / nothing to do, 1 repair failed, 2 rate limited.

use std::io::Write;

use crate::model::RepairTrigger;
use crate::state::AppState;

pub fn maybe_run_headless(args: &[String]) -> Option<i32> {
    let has = |flag: &str| args.iter().any(|a| a == flag);
    if !(has("--check") || has("--repair") || has("--repair-from-event") || has("--help") || has("-h")) {
        return None;
    }

    attach_parent_console();

    let out_path: Option<String> = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1))
        .cloned();
    let emit = |json: &str| {
        print_line(json);
        if let Some(p) = &out_path {
            if let Err(e) = std::fs::write(p, json) {
                tracing::warn!("could not write {p}: {e}");
            }
        }
    };

    if has("--help") || has("-h") {
        print_line(
            "claude-watchdog [--check | --repair [--dry-run] | --repair-from-event] [--out <file>] | --minimized",
        );
        return Some(0);
    }

    let (app_dir, log_dir) = crate::config::default_dirs();
    let cfg = crate::config::load(&app_dir).unwrap_or_default();
    if let Ok(guard) = crate::logger::init(&log_dir, &cfg.log_level) {
        std::mem::forget(guard);
    }
    let state = AppState::new(app_dir, log_dir, cfg.clone());
    state.load_history();

    if has("--check") {
        let snapshot = crate::status::collect(&state);
        emit(&serde_json::to_string_pretty(&snapshot).unwrap_or_default());
        return Some(0);
    }

    if has("--repair-from-event") {
        let aumid = cfg.aumid();
        let recent =
            crate::eventlog::recent_launch_failures(&aumid, cfg.recent_window_minutes, 1)
                .into_iter()
                .next();
        let Some(failure) = recent else {
            tracing::info!(
                "repair-from-event: no Claude launch failure (event 208) in the last {} min; nothing to do",
                cfg.recent_window_minutes
            );
            return Some(0);
        };
        if failure.error_code != crate::eventlog::ERROR_SHARING_VIOLATION_HRESULT {
            tracing::warn!(
                "repair-from-event: latest failure at {} has error {}, not 0x80070020; nothing to do",
                failure.time,
                failure.error_hex
            );
            return Some(0);
        }
        let record = crate::repair::run_blocking(
            None,
            &state,
            RepairTrigger::Task,
            false,
            Some(failure),
        );
        emit(&serde_json::to_string_pretty(&record).unwrap_or_default());
        return Some(exit_code_for(&record));
    }

    // --repair [--dry-run]
    let dry_run = has("--dry-run");
    let record = crate::repair::run_blocking(None, &state, RepairTrigger::Cli, dry_run, None);
    emit(&serde_json::to_string_pretty(&record).unwrap_or_default());
    Some(exit_code_for(&record))
}

fn exit_code_for(record: &crate::model::RepairRecord) -> i32 {
    if record.dry_run || record.success {
        0
    } else if record.message.contains("rate limit") {
        2
    } else {
        1
    }
}

/// Release builds use the GUI subsystem, so stdout is not connected to the
/// console that started us. Attach to the parent console so `--check`
/// output is visible in the terminal that invoked it.
#[cfg(windows)]
fn attach_parent_console() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
fn attach_parent_console() {}

fn print_line(s: &str) {
    // stdout first: it is the pipe when we are scripted, and the attached
    // console after AttachConsole. Fall back to CONOUT$ only if that fails.
    {
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        if writeln!(lock, "{s}").and_then(|_| lock.flush()).is_ok() {
            return;
        }
    }
    #[cfg(windows)]
    if let Ok(mut con) = std::fs::OpenOptions::new().write(true).open("CONOUT$") {
        let _ = writeln!(con, "{s}");
    }
}
