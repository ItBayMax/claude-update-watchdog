import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function formatTime(iso?: string | null): string {
  if (!iso) return "—";
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}

export function formatBytes(n?: number | null): string {
  if (n === null || n === undefined) return "n/a";
  if (n >= 1024 ** 3) return `${(n / 1024 ** 3).toFixed(2)} GB`;
  if (n >= 1024 ** 2) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  if (n >= 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${n} B`;
}

export function hex(n?: number | null): string {
  if (n === null || n === undefined) return "—";
  return "0x" + (n >>> 0).toString(16).toUpperCase().padStart(8, "0");
}

export const TRIGGER_LABEL: Record<string, string> = {
  event: "事件触发",
  poll: "轮询发现",
  manual: "手动",
  cli: "命令行",
  task: "计划任务",
};
