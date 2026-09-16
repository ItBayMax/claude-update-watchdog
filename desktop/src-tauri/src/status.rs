//! One full snapshot of "how is Claude doing right now".

use std::sync::atomic::Ordering;

use chrono::Utc;

use crate::model::{Health, WatchStatus};
use crate::state::AppState;

pub fn collect(state: &AppState) -> WatchStatus {
    let cfg = state.config.lock().clone();
    let aumid = cfg.aumid();

    let registered = crate::packages::registered_packages(&cfg.package_family);
    let current_full_name = registered.first().map(|r| r.full_name.clone());
    let current_version = registered.first().map(|r| r.version.clone());

    let snap = crate::packages::claude_processes(&cfg.package_family, current_full_name.as_deref());
    let processes = snap.members;
    let services = snap.services;
    let other_claude_processes = snap.others;
    let orphan_count = processes.iter().filter(|p| p.orphan).count();

    let recent_failure =
        crate::eventlog::recent_launch_failures(&aumid, cfg.recent_window_minutes, 1)
            .into_iter()
            .next();
    let last_failure = crate::eventlog::last_launch_failure(&aumid);
    let events = crate::eventlog::container_events(&cfg.package_family, 40);
    let open_containers: Vec<crate::model::OpenContainer> =
        crate::eventlog::live_containers(&cfg.package_family, 1500)
            .into_iter()
            .map(|(pkg, id)| {
                let stale = current_full_name
                    .as_deref()
                    .map(|c| !c.eq_ignore_ascii_case(&pkg))
                    .unwrap_or(false);
                crate::model::OpenContainer {
                    version: crate::packages::version_of(&pkg),
                    package_full_name: pkg,
                    container_id: id,
                    stale,
                }
            })
            .collect();

    let task = crate::task::cached_status();
    let policy_disable_auto_updates = crate::packages::policy_disable_auto_updates();
    let elevated = crate::admin::is_elevated();
    let repairing = state.repairing.load(Ordering::SeqCst);
    let last_repair = state.history.lock().iter().rev().find(|r| !r.dry_run).cloned();
    let recent_failure_handled = recent_failure
        .as_ref()
        .map(|f| state.handled_failures.lock().contains(&f.record_id))
        .unwrap_or(false);

    let health = if repairing {
        Health::Repairing
    } else if let Some(f) = &recent_failure {
        let repaired_ok = recent_failure_handled
            && last_repair
                .as_ref()
                .map(|r| r.success && r.finished_at >= f.time)
                .unwrap_or(false);
        if repaired_ok {
            if orphan_count > 0 {
                Health::Warning
            } else {
                Health::Healthy
            }
        } else {
            Health::Failure
        }
    } else if orphan_count > 0 {
        Health::Warning
    } else if current_full_name.is_some() {
        Health::Healthy
    } else {
        Health::Unknown
    };

    WatchStatus {
        checked_at: Utc::now(),
        health,
        aumid,
        registered,
        current_version,
        current_full_name,
        processes,
        orphan_count,
        services,
        other_claude_processes,
        recent_failure,
        recent_failure_handled,
        last_failure,
        last_repair,
        events,
        open_containers,
        task,
        policy_disable_auto_updates,
        elevated,
        repairing,
    }
}
