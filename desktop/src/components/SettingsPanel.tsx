import type { ReactNode } from "react";
import { useStore } from "@/store";
import { api } from "@/api";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { UpdateChecker } from "@/components/UpdateChecker";
import type { AppConfig } from "@/types";

export function SettingsPanel() {
  const { config, saveConfig } = useStore();
  if (!config) return null;
  const cfg: AppConfig = config;

  function update<K extends keyof AppConfig>(key: K, value: AppConfig[K]) {
    saveConfig({ ...cfg, [key]: value });
  }

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle>关于与更新</CardTitle>
          <CardDescription>
            更新包来自本仓库的 GitHub Releases，带签名校验。启动后 6 小时内会静默检查一次；点按钮可手动检查。
          </CardDescription>
        </CardHeader>
        <CardContent>
          <UpdateChecker />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>修复策略</CardTitle>
          <CardDescription>检测到事件 208 且错误码为 0x80070020 时如何处理。</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <Row label="自动修复" description="关闭后只弹出确认对话框和系统通知，由你决定是否修复。">
            <Switch checked={cfg.auto_repair} onCheckedChange={(v) => update("auto_repair", v)} />
          </Row>
          <Row label="修复前等待（毫秒）" description="让失败的启动过程完全退出后再动手。">
            <NumberInput value={cfg.repair_delay_ms} min={0} max={60000} onChange={(v) => update("repair_delay_ms", v)} />
          </Row>
          <Row label="失败有效期（分钟）" description="轮询兜底只处理这么久以内的失败事件。">
            <NumberInput value={cfg.recent_window_minutes} min={1} max={120} onChange={(v) => update("recent_window_minutes", v)} />
          </Row>
          <Row label="15 分钟内最多修复次数" description="防止修复失败时无限循环。">
            <NumberInput value={cfg.max_attempts_per_15min} min={1} max={20} onChange={(v) => update("max_attempts_per_15min", v)} />
          </Row>
          <Row label="系统通知" description="修复成功、失败或需要确认时弹出 Windows 通知。">
            <Switch checked={cfg.notify} onCheckedChange={(v) => update("notify", v)} />
          </Row>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>监视</CardTitle>
          <CardDescription>
            失败事件通过事件日志订阅即时送达；轮询负责刷新状态，并在订阅不可用时兜底。
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <Row label="状态刷新间隔">
            <select
              className="h-8 rounded-md border border-input bg-transparent px-2 text-sm"
              value={cfg.poll_seconds}
              onChange={(e) => update("poll_seconds", Number(e.target.value))}
            >
              <option value={2}>2 秒</option>
              <option value={5}>5 秒</option>
              <option value={10}>10 秒</option>
              <option value={30}>30 秒</option>
              <option value={60}>1 分钟</option>
              <option value={0}>关闭轮询</option>
            </select>
          </Row>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>启动与托盘</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <Row label="登录时自动启动" description="以 --minimized 参数静默运行，仅在托盘显示。看门狗要常驻才有意义。">
            <Switch checked={cfg.autostart} onCheckedChange={(v) => update("autostart", v)} />
          </Row>
          <Row label="关闭按钮等于最小化到托盘">
            <Switch checked={cfg.close_to_tray} onCheckedChange={(v) => update("close_to_tray", v)} />
          </Row>
          <Row label="退出前确认">
            <Switch checked={cfg.confirm_exit} onCheckedChange={(v) => update("confirm_exit", v)} />
          </Row>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>高级</CardTitle>
          <CardDescription>一般不需要改。包家族名的哈希部分由 Anthropic 的签名证书决定，所有机器相同。</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <Row label="包家族 (PackageFamilyName)">
            <Input
              className="w-72 font-mono text-xs"
              value={cfg.package_family}
              onChange={(e) => update("package_family", e.target.value.trim())}
            />
          </Row>
          <Row label="应用 ID">
            <Input className="w-40 font-mono text-xs" value={cfg.app_id} onChange={(e) => update("app_id", e.target.value.trim())} />
          </Row>
          <Row label="日志等级" description="修改后下次启动生效。">
            <select
              className="h-8 rounded-md border border-input bg-transparent px-2 text-sm"
              value={cfg.log_level}
              onChange={(e) => update("log_level", e.target.value)}
            >
              <option value="trace">trace</option>
              <option value="debug">debug</option>
              <option value="info">info</option>
              <option value="warn">warn</option>
              <option value="error">error</option>
            </select>
          </Row>
          <Row label="日志目录">
            <Button size="sm" variant="outline" onClick={() => api.openLogDir()}>打开日志目录</Button>
          </Row>
        </CardContent>
      </Card>
    </div>
  );
}

function NumberInput({
  value,
  min,
  max,
  onChange,
}: {
  value: number;
  min: number;
  max: number;
  onChange: (v: number) => void;
}) {
  return (
    <Input
      type="number"
      className="w-32"
      min={min}
      max={max}
      value={value}
      onChange={(e) => {
        const n = Number(e.target.value);
        if (Number.isFinite(n)) onChange(Math.min(max, Math.max(min, n)));
      }}
    />
  );
}

function Row({
  label,
  description,
  children,
}: {
  label: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div>
        <div className="text-sm font-medium">{label}</div>
        {description ? <div className="text-xs text-muted-foreground">{description}</div> : null}
      </div>
      <div>{children}</div>
    </div>
  );
}
