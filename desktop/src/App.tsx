import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Activity,
  Cpu,
  FileText,
  ListOrdered,
  Settings,
  ShieldAlert,
  ShieldCheck,
  Timer,
  Trash2,
  Wrench,
} from "lucide-react";
import { useStore } from "./store";
import { api, IS_TAURI } from "./api";
import type { LaunchFailure, RepairRecord, WatchStatus } from "./types";

import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Dashboard } from "./components/Dashboard";
import { ProcessesPanel } from "./components/ProcessesPanel";
import { EventsPanel } from "./components/EventsPanel";
import { StalePanel } from "./components/StalePanel";
import { TaskPanel } from "./components/TaskPanel";
import { SettingsPanel } from "./components/SettingsPanel";
import { LogsPanel } from "./components/LogsPanel";
import { RepairConfirmDialog } from "./components/RepairConfirmDialog";
import { ExitConfirmDialog } from "./components/ExitConfirmDialog";

export default function App() {
  const {
    config,
    status,
    checking,
    repairing,
    pendingFailure,
    loadConfig,
    loadLastStatus,
    refreshStatus,
    setStatus,
    loadHistory,
    refreshSchedulerState,
    refreshElevation,
    loadVersion,
    setRepairing,
    setLastRepair,
    setPendingFailure,
    repair,
  } = useStore();
  // `?tab=processes|events|stale|task|settings|logs` preselects a tab (used for documentation screenshots).
  const [tab, setTab] = useState(() => {
    const wanted = new URLSearchParams(window.location.search).get("tab");
    const known = ["dashboard", "processes", "events", "stale", "task", "settings", "logs"];
    return wanted && known.includes(wanted) ? wanted : "dashboard";
  });
  const [exitOpen, setExitOpen] = useState(false);

  useEffect(() => {
    loadConfig();
    loadLastStatus();
    loadHistory();
    refreshSchedulerState();
    refreshElevation();
    loadVersion();
    // A fresh snapshot right away so the dashboard is never empty.
    refreshStatus();
  }, []);

  useEffect(() => {
    // Event bridge exists only inside Tauri; the browser demo has nothing to listen to.
    if (!IS_TAURI) return;
    const unsubs: Array<() => void> = [];
    (async () => {
      unsubs.push(
        await listen<WatchStatus>("claudewatchdog://status", (e) => setStatus(e.payload))
      );
      unsubs.push(await listen("claudewatchdog://repair-started", () => setRepairing(true)));
      unsubs.push(
        await listen<RepairRecord>("claudewatchdog://repair-finished", (e) => {
          setRepairing(false);
          setLastRepair(e.payload);
          loadHistory();
        })
      );
      unsubs.push(
        await listen<LaunchFailure>("claudewatchdog://prompt-repair", (e) =>
          setPendingFailure(e.payload)
        )
      );
      unsubs.push(await listen("claudewatchdog://request-exit", () => setExitOpen(true)));
      unsubs.push(
        await listen("claudewatchdog://scheduler-changed", () => refreshSchedulerState())
      );
    })();
    return () => unsubs.forEach((u) => u());
  }, []);

  const health = status?.health ?? "unknown";
  const bad = health === "failure" || health === "repairing";
  // `?screenshot=1` hides the demo banner for documentation captures.
  const showDemoBanner = !IS_TAURI && !window.location.search.includes("screenshot=1");

  return (
    <div className="min-h-screen flex flex-col">
      {showDemoBanner ? (
        <div className="px-6 py-1 text-xs bg-amber-500/10 text-amber-800 border-b">
          演示模式：当前不在 Tauri 运行时中，界面显示的是示例数据，仅用于预览与截图。
        </div>
      ) : null}
      <header className="border-b px-6 py-3 flex items-center justify-between bg-card">
        <div className="flex items-center gap-3">
          {bad ? (
            <ShieldAlert className="h-6 w-6 text-destructive" />
          ) : (
            <ShieldCheck className="h-6 w-6 text-primary" />
          )}
          <div>
            <div className="font-semibold leading-tight">Claude Watchdog</div>
            <div className="text-xs text-muted-foreground">
              Claude 桌面版更新看门狗 · 监视启动失败 0x80070020，自动清理残留并重启
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <HealthBadge status={status} checking={checking} repairing={repairing} />
          <Button size="sm" variant="outline" onClick={refreshStatus} disabled={checking}>
            {checking ? "检查中…" : "立即检查"}
          </Button>
          <Button size="sm" variant="outline" onClick={() => repair(true)} disabled={repairing}>
            预演
          </Button>
          <Button
            size="sm"
            variant={bad ? "destructive" : "default"}
            onClick={() => repair(false)}
            disabled={repairing}
          >
            <Wrench className="h-4 w-4 mr-1" />
            {repairing ? "修复中…" : "立即修复"}
          </Button>
        </div>
      </header>

      <main className="flex-1 px-6 py-4 overflow-auto">
        <Tabs value={tab} onValueChange={setTab} className="w-full">
          <TabsList>
            <TabsTrigger value="dashboard"><Activity className="h-4 w-4 mr-1" />仪表盘</TabsTrigger>
            <TabsTrigger value="processes"><Cpu className="h-4 w-4 mr-1" />进程</TabsTrigger>
            <TabsTrigger value="events"><ListOrdered className="h-4 w-4 mr-1" />事件</TabsTrigger>
            <TabsTrigger value="stale"><Trash2 className="h-4 w-4 mr-1" />残留版本</TabsTrigger>
            <TabsTrigger value="task"><Timer className="h-4 w-4 mr-1" />计划任务</TabsTrigger>
            <TabsTrigger value="settings"><Settings className="h-4 w-4 mr-1" />设置</TabsTrigger>
            <TabsTrigger value="logs"><FileText className="h-4 w-4 mr-1" />日志</TabsTrigger>
          </TabsList>
          <TabsContent value="dashboard"><Dashboard /></TabsContent>
          <TabsContent value="processes"><ProcessesPanel /></TabsContent>
          <TabsContent value="events"><EventsPanel /></TabsContent>
          <TabsContent value="stale"><StalePanel /></TabsContent>
          <TabsContent value="task"><TaskPanel /></TabsContent>
          <TabsContent value="settings"><SettingsPanel /></TabsContent>
          <TabsContent value="logs"><LogsPanel /></TabsContent>
        </Tabs>
      </main>

      <RepairConfirmDialog
        open={pendingFailure !== null}
        failure={pendingFailure}
        onConfirm={async () => {
          setPendingFailure(null);
          await repair(false);
        }}
        onCancel={() => setPendingFailure(null)}
      />

      <ExitConfirmDialog
        open={exitOpen}
        onClose={() => setExitOpen(false)}
        onConfirm={async () => {
          setExitOpen(false);
          await api.quitApp();
        }}
        confirmExit={config?.confirm_exit ?? true}
      />
    </div>
  );
}

function HealthBadge({
  status,
  checking,
  repairing,
}: {
  status: WatchStatus | null;
  checking: boolean;
  repairing: boolean;
}) {
  if (repairing) return <Badge variant="destructive">修复中…</Badge>;
  if (!status) return <Badge variant="outline">{checking ? "检查中…" : "未检查"}</Badge>;
  const ver = status.current_version ?? "未注册";
  switch (status.health) {
    case "healthy":
      return <Badge variant="success">正常 · Claude {ver} · {status.processes.length} 个进程</Badge>;
    case "warning":
      return <Badge variant="warning">注意 · {status.orphan_count} 个旧版本进程残留</Badge>;
    case "failure":
      return (
        <Badge variant="destructive">
          启动失败 · {status.recent_failure?.error_hex ?? "0x80070020"}
        </Badge>
      );
    case "repairing":
      return <Badge variant="destructive">修复中…</Badge>;
    default:
      return <Badge variant="outline">未知 · 未找到已注册的 Claude</Badge>;
  }
}
