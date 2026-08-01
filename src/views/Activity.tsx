/**
 * Activity (§3.5, P2) — "was I actually doing the thing the timer said?"
 *
 * Three panels, and the order is the argument:
 *
 *   1. **Against the plan.** Each block on the day, with the apps that were
 *      actually in front of you while it ran. This is the only place in Fruit
 *      where an intention meets an *observation* rather than another record of
 *      itself, and it is the reason the feature exists at all.
 *   2. **Where the day went.** Per-app totals, longest first.
 *   3. **The day itself**, on the Planner's exact time axis and hour height, so
 *      a block and the app usage beneath it read as one picture rather than as
 *      two charts that happen to be about the same hours.
 *
 * Off by default. When it is off this screen says so and links to the switch —
 * it never shows an empty chart that implies the data is merely missing.
 */

import { useEffect, useMemo, useRef } from "react";
import { useApp } from "../store/app";
import * as fmt from "../lib/format";
import type { ActivityDay, AppTotal } from "../lib/types";
import { Empty } from "../components/chrome";

/**
 * Which of the eight `--app-*` tokens an application gets (§5.2).
 *
 * A stable hash rather than order-of-appearance, so an app keeps its colour
 * from one day to the next — a legend you have to re-learn every morning is
 * worse than no colour at all.
 */
const APP_RAMP = 8;

function appColour(appId: string): string {
  let hash = 0;
  for (let i = 0; i < appId.length; i++) hash = (hash * 31 + appId.charCodeAt(i)) | 0;
  return `var(--app-${(Math.abs(hash) % APP_RAMP) + 1})`;
}

/** `code.exe` and `Code.app` are the same thing to a human reading a total. */
function appLabel(appId: string): string {
  return appId.replace(/\.(exe|app)$/i, "");
}

export function Activity() {
  const status = useApp((s) => s.activityStatus);
  const day = useApp((s) => s.activityDay);
  const date = useApp((s) => s.activityDate);
  const setDate = useApp((s) => s.setActivityDate);
  const load = useApp((s) => s.loadActivity);
  const go = useApp((s) => s.go);
  const hourHeight = useApp((s) => s.hourHeight);

  useEffect(() => {
    void load();
  }, [load]);

  if (!status) return <Empty>Loading Activity…</Empty>;

  // The platform's own sentence, never a bare "unavailable" (§3.10).
  if (status.support !== "full") {
    return <Empty>{status.supportNote}</Empty>;
  }

  if (!status.settings.enabled) {
    return (
      <Empty>
        Activity is off. It samples which application is in front of you every 20 seconds,
        stores it in the same local database as everything else, and never sends it anywhere.{" "}
        <button className="btn" onClick={() => go("settings")} style={{ marginLeft: 6 }}>
          Turn it on in Settings
        </button>
      </Empty>
    );
  }

  return (
    <div className="view-pad scroll-y">
      <div className="row" style={{ marginBottom: 12 }}>
        <button className="btn" aria-label="Previous day" onClick={() => setDate(fmt.addDays(date, -1))}>
          ‹
        </button>
        <button className="btn" aria-label="Next day" onClick={() => setDate(fmt.addDays(date, 1))}>
          ›
        </button>
        <strong className="display" style={{ fontSize: "1rem" }}>
          {fmt.longDate(date)}
        </strong>
        <span className="grow" />
        {status.settings.paused && (
          <span className="micro" style={{ color: "var(--track)" }}>
            Paused — nothing is being recorded
          </span>
        )}
        <span className="data caption">
          {day ? fmt.duration(day.trackedSec) : "0m"} observed
        </span>
        <button className="btn" onClick={() => setDate(fmt.today())}>
          Today
        </button>
      </div>

      {!day || day.spans.length === 0 ? (
        <Empty>
          Nothing recorded on this day. Sampling only runs while Fruit is open and the machine
          is awake, so a day you spent elsewhere is genuinely empty rather than lost.
        </Empty>
      ) : (
        <>
          <Correlations day={day} />
          <ByApp totals={day.byApp} trackedSec={day.trackedSec} />
          <DayTimeline day={day} hourHeight={hourHeight} />
        </>
      )}
    </div>
  );
}

/**
 * §2.3 CALIBRATE — the plotted block against what was actually in front of you.
 *
 * The line "you were in Slack for the hour you plotted for the refactor" is the
 * whole payoff, so it is the first thing on the screen, not a footnote under a
 * chart.
 */
function Correlations({ day }: { day: ActivityDay }) {
  if (day.correlations.length === 0) {
    return (
      <section className="panel">
        <h2>Against the plan</h2>
        <p className="caption">
          Nothing was plotted on this day, so there is no intention to compare against. Plot a
          block in the Planner and this panel fills itself in.
        </p>
      </section>
    );
  }

  return (
    <section className="panel">
      <h2>Against the plan</h2>
      <div className="stack">
        {day.correlations.map((c) => {
          const total = c.topApps.reduce((sum, a) => sum + a.seconds, 0);
          const top = c.topApps[0];
          return (
            <div key={c.blockId} className="stack" style={{ gap: 4 }}>
              <div className="row">
                <span className="grow" style={{ minWidth: 0 }}>
                  {c.title}
                </span>
                <span className="data micro" style={{ color: "var(--muted)" }}>
                  {fmt.clockRange(c.startsAt, c.durationSec)}
                </span>
              </div>
              {/* One stacked bar per block: proportions, not a legend to decode. */}
              <span
                className="app-bar"
                role="img"
                aria-label={c.topApps
                  .map((a) => `${appLabel(a.appId)} ${fmt.duration(a.seconds)}`)
                  .join(", ")}
              >
                {c.topApps.map((a) => (
                  <i
                    key={a.appId}
                    style={{
                      width: `${(a.seconds / Math.max(1, total)) * 100}%`,
                      background: appColour(a.appId),
                    }}
                    title={`${appLabel(a.appId)} · ${fmt.duration(a.seconds)}`}
                  />
                ))}
              </span>
              {top && (
                <span className="caption">
                  Mostly <strong>{appLabel(top.appId)}</strong> ·{" "}
                  {fmt.duration(top.seconds)} of {fmt.duration(c.durationSec)} plotted
                </span>
              )}
            </div>
          );
        })}
      </div>
    </section>
  );
}

function ByApp({ totals, trackedSec }: { totals: AppTotal[]; trackedSec: number }) {
  const max = Math.max(1, ...totals.map((t) => t.seconds));
  return (
    <section className="panel">
      <h2>Where the day went</h2>
      <div className="stack" style={{ gap: 6 }}>
        {totals.slice(0, 12).map((t) => (
          <div key={t.appId} className="row" style={{ gap: 8 }}>
            <span style={{ width: 160, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis" }}>
              {appLabel(t.appId)}
            </span>
            {/* Decorative: the app name, the duration and the share are all
                already text on this row, so a label here would make a screen
                reader say each of them twice (I3). */}
            <span className="app-bar grow" aria-hidden="true">
              <i style={{ width: `${(t.seconds / max) * 100}%`, background: appColour(t.appId) }} />
            </span>
            <span className="data micro" style={{ width: 64, textAlign: "right" }}>
              {fmt.duration(t.seconds)}
            </span>
            <span className="micro" style={{ width: 40, textAlign: "right", color: "var(--muted)" }}>
              {Math.round((t.seconds / Math.max(1, trackedSec)) * 100)}%
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}

/**
 * The same 24-hour column as the Planner, at the same hour height, so the two
 * screens can be compared by looking rather than by reading numbers off both.
 */
function DayTimeline({ day, hourHeight }: { day: ActivityDay; hourHeight: number }) {
  const dayStart = useMemo(() => new Date(`${day.localDate}T00:00:00`).getTime(), [day.localDate]);
  const scrollRef = useRef<HTMLDivElement>(null);

  const firstMin = useMemo(() => {
    const first = day.spans[0];
    return first ? (first.startedAt - dayStart) / 60_000 : 8 * 60;
  }, [day.spans, dayStart]);

  /* Open on the first thing that happened, not on midnight. A 24-hour column
     is right — night work is real — but scrolled to 00:00 it is eight screens
     of nothing before the day starts. */
  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = Math.max(0, (firstMin / 60) * hourHeight - hourHeight / 2);
  }, [firstMin, hourHeight]);

  return (
    <section className="panel">
      <h2>The day</h2>
      <div className="activity-timeline" ref={scrollRef}>
        <div className="activity-canvas" style={{ height: 24 * hourHeight }}>
          {Array.from({ length: 24 }, (_, h) => (
            <div key={h} className="hour-row" style={{ height: hourHeight, top: h * hourHeight }}>
              <span className="hour-label micro" aria-hidden="true">
                {String(h).padStart(2, "0")}
              </span>
            </div>
          ))}
          <div className="activity-lane">
            {day.spans.map((s) => {
              const top = ((s.startedAt - dayStart) / 3_600_000) * hourHeight;
              const height = Math.max(2, ((s.endedAt - s.startedAt) / 3_600_000) * hourHeight);
              return (
                <span
                  key={s.id}
                  className="activity-span"
                  style={{ top, height, background: appColour(s.appId) }}
                  title={`${appLabel(s.appId)}${s.windowTitle ? ` — ${s.windowTitle}` : ""} · ${fmt.clock(
                    s.startedAt,
                  )}–${fmt.clock(s.endedAt)}`}
                >
                  {height >= 14 && (
                    <span className="micro">
                      {appLabel(s.appId)}
                      {s.windowTitle && height >= 28 && <> — {s.windowTitle}</>}
                    </span>
                  )}
                </span>
              );
            })}
          </div>
        </div>
      </div>
    </section>
  );
}
