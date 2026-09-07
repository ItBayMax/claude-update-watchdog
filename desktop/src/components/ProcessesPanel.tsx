import { useStore } from "@/store";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { formatTime } from "@/lib/utils";
import { RefreshCw, Skull } from "lucide-react";

export function ProcessesPanel() {
  const { status, checking, refreshStatus, killPids, launchClaude } = useStore();
  const procs = status?.processes ?? [];
  const orphans = procs.filter((p) => p.orphan);
  const services = status?.services ?? [];
  const others = status?.other_claude_processes ?? [];

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader className="flex flex-row items-start justify-between space-y-0">
          <div>
            <CardTitle>带 Claude 包身份的进程</CardTitle>
            <CardDescription>
              通过 GetPackageFullName 识别。标记为「旧版本」的进程就是更新后让新版本无法启动的残留。
            </CardDescription>
          </div>
          <div className="flex items-center gap-2">
            <Button size="sm" variant="outline" onClick={refreshStatus} disabled={checking}>
              <RefreshCw className="h-4 w-4 mr-1" />刷新
            </Button>
            <Button
              size="sm"
              variant="destructive"
              disabled={!orphans.length}
              onClick={() => killPids(orphans.map((p) => p.pid))}
            >
              <Skull className="h-4 w-4 mr-1" />结束全部旧版本残留 ({orphans.length})
            </Button>
            <Button size="sm" variant="outline" onClick={launchClaude}>
              启动 Claude
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          {!procs.length ? (
            <div className="text-sm text-muted-foreground">当前没有 Claude 进程在运行。</div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead className="text-left text-xs uppercase text-muted-foreground">
                  <tr>
                    <th className="py-2 pr-3">PID</th>
                    <th className="py-2 pr-3">名称</th>
                    <th className="py-2 pr-3">版本</th>
                    <th className="py-2 pr-3">启动时间</th>
                    <th className="py-2 pr-3">路径</th>
                    <th className="py-2 pr-3 text-right">操作</th>
                  </tr>
                </thead>
                <tbody>
                  {procs.map((p) => (
                    <tr key={p.pid} className={"border-t " + (p.orphan ? "bg-destructive/5" : "")}>
                      <td className="py-2 pr-3 font-mono">{p.pid}</td>
                      <td className="py-2 pr-3 font-medium">{p.name}</td>
                      <td className="py-2 pr-3">
                        <span className="font-mono">{p.version}</span>
                        {p.orphan ? (
                          <Badge variant="destructive" className="ml-2">旧版本</Badge>
                        ) : (
                          <Badge variant="success" className="ml-2">当前</Badge>
                        )}
                      </td>
                      <td className="py-2 pr-3 whitespace-nowrap">{formatTime(p.started_at)}</td>
                      <td className="py-2 pr-3 text-xs text-muted-foreground break-all">{p.exe ?? "—"}</td>
                      <td className="py-2 pr-3 text-right">
                        <Button size="sm" variant="destructive" onClick={() => killPids([p.pid])}>
                          结束
                        </Button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>

      {services.length ? (
        <Card>
          <CardHeader>
            <CardTitle>打包服务 / 其他会话的进程（不处置）</CardTitle>
            <CardDescription>
              带 Claude 包身份但不在当前用户会话里，例如以 SYSTEM 身份运行在会话 0 的 CoworkVMService（cowork-svc.exe）。
              它不属于你的应用容器，更新时由 Windows 部署服务自行终止；只有以管理员运行时才看得到。
            </CardDescription>
          </CardHeader>
          <CardContent>
            <ul className="text-xs font-mono space-y-1">
              {services.map((p) => (
                <li key={p.pid} className="text-muted-foreground">
                  [{p.pid}] {p.name} — {p.version} — 会话 {p.session_id ?? "?"} — {p.exe ?? "—"}
                </li>
              ))}
            </ul>
          </CardContent>
        </Card>
      ) : null}

      <Card>
        <CardHeader>
          <CardTitle>无包身份的 claude 进程</CardTitle>
          <CardDescription>
            Claude Code 命令行、它派生的终端等。它们不在应用容器里，不会阻塞更新，看门狗永远不会结束它们。
          </CardDescription>
        </CardHeader>
        <CardContent>
          {!others.length ? (
            <div className="text-sm text-muted-foreground">无。</div>
          ) : (
            <ul className="text-xs font-mono space-y-1">
              {others.map((p) => (
                <li key={p.pid} className="text-muted-foreground">
                  [{p.pid}] {p.name} — {p.exe ?? "—"}
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
