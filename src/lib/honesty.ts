/**
 * C1 — the live-versus-reconstructed split, the one number the whole frame is
 * about.
 *
 * The build already stores how each confirmed interval of work was captured:
 * `SessionSource` ∈ `timer` | `pomodoro` | `manual` | `recovered`. What it
 * never surfaced is the *ratio* — how much of a day's confirmed time was
 * captured while it happened versus filled in from memory afterwards. That
 * ratio is the product's success metric (≥90% live), and until it is on screen
 * it is a promise nobody can check.
 *
 *   live          = timer + pomodoro   (captured as it happened)
 *   reconstructed = manual             (asserted after the fact)
 *   recovered     = recovered          (a crash restored it — shown apart, not
 *                                        counted as either, because it is
 *                                        neither a clean live capture nor a
 *                                        memory fill)
 *
 * # Why this reads segments and not sessions
 *
 * The obvious implementation sums `SessionRow.elapsedSec` grouped by source.
 * It is wrong in a way that matters here more than anywhere else in the app: a
 * session row is the whole session, and the Day view is one date. A session
 * that runs across midnight belongs to two days; a session outranked by
 * confirmed life time (§7 precedence) is not counted as work at all. Summing
 * rows would put a percentage of a number the Day view never shows directly
 * beside the number it does — which is exactly the kind of quietly-wrong figure
 * this product exists to stop producing.
 *
 * So the input is `DaySegment[]`: intervals already clipped to the day and
 * already resolved to exactly one owner in Rust. The split is then a partition
 * of the same seconds `totals.confirmedWorkSec` is summed from, and the two
 * agree by construction rather than by luck. Nothing here decides anything —
 * it groups what the core already owns.
 */

import type { DaySegment, SessionSource } from "./types";

/** The frame's gate: ≥90% of confirmed work captured live. */
export const LIVE_TARGET = 0.9;

/** One interval of confirmed work, and how it was captured. */
export interface CaptureSlice {
  source: SessionSource;
  seconds: number;
}

export interface CaptureSplit {
  liveSec: number;
  reconstructedSec: number;
  recoveredSec: number;
  /** Every confirmed second, however it was captured. */
  confirmedSec: number;
  /** `null` when nothing is confirmed yet — an empty day, not a failed one. */
  livePct: number | null;
  /** Whether the ratio clears the gate. `false` while `livePct` is null. */
  meetsTarget: boolean;
}

/**
 * The confirmed-work intervals of a day, as `(source, seconds)` pairs.
 *
 * Segments whose owner is anything but work are skipped: life time, machine
 * observation, idle and gaps are not claims about work at all, so they belong
 * in neither half of a ratio about how work was captured.
 */
export function captureSlices(segments: DaySegment[]): CaptureSlice[] {
  const slices: CaptureSlice[] = [];
  for (const seg of segments) {
    if (seg.owner.kind !== "work") continue;
    slices.push({ source: seg.owner.source, seconds: Math.max(0, (seg.to - seg.from) / 1000) });
  }
  return slices;
}

/** Seconds grouped by how the time was captured. Pure; trivially testable. */
export function splitByCapture(slices: CaptureSlice[]): CaptureSplit {
  let liveSec = 0;
  let reconstructedSec = 0;
  let recoveredSec = 0;

  for (const slice of slices) {
    switch (slice.source) {
      case "timer":
      case "pomodoro":
        liveSec += slice.seconds;
        break;
      case "manual":
        reconstructedSec += slice.seconds;
        break;
      case "recovered":
        recoveredSec += slice.seconds;
        break;
    }
  }

  // The denominator is live + reconstructed only. Recovered time is real work,
  // but its provenance is the crash-recovery machine rather than a choice the
  // user made about honesty, so it does not belong on either side of the ratio.
  // It is still shown, because hiding it would leave a total that doesn't add up.
  const denom = liveSec + reconstructedSec;
  const livePct = denom > 0 ? liveSec / denom : null;

  return {
    liveSec,
    reconstructedSec,
    recoveredSec,
    confirmedSec: liveSec + reconstructedSec + recoveredSec,
    livePct,
    meetsTarget: livePct !== null && livePct >= LIVE_TARGET,
  };
}

/** The whole derivation, for a caller that just has the day's segments. */
export function captureSplit(segments: DaySegment[]): CaptureSplit {
  return splitByCapture(captureSlices(segments));
}
