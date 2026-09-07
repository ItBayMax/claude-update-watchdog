import { create } from "zustand";
import { api } from "./api";
import type {
  AppConfig,
  ContainerEvent,
  LaunchFailure,
  RemoveOutcome,
  RepairRecord,
  SchedulerState,
  StaleScan,
  TaskStatus,
  WatchStatus,
} from "./types";

interface AppStore {
  config: AppConfig | null;
  status: WatchStatus | null;
  history: RepairRecord[];
  events: ContainerEvent[];
  stale: StaleScan | null;
  staleBusy: boolean;
  removeOutcomes: RemoveOutcome[];
  task: TaskStatus | null;
  taskBusy: boolean;
  logs: string;
  schedulerState: SchedulerState;
  elevated: boolean | null;
  version: string;
  checking: boolean;
  repairing: boolean;
  lastRepair: RepairRecord | null;
  pendingFailure: LaunchFailure | null;

  loadConfig: () => Promise<void>;
  saveConfig: (cfg: AppConfig) => Promise<void>;
  setStatus: (s: WatchStatus | null) => void;
  loadLastStatus: () => Promise<void>;
  refreshStatus: () => Promise<void>;
  loadHistory: () => Promise<void>;
  clearHistory: () => Promise<void>;
  loadEvents: (max?: number) => Promise<void>;
  scanStale: (includeDeleted: boolean) => Promise<void>;
  removeStale: (names: string[]) => Promise<void>;
  dismissRemoveOutcomes: () => void;
  loadTask: () => Promise<void>;
  installTask: () => Promise<void>;
  uninstallTask: () => Promise<void>;
  runTaskNow: () => Promise<void>;
  loadLogs: () => Promise<void>;
  refreshSchedulerState: () => Promise<void>;
  pauseScheduler: () => Promise<void>;
  resumeScheduler: () => Promise<void>;
  refreshElevation: () => Promise<void>;
  loadVersion: () => Promise<void>;
  repair: (dryRun: boolean) => Promise<RepairRecord | null>;
  launchClaude: () => Promise<void>;
  killPids: (pids: number[]) => Promise<void>;
  setRepairing: (v: boolean) => void;
  setLastRepair: (r: RepairRecord | null) => void;
  setPendingFailure: (f: LaunchFailure | null) => void;
}

export const useStore = create<AppStore>((set, get) => ({
  config: null,
  status: null,
  history: [],
  events: [],
  stale: null,
  staleBusy: false,
  removeOutcomes: [],
  task: null,
  taskBusy: false,
  logs: "",
  schedulerState: "running",
  elevated: null,
  version: "",
  checking: false,
  repairing: false,
  lastRepair: null,
  pendingFailure: null,

  loadConfig: async () => set({ config: await api.getConfig() }),
  saveConfig: async (cfg) => {
    await api.saveConfig(cfg);
    set({ config: cfg });
    try {
      set({ schedulerState: await api.schedulerStatus() });
    } catch {
      /* ignore */
    }
  },
  setStatus: (s) => set({ status: s }),
  loadLastStatus: async () => {
    const s = await api.lastStatus();
    if (s) set({ status: s });
  },
  refreshStatus: async () => {
    set({ checking: true });
    try {
      set({ status: await api.getStatus() });
    } finally {
      set({ checking: false });
    }
  },
  loadHistory: async () => set({ history: await api.getHistory() }),
  clearHistory: async () => {
    await api.clearHistory();
    set({ history: [] });
  },
  loadEvents: async (max) => set({ events: await api.recentEvents(max ?? 100) }),
  scanStale: async (includeDeleted) => {
    set({ staleBusy: true });
    try {
      set({ stale: await api.scanStale(includeDeleted) });
    } finally {
      set({ staleBusy: false });
    }
  },
  removeStale: async (names) => {
    set({ staleBusy: true });
    try {
      const outcomes = await api.removeStale(names);
      set({ removeOutcomes: outcomes });
      set({ stale: await api.scanStale(false) });
    } finally {
      set({ staleBusy: false });
    }
  },
  dismissRemoveOutcomes: () => set({ removeOutcomes: [] }),
  loadTask: async () => {
    set({ taskBusy: true });
    try {
      set({ task: await api.taskStatus() });
    } finally {
      set({ taskBusy: false });
    }
  },
  installTask: async () => {
    set({ taskBusy: true });
    try {
      set({ task: await api.installTask() });
    } finally {
      set({ taskBusy: false });
    }
  },
  uninstallTask: async () => {
    set({ taskBusy: true });
    try {
      set({ task: await api.uninstallTask() });
    } finally {
      set({ taskBusy: false });
    }
  },
  runTaskNow: async () => {
    await api.runTaskNow();
    await new Promise((r) => setTimeout(r, 4000));
    await get().loadTask();
  },
  loadLogs: async () => set({ logs: await api.readLogs(256) }),
  refreshSchedulerState: async () => set({ schedulerState: await api.schedulerStatus() }),
  pauseScheduler: async () => set({ schedulerState: await api.pauseScheduler() }),
  resumeScheduler: async () => set({ schedulerState: await api.resumeScheduler() }),
  refreshElevation: async () => {
    try {
      set({ elevated: await api.isElevated() });
    } catch {
      set({ elevated: null });
    }
  },
  loadVersion: async () => {
    try {
      set({ version: await api.appVersion() });
    } catch {
      /* ignore */
    }
  },
  repair: async (dryRun) => {
    set({ repairing: true });
    try {
      const r = await api.repairNow(dryRun);
      set({ lastRepair: r });
      await get().loadHistory();
      return r;
    } catch (e) {
      console.warn("repair failed:", e);
      return null;
    } finally {
      set({ repairing: false });
    }
  },
  launchClaude: async () => {
    await api.launchClaude();
  },
  killPids: async (pids) => {
    await api.killProcesses(pids);
    await get().refreshStatus();
  },
  setRepairing: (v) => set({ repairing: v }),
  setLastRepair: (r) => set({ lastRepair: r }),
  setPendingFailure: (f) => set({ pendingFailure: f }),
}));
