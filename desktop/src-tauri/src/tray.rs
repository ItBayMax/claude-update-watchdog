use std::sync::Mutex as StdMutex;

use anyhow::Result;
use once_cell::sync::OnceCell;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconEvent},
    AppHandle, Emitter, Manager,
};
use tracing::warn;

use crate::model::{Health, RepairTrigger};
use crate::scheduler::SchedulerState;
use crate::state::AppState;

const ICON_IDLE: &[u8] = include_bytes!("../icons/tray-idle.png");
const ICON_OK: &[u8] = include_bytes!("../icons/tray-ok.png");
const ICON_ATTN: &[u8] = include_bytes!("../icons/tray-attn.png");
const ICON_WARN: &[u8] = include_bytes!("../icons/tray-warn.png");

static PAUSE_RESUME_ITEM: OnceCell<StdMutex<MenuItem<tauri::Wry>>> = OnceCell::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayVisual {
    Idle,
    Ok,
    Attention,
    Warn,
}

pub fn install(app: &AppHandle) -> Result<()> {
    let handle = app.clone();
    let show = MenuItem::with_id(&handle, "show", "显示主窗口", true, None::<&str>)?;
    let check = MenuItem::with_id(&handle, "check", "立即检查", true, None::<&str>)?;
    let repair = MenuItem::with_id(&handle, "repair", "立即修复（结束残留进程并重启 Claude）", true, None::<&str>)?;
    let pause_resume = MenuItem::with_id(&handle, "pause_resume", "暂停监视", true, None::<&str>)?;
    let separator = MenuItem::with_id(&handle, "sep", "──────────", false, None::<&str>)?;
    let quit = MenuItem::with_id(&handle, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(&handle, &[&show, &check, &repair, &pause_resume, &separator, &quit])?;
    let _ = PAUSE_RESUME_ITEM.set(StdMutex::new(pause_resume));

    if let Some(tray) = app.tray_by_id("main-tray") {
        tray.set_menu(Some(menu))?;
        tray.on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "check" => {
                let state = app.state::<AppState>().inner().clone();
                let app2 = app.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let snapshot = crate::status::collect(&state);
                    *state.status.lock() = Some(snapshot.clone());
                    let _ = app2.emit("claudewatchdog://status", &snapshot);
                    refresh_state(&app2, &state);
                });
            }
            "repair" => {
                let state = app.state::<AppState>().inner().clone();
                let app2 = app.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let _ = crate::repair::run_blocking(Some(&app2), &state, RepairTrigger::Manual, false, None);
                });
            }
            "pause_resume" => {
                let state = app.state::<AppState>().inner().clone();
                match crate::scheduler::current_state(&state) {
                    SchedulerState::Running => crate::scheduler::pause(&state),
                    SchedulerState::Paused => crate::scheduler::resume(app.clone(), state.clone()),
                    SchedulerState::Disabled => {}
                }
                refresh_state(app, &state);
                let _ = app.emit("claudewatchdog://scheduler-changed", ());
            }
            "quit" => {
                let _ = app.emit("claudewatchdog://request-exit", ());
                show_main_window(app);
            }
            _ => {}
        });
        tray.on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    }
    Ok(())
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// Recompute icon + tooltip + menu labels from the current state.
pub fn refresh_state(app: &AppHandle, state: &AppState) {
    let sched = crate::scheduler::current_state(state);
    let status = state.status.lock().clone();
    let repairing = state.repairing.load(std::sync::atomic::Ordering::SeqCst);

    let visual = if repairing {
        TrayVisual::Warn
    } else {
        match sched {
            SchedulerState::Disabled | SchedulerState::Paused => TrayVisual::Idle,
            SchedulerState::Running => match status.as_ref().map(|s| s.health) {
                Some(Health::Healthy) => TrayVisual::Ok,
                Some(Health::Warning) => TrayVisual::Attention,
                Some(Health::Failure) | Some(Health::Repairing) => TrayVisual::Warn,
                _ => TrayVisual::Idle,
            },
        }
    };

    if let Some(tray) = app.tray_by_id("main-tray") {
        let bytes: &[u8] = match visual {
            TrayVisual::Idle => ICON_IDLE,
            TrayVisual::Ok => ICON_OK,
            TrayVisual::Attention => ICON_ATTN,
            TrayVisual::Warn => ICON_WARN,
        };
        match Image::from_bytes(bytes) {
            Ok(img) => {
                if let Err(e) = tray.set_icon(Some(img)) {
                    warn!("tray set_icon failed: {e}");
                }
            }
            Err(e) => warn!("decoding tray icon failed: {e}"),
        }
        let _ = tray.set_tooltip(Some(tooltip_for(sched, repairing, status.as_ref()).as_str()));
    }

    if let Some(item) = PAUSE_RESUME_ITEM.get() {
        if let Ok(mi) = item.lock() {
            let label = match sched {
                SchedulerState::Running => "暂停监视",
                SchedulerState::Paused => "恢复监视",
                SchedulerState::Disabled => "监视已在设置中关闭",
            };
            let _ = mi.set_text(label);
            let _ = mi.set_enabled(!matches!(sched, SchedulerState::Disabled));
        }
    }
}

fn tooltip_for(
    sched: SchedulerState,
    repairing: bool,
    status: Option<&crate::model::WatchStatus>,
) -> String {
    let head = if repairing {
        "Claude Watchdog · 修复中"
    } else {
        match sched {
            SchedulerState::Disabled => "Claude Watchdog · 监视已关闭",
            SchedulerState::Paused => "Claude Watchdog · 已暂停",
            SchedulerState::Running => "Claude Watchdog · 监视中",
        }
    };
    let tail = match status {
        None => "尚未检查".to_string(),
        Some(s) => {
            let ver = s.current_version.clone().unwrap_or_else(|| "未注册".into());
            let mut t = format!("Claude {ver} · {} 个进程", s.processes.len());
            if s.orphan_count > 0 {
                t.push_str(&format!(" · {} 个旧版本残留", s.orphan_count));
            }
            if matches!(s.health, Health::Failure) {
                t.push_str(" · 检测到启动失败");
            }
            t
        }
    };
    format!("{head}\n{tail}")
}
