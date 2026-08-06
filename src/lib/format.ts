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

/**
 * The estimate scale. A fixed ladder rather than free text, at the product
 * owner's call — note this reverses spec §1.5, which replaced a dropdown with
 * a parser because a dropdown contradicts the `~45m` capture token. The
 * contradiction is real, so `estimateOptions` keeps any off-ladder value the
 * parser produced as an extra rung: choosing from the list is easy, and no
 * existing estimate is silently rounded away.
 *
 * `Rollover` is the top of the ladder — work that doesn't fit one sitting —
 * and is a different state from "not estimated yet", which is why it is a
 * column of its own rather than a null.
 */
export const ESTIMATE_LADDER_SEC = [
  1800, 3600, 5400, 7200, 9000, 10800, 12600, 14400,
] as const;

export interface EstimateOption {
  /** `""` none · `"rollover"` · otherwise the duration in seconds, as a string. */
  value: string;
  label: string;
  /** True for a value that came from the task rather than the ladder. */
  offLadder?: boolean;
}

export function estimateLabel(sec: number): string {
  const hours = sec / 3600;
  if (sec < 3600) return `${Math.round(sec / 60)} min`;
  return `${Number.isInteger(hours) ? hours : hours.toFixed(1)} ${hours === 1 ? "Hr" : "Hrs"}`;
}

export function estimateOptions(current: number | null): EstimateOption[] {
  const ladder: EstimateOption[] = ESTIMATE_LADDER_SEC.map((sec) => ({
    value: String(sec),
    label: estimateLabel(sec),
  }));
  // A task captured as `~45m` keeps its 45m rather than being rounded to the
  // nearest rung the moment someone opens the dropdown.
  if (current != null && !ESTIMATE_LADDER_SEC.includes(current as never)) {
    ladder.push({ value: String(current), label: estimateLabel(current), offLadder: true });
    ladder.sort((a, b) => Number(a.value) - Number(b.value));
  }
  return [
    { value: "", label: "No estimate" },
    ...ladder,
    { value: "rollover", label: "Rollover" },
  ];
}

/**
 * A keyboard hint, spelled the way the platform spells it.
 *
 * The commands registry writes `⌘K` because that reads clearly in source. The
 * MVP ships on **Windows only** (§5.1), where a `⌘` on a button is not a
 * stylistic choice — it is a key the user does not have. So the glyph is
 * rewritten at the point it is shown, in one place, rather than every command
 * carrying two spellings.
 *
 * `⇧` and `⌥` are left alone: Shift is Shift, and Alt is close enough that
 * spelling it out costs more room than it buys.
 */
export function keys(hint: string): string {
  return isApple() ? hint : hint.replace(/⌘/g, "Ctrl ");
}

function isApple(): boolean {
  if (typeof navigator === "undefined") return false;
  return /mac|iphone|ipad/i.test(navigator.platform || navigator.userAgent);
}
