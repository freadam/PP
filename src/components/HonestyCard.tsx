/**
 * C1 — how much of the day was captured live, and how much was filled in later.
 *
 * The one number the whole frame is about. Everything it reports is derived in
 * `lib/honesty.ts` from the day's already-resolved segments, so it adds a card
 * rather than a query, and its denominator is the same set of seconds the Work
 * card beside it is summed from.
 *
 * It sits in the summary strip but it is **not** a seventh bucket: the six
 * duration cards partition the day and sum to it, and this one is a ratio over
 * one of them. That is why it is wider, carries a bar rather than a swatch, and
 * says what it is a percentage *of* in the line underneath — a card that looked
 * like the others would be read as a duration, and there is no worse place for
 * an ambiguous figure than the card about honesty.
 *
 * The gate is marked with a glyph as well as a colour (I3/M16, the same rule
 * the row states follow): the reading has to survive a greyscale screenshot and
 * a reader who cannot tell the two greens apart.
 */

import * as fmt from "../lib/format";
import { LIVE_TARGET, captureSplit } from "../lib/honesty";
import type { DaySegment } from "../lib/types";

export function HonestyCard({ segments }: { segments: DaySegment[] }) {
  const { liveSec, reconstructedSec, recoveredSec, livePct, meetsTarget } =
    captureSplit(segments);

  // Nothing confirmed yet — said plainly rather than rendered as a hollow 0%,
  // which reads as failure when it is really just an empty day.
  if (livePct === null && recoveredSec === 0) {
    return (
      <div className="card honesty" aria-label="Captured live: no confirmed work yet">
        <span className="micro">Captured live</span>
        <strong className="data" data-empty="true">
          —
        </strong>
        <span className="honesty-sub">no confirmed work yet</span>
      </div>
    );
  }

  const pct = livePct === null ? 0 : Math.round(livePct * 100);

  return (
    <div className="card honesty" aria-label={`Captured live: ${pct} percent of confirmed work`}>
      <span className="micro">Captured live</span>
      <strong className="data" data-met={meetsTarget}>
        {pct}%{meetsTarget ? " ✓" : ""}
      </strong>

      {/* One hairline bar, live against reconstructed, with the 90% gate
          marked — the same visual grammar as the day bar above it. */}
      <span
        className="honesty-bar"
        role="img"
        aria-label={`${fmt.duration(liveSec)} live, ${fmt.duration(reconstructedSec)} from memory. Target ${Math.round(LIVE_TARGET * 100)} percent.`}
      >
        <i className="honesty-live" style={{ width: `${pct}%` }} />
        <i className="honesty-gate" style={{ left: `${LIVE_TARGET * 100}%` }} />
      </span>

      <span className="honesty-sub">
        <span className="data">{fmt.duration(liveSec)}</span> live
        <span className="honesty-dot" aria-hidden="true">
          ·
        </span>
        <span className="data">{fmt.duration(reconstructedSec)}</span> from memory
        {recoveredSec > 0 && (
          <>
            <span className="honesty-dot" aria-hidden="true">
              ·
            </span>
            {/* In neither half of the ratio, and never hidden either: a total
                that doesn't add up is its own kind of dishonesty. */}
            <span className="honesty-recovered">
              <span className="data">{fmt.duration(recoveredSec)}</span> recovered
            </span>
          </>
        )}
      </span>
    </div>
  );
}
