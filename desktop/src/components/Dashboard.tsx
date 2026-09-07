import type { ReactNode } from "react";
import { useStore } from "@/store";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { formatTime, TRIGGER_LABEL } from "@/lib/utils";
import { AlertTriangle, CheckCircle2, Pause, Play, XCircle } from "lucide-react";

export function Dashboard() {
  const {
    status,
    history,
    repairing,
    repair,
    schedulerState,
    pauseScheduler,
    resumeScheduler,
    clearHistory,
    launchClaude,
  } = useStore();

  const s = status;
  const failure = s?.recent_failure ?? s?.last_failure ?? null;
  const failureIsRecent = !!s?.recent_failure;

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <Card>
          <CardHeader className="flex flex-row items-start justify-between space-y-0">
            <div>
              <CardTitle>当前状态</CardTitle>
              <CardDescription>
                {s ? `上次检查 ${formatTime(s.checked_at)}` : "尚未检查"}
              </CardDescription>
            </div>
            <div className="flex items-center gap-2">
              {schedulerState === "running" ? (
                <>
                  <Badge variant="success">监视中</Badge>
                  <Button size="sm" variant="outline" onClick={pauseScheduler}>
                    <Pause className="h-4 w-4 mr-1" />暂停
                  </Button>
                </>
              ) : schedulerState === "paused" ? (
                <>
                  <Badge variant="warning">已暂停</Badge>
                  <Button size="sm" onClick={resumeScheduler}>
                    <Play className="h-4 w-4 mr-1" />恢复
                  </Button>
                </>
              ) : (
                <Badge variant="outline">监视已关闭</Badge>
              )}
            </div>
          </CardHeader>
          <CardContent className="space-y-2 text-sm">
            <Row label="已注册版本">
              {s?.current_version ? (
                <span className="font-mono">{s.current_version}</span>
              ) : (
                <span className="text-muted-foreground">未找到（当前用户未安装 Claude 桌面版）</span>
              )}
            </Row>
            <Row label="AUMID">
              <span className="font-mono text-xs">{s?.aumid ?? "—"}</span>
            </Row>
            <Row label="Claude 进程">
              {s ? (
                <span>
                  {s.processes.length} 个
                  {s.orphan_count > 0 ? (
                    <Badge variant="warning" className="ml-2">
                      {s.orphan_count} 个属于旧版本
                    </Badge>
                  ) : null}
                </span>
              ) : (
                "—"
              )}
            </Row>
            <Row label="无包身份的 claude 进程">
              <span className="text-muted-foreground">
                {s ? `${s.other_claude_processes.length} 个（Claude Code CLI 等，不会被处置）` : "—"}
              </span>
            </Row>
            {s && s.services.length ? (
              <Row label="打包服务进程">
                <span className="text-muted-foreground">
                  {s.services.length} 个（cowork-svc 等，会话 0，由 Windows 管理，不会被处置）
                </span>
              </Row>
            ) : null}
            <Row label="运行身份">
              {s?.elevated ? (
                <Badge variant="secondary">管理员</Badge>
              ) : (
                <span className="text-muted-foreground">普通用户（监视和修复不需要管理员）</span>
              )}
            </Row>
            <Row label="官方策略 disableAutoUpdates">
              {s?.policy_disable_auto_updates === true ? (
                <Badge variant="secondary">已启用，Claude 不再自动更新</Badge>
              ) : s?.policy_disable_auto_updates === false ? (
                <span className="text-muted-foreground">已设置为关闭</span>
              ) : (
                <span className="text-muted-foreground">未设置</span>
              )}
            </Row>
            <div className="pt-2 flex gap-2">
              <Button size="sm" variant="outline" onClick={launchClaude}>
                启动 Claude
              </Button>
            </div>
          </CardContent>
        </Card>

        <Card className={failureIsRecent ? "border-destructive/50" : undefined}>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              {failureIsRecent ? (
                <XCircle className="h-5 w-5 text-destructive" />
              ) : failure ? (
                <AlertTriangle className="h-5 w-5 text-amber-500" />
              ) : (
                <CheckCircle2 className="h-5 w-5 text-emerald-500" />
              )}
              {failureIsRecent ? "最近几分钟内有启动失败" : failure ? "最后一次启动失败" : "没有启动失败记录"}
            </CardTitle>
            <CardDescription>
              来源：事件日志 Microsoft-Windows-AppModel-Runtime/Admin，事件 208
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-2 text-sm">
            {failure ? (
              <>
                <Row label="时间">{formatTime(failure.time)}</Row>
                <Row label="尝试启动的版本">
                  <span className="font-mono text-xs">{failure.package_full_name}</span>
                </Row>
                <Row label="错误码">
                  <span className="font-mono">{failure.error_hex}</span>
                  {failure.error_code === 0x80070020 ? (
                    <span className="text-muted-foreground ml-2">
                      ERROR_SHARING_VIOLATION，旧版本进程仍在运行
                    </span>
                  ) : null}
                </Row>
                {failureIsRecent ? (
                  <Row label="处理状态">
                    {s?.recent_failure_handled ? (
                      <Badge variant="secondary">已处理</Badge>
                    ) : (
                      <Badge variant="destructive">未处理</Badge>
                    )}
                  </Row>
                ) : null}
                <div className="pt-2 flex gap-2">
                  <Button
                    size="sm"
                    variant={failureIsRecent ? "destructive" : "outline"}
                    onClick={() => repair(false)}
                    disabled={repairing}
                  >
                    {repairing ? "修复中…" : "结束残留进程并重启 Claude"}
                  </Button>
                  <Button size="sm" variant="outline" onClick={() => repair(true)} disabled={repairing}>
                    预演
                  </Button>
                </div>
              </>
            ) : (
              <div className="text-muted-foreground">
                事件日志里没有 Claude 的启动失败记录。看门狗会在失败发生时自动处理。
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader className="flex flex-row items-center justify-between space-y-0">
          <div>
            <CardTitle>修复记录</CardTitle>
            <CardDescription>最近 100 次，包含预演。事件触发的修复会在这里留下证据。</CardDescription>
          </div>
          <Button size="sm" variant="outline" onClick={clearHistory} disabled={!history.length}>
            清空
          </Button>
        </CardHeader>
        <CardContent>
          {!history.length ? (
            <div className="text-sm text-muted-foreground">尚无修复记录。</div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead className="text-left text-xs uppercase text-muted-foreground">
                  <tr>
                    <th className="py-2 pr-3">时间</th>
                    <th className="py-2 pr-3">触发</th>
                    <th className="py-2 pr-3">结果</th>
                    <th className="py-2 pr-3">进程</th>
                    <th className="py-2 pr-3">说明</th>
                  </tr>
                </thead>
                <tbody>
                  {history.map((r) => (
                    <tr key={r.id} className="border-t align-top">
                      <td className="py-2 pr-3 whitespace-nowrap">{formatTime(r.started_at)}</td>
                      <td className="py-2 pr-3">
                        <Badge variant="secondary">{TRIGGER_LABEL[r.trigger] ?? r.trigger}</Badge>
                        {r.dry_run ? <Badge variant="outline" className="ml-1">预演</Badge> : null}
                      </td>
                      <td className="py-2 pr-3">
                        {r.dry_run ? (
                          <Badge variant="outline">—</Badge>
                        ) : r.success ? (
                          <Badge variant="success">成功</Badge>
                        ) : (
                          <Badge variant="destructive">失败</Badge>
                        )}
                      </td>
                      <td className="py-2 pr-3 whitespace-nowrap">
                        {r.dry_run
                          ? `${r.planned.length} 个`
                          : `${r.killed.filter((k) => k.killed).length} / ${r.planned.length} 个`}
                      </td>
                      <td className="py-2 pr-3 text-muted-foreground">{r.message}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-start gap-3">
      <div className="w-44 shrink-0 text-muted-foreground">{label}</div>
      <div className="min-w-0 break-all">{children}</div>
    </div>
  );
}
