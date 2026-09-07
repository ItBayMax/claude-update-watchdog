import { useEffect } from "react";
import { useStore } from "@/store";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { api } from "@/api";

export function LogsPanel() {
  const { logs, loadLogs } = useStore();

  useEffect(() => {
    loadLogs();
    const t = setInterval(loadLogs, 3000);
    return () => clearInterval(t);
  }, []);

  return (
    <Card>
      <CardHeader className="flex flex-row items-start justify-between space-y-0">
        <div>
          <CardTitle>运行日志</CardTitle>
          <CardDescription>每 3 秒刷新，显示当日日志尾部 256KB。每次修复的进程清单都在这里。</CardDescription>
        </div>
        <div className="flex gap-2">
          <Button size="sm" variant="outline" onClick={loadLogs}>手动刷新</Button>
          <Button size="sm" variant="outline" onClick={() => api.openLogDir()}>打开日志目录</Button>
        </div>
      </CardHeader>
      <CardContent>
        <pre className="max-h-[60vh] overflow-auto rounded-md bg-muted p-3 text-xs leading-relaxed font-mono whitespace-pre-wrap">
{logs || "（暂无日志）"}
        </pre>
      </CardContent>
    </Card>
  );
}
