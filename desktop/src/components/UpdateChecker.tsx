import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { IS_TAURI } from "@/api";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { CheckCircle2, Download, RefreshCw } from "lucide-react";

type State =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "no-update" }
  | { kind: "available"; update: Update }
  | { kind: "downloading"; progress: number }
  | { kind: "installed" }
  | { kind: "error"; message: string };

const LAST_CHECK_KEY = "claudewatchdog.lastUpdateCheckAt";
const SIX_HOURS = 6 * 60 * 60 * 1000;

/**
 * Version row + "check for updates" button, with a dialog when an update is
 * found. Auto-checks silently on mount, at most once every 6 hours. Updates
 * come from this repository's GitHub Releases (latest.json + signed NSIS
 * installer); the signature is verified against the pubkey in tauri.conf.json.
 */
export function UpdateChecker({ autoCheck = true }: { autoCheck?: boolean }) {
  const [version, setVersion] = useState<string>("…");
  const [state, setState] = useState<State>({ kind: "idle" });
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!IS_TAURI) {
      setVersion("demo");
      return;
    }
    getVersion().then(setVersion).catch(() => setVersion("?"));
  }, []);

  useEffect(() => {
    if (!autoCheck || !IS_TAURI) return;
    let last = 0;
    try {
      last = Number(localStorage.getItem(LAST_CHECK_KEY) || 0);
    } catch {
      /* ignore */
    }
    if (Date.now() - last < SIX_HOURS) return;
    runCheck(true).catch(() => {});
  }, [autoCheck]);

  async function runCheck(silentIfNone: boolean) {
    if (!IS_TAURI) {
      setState({ kind: "error", message: "演示模式下没有更新通道。" });
      setOpen(true);
      return;
    }
    setState({ kind: "checking" });
    try {
      const update = await check();
      try {
        localStorage.setItem(LAST_CHECK_KEY, String(Date.now()));
      } catch {
        /* ignore */
      }
      if (!update) {
        setState({ kind: "no-update" });
        if (!silentIfNone) setOpen(true);
        return;
      }
      setState({ kind: "available", update });
      setOpen(true);
    } catch (e) {
      setState({ kind: "error", message: String(e) });
      if (!silentIfNone) setOpen(true);
    }
  }

  async function installNow(update: Update) {
    setState({ kind: "downloading", progress: 0 });
    try {
      let downloaded = 0;
      let total = 0;
      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            total = event.data.contentLength ?? 0;
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            setState({ kind: "downloading", progress: total > 0 ? downloaded / total : 0 });
            break;
          case "Finished":
            setState({ kind: "installed" });
            break;
        }
      });
      await relaunch();
    } catch (e) {
      setState({ kind: "error", message: String(e) });
    }
  }

  return (
    <>
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        <Badge variant="outline" className="font-mono">v{version}</Badge>
        <Button
          size="sm"
          variant="ghost"
          className="h-7 px-2"
          onClick={() => runCheck(false)}
          disabled={state.kind === "checking" || state.kind === "downloading"}
        >
          <RefreshCw className={"h-3.5 w-3.5 mr-1 " + (state.kind === "checking" ? "animate-spin" : "")} />
          {state.kind === "checking" ? "检查中…" : "检查更新"}
        </Button>
      </div>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{titleFor(state)}</DialogTitle>
            <DialogDescription className="whitespace-pre-wrap">{descFor(state, version)}</DialogDescription>
          </DialogHeader>
          {state.kind === "downloading" ? (
            <div className="w-full">
              <div className="h-2 w-full rounded-full bg-muted overflow-hidden">
                <div className="h-full bg-primary transition-[width]" style={{ width: `${Math.round(state.progress * 100)}%` }} />
              </div>
              <div className="mt-1 text-xs text-muted-foreground text-right">{Math.round(state.progress * 100)}%</div>
            </div>
          ) : null}
          <DialogFooter>
            {state.kind === "available" ? (
              <>
                <Button variant="outline" onClick={() => setOpen(false)}>稍后</Button>
                <Button onClick={() => installNow(state.update)}>
                  <Download className="h-4 w-4 mr-1" />下载并安装
                </Button>
              </>
            ) : null}
            {state.kind === "installed" ? (
              <Button onClick={() => relaunch()}>
                <CheckCircle2 className="h-4 w-4 mr-1" />立即重启
              </Button>
            ) : null}
            {state.kind === "no-update" || state.kind === "error" ? (
              <Button variant="outline" onClick={() => setOpen(false)}>关闭</Button>
            ) : null}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

function titleFor(s: State): string {
  switch (s.kind) {
    case "checking":
      return "检查更新中…";
    case "no-update":
      return "已是最新版本";
    case "available":
      return `发现新版本 v${s.update.version}`;
    case "downloading":
      return "下载并安装中…";
    case "installed":
      return "更新已下载，准备重启";
    case "error":
      return "检查 / 安装失败";
    default:
      return "更新";
  }
}

function descFor(s: State, version: string): string {
  switch (s.kind) {
    case "no-update":
      return `当前 v${version} 已经是最新可用版本。`;
    case "available": {
      const notes = (s.update.body || "").trim();
      return notes ? `当前 v${version} → 新版本 v${s.update.version}\n\n${notes}` : `当前 v${version} → 新版本 v${s.update.version}`;
    }
    case "downloading":
      return "正在从 GitHub Releases 下载新版本…";
    case "installed":
      return "新版本已就绪，重启后生效。";
    case "error":
      return s.message;
    default:
      return "";
  }
}
