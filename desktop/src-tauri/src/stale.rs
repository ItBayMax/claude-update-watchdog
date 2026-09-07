//! Orphaned package folders under C:\Program Files\WindowsApps.
//!
//! Updaters before ~1.300xx left old `Claude_<ver>_x64__<hash>` folders
//! behind when a leftover process blocked the rename into WindowsApps\Deleted.
//! The deployment engine has forgotten them (warning 1230), so the only way
//! to remove them is: take ownership (Administrators) → grant full control →
//! delete. Elevation is required for that step; scanning is not.
//!
//! Safety rules (same as Remove-StaleClaudePackages.ps1):
//! - only folder names matching the family pattern are considered;
//! - registered (any user, when elevated) / current / in-use versions are protected;
//! - the folder's AppxManifest.xml must identify the family before deletion;
//! - ACL changes touch the stale folder only, never the WindowsApps root.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::Utc;
use regex::Regex;
use tracing::{info, warn};

use crate::model::{RemoveOutcome, StalePackage, StaleScan};
use crate::state::AppState;

const ADMIN_SID: &str = "*S-1-5-32-544";

fn name_regex(family: &str) -> Regex {
    let (name, hash) = crate::packages::family_parts(family);
    Regex::new(&format!(
        r"^{}_[0-9]+(\.[0-9]+){{3}}_x64__{}$",
        regex::escape(&name),
        regex::escape(&hash)
    ))
    .expect("regex")
}

fn deleted_regex(family: &str) -> Regex {
    let (name, hash) = crate::packages::family_parts(family);
    Regex::new(&format!(
        r"^{}_[0-9]+(\.[0-9]+){{3}}_x64__{}[0-9a-f-]{{36}}$",
        regex::escape(&name),
        regex::escape(&hash)
    ))
    .expect("regex")
}

fn folder_state(path: &Path) -> &'static str {
    match std::fs::metadata(path) {
        Ok(_) => "exists",
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => "missing",
            std::io::ErrorKind::PermissionDenied => "denied",
            _ => "missing",
        },
    }
}

/// Apparent size (hard-linked files counted in full). `None` if unreadable.
fn dir_size(path: &Path) -> Option<u64> {
    let mut total: u64 = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let rd = std::fs::read_dir(&dir).ok()?;
        for entry in rd {
            let entry = entry.ok()?;
            let meta = entry.metadata().ok()?;
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total += meta.len();
            }
        }
    }
    Some(total)
}

/// Registered package full names for every user (needs elevation; falls
/// back to the current user's list otherwise).
fn registered_all_users(family: &str) -> HashSet<String> {
    let mut set: HashSet<String> = crate::packages::registered_packages(family)
        .into_iter()
        .map(|r| r.full_name.to_lowercase())
        .collect();
    if crate::admin::is_elevated() {
        let (name, _) = crate::packages::family_parts(family);
        let script = format!(
            "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-AppxPackage -AllUsers -Name '{}' -ErrorAction SilentlyContinue | ForEach-Object {{ $_.PackageFullName }}",
            name.replace('\'', "''")
        );
        if let Ok(out) = crate::task::run_powershell(&script) {
            for line in out.lines() {
                let l = line.trim();
                if !l.is_empty() {
                    set.insert(l.to_lowercase());
                }
            }
        }
    }
    set
}

pub fn scan(state: &AppState, include_deleted: bool) -> StaleScan {
    let cfg = state.config.lock().clone();
    let family = cfg.package_family.clone();
    let re_name = name_regex(&family);
    let windows_apps = crate::packages::windows_apps_dir();
    let deleted_root = windows_apps.join("Deleted");

    let registered = registered_all_users(&family);
    let snap = crate::packages::claude_processes(&family, None);
    let running: HashSet<String> = snap
        .members
        .iter()
        .chain(snap.services.iter())
        .map(|p| p.package_full_name.to_lowercase())
        .collect();

    let mut cands: BTreeMap<String, (PathBuf, &'static str)> = BTreeMap::new();
    for n in crate::eventlog::stale_folder_names_from_warnings(&family) {
        if re_name.is_match(&n) {
            cands.insert(n.clone(), (windows_apps.join(&n), "event"));
        }
    }
    let listing_permitted = match std::fs::read_dir(&windows_apps) {
        Ok(rd) => {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if re_name.is_match(&n) {
                    cands.entry(n).or_insert((e.path(), "listing"));
                }
            }
            true
        }
        Err(_) => false,
    };
    if include_deleted {
        let re_del = deleted_regex(&family);
        for n in crate::eventlog::moved_to_deleted_names(&family) {
            if re_del.is_match(&n) {
                cands.insert(format!("Deleted\\{n}"), (deleted_root.join(&n), "deleted"));
            }
        }
    }

    let mut candidates = Vec::new();
    let mut removable_count = 0usize;
    let mut removable_bytes = 0u64;
    for (name, (path, source)) in cands {
        let state_str = folder_state(&path);
        let bare = name.trim_start_matches("Deleted\\").to_lowercase();
        let reason: Option<String> = if state_str == "missing" {
            Some("目录已不存在".into())
        } else if registered.contains(&bare) {
            Some("受保护：已注册的版本".into())
        } else if running.contains(&bare) {
            Some("受保护：仍有进程在运行".into())
        } else {
            None
        };
        let size_bytes = if state_str == "exists" { dir_size(&path) } else { None };
        let protected = reason.is_some();
        if !protected {
            removable_count += 1;
            removable_bytes += size_bytes.unwrap_or(0);
        }
        candidates.push(StalePackage {
            name,
            path: path.display().to_string(),
            state: state_str.into(),
            size_bytes,
            protected,
            reason,
            source: source.into(),
        });
    }

    StaleScan {
        scanned_at: Utc::now(),
        windows_apps: windows_apps.display().to_string(),
        listing_permitted,
        candidates,
        removable_count,
        removable_bytes,
    }
}

#[cfg(windows)]
fn run_hidden(exe: &str, args: &[&str]) -> bool {
    use std::os::windows::process::CommandExt;
    std::process::Command::new(exe)
        .args(args)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn run_hidden(_exe: &str, _args: &[&str]) -> bool {
    false
}

/// `Some(true)` = manifest names this family, `Some(false)` = another
/// package (never delete), `None` = manifest missing / unreadable.
fn manifest_is_family(path: &Path, family: &str) -> Option<bool> {
    let (name, _) = crate::packages::family_parts(family);
    let text = std::fs::read_to_string(path.join("AppxManifest.xml")).ok()?;
    let has_name = text.contains(&format!("Name=\"{name}\""));
    let has_publisher = text.contains("Anthropic");
    Some(has_name && has_publisher)
}

fn remove_folder(path: &Path, family: &str) -> Result<(), String> {
    let p = path.to_string_lossy().to_string();
    // 1. ownership → Administrators (takeown first, icacls /setowner as fallback)
    if !run_hidden("takeown.exe", &["/F", &p, "/R", "/A", "/D", "Y"]) {
        if !run_hidden("icacls.exe", &[&p, "/setowner", ADMIN_SID, "/T", "/C", "/Q"]) {
            warn!("ownership change reported errors on {p} (continuing)");
        }
    }
    // 2. full control for Administrators
    let grant = format!("{ADMIN_SID}:(OI)(CI)F");
    if !run_hidden("icacls.exe", &[&p, "/grant", &grant, "/T", "/C", "/Q"]) {
        warn!("ACL grant reported errors on {p} (continuing)");
    }
    // 3. sanity check now that the folder is readable
    if manifest_is_family(path, family) == Some(false) {
        return Err("AppxManifest.xml 不是 Claude/Anthropic 的包，已跳过".into());
    }
    // 4. delete, with a long-path-capable fallback
    let _ = std::fs::remove_dir_all(path);
    if path.exists() {
        let long = format!(r"\\?\{p}");
        let _ = run_hidden("cmd.exe", &["/d", "/c", "rd", "/s", "/q", &long]);
    }
    if path.exists() {
        Err("目录仍然存在（可能有文件被占用）".into())
    } else {
        Ok(())
    }
}

pub fn remove(state: &AppState, names: &[String]) -> Vec<RemoveOutcome> {
    if !crate::admin::is_elevated() {
        return names
            .iter()
            .map(|n| RemoveOutcome {
                name: n.clone(),
                removed: false,
                error: Some("需要以管理员身份运行".into()),
                freed_bytes: None,
            })
            .collect();
    }
    let family = state.config.lock().package_family.clone();
    let windows_apps = crate::packages::windows_apps_dir();
    // Fresh scan so protections reflect the current state.
    let scan = scan(state, true);

    let mut out = Vec::new();
    for name in names {
        let Some(c) = scan.candidates.iter().find(|c| &c.name == name) else {
            out.push(RemoveOutcome { name: name.clone(), removed: false, error: Some("不在候选列表中".into()), freed_bytes: None });
            continue;
        };
        if c.protected {
            out.push(RemoveOutcome { name: name.clone(), removed: false, error: c.reason.clone(), freed_bytes: None });
            continue;
        }
        if c.state != "exists" {
            out.push(RemoveOutcome { name: name.clone(), removed: false, error: Some(format!("目录状态：{}", c.state)), freed_bytes: None });
            continue;
        }
        let path = PathBuf::from(&c.path);
        if !path.starts_with(&windows_apps) {
            out.push(RemoveOutcome { name: name.clone(), removed: false, error: Some("路径不在 WindowsApps 下，拒绝操作".into()), freed_bytes: None });
            continue;
        }
        info!("removing stale package folder {}", c.path);
        match remove_folder(&path, &family) {
            Ok(()) => out.push(RemoveOutcome { name: name.clone(), removed: true, error: None, freed_bytes: c.size_bytes }),
            Err(e) => {
                warn!("remove {} failed: {e}", c.path);
                out.push(RemoveOutcome { name: name.clone(), removed: false, error: Some(e), freed_bytes: None })
            }
        }
    }
    out
}
