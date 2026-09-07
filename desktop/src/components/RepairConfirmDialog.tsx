import { useEffect } from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { useStore } from "@/store";
import { formatTime } from "@/lib/utils";
import type { LaunchFailure } from "@/types";

export function RepairConfirmDialog({
  open,
  failure,
  onConfirm,
  onCancel,
}: {
  open: boolean;
  failure: LaunchFailure | null;
  onConfirm: () => void | Promise<void>;
  onCancel: () => void;
}) {
  const { status, refreshStatus } = useStore();
  useEffect(() => {
    if (open) refreshStatus();
  }, [open]);
  const procs = status?.processes ?? [];

  return (
    <Dialog open={open} onOpenChange={(v) => (!v ? onCancel() : null)}>
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <DialogTitle>⚠ Claude 启动失败，需要修复</DialogTitle>
          <DialogDescription>
            新版本无法创建应用容器，因为旧版本的进程还在运行。修复会结束下列进程并重新启动 Claude。
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-3 text-sm">
          {failure ? (
            <div className="text-xs text-muted-foreground">
              {formatTime(failure.time)} · {failure.package_full_name} · {failure.error_hex}
            </div>
          ) : null}
          <div>
            <div className="text-xs text-muted-foreground mb-1">将被结束的进程 ({procs.length})</div>
            <ul className="text-xs font-mono max-h-40 overflow-auto border rounded p-2 bg-muted">
              {procs.length ? (
                procs.map((p) => (
                  <li key={p.pid}>
                    [{p.pid}] {p.name} — {p.version}{" "}
                    {p.orphan ? <Badge variant="destructive" className="ml-1">旧版本</Badge> : null}
                  </li>
                ))
              ) : (
                <li className="text-muted-foreground">无</li>
              )}
            </ul>
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onCancel}>稍后</Button>
          <Button variant="destructive" onClick={() => onConfirm()}>立即修复</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
