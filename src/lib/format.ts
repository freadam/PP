/** Formatting only. Anything that decides something lives in Rust. */

import type { LocalDate, Millis } from "./types";

/** `74m` → `1h 14m`. The unit everywhere is seconds (§6.1 rule 3). */
export function duration(sec: number): string {
  const s = Math.abs(Math.round(sec));
  const h = Math.floor(s / 3600);
  const m = Math.round((s % 3600) / 60);
  if (h === 0) return `${m}m`;
  if (m === 0) return `${h}h`;
  return `${h}h ${m}m`;
}

/** Signed, for drift badges: `+14m`, `−22m`. Uses a real minus sign. */
export function drift(sec: number): string {
  if (sec === 0) return "0m";
  return `${sec > 0 ? "+" : "−"}${duration(sec)}`;
}

/** Seconds resolution, used only where seconds are displayed (§6.9). */
export function stopwatch(sec: number): string {
  const s = Math.max(0, Math.round(sec));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = s % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(r)}` : `${pad(m)}:${pad(r)}`;
}

let hour12 = false;
export function setHour12(value: boolean) {
  hour12 = value;
}

export function clock(at: Millis): string {
  return new Date(at).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    hour12,
  });
}

export function clockRange(from: Millis, durationSec: number): string {
  return `${clock(from)}–${clock(from + durationSec * 1000)}`;
}

export function weekdayShort(date: LocalDate): string {
  return new Date(`${date}T12:00:00`).toLocaleDateString(undefined, { weekday: "short" });
}

export function dayOfMonth(date: LocalDate): string {
  return String(new Date(`${date}T12:00:00`).getDate());
}

export function rangeLabel(from: LocalDate, to: LocalDate): string {
  const a = new Date(`${from}T12:00:00`);
  const b = new Date(`${to}T12:00:00`);
  const opts: Intl.DateTimeFormatOptions = { day: "numeric", month: "short" };
  if (from === to) {
    return a.toLocaleDateString(undefined, { weekday: "long", ...opts });
  }
  return `${a.toLocaleDateString(undefined, opts)} – ${b.toLocaleDateString(undefined, opts)}`;
}

export function longDate(date: LocalDate): string {
  return new Date(`${date}T12:00:00`).toLocaleDateString(undefined, {
    weekday: "long",
    day: "numeric",
    month: "long",
  });
}

/** Local `YYYY-MM-DD` — never `toISOString`, which is UTC and moves the day. */
export function toLocalDate(d: Date): LocalDate {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function addDays(date: LocalDate, days: number): LocalDate {
  const d = new Date(`${date}T12:00:00`);
  d.setDate(d.getDate() + days);
  return toLocalDate(d);
}

/** Monday-based, matching `time::week_start` in Rust. */
export function weekStart(date: LocalDate): LocalDate {
  const d = new Date(`${date}T12:00:00`);
  return addDays(date, -((d.getDay() + 6) % 7));
}

export function today(): LocalDate {
  return toLocalDate(new Date());
}

export const tz = (): string => Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";

/** Minutes since local midnight — the planner's y-axis. */
export function minutesIntoDay(at: Millis): number {
  const d = new Date(at);
  return d.getHours() * 60 + d.getMinutes();
}

export function priorityLabel(p: number): string {
  return ["", "low", "medium", "high"][p] ?? "";
}
