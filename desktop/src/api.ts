import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  ClaudeProcess,
  ContainerEvent,
  KillOutcome,
  RemoveOutcome,
  RepairRecord,
  SchedulerState,
  StaleScan,
  TaskStatus,
  WatchStatus,
} from "./types";

import { mockApi } from "./mock";

/** True inside the Tauri desktop app. False in a plain browser (`vite` dev
 *  server), where the UI runs on demo data for screenshots and layout work. */
export const IS_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// Argument keys are camelCase here; Tauri maps them onto the snake_case
// Rust parameters (dryRun → dry_run, includeDeleted → include_deleted, …).
const realApi = {
  getConfig: () => invoke<AppConfig>("get_config"),
  saveConfig: (cfg: AppConfig) => invoke<void>("save_config", { cfg }),
  getStatus: () => invoke<WatchStatus>("get_status"),
  lastStatus: () => invoke<WatchStatus | null>("last_status"),
  listProcesses: () => invoke<ClaudeProcess[]>("list_processes"),
  killProcesses: (pids: number[]) => invoke<KillOutcome[]>("kill_processes", { pids }),
  repairNow: (dryRun: boolean) => invoke<RepairRecord>("repair_now", { dryRun }),
  launchClaude: () => invoke<void>("launch_claude"),
  getHistory: () => invoke<RepairRecord[]>("get_history"),
  clearHistory: () => invoke<void>("clear_history"),
  recentEvents: (max?: number) => invoke<ContainerEvent[]>("recent_events", { max }),
  scanStale: (includeDeleted: boolean) => invoke<StaleScan>("scan_stale", { includeDeleted }),
  removeStale: (names: string[]) => invoke<RemoveOutcome[]>("remove_stale", { names }),
  taskStatus: () => invoke<TaskStatus>("task_status"),
  installTask: () => invoke<TaskStatus>("install_task"),
  uninstallTask: () => invoke<TaskStatus>("uninstall_task"),
  runTaskNow: () => invoke<void>("run_task_now"),
  readLogs: (maxKb?: number) => invoke<string>("read_logs", { maxKb }),
  openLogDir: () => invoke<void>("open_log_dir"),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  autostartStatus: () => invoke<boolean>("autostart_status"),
  setAutostart: (enabled: boolean) => invoke<void>("set_autostart", { enabled }),
  quitApp: () => invoke<void>("quit_app"),
  showMainWindow: () => invoke<void>("show_main_window"),
  schedulerStatus: () => invoke<SchedulerState>("scheduler_status"),
  pauseScheduler: () => invoke<SchedulerState>("pause_scheduler"),
  resumeScheduler: () => invoke<SchedulerState>("resume_scheduler"),
  isElevated: () => invoke<boolean>("is_elevated"),
  /** true = UAC accepted (caller should quit so the elevated instance takes over). */
  relaunchAsAdmin: () => invoke<boolean>("relaunch_as_admin"),
  getPlatform: () => invoke<string>("get_platform"),
  appVersion: () => invoke<string>("app_version"),
};

export type Api = typeof realApi;

export const api: Api = IS_TAURI ? realApi : mockApi;
