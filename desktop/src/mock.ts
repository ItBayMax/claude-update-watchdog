// Demo data used when the UI runs outside the Tauri runtime (plain `vite`
// in a browser): documentation screenshots, layout work, UI tests. Every
// value mirrors a real machine's shape; nothing here talks to the system.
// The desktop app never uses this module (see IS_TAURI in api.ts).

import type { Api } from "./api";
import type {
  AppConfig,
  ClaudeProcess,
  ContainerEvent,
  KillOutcome,
  LaunchFailure,
  RepairRecord,
  SchedulerState,
  StalePackage,
  StaleScan,
  TaskStatus,
  WatchStatus,
} from "./types";

const AUMID = "Claude_pzs8sxrjxfjjc!Claude";
const CURRENT = "Claude_1.46388.4.0_x64__pzs8sxrjxfjjc";
const PREV = "Claude_1.46388.3.0_x64__pzs8sxrjxfjjc";
const PREV2 = "Claude_1.46388.2.0_x64__pzs8sxrjxfjjc";
const ROOT = `C:\\Program Files\\WindowsApps\\${CURRENT}`;

const t = (local: string) => new Date(local).toISOString();

const PIDS = [2628, 6588, 13976, 34128, 4724, 10448, 11444, 26540, 1904, 5020, 24016, 20792];
const STARTS = ["08:43:16", "08:43:16", "08:43:17", "08:43:17", "08:43:17", "08:43:17", "08:43:23", "08:43:26", "08:43:26", "08:43:26", "08:43:26", "08:43:41"];

const processes: ClaudeProcess[] = PIDS.map((pid, i) => ({
  pid,
  name: "claude.exe",
  package_full_name: CURRENT,
  version: "1.46388.4.0",
  exe: `${ROOT}\\app\\Claude.exe`,
  started_at: t(`2026-09-06T${STARTS[i]}+08:00`),
  orphan: false,
  session_id: 2,
  is_service: false,
}));

const lastFailure: LaunchFailure = {
  record_id: 55368,
  time: t("2026-09-05T01:31:43+08:00"),
  event_id: 208,
  package_full_name: PREV,
  application: AUMID,
  error_code: 0x80070020,
  error_hex: "0x80070020",
};

const killed: KillOutcome[] = processes.slice(0, 11).map((p) => ({
  pid: p.pid,
  name: p.name,
  package_full_name: p.package_full_name,
  killed: true,
  error: null,
  protected: false,
}));

const manualRepair: RepairRecord = {
  id: "8d1c0f2e-demo-0001",
  started_at: t("2026-09-06T10:53:52+08:00"),
  finished_at: t("2026-09-06T10:54:15+08:00"),
  trigger: "manual",
  dry_run: false,
  target_version: "1.46388.4.0",
  planned: processes.slice(0, 11),
  killed,
  relaunched: true,
  success: true,
  message: "已结束 11 个进程并重新启动 Claude 1.46388.4.0",
  failure: null,
};

const dryRun: RepairRecord = {
  id: "8d1c0f2e-demo-0000",
  started_at: t("2026-09-06T10:53:43+08:00"),
  finished_at: t("2026-09-06T10:53:43+08:00"),
  trigger: "manual",
  dry_run: true,
  target_version: "1.46388.4.0",
  planned: processes,
  killed: [],
  relaunched: false,
  success: true,
  message: `预演：将结束 ${processes.length} 个进程并重新启动 ${AUMID}`,
  failure: null,
};

let history: RepairRecord[] = [dryRun, manualRepair];

const ev = (
  record_id: number,
  local: string,
  event_id: number,
  level: string,
  pkg: string,
  summary: string,
  error_hex: string | null = null
): ContainerEvent => ({ record_id, time: t(local), event_id, level, package_full_name: pkg, summary, error_hex });

const events: ContainerEvent[] = [
  ev(55412, "2026-09-05T11:55:51.613+08:00", 201, "info", CURRENT, "已创建进程"),
  ev(55411, "2026-09-05T11:55:51.612+08:00", 211, "info", CURRENT, "进程加入容器"),
  ev(55410, "2026-09-05T11:55:51.276+08:00", 210, "info", CURRENT, "创建桌面 AppX 容器"),
  ev(55409, "2026-09-05T11:55:50.902+08:00", 217, "info", PREV, "销毁桌面 AppX 容器"),
  ev(55368, "2026-09-05T01:31:43.912+08:00", 208, "error", PREV, "启动进程失败 [LaunchProcess]", "0x80070020"),
  ev(55367, "2026-09-05T01:31:43.910+08:00", 215, "error", PREV, "创建桌面 AppX 容器失败（转换作业出错）", "0x80070020"),
  ev(55366, "2026-09-05T01:31:43.905+08:00", 215, "error", PREV, "创建桌面 AppX 容器失败（转换作业出错）", "0x80070020"),
  ev(55365, "2026-09-05T01:31:43.640+08:00", 211, "info", PREV, "进程加入容器"),
  ev(55364, "2026-09-05T01:31:43.531+08:00", 210, "info", PREV, "创建桌面 AppX 容器"),
  ev(55363, "2026-09-05T01:31:42.988+08:00", 217, "info", PREV2, "销毁桌面 AppX 容器"),
  ev(55290, "2026-09-04T18:18:32.401+08:00", 201, "info", PREV2, "已创建进程"),
  ev(55289, "2026-09-04T18:18:32.398+08:00", 211, "info", PREV2, "进程加入容器"),
];

let task: TaskStatus = {
  installed: true,
  state: "Ready",
  action: "%LOCALAPPDATA%\\Claude Watchdog\\claude-watchdog.exe --repair-from-event",
  last_run_time: "2026-09-06 08:12:03",
  last_result: 0,
  error: null,
};

let config: AppConfig = {
  package_family: "Claude_pzs8sxrjxfjjc",
  app_id: "Claude",
  auto_repair: true,
  repair_delay_ms: 3000,
  recent_window_minutes: 5,
  max_attempts_per_15min: 3,
  poll_seconds: 5,
  notify: true,
  autostart: true,
  close_to_tray: true,
  confirm_exit: true,
  log_level: "info",
};

let scheduler: SchedulerState = "running";

const STALE_VERSIONS = [
  "1.13576.4.0", "1.14271.0.0", "1.15200.0.0", "1.15962.0.0", "1.15962.1.0", "1.15962.2.0", "1.17377.2.0",
  "1.18286.2.0", "1.19367.0.0", "1.21459.0.0", "1.21459.1.0", "1.21459.3.0", "1.24012.1.0", "1.24012.11.0", "1.25927.0.0",
];
let staleCandidates: StalePackage[] = STALE_VERSIONS.map((v, i) => ({
  name: `Claude_${v}_x64__pzs8sxrjxfjjc`,
  path: `C:\\Program Files\\WindowsApps\\Claude_${v}_x64__pzs8sxrjxfjjc`,
  state: "exists",
  size_bytes: 540 * 1024 * 1024 + i * 4 * 1024 * 1024,
  protected: false,
  reason: null,
  source: "event",
}));

function scan(): StaleScan {
  const removable = staleCandidates.filter((c) => !c.protected && c.state === "exists");
  return {
    scanned_at: new Date().toISOString(),
    windows_apps: "C:\\Program Files\\WindowsApps",
    listing_permitted: false,
    candidates: staleCandidates,
    removable_count: removable.length,
    removable_bytes: removable.reduce((n, c) => n + (c.size_bytes ?? 0), 0),
  };
}

function status(): WatchStatus {
  return {
    checked_at: new Date().toISOString(),
    health: "healthy",
    aumid: AUMID,
    registered: [{ full_name: CURRENT, version: "1.46388.4.0", root_folder: ROOT }],
    current_version: "1.46388.4.0",
    current_full_name: CURRENT,
    processes,
    orphan_count: 0,
    services: [],
    other_claude_processes: [
      { pid: 26664, name: "claude.exe", exe: "%APPDATA%\\Claude\\claude-code\\2.1.260\\claude.exe" },
    ],
    recent_failure: null,
    recent_failure_handled: false,
    last_failure: lastFailure,
    last_repair: (() => {
      const real = history.filter((r) => !r.dry_run);
      return real.length ? real[real.length - 1] : null;
    })(),
    events,
    task,
    policy_disable_auto_updates: null,
    elevated: false,
    repairing: false,
  };
}

const LOGS = [
  "2026-09-06T00:43:16.102Z  INFO starting claude-watchdog app_dir=... log_dir=...",
  "2026-09-06T00:43:16.331Z  INFO scheduler started: every 5s",
  "2026-09-06T00:43:16.340Z  INFO subscribed to launch-failure events channel=Microsoft-Windows-AppModel-Runtime/Admin aumid=Claude_pzs8sxrjxfjjc!Claude",
  "2026-09-06T00:43:16.341Z  INFO event watcher running",
  "2026-09-06T02:53:52.329Z  INFO repair: snapshot taken trigger=Manual dry_run=false processes=11 orphans=0 skipped_services=0",
  "2026-09-06T02:53:52.356Z  INFO repair: processes stopped killed=11 of=11",
  "2026-09-06T02:53:55.101Z  INFO repair: launched Claude_pzs8sxrjxfjjc!Claude attempt=1",
  "2026-09-06T02:54:15.601Z  INFO repair: 已结束 11 个进程并重新启动 Claude 1.46388.4.0 success=true",
].join("\n");

const ok = <T,>(v: T) => Promise.resolve(v);

export const mockApi: Api = {
  getConfig: () => ok(config),
  saveConfig: (cfg) => {
    config = cfg;
    return ok(undefined);
  },
  getStatus: () => ok(status()),
  lastStatus: () => ok(status()),
  listProcesses: () => ok(processes),
  killProcesses: (pids) =>
    ok(
      pids.map((pid) => ({
        pid,
        name: "claude.exe",
        package_full_name: CURRENT,
        killed: true,
        error: null,
        protected: false,
      }))
    ),
  repairNow: (dry) => {
    const rec: RepairRecord = dry
      ? { ...dryRun, id: `demo-${Date.now()}`, started_at: new Date().toISOString(), finished_at: new Date().toISOString() }
      : { ...manualRepair, id: `demo-${Date.now()}`, started_at: new Date().toISOString(), finished_at: new Date().toISOString() };
    history = [...history, rec];
    return ok(rec);
  },
  launchClaude: () => ok(undefined),
  getHistory: () => ok([...history].reverse()),
  clearHistory: () => {
    history = [];
    return ok(undefined);
  },
  recentEvents: (max) => ok(events.slice(0, max ?? 60)),
  scanStale: () => ok(scan()),
  removeStale: (names) => {
    const outcomes = names.map((name) => {
      const c = staleCandidates.find((x) => x.name === name);
      return { name, removed: !!c, error: c ? null : "不在候选列表中", freed_bytes: c?.size_bytes ?? null };
    });
    staleCandidates = staleCandidates.filter((c) => !names.includes(c.name));
    return ok(outcomes);
  },
  taskStatus: () => ok(task),
  installTask: () => {
    task = { ...task, installed: true, state: "Ready" };
    return ok(task);
  },
  uninstallTask: () => {
    task = { installed: false, state: null, action: null, last_run_time: null, last_result: null, error: null };
    return ok(task);
  },
  runTaskNow: () => ok(undefined),
  readLogs: () => ok(LOGS),
  openLogDir: () => ok(undefined),
  openPath: () => ok(undefined),
  autostartStatus: () => ok(config.autostart),
  setAutostart: (enabled) => {
    config = { ...config, autostart: enabled };
    return ok(undefined);
  },
  quitApp: () => ok(undefined),
  showMainWindow: () => ok(undefined),
  schedulerStatus: () => ok(scheduler),
  pauseScheduler: () => {
    scheduler = "paused";
    return ok(scheduler);
  },
  resumeScheduler: () => {
    scheduler = "running";
    return ok(scheduler);
  },
  isElevated: () => ok(false),
  relaunchAsAdmin: () => ok(false),
  getPlatform: () => ok("windows"),
  appVersion: () => ok("0.1.4 (demo data)"),
};
