import { useEffect, useState } from "react";

export function formatTime(iso: string): string {
  try {
    const d = new Date(iso);
    const pad = (n: number) => n.toString().padStart(2, "0");
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  } catch {
    return iso;
  }
}

export function useAsync<T>(
  fn: () => Promise<T>,
  deps: unknown[] = [],
  options: { immediate?: boolean; onError?: (e: unknown) => void } = {}
) {
  const { immediate = true, onError } = options;
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(immediate);
  const [error, setError] = useState<unknown>(null);

  const run = async (silent = false) => {
    if (!silent) setLoading(true);
    setError(null);
    try {
      const res = await fn();
      setData(res);
      return res;
    } catch (e) {
      setError(e);
      onError?.(e);
      throw e;
    } finally {
      if (!silent) setLoading(false);
    }
  };

  useEffect(() => {
    if (immediate) run().catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  return { data, loading, error, refresh: run, setData };
}

export function cls(...arr: (string | false | null | undefined)[]): string {
  return arr.filter(Boolean).join(" ");
}

export function toast(message: string, kind: "info" | "success" | "error" = "info") {
  // 轻量版：使用浏览器原生alert + console；也可接入toast库
  const prefix = kind === "success" ? "[✓]" : kind === "error" ? "[✗]" : "[i]";
  console.log(`${prefix} ${message}`);
  if (kind === "error") {
    // 非阻断式显示，避免打断
    const el = document.createElement("div");
    el.textContent = message;
    el.style.cssText =
      "position:fixed;top:20px;right:20px;z-index:9999;background:#f38ba8;color:#111;padding:10px 16px;border-radius:8px;box-shadow:0 4px 12px rgba(0,0,0,0.5);font-family:sans-serif;max-width:420px;word-break:break-all;";
    document.body.appendChild(el);
    setTimeout(() => el.remove(), 5000);
  } else if (kind === "success") {
    const el = document.createElement("div");
    el.textContent = message;
    el.style.cssText =
      "position:fixed;top:20px;right:20px;z-index:9999;background:#a6e3a1;color:#111;padding:10px 16px;border-radius:8px;box-shadow:0 4px 12px rgba(0,0,0,0.5);font-family:sans-serif;max-width:420px;word-break:break-all;";
    document.body.appendChild(el);
    setTimeout(() => el.remove(), 3500);
  }
}

export function copyToClipboard(text: string) {
  navigator.clipboard?.writeText(text).catch(() => {
    const ta = document.createElement("textarea");
    ta.value = text;
    document.body.appendChild(ta);
    ta.select();
    document.execCommand("copy");
    ta.remove();
  });
}
