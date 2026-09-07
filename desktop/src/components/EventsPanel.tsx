import { useEffect } from "react";
import { useStore } from "@/store";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { formatTime } from "@/lib/utils";
import { RefreshCw } from "lucide-react";

export function EventsPanel() {
  const { events, loadEvents } = useStore();

  useEffect(() => {
    loadEvents(120);
  }, []);

  return (
    <Card>
      <CardHeader className="flex flex-row items-start justify-between space-y-0">
        <div>
          <CardTitle>应用容器事件时间线</CardTitle>
          <CardDescription>
            Microsoft-Windows-AppModel-Runtime/Admin 中与 Claude 相关的事件。一次失败的更新典型序列是：
            217 销毁旧容器 → 210/211 新容器创建 → 215 ×2 创建容器失败 → 208 启动进程失败（0x80070020）。
          </CardDescription>
        </div>
        <Button size="sm" variant="outline" onClick={() => loadEvents(120)}>
          <RefreshCw className="h-4 w-4 mr-1" />刷新
        </Button>
      </CardHeader>
      <CardContent>
        {!events.length ? (
          <div className="text-sm text-muted-foreground">没有读到相关事件。</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead className="text-left text-xs uppercase text-muted-foreground">
                <tr>
                  <th className="py-2 pr-3">时间</th>
                  <th className="py-2 pr-3">事件</th>
                  <th className="py-2 pr-3">说明</th>
                  <th className="py-2 pr-3">版本</th>
                  <th className="py-2 pr-3">错误码</th>
                </tr>
              </thead>
              <tbody>
                {events.map((e) => (
                  <tr key={e.record_id} className="border-t">
                    <td className="py-1.5 pr-3 whitespace-nowrap">{formatTime(e.time)}</td>
                    <td className="py-1.5 pr-3">
                      <Badge
                        variant={
                          e.level === "error" ? "destructive" : e.level === "warning" ? "warning" : "secondary"
                        }
                      >
                        {e.event_id}
                      </Badge>
                    </td>
                    <td className="py-1.5 pr-3">{e.summary}</td>
                    <td className="py-1.5 pr-3 font-mono text-xs">
                      {e.package_full_name ? versionOf(e.package_full_name) : "—"}
                    </td>
                    <td className="py-1.5 pr-3 font-mono text-xs">{e.error_hex ?? ""}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function versionOf(fullName: string): string {
  return fullName.split("_")[1] ?? fullName;
}
