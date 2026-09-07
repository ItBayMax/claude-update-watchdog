//! Serde structs shared with the frontend. Keep src/types.ts in sync.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredPackage {
    pub full_name: String,
    pub version: String,
    pub root_folder: Option<String>,
}

/// A running process that carries Claude package identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeProcess {
    pub pid: u32,
    pub name: String,
    pub package_full_name: String,
    pub version: String,
    pub exe: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    /// Package differs from the currently registered version — a leftover
    /// from before an update. This is what blocks the new version's container.
    pub orphan: bool,
    /// Windows session the process runs in (0 = services).
    pub session_id: Option<u32>,
    /// Runs outside the user's session: the packaged CoworkVMService
    /// (LocalSystem, session 0) or another user's Claude. Never terminated —
    /// it is not part of this user's Desktop AppX container and the
    /// deployment engine manages the service itself.
    pub is_service: bool,
}

/// A process whose name contains "claude" but has no package identity
/// (Claude Code CLI, helper shells). Shown for transparency, never touched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleProcess {
    pub pid: u32,
    pub name: String,
    pub exe: Option<String>,
}

/// AppModel-Runtime event 208 for the Claude AUMID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchFailure {
    pub record_id: u64,
    pub time: DateTime<Utc>,
    pub event_id: u32,
    pub package_full_name: String,
    pub application: String,
    pub error_code: u32,
    pub error_hex: String,
}

/// Container lifecycle timeline entry (events 201/208/210/211/215/217).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerEvent {
    pub record_id: u64,
    pub time: DateTime<Utc>,
    pub event_id: u32,
    pub level: String,
    pub package_full_name: Option<String>,
    pub summary: String,
    pub error_hex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillOutcome {
    pub pid: u32,
    pub name: String,
    pub package_full_name: Option<String>,
    pub killed: bool,
    pub error: Option<String>,
    /// ACCESS_DENIED or already exited — not a real failure.
    pub protected: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepairTrigger {
    /// Event-log subscription delivered a 208.
    Event,
    /// The periodic poll noticed a recent 208 the subscription missed.
    Poll,
    /// User clicked "repair" in the UI.
    Manual,
    /// `--repair` on the command line.
    Cli,
    /// `--repair-from-event`, launched by the scheduled task.
    Task,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairRecord {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub trigger: RepairTrigger,
    pub dry_run: bool,
    pub target_version: Option<String>,
    /// Processes that were (or would be) stopped.
    pub planned: Vec<ClaudeProcess>,
    pub killed: Vec<KillOutcome>,
    pub relaunched: bool,
    pub success: bool,
    pub message: String,
    pub failure: Option<LaunchFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskStatus {
    pub installed: bool,
    pub state: Option<String>,
    pub action: Option<String>,
    pub last_run_time: Option<String>,
    pub last_result: Option<u32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Health {
    Unknown,
    Healthy,
    /// Orphan processes present, or a repair had to be skipped.
    Warning,
    /// A recent launch failure that has not been repaired successfully.
    Failure,
    Repairing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchStatus {
    pub checked_at: DateTime<Utc>,
    pub health: Health,
    pub aumid: String,
    pub registered: Vec<RegisteredPackage>,
    pub current_version: Option<String>,
    pub current_full_name: Option<String>,
    pub processes: Vec<ClaudeProcess>,
    pub orphan_count: usize,
    /// Package-identity processes outside this session (visible when elevated).
    pub services: Vec<ClaudeProcess>,
    pub other_claude_processes: Vec<SimpleProcess>,
    pub recent_failure: Option<LaunchFailure>,
    pub recent_failure_handled: bool,
    pub last_failure: Option<LaunchFailure>,
    pub last_repair: Option<RepairRecord>,
    pub events: Vec<ContainerEvent>,
    pub task: TaskStatus,
    pub policy_disable_auto_updates: Option<bool>,
    pub elevated: bool,
    pub repairing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StalePackage {
    pub name: String,
    pub path: String,
    /// "exists" | "missing" | "denied"
    pub state: String,
    pub size_bytes: Option<u64>,
    pub protected: bool,
    pub reason: Option<String>,
    /// "event" | "listing" | "deleted"
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaleScan {
    pub scanned_at: DateTime<Utc>,
    pub windows_apps: String,
    pub listing_permitted: bool,
    pub candidates: Vec<StalePackage>,
    pub removable_count: usize,
    pub removable_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveOutcome {
    pub name: String,
    pub removed: bool,
    pub error: Option<String>,
    pub freed_bytes: Option<u64>,
}
