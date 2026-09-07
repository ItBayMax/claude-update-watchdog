import { useEffect, type ReactNode } from "react";
import { useStore } from "@/store";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { hex } from "@/lib/utils";
import { RefreshCw } from "lucide-react";

export function TaskPanel() {
  const { task, taskBusy, loadTask, installTask, uninstallTask, runTaskNow } = useStore();

  useEffect(() => {
    loadTask();
  }, []);

  const pointsToThisApp = (task?.action ?? "").toLowerCase().includes("claude-watchdog");

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader className="flex flex-row items-start justify-between space-y-0">
          <div>
            <CardTitle>计划任务兜底</CardTitle>
            <CardDescription>
              这个窗口关掉后监视就停了。计划任务「Claude Update Watchdog」订阅同一个事件 208，
              触发后 3 秒运行 <code>claude-watchdog.exe --repair-from-event</code> 做无界面修复，
              所以即使看门狗没在运行也能自愈。安装会覆盖同名的旧版 PowerShell 任务。
            </CardDescription>
          </div>
          <Button size="sm" variant="outline" onClick={loadTask} disabled={taskBusy}>
            <RefreshCw className="h-4 w-4 mr-1" />刷新
          </Button>
        </CardHeader>
        <CardContent className="space-y-3 text-sm">
          {task?.error ? (
            <div className="text-destructive text-xs">查询失败：{task.error}</div>
          ) : null}
          <Row label="状态">
            {!task ? (
              <span className="text-muted-foreground">查询中…</span>
            ) : task.installed ? (
              <span className="flex items-center gap-2">
                <Badge variant="success">已安装</Badge>
                <span className="text-muted-foreground">{task.state ?? ""}</span>
                {task.installed && !pointsToThisApp ? (
                  <Badge variant="warning">指向的不是本程序（可能是旧版脚本），建议重新安装</Badge>
                ) : null}
              </span>
            ) : (
              <Badge variant="outline">未安装</Badge>
            )}
          </Row>
          <Row label="动作">
            <span className="font-mono text-xs break-all">{task?.action ?? "—"}</span>
          </Row>
          <Row label="上次运行">
            <span>
              {task?.last_run_time ?? "—"}
              {task?.last_result !== null && task?.last_result !== undefined ? (
                <span className="ml-2 font-mono text-xs text-muted-foreground">
                  结果 {hex(task.last_result)}
                  {task.last_result === 0 ? "（成功）" : task.last_result === 267009 ? "（正在运行）" : ""}
                </span>
              ) : null}
            </span>
          </Row>
          <div className="flex gap-2 pt-2">
            <Button size="sm" onClick={installTask} disabled={taskBusy}>
              {task?.installed ? "重新安装 / 更新" : "安装"}
            </Button>
            <Button size="sm" variant="outline" onClick={runTaskNow} disabled={taskBusy || !task?.installed}>
              立即运行一次（自检）
            </Button>
            <Button size="sm" variant="destructive" onClick={uninstallTask} disabled={taskBusy || !task?.installed}>
              卸载
            </Button>
          </div>
          <div className="text-xs text-muted-foreground">
            自检时若最近 5 分钟内没有启动失败，任务只会写一条「nothing to do」日志并以 0 退出，这是正常的。
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-start gap-3">
      <div className="w-24 shrink-0 text-muted-foreground">{label}</div>
      <div className="min-w-0">{children}</div>
    </div>
  );
}
