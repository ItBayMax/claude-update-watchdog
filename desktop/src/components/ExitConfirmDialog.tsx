import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";

export function ExitConfirmDialog({
  open,
  onClose,
  onConfirm,
  confirmExit,
}: {
  open: boolean;
  onClose: () => void;
  onConfirm: () => void;
  confirmExit: boolean;
}) {
  if (open && !confirmExit) {
    onConfirm();
    return null;
  }
  return (
    <Dialog open={open} onOpenChange={(v) => (!v ? onClose() : null)}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>确认退出 Claude Watchdog？</DialogTitle>
          <DialogDescription>
            退出后不再监视 Claude 的启动失败。如果已安装计划任务兜底，失败仍会被无界面修复；否则建议最小化到托盘继续运行。
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>取消</Button>
          <Button variant="destructive" onClick={onConfirm}>退出</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
