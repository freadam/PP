/**
 * §5.6 — the signature.
 *
 * Two parallel traces: what you intended, and what you spent. v1 drew a fill
 * bar; a fill bar shows a ratio, and this product is about a *gap*, so the gap
 * has to be the visible object.
 *
 * The same encoding appears at three scales — 3px traces on a planner plate, a
 * 2px compact form in task rows, full-width bars in Reports. Learn it once,
 * read it everywhere. All three read their state from the same Rust-computed
 * `DriftState`, so they cannot disagree.
 */

import { useState } from "react";
import type { DriftState } from "../lib/types";
import { drift as fmtDrift, duration } from "../lib/format";

/** Bounded so the tail can never collide with the next block (§5.6). */
const MAX_TAIL_PX = 24;

export interface DriftProps {
  plannedSec: number;
  trackedSec: number;
  driftSec: number;
  state: DriftState;
  isPast?: boolean;
}

/** The redundancy table (§5.6): every state also has texture, a badge and words. */
export function driftLabel({ plannedSec, trackedSec, driftSec, state }: DriftProps): string {
  switch (state) {
    case "notStarted":
      return `planned ${duration(plannedSec)}, not started`;
    case "inProgress":
      return `tracked ${duration(trackedSec)} of ${duration(plannedSec)}`;
    case "onEstimate":
      return `tracked ${duration(trackedSec)} of ${duration(plannedSec)}`;
    case "overrun":
      return `${duration(driftSec)} over`;
    case "underrunPast":
      return `${duration(-driftSec)} under plan`;
  }
}

/** Vertical rail for a planner plate. `height` is the plate height in px. */
export function DriftRail({
  planned,
  tracked,
  driftSec,
  state,
  height,
  isPast = false,
  settleIndex = 0,
}: {
  planned: number;
  tracked: number;
  driftSec: number;
  state: DriftState;
  height: number;
  isPast?: boolean;
  settleIndex?: number;
}) {
  const [readout, setReadout] = useState(false);
  // The rail sits inside the plate's 1px borders.
  const inner = Math.max(0, height - 2);
  const ratio = planned > 0 ? Math.min(1, tracked / planned) : 0;
  const trackH = Math.round(ratio * inner);
  const overrun = Math.max(0, driftSec);
  const tailH =
    planned > 0 ? Math.min(Math.round((overrun / planned) * inner), MAX_TAIL_PX) : 0;

  return (
    <span
      className="rail rail-settle"
      data-state={state}
      data-past={isPast}
      style={{ ["--settle-delay" as string]: `${settleIndex * 12}ms` }}
    >
      <span className="plot" />
      {trackH > 0 && <span className="track" style={{ height: trackH }} />}
      {tailH > 0 && <span className="tail" style={{ height: tailH }} />}
      {/* Invisible 10px column spanning both traces; hover or focus reveals a readout. */}
      <span
        className="rail-hit"
        onMouseEnter={() => setReadout(true)}
        onMouseLeave={() => setReadout(false)}
        onFocus={() => setReadout(true)}
        onBlur={() => setReadout(false)}
        tabIndex={-1}
      />
      {readout && (
        <span className="rail-readout caption data">
          plotted {duration(planned)} · tracked {duration(tracked)}
          {driftSec !== 0 && <> · {fmtDrift(driftSec)}</>}
        </span>
      )}
    </span>
  );
}

/** Compact horizontal form for task rows — 40px wide, 2px traces. */
export function DriftRailCompact({
  planned,
  tracked,
  driftSec,
  state,
}: {
  planned: number;
  tracked: number;
  driftSec: number;
  state: DriftState;
}) {
  const width = 40;
  const ratio = planned > 0 ? Math.min(1, tracked / planned) : 0;
  const trackW = Math.round(ratio * width);
  const overrun = Math.max(0, driftSec);
  const tailW = planned > 0 ? Math.min(Math.round((overrun / planned) * width), 12) : 0;

  return (
    <span
      className="rail-compact"
      data-state={state}
      role="img"
      aria-label={driftLabel({ plannedSec: planned, trackedSec: tracked, driftSec, state })}
    >
      <span className="plot" />
      {trackW > 0 && <span className="track" style={{ width: trackW }} />}
      {tailW > 0 && <span className="tail" style={{ left: trackW, width: tailW }} />}
    </span>
  );
}

/** Full-width bar for Reports — the drift rail rotated horizontal (§3.6). */
export function DriftBar({
  planned,
  tracked,
  max,
}: {
  planned: number;
  tracked: number;
  max: number;
}) {
  const scale = max > 0 ? 100 / max : 0;
  const plotPct = Math.min(100, planned * scale);
  const trackPct = Math.min(100, Math.min(tracked, planned) * scale);
  const overPct = Math.min(100 - plotPct, Math.max(0, tracked - planned) * scale);
  const driftSec = tracked - planned;

  return (
    <span
      className="rail-bar"
      role="img"
      aria-label={`plotted ${duration(planned)}, tracked ${duration(tracked)}${
        driftSec === 0 ? "" : `, ${fmtDrift(driftSec)}`
      }`}
    >
      <span className="plot" style={{ left: 0, width: `${plotPct}%` }} />
      <span className="track" style={{ width: `${trackPct}%` }} />
      {overPct > 0 && (
        <span className="tail" style={{ left: `${plotPct}%`, width: `${overPct}%` }} />
      )}
    </span>
  );
}

/** `+14m` over, `−22m` under, in `micro` mono. Redundant with the texture. */
export function DriftBadge({ driftSec, state }: { driftSec: number; state: DriftState }) {
  if (state !== "overrun" && state !== "underrunPast") return null;
  return (
    <span className="micro badge-drift" data-under={state === "underrunPast"}>
      {fmtDrift(driftSec)}
    </span>
  );
}
