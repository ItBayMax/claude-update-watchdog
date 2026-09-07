//! Claude Watchdog — Tauri 2 desktop app that watches for failed Claude
//! Desktop (MSIX) self-updates and repairs them.
//!
//! Module map (mirrors the ip-killswitch layout):
//!
//! - `config`    JSON config persisted in the app config dir
//! - `state`     `AppState` shared by commands, scheduler, watcher, tray
//! - `logger`    tracing → daily rolling file
//! - `model`     serde structs shared with the React frontend (see src/types.ts)
//! - `packages`  registered Claude packages (registry), processes with Claude
//!               package identity (GetPackageFullName), kill, relaunch
//! - `eventlog`  Windows Event Log: pull queries + push subscription (event 208)
//! - `status`    one full snapshot = `WatchStatus`
//! - `repair`    the repair pipeline (stop leftovers → relaunch) + history
//! - `stale`     orphaned package folders under WindowsApps (scan / remove)
//! - `task`      the Task Scheduler fallback (runs this exe --repair-from-event)
//! - `scheduler` periodic status refresh (poll fallback for the subscription)
//! - `watcher`   event-driven reaction thread (analog of process_watcher.rs)
//! - `tray`      tray icon + menu, three visual states
//! - `admin`     elevation check / UAC relaunch
//! - `cli`       headless modes for the scheduled task and scripting
//! - `commands`  Tauri invoke handlers

mod admin;
mod cli;
mod commands;
mod config;
mod eventlog;
mod logger;
mod model;
mod packages;
mod repair;
mod scheduler;
mod stale;
mod state;
mod status;
mod task;
mod tray;
mod watcher;

use std::path::PathBuf;

use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tracing::{info, warn};

use crate::state::AppState;

/// Must match `identifier` in tauri.conf.json — used to compute the same
/// config / log directories in the headless (no Tauri app) modes.
pub const IDENTIFIER: &str = "io.github.itbaymax.claudewatchdog";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = cli::maybe_run_headless(&args) {
        std::process::exit(code);
    }

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            crate::tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        // Self-update from GitHub Releases (latest.json + signed NSIS installer).
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ));

    builder
        .setup(|app| {
            let handle = app.handle().clone();
            let app_dir: PathBuf = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| config::default_dirs().0);
            let log_dir: PathBuf = app
                .path()
                .app_log_dir()
                .unwrap_or_else(|_| app_dir.join("logs"));

            let cfg = config::load(&app_dir).unwrap_or_default();
            match logger::init(&log_dir, &cfg.log_level) {
                // Leak the guard on purpose: dropping it would stop the
                // background writer and silently swallow every later log line.
                Ok(guard) => std::mem::forget(guard),
                Err(e) => eprintln!("failed to init logger: {e}"),
            }
            info!(?app_dir, ?log_dir, "starting claude-watchdog");

            let state = AppState::new(app_dir.clone(), log_dir.clone(), cfg.clone());
            state.load_history();
            app.manage(state.clone());

            crate::tray::install(&handle).unwrap_or_else(|e| warn!("tray install failed: {e}"));

            // Force the window icon early so the taskbar shows it on first launch.
            const WINDOW_ICON_BYTES: &[u8] = include_bytes!("../icons/icon.png");
            if let Some(win) = handle.get_webview_window("main") {
                if let Ok(img) = tauri::image::Image::from_bytes(WINDOW_ICON_BYTES) {
                    let _ = win.set_icon(img);
                }
            }

            if cfg.autostart {
                let _ = handle.autolaunch().enable();
            }

            // Periodic status refresh (also the polling fallback if the
            // event-log subscription cannot be established).
            crate::scheduler::restart(handle.clone(), state.clone());

            // Push-based reaction to launch failures.
            crate::watcher::spawn(handle.clone(), state.clone());

            // First snapshot without waiting for the first scheduler tick.
            {
                let app2 = handle.clone();
                let state2 = state.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let snapshot = crate::status::collect(&state2);
                    *state2.status.lock() = Some(snapshot.clone());
                    let _ = app2.emit("claudewatchdog://status", &snapshot);
                    crate::tray::refresh_state(&app2, &state2);
                });
            }

            let start_hidden = std::env::args().any(|a| a == "--minimized");
            if start_hidden {
                if let Some(win) = handle.get_webview_window("main") {
                    let _ = win.hide();
                }
            }
            Ok(())
        })
        .on_window_event(on_window_event)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::get_status,
            commands::last_status,
            commands::list_processes,
            commands::kill_processes,
            commands::repair_now,
            commands::launch_claude,
            commands::get_history,
            commands::clear_history,
            commands::recent_events,
            commands::scan_stale,
            commands::remove_stale,
            commands::task_status,
            commands::install_task,
            commands::uninstall_task,
            commands::run_task_now,
            commands::read_logs,
            commands::open_log_dir,
            commands::open_path,
            commands::autostart_status,
            commands::set_autostart,
            commands::quit_app,
            commands::show_main_window,
            commands::scheduler_status,
            commands::pause_scheduler,
            commands::resume_scheduler,
            commands::is_elevated,
            commands::relaunch_as_admin,
            commands::get_platform,
            commands::app_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        if window.label() != "main" {
            return;
        }
        let app = window.app_handle();
        let state = app.state::<AppState>();
        let cfg = state.config.lock().clone();
        if cfg.close_to_tray {
            api.prevent_close();
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.hide();
            }
        } else if cfg.confirm_exit {
            api.prevent_close();
            let _ = app.emit("claudewatchdog://request-exit", ());
        }
    }
}
