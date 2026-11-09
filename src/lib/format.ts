import { i18n } from "./i18n.svelte";

/** Compact interval label: 1m, 10m, 3h, 4d, 2.5mo, 1.2y. */
export function intervalLabel(secs: number, days: number | null): string {
  if (days !== null) {
    if (days < 30) return `${days}${i18n.lang === "zh" ? "天" : "d"}`;
    if (days < 365) {
      const mo = Math.round((days / 30) * 10) / 10;
      return i18n.lang === "zh" ? `${mo}个月` : `${mo}mo`;
    }
    const y = Math.round((days / 365) * 10) / 10;
    return i18n.lang === "zh" ? `${y}年` : `${y}y`;
  }
  const mins = Math.max(1, Math.round(secs / 60));
  if (mins < 60) return i18n.lang === "zh" ? `${mins}分钟` : `${mins}m`;
  const h = Math.round(mins / 60);
  return i18n.lang === "zh" ? `${h}小时` : `${h}h`;
}

/** Human "next review" text relative to now (unix seconds). */
export function dueLabel(due: number | null): string {
  if (due === null) return "—";
  const now = Date.now() / 1000;
  const diff = due - now;
  const zh = i18n.lang === "zh";
  if (diff <= 0) return zh ? "现在" : "now";
  const startOfToday = new Date();
  startOfToday.setHours(0, 0, 0, 0);
  const dayDiff = Math.floor((due * 1000 - startOfToday.getTime()) / 86_400_000);
  if (dayDiff === 0) {
    const mins = Math.round(diff / 60);
    if (mins < 60) return zh ? `${mins}分钟后` : `in ${mins}m`;
    return zh ? "今天" : "today";
  }
  if (dayDiff === 1) return zh ? "明天" : "tomorrow";
  if (dayDiff < 60) return zh ? `${dayDiff}天后` : `in ${dayDiff}d`;
  return new Date(due * 1000).toLocaleDateString(zh ? "zh-CN" : undefined);
}

export function durationLabel(ms: number): string {
  const totalMin = Math.round(ms / 60000);
  const zh = i18n.lang === "zh";
  if (totalMin < 1) {
    const s = Math.round(ms / 1000);
    return zh ? `${s}秒` : `${s}s`;
  }
  if (totalMin < 60) return zh ? `${totalMin}分钟` : `${totalMin} min`;
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  return zh ? `${h}小时${m}分` : `${h}h ${m}m`;
}

export function pct(x: number | null): string {
  return x === null ? "—" : `${Math.round(x * 100)}%`;
}

export function fmtNum(n: number): string {
  return n.toLocaleString(i18n.lang === "zh" ? "zh-CN" : undefined);
}
