//! Windows Event Log access through the Evt* API (windows-sys).
//!
//! Two access patterns, both feeding the same XML parser:
//! - pull (`query`): status snapshots, stale-package scan.
//! - push (`subscribe_launch_failures`): `EvtSubscribe` with a callback that
//!   forwards the rendered event XML over an mpsc channel to the watcher
//!   thread (analog of the WMI subscription in ip-killswitch).
//!
//! Events of interest, all from provider Microsoft-Windows-AppModel-Runtime:
//!   208  LaunchProcess failed          (ErrorCode 2147942432 = 0x80070020)
//!   215  container creation failed     ("conversion job" error)
//!   210 / 211 / 217  container created / process added / container destroyed
//!   201  process created

use std::ffi::c_void;
use std::sync::mpsc::Sender;

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use once_cell::sync::{Lazy, OnceCell};
use regex::Regex;
use tracing::{debug, info};

use crate::model::{ContainerEvent, LaunchFailure};

pub const CHANNEL_APPMODEL: &str = "Microsoft-Windows-AppModel-Runtime/Admin";
pub const CHANNEL_DEPLOY: &str = "Microsoft-Windows-AppXDeploymentServer/Operational";
pub const PROVIDER_APPMODEL: &str = "Microsoft-Windows-AppModel-Runtime";
/// HRESULT_FROM_WIN32(ERROR_SHARING_VIOLATION): "another program is using this file".
pub const ERROR_SHARING_VIOLATION_HRESULT: u32 = 0x8007_0020;

static RE_EVENT_ID: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"<EventID[^>]*>(\d+)</EventID>").expect("regex"));
static RE_RECORD_ID: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"<EventRecordID>(\d+)</EventRecordID>").expect("regex"));
static RE_TIME: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"SystemTime=['"]([^'"]+)['"]"#).expect("regex"));
static RE_LEVEL: Lazy<Regex> = Lazy::new(|| Regex::new(r"<Level>(\d+)</Level>").expect("regex"));
static RE_PACKAGE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[A-Za-z][A-Za-z0-9.]*_[0-9]+(?:\.[0-9]+){3}_[a-z0-9]+__[a-z0-9]{13}").expect("regex")
});

fn cap1(re: &Regex, s: &str) -> Option<String> {
    re.captures(s)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

fn data_field(xml: &str, name: &str) -> Option<String> {
    let re = Regex::new(&format!(
        r#"<Data Name=['"]{}['"]>([^<]*)</Data>"#,
        regex::escape(name)
    ))
    .ok()?;
    cap1(&re, xml)
}

fn parse_time(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

/// XPath selecting event 208 for one AUMID, optionally limited to the last N minutes.
pub fn failure_xpath(aumid: &str, within_minutes: Option<u64>) -> String {
    let time = within_minutes
        .map(|m| format!(" and TimeCreated[timediff(@SystemTime) <= {}]", m * 60_000))
        .unwrap_or_default();
    format!(
        "*[System[Provider[@Name='{PROVIDER_APPMODEL}'] and EventID=208{time}]] \
         and *[EventData[Data[@Name='ApplicationName']='{aumid}']]"
    )
}

pub fn parse_failure(xml: &str) -> Option<LaunchFailure> {
    let event_id = cap1(&RE_EVENT_ID, xml)?.parse::<u32>().ok()?;
    let record_id = cap1(&RE_RECORD_ID, xml)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let time = cap1(&RE_TIME, xml)
        .map(|s| parse_time(&s))
        .unwrap_or_else(Utc::now);
    let package_full_name = data_field(xml, "PackageName")
        .or_else(|| cap1(&RE_PACKAGE, xml))
        .unwrap_or_default();
    let application = data_field(xml, "ApplicationName").unwrap_or_default();
    let error_code = data_field(xml, "ErrorCode")
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);
    Some(LaunchFailure {
        record_id,
        time,
        event_id,
        package_full_name,
        application,
        error_code,
        error_hex: format!("0x{error_code:08X}"),
    })
}

pub fn parse_container_event(xml: &str) -> Option<ContainerEvent> {
    let event_id = cap1(&RE_EVENT_ID, xml)?.parse::<u32>().ok()?;
    let record_id = cap1(&RE_RECORD_ID, xml)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let time = cap1(&RE_TIME, xml)
        .map(|s| parse_time(&s))
        .unwrap_or_else(Utc::now);
    let level = match cap1(&RE_LEVEL, xml).as_deref() {
        Some("1") | Some("2") => "error",
        Some("3") => "warning",
        _ => "info",
    }
    .to_string();
    let package_full_name = data_field(xml, "PackageName").or_else(|| cap1(&RE_PACKAGE, xml));
    let error_code = data_field(xml, "ErrorCode").and_then(|s| s.trim().parse::<u32>().ok());
    let error_hex = error_code.filter(|c| *c != 0).map(|c| format!("0x{c:08X}"));
    let summary = match event_id {
        201 => "已创建进程".to_string(),
        208 => "启动进程失败 [LaunchProcess]".to_string(),
        210 => "创建桌面 AppX 容器".to_string(),
        211 => "进程加入容器".to_string(),
        215 => "创建桌面 AppX 容器失败（转换作业出错）".to_string(),
        217 => "销毁桌面 AppX 容器".to_string(),
        other => format!("事件 {other}"),
    };
    Some(ContainerEvent {
        record_id,
        time,
        event_id,
        level,
        package_full_name,
        summary,
        error_hex,
    })
}

// ------------------------------------------------------------- Win32 layer --

#[cfg(windows)]
mod win {
    use super::*;
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS};
    use windows_sys::Win32::System::EventLog::{
        EvtClose, EvtNext, EvtQuery, EvtQueryChannelPath, EvtQueryReverseDirection, EvtRender,
        EvtRenderEventXml, EvtSubscribe, EvtSubscribeActionDeliver, EvtSubscribeToFutureEvents,
        EVT_HANDLE, EVT_SUBSCRIBE_NOTIFY_ACTION,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub unsafe fn render_xml(event: EVT_HANDLE) -> Option<String> {
        let mut used: u32 = 0;
        let mut props: u32 = 0;
        let ok = EvtRender(
            0,
            event,
            EvtRenderEventXml as u32,
            0,
            std::ptr::null_mut(),
            &mut used,
            &mut props,
        );
        if ok == 0 && GetLastError() != ERROR_INSUFFICIENT_BUFFER {
            return None;
        }
        let len = (used as usize / 2) + 1;
        let mut buf: Vec<u16> = vec![0; len];
        let ok = EvtRender(
            0,
            event,
            EvtRenderEventXml as u32,
            (buf.len() * 2) as u32,
            buf.as_mut_ptr() as *mut c_void,
            &mut used,
            &mut props,
        );
        if ok == 0 {
            return None;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..end]))
    }

    pub fn query(channel: &str, xpath: &str, max: usize, newest_first: bool) -> Result<Vec<String>> {
        if max == 0 {
            return Ok(Vec::new());
        }
        let channel_w = wide(channel);
        let xpath_w = wide(xpath);
        let mut flags = EvtQueryChannelPath as u32;
        if newest_first {
            flags |= EvtQueryReverseDirection as u32;
        }
        let mut out: Vec<String> = Vec::new();
        unsafe {
            let result = EvtQuery(0, channel_w.as_ptr(), xpath_w.as_ptr(), flags);
            if result == 0 {
                return Err(anyhow!("EvtQuery failed (Win32 error {})", GetLastError()));
            }
            let mut batch: [EVT_HANDLE; 32] = [0; 32];
            loop {
                let want = (max - out.len()).min(batch.len()) as u32;
                if want == 0 {
                    break;
                }
                let mut returned: u32 = 0;
                let ok = EvtNext(result, want, batch.as_mut_ptr(), 5_000, 0, &mut returned);
                if ok == 0 {
                    let err = GetLastError();
                    if err != ERROR_NO_MORE_ITEMS {
                        debug!("EvtNext ended with Win32 error {err}");
                    }
                    break;
                }
                for &h in batch.iter().take(returned as usize) {
                    if let Some(xml) = render_xml(h) {
                        out.push(xml);
                    }
                    EvtClose(h);
                }
                if returned == 0 {
                    break;
                }
            }
            EvtClose(result);
        }
        Ok(out)
    }

    static SUBSCRIPTION: OnceCell<isize> = OnceCell::new();

    unsafe extern "system" fn on_event(
        action: EVT_SUBSCRIBE_NOTIFY_ACTION,
        ctx: *const c_void,
        event: EVT_HANDLE,
    ) -> u32 {
        if action == EvtSubscribeActionDeliver && !ctx.is_null() && event != 0 {
            let tx = &*(ctx as *const Sender<String>);
            if let Some(xml) = render_xml(event) {
                let _ = tx.send(xml);
            }
        }
        0
    }

    pub fn subscribe(channel: &str, xpath: &str, tx: Sender<String>) -> Result<()> {
        let channel_w = wide(channel);
        let xpath_w = wide(xpath);
        // The sender lives as long as the subscription (i.e. the process).
        let ctx = Box::into_raw(Box::new(tx)) as *const c_void;
        let handle = unsafe {
            EvtSubscribe(
                0,
                std::ptr::null_mut(),
                channel_w.as_ptr(),
                xpath_w.as_ptr(),
                0,
                ctx,
                Some(on_event),
                EvtSubscribeToFutureEvents as u32,
            )
        };
        if handle == 0 {
            let err = unsafe { GetLastError() };
            unsafe {
                drop(Box::from_raw(ctx as *mut Sender<String>));
            }
            return Err(anyhow!("EvtSubscribe failed (Win32 error {err})"));
        }
        // Keep the strings alive for the lifetime of the subscription too.
        std::mem::forget(channel_w);
        std::mem::forget(xpath_w);
        let _ = SUBSCRIPTION.set(handle);
        Ok(())
    }
}

#[cfg(windows)]
pub fn query(channel: &str, xpath: &str, max: usize, newest_first: bool) -> Result<Vec<String>> {
    win::query(channel, xpath, max, newest_first)
}

#[cfg(not(windows))]
pub fn query(_channel: &str, _xpath: &str, _max: usize, _newest_first: bool) -> Result<Vec<String>> {
    Ok(Vec::new())
}

/// Launch failures (event 208) for the AUMID within the last `minutes`, newest first.
pub fn recent_launch_failures(aumid: &str, minutes: u64, max: usize) -> Vec<LaunchFailure> {
    let xpath = failure_xpath(aumid, Some(minutes.max(1)));
    match query(CHANNEL_APPMODEL, &xpath, max, true) {
        Ok(list) => list.iter().filter_map(|x| parse_failure(x)).collect(),
        Err(e) => {
            debug!("recent_launch_failures: {e}");
            Vec::new()
        }
    }
}

/// The most recent launch failure for the AUMID, regardless of age.
pub fn last_launch_failure(aumid: &str) -> Option<LaunchFailure> {
    let xpath = failure_xpath(aumid, None);
    query(CHANNEL_APPMODEL, &xpath, 1, true)
        .ok()
        .and_then(|l| l.first().and_then(|x| parse_failure(x)))
}

/// Container lifecycle timeline for the package family, newest first.
pub fn container_events(family: &str, max: usize) -> Vec<ContainerEvent> {
    let hash = crate::packages::family_parts(family).1;
    let xpath = format!(
        "*[System[Provider[@Name='{PROVIDER_APPMODEL}'] and \
         (EventID=201 or EventID=208 or EventID=210 or EventID=211 or EventID=215 or EventID=217)]]"
    );
    let raw = match query(CHANNEL_APPMODEL, &xpath, (max * 10).clamp(50, 600), true) {
        Ok(l) => l,
        Err(e) => {
            debug!("container_events: {e}");
            return Vec::new();
        }
    };
    raw.iter()
        .filter(|xml| hash.is_empty() || xml.contains(&hash))
        .filter_map(|xml| parse_container_event(xml))
        .take(max)
        .collect()
}

/// Folder names the deployment engine flagged in warning 1230 ("hard links
/// without a package in the repository") — its own list of orphans.
pub fn stale_folder_names_from_warnings(family: &str) -> Vec<String> {
    let (name, _) = crate::packages::family_parts(family);
    let re = match Regex::new(&format!(r"\\WindowsApps\\({}_[^\\;<]+?)\\", regex::escape(&name))) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let raw = query(CHANNEL_DEPLOY, "*[System[EventID=1230]]", 40, true).unwrap_or_default();
    let mut out: Vec<String> = Vec::new();
    for xml in raw {
        for cap in re.captures_iter(&xml) {
            if let Some(m) = cap.get(1) {
                let s = m.as_str().to_string();
                if !out.iter().any(|x| x.eq_ignore_ascii_case(&s)) {
                    out.push(s);
                }
            }
        }
    }
    out.sort();
    out
}

/// Folders event 472 moved into WindowsApps\Deleted (Windows normally
/// removes them on its own at the next boot).
pub fn moved_to_deleted_names(family: &str) -> Vec<String> {
    let (name, _) = crate::packages::family_parts(family);
    let re = match Regex::new(&format!(
        r"WindowsApps\\Deleted\\({}_[^\\<\s\u{{3002}}]+)",
        regex::escape(&name)
    )) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let raw = query(CHANNEL_DEPLOY, "*[System[EventID=472]]", 200, true).unwrap_or_default();
    let mut out: Vec<String> = Vec::new();
    for xml in raw {
        for cap in re.captures_iter(&xml) {
            if let Some(m) = cap.get(1) {
                let s = m.as_str().to_string();
                if !out.iter().any(|x| x.eq_ignore_ascii_case(&s)) {
                    out.push(s);
                }
            }
        }
    }
    out
}

/// Push subscription for future launch failures of the AUMID. Rendered
/// event XML is sent through `tx`; parse it with `parse_failure`.
#[cfg(windows)]
pub fn subscribe_launch_failures(aumid: &str, tx: Sender<String>) -> Result<()> {
    let xpath = failure_xpath(aumid, None);
    win::subscribe(CHANNEL_APPMODEL, &xpath, tx)?;
    info!(channel = CHANNEL_APPMODEL, aumid, "subscribed to launch-failure events");
    Ok(())
}

#[cfg(not(windows))]
pub fn subscribe_launch_failures(_aumid: &str, _tx: Sender<String>) -> Result<()> {
    Err(anyhow!("event log subscription is only available on Windows"))
}
