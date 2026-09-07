// Mirrors src-tauri/src/model.rs (serde snake_case).

export interface RegisteredPackage {
  full_name: string;
  version: string;
  root_folder?: string | null;
}

export interface ClaudeProcess {
  pid: number;
  name: string;
  package_full_name: string;
  version: string;
  exe?: string | null;
  started_at?: string | null;
  /** Belongs to a version other than the registered one — the leftover that blocks the new container. */
  orphan: boolean;
  /** Windows session (0 = services). */
  session_id?: number | null;
  /** Outside this user's session (packaged service / other user); never terminated. */
  is_service: boolean;
}

export interface SimpleProcess {
  pid: number;
  name: string;
  exe?: string | null;
}

export interface LaunchFailure {
  record_id: number;
  time: string;
  event_id: number;
  package_full_name: string;
  application: string;
  error_code: number;
  error_hex: string;
}

export interface ContainerEvent {
  record_id: number;
  time: string;
  event_id: number;
  level: string;
  package_full_name?: string | null;
  summary: string;
  error_hex?: string | null;
}

export interface KillOutcome {
  pid: number;
  name: string;
  package_full_name?: string | null;
  killed: boolean;
  error?: string | null;
  protected: boolean;
}

export type RepairTrigger = "event" | "poll" | "manual" | "cli" | "task";

export interface RepairRecord {
  id: string;
  started_at: string;
  finished_at: string;
  trigger: RepairTrigger;
  dry_run: boolean;
  target_version?: string | null;
  planned: ClaudeProcess[];
  killed: KillOutcome[];
  relaunched: boolean;
  success: boolean;
  message: string;
  failure?: LaunchFailure | null;
}

export interface TaskStatus {
  installed: boolean;
  state?: string | null;
  action?: string | null;
  last_run_time?: string | null;
  last_result?: number | null;
  error?: string | null;
}

export type Health = "unknown" | "healthy" | "warning" | "failure" | "repairing";

export interface WatchStatus {
  checked_at: string;
  health: Health;
  aumid: string;
  registered: RegisteredPackage[];
  current_version?: string | null;
  current_full_name?: string | null;
  processes: ClaudeProcess[];
  orphan_count: number;
  services: ClaudeProcess[];
  other_claude_processes: SimpleProcess[];
  recent_failure?: LaunchFailure | null;
  recent_failure_handled: boolean;
  last_failure?: LaunchFailure | null;
  last_repair?: RepairRecord | null;
  events: ContainerEvent[];
  task: TaskStatus;
  policy_disable_auto_updates?: boolean | null;
  elevated: boolean;
  repairing: boolean;
}

export interface StalePackage {
  name: string;
  path: string;
  state: string;
  size_bytes?: number | null;
  protected: boolean;
  reason?: string | null;
  source: string;
}

export interface StaleScan {
  scanned_at: string;
  windows_apps: string;
  listing_permitted: boolean;
  candidates: StalePackage[];
  removable_count: number;
  removable_bytes: number;
}

export interface RemoveOutcome {
  name: string;
  removed: boolean;
  error?: string | null;
  freed_bytes?: number | null;
}

export type SchedulerState = "disabled" | "paused" | "running";

export interface AppConfig {
  package_family: string;
  app_id: string;
  auto_repair: boolean;
  repair_delay_ms: number;
  recent_window_minutes: number;
  max_attempts_per_15min: number;
  poll_seconds: number;
  notify: boolean;
  autostart: boolean;
  close_to_tray: boolean;
  confirm_exit: boolean;
  log_level: string;
}
