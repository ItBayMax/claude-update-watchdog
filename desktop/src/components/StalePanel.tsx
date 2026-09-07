import { useEffect, useState } from "react";
import { useStore } from "@/store";
import { api, IS_TAURI } from "@/api";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { formatBytes, formatTime } from "@/lib/utils";
import { ShieldAlert, Search, Trash2, X } from "lucide-react";

export function StalePanel() {
  const {
    stale,
    staleBusy,
    scanStale,
    removeStale,
    removeOutcomes,
    dismissRemoveOutcomes,
    elevated,
  } = useStore();
  const [includeDeleted, setIncludeDeleted] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());

  // In the browser demo (documentation screenshots) show a populated table right away.
  // Inside the desktop app the scan stays a deliberate click.
  useEffect(() => {
    if (!IS_TAURI && !stale) scanStale(false);
  }, []);

  const removable = (stale?.candidates ?? []).filter((c) => !c.protected && c.state === "exists");
  const chosen = removable.filter((c) => selected.has(c.name));
  const chosenBytes = chosen.reduce((n, c) => n + (c.size_bytes ?? 0), 0);

  function toggle(name: string) {
    const next = new Set(selected);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    setSelected(next);
  }
  function selectAll() {
    setSelected(new Set(removable.map((c) => c.name)));
  }

  async function restartAsAdmin() {
    try {
      const accepted = await api.relaunchAsAdmin();
      if (accepted) await api.quitApp();
    } catch (e) {
      console.warn("relaunchAsAdmin failed:", e);
    }
  }

  return (
    <div className="space-y-4">
      {elevated === false ? (
        <Card className="border-amber-500/40 bg-amber-500/5">
          <CardContent className="flex items-center justify-between gap-4 py-3">
            <div className="flex items-start gap-2 text-sm">
              <ShieldAlert className="h-5 w-5 text-amber-600 mt-0.5 flex-shrink-0" />
              <div>
                <div className="font-medium">删除残留目录需要管理员权限</div>
                <div className="text-xs text-muted-foreground">
                  WindowsApps 下的目录归 SYSTEM / TrustedInstaller 所有，普通用户只能扫描。
                  以管理员身份重启后即可删除；监视和修复功能不需要管理员。
                </div>
              </div>
            </div>
            <Button size="sm" onClick={restartAsAdmin}>以管理员身份重启</Button>
          </CardContent>
        </Card>
      ) : null}

      {removeOutcomes.length ? (
        <Card>
          <CardContent className="py-3">
            <div className="flex items-start justify-between gap-3">
              <div className="text-sm space-y-1">
                <div className="font-medium">
                  删除结果：成功 {removeOutcomes.filter((o) => o.removed).length}，失败{" "}
                  {removeOutcomes.filter((o) => !o.removed).length}，释放约{" "}
                  {formatBytes(removeOutcomes.reduce((n, o) => n + (o.freed_bytes ?? 0), 0))}
                </div>
                <ul className="text-xs font-mono text-muted-foreground space-y-0.5 max-h-32 overflow-auto">
                  {removeOutcomes.map((o) => (
                    <li key={o.name}>
                      {o.removed ? "✔" : "✖"} {o.name} {o.error ? `— ${o.error}` : ""}
                    </li>
                  ))}
                </ul>
              </div>
              <Button size="icon" variant="ghost" onClick={dismissRemoveOutcomes}>
                <X className="h-4 w-4" />
              </Button>
            </div>
          </CardContent>
        </Card>
      ) : null}

      <Card>
        <CardHeader className="flex flex-row items-start justify-between space-y-0">
          <div>
            <CardTitle>残留的历史版本目录</CardTitle>
            <CardDescription>
              1.300xx 之前的更新器在旧版本进程未退出时无法搬走旧目录，于是留下了孤儿目录。
              候选来自部署日志警告 1230（仓库里没有对应程序包的硬链接）和 WindowsApps 列表（若有权限）。
              已注册版本、正在运行的版本受保护；删除前还会核对目录内的 AppxManifest.xml。
            </CardDescription>
          </div>
          <div className="flex items-center gap-3">
            <label className="flex items-center gap-2 text-xs text-muted-foreground cursor-pointer">
              <Switch checked={includeDeleted} onCheckedChange={setIncludeDeleted} />
              <span>包含 Deleted 子目录</span>
            </label>
            <Button size="sm" variant="outline" onClick={() => scanStale(includeDeleted)} disabled={staleBusy}>
              <Search className="h-4 w-4 mr-1" />{staleBusy ? "处理中…" : "扫描"}
            </Button>
          </div>
        </CardHeader>
        <CardContent className="space-y-3">
          {stale ? (
            <div className="text-xs text-muted-foreground">
              扫描于 {formatTime(stale.scanned_at)} · {stale.windows_apps} ·{" "}
              {stale.listing_permitted ? "可列出 WindowsApps" : "无法列出 WindowsApps，依赖事件日志"} · 可删除{" "}
              {stale.removable_count} 个，表面体积 {formatBytes(stale.removable_bytes)}（跨版本硬链接被重复计算，实际释放略少）
            </div>
          ) : (
            <div className="text-sm text-muted-foreground">点击「扫描」开始。扫描是只读的。</div>
          )}

          {stale && stale.candidates.length ? (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead className="text-left text-xs uppercase text-muted-foreground">
                  <tr>
                    <th className="py-2 pr-3 w-8"></th>
                    <th className="py-2 pr-3">目录</th>
                    <th className="py-2 pr-3">状态</th>
                    <th className="py-2 pr-3">大小</th>
                    <th className="py-2 pr-3">来源</th>
                    <th className="py-2 pr-3">说明</th>
                  </tr>
                </thead>
                <tbody>
                  {stale.candidates.map((c) => {
                    const canPick = !c.protected && c.state === "exists";
                    return (
                      <tr key={c.name} className="border-t">
                        <td className="py-2 pr-3">
                          <input
                            type="checkbox"
                            disabled={!canPick}
                            checked={selected.has(c.name)}
                            onChange={() => toggle(c.name)}
                          />
                        </td>
                        <td className="py-2 pr-3 font-mono text-xs break-all">{c.name}</td>
                        <td className="py-2 pr-3">
                          {c.state === "exists" ? (
                            <Badge variant="secondary">存在</Badge>
                          ) : c.state === "denied" ? (
                            <Badge variant="warning">无权访问</Badge>
                          ) : (
                            <Badge variant="outline">已不存在</Badge>
                          )}
                        </td>
                        <td className="py-2 pr-3 whitespace-nowrap">{formatBytes(c.size_bytes)}</td>
                        <td className="py-2 pr-3 text-xs text-muted-foreground">{c.source}</td>
                        <td className="py-2 pr-3 text-xs">
                          {c.protected ? (
                            <Badge variant="warning">{c.reason ?? "受保护"}</Badge>
                          ) : (
                            <span className="text-muted-foreground">可删除</span>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          ) : stale ? (
            <div className="text-sm text-muted-foreground">没有发现残留目录。</div>
          ) : null}

          {stale && removable.length ? (
            <div className="flex items-center gap-2 pt-2">
              <Button size="sm" variant="outline" onClick={selectAll}>
                全选可删除 ({removable.length})
              </Button>
              <Button
                size="sm"
                variant="destructive"
                disabled={!chosen.length || staleBusy || elevated !== true}
                onClick={() => removeStale(chosen.map((c) => c.name))}
                title={elevated !== true ? "需要以管理员身份运行" : undefined}
              >
                <Trash2 className="h-4 w-4 mr-1" />
                删除选中 ({chosen.length}，约 {formatBytes(chosenBytes)})
              </Button>
            </div>
          ) : null}
        </CardContent>
      </Card>
    </div>
  );
}
