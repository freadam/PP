/**
 * The work reports — what the period held, and how it compares.
 *
 * *How many hours? Against what target? When in the day? On what kind of work?
 * On which projects? How much was deliberate? What was actually on screen?*
 *
 * # Every figure carries its change
 *
 * A number on its own is nearly useless — "8h 37m" is neither good nor bad
 * until you know last month was 7h 10m. So every headline reads against the
 * previous four comparable periods, and where there is no history to compare
 * against the panel says so instead of drawing an arrow it cannot justify.
 *
 * # Confirmed work only
 *
 * Every panel except the last reads the *confirmed* record. Observed time is
 * not work until somebody says it was, and a work-hours graph that quietly
 * included "Chrome was in front" would be the exact dishonesty this app exists
 * to avoid. The apps panel is the one that reports observation, and it says so
 * on screen rather than in a tooltip.
 *
 * All of it comes from one `get_work_report` call, so the panels on this screen
 * can never disagree about which week it is.
 */

import { useEffect, useMemo, useState } from "react";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type {
  ScoreComponent,
  WorkPeriod,
  WorkReport,
  WorkSlice,
} from "../lib/types";
import { appColour, appLabel } from "../lib/apps";

const PERIODS: { key: WorkPeriod; label: string }[] = [
  { key: "day", label: "Day" },
  { key: "week", label: "Week" },
  { key: "month", label: "Month" },
];

export function WorkReports() {
  const [period, setPeriod] = useState<WorkPeriod>("week");
  const [date, setDate] = useState(fmt.today());
  const [report, setReport] = useState<WorkReport | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let live = true;
    void ipc
      .getWorkReport(date, period, fmt.tz())
      .then((r) => live && setReport(r))
      .catch(() => live && setReport(null))
      .finally(() => live && setLoaded(true));
    return () => {
      live = false;
    };
  }, [date, period]);

  // One step is one period — a "previous" button that moves a week when you are
  // looking at a month is a button you stop trusting.
  const shift = (dir: -1 | 1) => {
    const d = new Date(`${date}T12:00:00Z`);
    if (period === "day") d.setUTCDate(d.getUTCDate() + dir);
    else if (period === "week") d.setUTCDate(d.getUTCDate() + 7 * dir);
    else d.setUTCMonth(d.getUTCMonth() + dir);
    setDate(d.toISOString().slice(0, 10));
  };

  const label = report
    ? period === "day"
      ? fmt.longDate(report.from)
      : fmt.rangeLabel(report.from, report.to)
    : date;

  return (
    <div className="stack">
      <div className="row">
        <div className="segmented" role="group" aria-label="Period">
          {PERIODS.map((p) => (
            <button
              key={p.key}
              className="btn"
              aria-pressed={period === p.key}
              onClick={() => setPeriod(p.key)}
            >
              {p.label}
            </button>
          ))}
        </div>
        <button className="btn" aria-label="Previous period" onClick={() => shift(-1)}>
          ‹
        </button>
        <button className="btn" onClick={() => setDate(fmt.today())}>
          Today
        </button>
        <button className="btn" aria-label="Next period" onClick={() => shift(1)}>
          ›
        </button>
        <span className="caption data grow">{label}</span>
      </div>

      {!report ? (
        <p className="caption">
          {loaded
            ? "No work report for that range. Nothing has been recorded in it yet."
            : "Reading…"}
        </p>
      ) : (
        <>
          <Headline report={report} />
          <FindingsPanel report={report} />
          <div className="report-grid">
            <ScorePanel report={report} />
            <WorkHours report={report} />
            <AgainstTarget report={report} />
            <DonutPanel
              title="By kind of work"
              slices={report.byCategory}
              empty="No categorised work in this range. Settings → Work sets the list; a task's kind is on its detail panel."
            />
            <DayShape report={report} />
            <HourHistogram report={report} />
            <DonutPanel
              title="By project"
              slices={report.byProject}
              empty="No work recorded against a project in this range."
            />
            <FocusPanel report={report} />
            <AppsPanel report={report} />
            <ByLabelPanel report={report} />
          </div>
          {/* Full width, below the grid: each row is a block title, a bar and a
              sentence, and squeezed into a grid cell the titles ellipsis away
              into nothing. It is also the panel people read last — the working,
              after the conclusions. */}
          <AgainstPlanPanel report={report} />
        </>
      )}
    </div>
  );
}

/* ─── what the numbers say ───────────────────────────────────────────────
   The report, read back in sentences. Generated in Rust so the rules live in
   one place; this only draws them.

   Each finding states a fact and stops. Rize's equivalent ends "Consider
   taking this week lightly if possible" — Fruit does not tell you what to do
   about your own week, because it does not know what your week was for. */

function FindingsPanel({ report }: { report: WorkReport }) {
  if (report.findings.length === 0) return null;
  return (
    <section className="panel findings">
      <h3>What this {report.period} says</h3>
      {report.findings.map((f) => (
        <div className="finding" key={f.id} data-tone={f.tone}>
          <strong>{f.headline}</strong>
          <span className="caption">{f.detail}</span>
        </div>
      ))}
    </section>
  );
}

/* ─── over and under the target ──────────────────────────────────────────
   A diverging chart around a zero line: how far each day landed above or
   below the daily target.

   Deliberately not a second copy of the hours chart. "Six hours on Tuesday"
   and "an hour under on Tuesday" are the same datum and different readings,
   and the second is the one you act on. It only exists when a target does —
   a deviation from nothing is not a quantity. */

function AgainstTarget({ report }: { report: WorkReport }) {
  const target = report.targetSec;
  if (target === null) {
    return (
      <section className="panel">
        <h3>Against the target</h3>
        <p className="caption">
          No daily work target set, so there is nothing to measure a deviation from. Settings →
          Work sets one.
        </p>
      </section>
    );
  }

  const days = report.days.filter((d) => d.targetApplies);
  if (days.length === 0) {
    return (
      <section className="panel">
        <h3>Against the target</h3>
        <p className="caption">The target applies to no day in this range.</p>
      </section>
    );
  }

  const deltas = days.map((d) => ({ date: d.date, diff: d.workSec - target }));
  const scale = Math.max(...deltas.map((d) => Math.abs(d.diff)), 1800);
  const over = deltas.filter((d) => d.diff > 0);
  const under = deltas.filter((d) => d.diff < 0);

  return (
    <section className="panel">
      <h3>Against the target</h3>
      <div className="divchart">
        {deltas.map((d) => (
          <div className="divrow" key={d.date}>
            <span className="micro">{fmt.weekdayShort(d.date)}</span>
            <span className="divtrack" aria-hidden="true">
              <u />
              <i
                data-over={d.diff > 0 || undefined}
                style={
                  d.diff >= 0
                    ? { left: "50%", width: `${(d.diff / scale) * 50}%` }
                    : { right: "50%", width: `${(-d.diff / scale) * 50}%` }
                }
              />
            </span>
            <span className="micro data">
              {d.diff === 0 ? "on target" : `${d.diff > 0 ? "+" : "−"}${fmt.duration(Math.abs(d.diff))}`}
            </span>
          </div>
        ))}
      </div>
      <p className="caption">
        <span className="data">{over.length}</span> {over.length === 1 ? "day" : "days"} over,{" "}
        <span className="data">{under.length}</span> under, against{" "}
        <span className="data">{fmt.duration(target)}</span> a day. A day under the target is not a
        failed day — it is a day that went somewhere else, and the Day view says where.
      </p>
    </section>
  );
}

/* ─── the change language ────────────────────────────────────────────────
   One component for "this figure, against the baseline". Arrows are paired
   with a signed number and a word, never used alone, and "up" is not assumed
   to be good — `good` says which direction the reading favours, and where
   neither direction is better it is left out and the delta is stated flat. */

function Delta({
  now,
  was,
  periods,
  good,
  format = fmt.duration,
}: {
  now: number;
  was: number | undefined;
  periods: number | undefined;
  good?: "up" | "down";
  format?: (n: number) => string;
}) {
  if (was === undefined || periods === undefined) {
    return (
      <span className="caption">
        {/* Said, not hidden. An absent comparison is a fact about your history,
            and an arrow invented to fill the space would be worse than a gap. */}
        no earlier period to compare with yet
      </span>
    );
  }
  const diff = now - was;
  const pct = was > 0 ? Math.round((diff / was) * 100) : null;
  const dir = diff === 0 ? "flat" : diff > 0 ? "up" : "down";
  const tone = !good || dir === "flat" ? "flat" : dir === good ? "good" : "poor";
  return (
    <span className="delta" data-tone={tone}>
      <span aria-hidden="true">{dir === "flat" ? "→" : dir === "up" ? "↑" : "↓"}</span>
      <span className="data">
        {diff > 0 ? "+" : diff < 0 ? "−" : ""}
        {format(Math.abs(diff))}
        {pct !== null && pct !== 0 && ` · ${Math.abs(pct)}%`}
      </span>
      <span className="caption">
        vs. {periods === 1 ? "the previous period" : `the last ${periods}`}
      </span>
    </span>
  );
}

/** The three numbers worth reading first, each against its baseline. */
function Headline({ report }: { report: WorkReport }) {
  const b = report.baseline ?? undefined;
  const worked = report.days.filter((d) => d.workSec > 0);
  const dayLength = worked.length ? Math.round(report.totalWorkSec / worked.length) : 0;

  return (
    <div className="cards report-cards">
      <div className="card">
        <span className="micro">Work</span>
        <strong>{fmt.duration(report.totalWorkSec)}</strong>
        <Delta now={report.totalWorkSec} was={b?.totalWorkSec} periods={b?.periods} />
      </div>
      <div className="card">
        <span className="micro">Average working day</span>
        <strong>{fmt.duration(dayLength)}</strong>
        <Delta now={dayLength} was={b?.dayLengthSec} periods={b?.periods} />
      </div>
      <div className="card">
        <span className="micro">In a focus session</span>
        <strong>{fmt.duration(report.focus.trackedSec)}</strong>
        <Delta
          now={report.focus.trackedSec}
          was={b?.focusSec}
          periods={b?.periods}
          good="up"
        />
      </div>
      <div className="card">
        <span className="micro">Days worked</span>
        <strong>{worked.length}</strong>
        <span className="caption">of {report.days.length} in the range</span>
      </div>
    </div>
  );
}

/* ─── the score ──────────────────────────────────────────────────────────
   One number, four inputs, and the arithmetic printed underneath.

   Rize ships a focus-quality score derived from twenty-plus attributes and
   shows none of them. A score you cannot open is a score you cannot argue
   with, and this app's whole position is that a figure you cannot audit is a
   figure you do not believe. So every component is on screen with its value,
   its weight and the sentence it came from — and the panel says out loud that
   the weights are a judgement rather than a measurement. */

function ScorePanel({ report }: { report: WorkReport }) {
  const s = report.score;
  const b = report.baseline ?? undefined;
  const [open, setOpen] = useState(false);

  if (!s.scored) {
    return (
      <section className="panel">
        <h3>Score</h3>
        <p className="caption">
          Nothing was recorded in this range, so there is nothing to score. A zero here would be a
          verdict on a week you may simply have been away for.
        </p>
      </section>
    );
  }

  // A ring drawn with one conic gradient. The numeral inside is the reading;
  // the ring is the decoration, so nothing depends on reading an angle.
  return (
    <section className="panel">
      <h3>Score</h3>
      <div className="row" style={{ gap: 20, alignItems: "center", flexWrap: "wrap" }}>
        <span
          className="gauge"
          style={{ "--pct": `${s.value}%` } as React.CSSProperties}
          aria-hidden="true"
        >
          <b className="data">{s.value}</b>
        </span>
        <span className="stack" style={{ gap: 2 }}>
          <strong className="display">{s.rating}</strong>
          <span className="caption">
            <span className="data">{s.value}</span> out of 100
          </span>
          <Delta
            now={s.value}
            was={b?.score}
            periods={b?.periods}
            good="up"
            format={(n) => String(Math.round(n))}
          />
        </span>
      </div>

      <button className="btn btn-quiet" aria-expanded={open} onClick={() => setOpen((v) => !v)}>
        {open ? "Hide the arithmetic" : "Show the arithmetic"}
      </button>

      {open && (
        <>
          <div className="bars" style={{ marginTop: 8 }}>
            {s.components.map((c: ScoreComponent) => (
              <div className="bar-row" key={c.label}>
                <span className="micro">{c.label}</span>
                <span className="bar" aria-label={`${c.label}: ${c.value} of 100`}>
                  <i style={{ width: `${c.value}%`, background: "var(--plot)" }} />
                </span>
                <span className="micro data">{c.value}</span>
                <span className="caption data">×{c.weight.toFixed(2)}</span>
              </div>
            ))}
          </div>
          <p className="caption">
            {s.components.map((c) => `${c.label}: ${c.detail}`).join(" · ")}.
          </p>
          <p className="caption">
            {s.components
              .map((c) => `${c.value} × ${c.weight.toFixed(2)}`)
              .join(" + ")}{" "}
            = <span className="data">{s.value}</span>. The four weights are a judgement, not a
            measurement — they are here so you can disagree with them rather than with the score.
          </p>
        </>
      )}
    </section>
  );
}

/* ─── hours per day, against the target ──────────────────────────────── */

function WorkHours({ report }: { report: WorkReport }) {
  const max = Math.max(report.targetSec ?? 0, ...report.days.map((d) => d.workSec), 3600);
  const today = fmt.today();

  return (
    <section className="panel">
      <h3>Work hours by day</h3>
      {report.targetSec !== null && report.targetDaysApplicable !== null && (
        <p className="caption">
          Target <span className="data">{fmt.duration(report.targetSec)}</span> a day ·{" "}
          <span className="data">
            {report.targetDaysMet} of {report.targetDaysApplicable}
          </span>{" "}
          {report.targetDaysApplicable === 1 ? "day" : "days"} met.
          {/* Only days that have happened are counted. A Friday that has not
              arrived is not a day you failed to work six hours on. */}
        </p>
      )}

      <div className="workbars" aria-hidden="true">
        {report.days.map((d) => {
          const met = report.targetSec !== null && d.workSec >= report.targetSec;
          return (
            <span
              key={d.date}
              className="workbar"
              data-today={d.date === today || undefined}
              data-applies={d.targetApplies || undefined}
              title={`${d.date} · ${fmt.duration(d.workSec)}`}
            >
              <i style={{ height: `${(d.workSec / max) * 100}%` }} data-met={met || undefined} />
              {report.targetSec !== null && d.targetApplies && (
                <b style={{ bottom: `${(report.targetSec / max) * 100}%` }} />
              )}
            </span>
          );
        })}
      </div>
      <div className="workbar-labels micro" aria-hidden="true">
        {report.days.map((d) => (
          <span key={d.date}>{barLabel(d.date, report.days.length)}</span>
        ))}
      </div>

      {/* The chart is decorative; this is the reading. Never colour alone. */}
      <p className="caption">
        {report.days
          .filter((d) => d.workSec > 0)
          .map((d) => `${fmt.weekdayShort(d.date)} ${fmt.duration(d.workSec)}`)
          .join(" · ") || "Nothing recorded in this range."}
      </p>
    </section>
  );
}

/** Day-of-month for a month, weekday initial for a week, nothing for one day. */
function barLabel(date: string, count: number): string {
  if (count <= 1) return "";
  if (count <= 7) return fmt.weekdayShort(date);
  const day = Number(date.slice(8));
  // Every fifth, or a month of 31 labels becomes a grey smear.
  return day === 1 || day % 5 === 0 ? String(day) : "";
}

/* ─── the shape of the working day ───────────────────────────────────────
   One bar per day, drawn on a 24-hour axis from when work first started to
   when it last stopped.

   This is the panel a total cannot replace. Six hours between 09:00 and 15:00
   and six hours between 09:00 and 23:00 are the same figure everywhere else in
   this report and very different days, and only the span says which one you
   had. */

function DayShape({ report }: { report: WorkReport }) {
  const rows = report.days.filter((d) => d.firstAt !== null && d.lastAt !== null);
  if (rows.length === 0) {
    return (
      <section className="panel">
        <h3>The shape of the day</h3>
        <p className="caption">No work recorded in this range, so there is no day to draw.</p>
      </section>
    );
  }

  const minuteOf = (ms: number) => {
    const d = new Date(ms);
    return d.getHours() * 60 + d.getMinutes();
  };
  // The axis covers only the hours actually used, rounded out to the hour, so a
  // 09:00–17:00 week is not drawn as eight thin bars inside a mostly empty day.
  const starts = rows.map((d) => minuteOf(d.firstAt!));
  const ends = rows.map((d) => minuteOf(d.lastAt!));
  const lo = Math.max(0, Math.floor(Math.min(...starts) / 60) * 60 - 60);
  const hi = Math.min(1440, Math.ceil(Math.max(...ends) / 60) * 60 + 60);
  const span = Math.max(60, hi - lo);
  const ticks: number[] = [];
  for (let m = Math.ceil(lo / 120) * 120; m <= hi; m += 120) ticks.push(m);

  return (
    <section className="panel">
      <h3>The shape of the day</h3>
      <p className="caption">
        When work started and stopped. The bar is the span, not the total — a gap inside it is time
        that went somewhere else.
      </p>
      <div className="spanchart">
        {rows.map((d) => {
          const from = minuteOf(d.firstAt!);
          const to = minuteOf(d.lastAt!);
          return (
            <div className="spanrow" key={d.date}>
              <span className="micro">{fmt.weekdayShort(d.date)}</span>
              <span className="spantrack" aria-hidden="true">
                {ticks.map((m) => (
                  <u key={m} style={{ left: `${((m - lo) / span) * 100}%` }} />
                ))}
                <i
                  style={{
                    left: `${((from - lo) / span) * 100}%`,
                    width: `${(Math.max(1, to - from) / span) * 100}%`,
                  }}
                />
              </span>
              <span className="caption data">
                {clockOf(d.firstAt!)}–{clockOf(d.lastAt!)}
              </span>
              <span className="micro data">{fmt.duration(d.workSec)}</span>
            </div>
          );
        })}
        <div className="spanrow spanaxis micro" aria-hidden="true">
          <span />
          <span className="spantrack">
            {ticks.map((m) => (
              <em key={m} style={{ left: `${((m - lo) / span) * 100}%` }}>
                {String(Math.floor(m / 60)).padStart(2, "0")}
              </em>
            ))}
          </span>
          <span />
          <span />
        </div>
      </div>
    </section>
  );
}

function clockOf(ms: number): string {
  const d = new Date(ms);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/* ─── when in the day the work happens ───────────────────────────────── */

function HourHistogram({ report }: { report: WorkReport }) {
  const max = Math.max(...report.byHour, 1);
  const peak = report.byHour.indexOf(max);
  const total = report.byHour.reduce((a, b) => a + b, 0);
  if (total === 0) return null;

  // Before 12:00, and after 18:00 — the two readings people actually want out
  // of a histogram like this.
  const morning = report.byHour.slice(0, 12).reduce((a, b) => a + b, 0);
  const evening = report.byHour.slice(18).reduce((a, b) => a + b, 0);

  return (
    <section className="panel">
      <h3>When the work happens</h3>
      <div className="hourbars" aria-hidden="true">
        {report.byHour.map((sec, h) => (
          <span
            key={h}
            className="hourbar"
            data-peak={h === peak || undefined}
            title={`${String(h).padStart(2, "0")}:00 · ${fmt.duration(sec)}`}
          >
            <i style={{ height: `${(sec / max) * 100}%` }} />
          </span>
        ))}
      </div>
      <div className="hourbar-labels micro" aria-hidden="true">
        {report.byHour.map((_, h) => (
          <span key={h}>{h % 6 === 0 ? String(h).padStart(2, "0") : ""}</span>
        ))}
      </div>
      <p className="caption">
        Busiest hour is <span className="data">{String(peak).padStart(2, "0")}:00</span> with{" "}
        <span className="data">{fmt.duration(max)}</span>.{" "}
        <span className="data">{Math.round((morning / total) * 100)}%</span> of the work happened
        before noon, <span className="data">{Math.round((evening / total) * 100)}%</span> after
        18:00.
      </p>
    </section>
  );
}

/* ─── a named split, as a ring and a list ────────────────────────────────
   The ring is the proportion at a glance; the list is the reading. The
   uncategorised bucket is always shown — hiding it would make the shares add
   to less than the total with nothing on screen explaining where the rest
   went, which is the one thing a report must never do. */

function DonutPanel({
  title,
  slices,
  empty,
}: {
  title: string;
  slices: WorkSlice[];
  empty: string;
}) {
  const gradient = useMemo(() => {
    let at = 0;
    const stops = slices.map((s) => {
      const from = at;
      at += s.share * 100;
      return `${s.colour} ${from}% ${at}%`;
    });
    return stops.length ? `conic-gradient(${stops.join(",")})` : undefined;
  }, [slices]);

  return (
    <section className="panel">
      <h3>{title}</h3>
      {slices.length === 0 ? (
        <p className="caption">{empty}</p>
      ) : (
        <div className="donut-row">
          <span className="donut" style={{ background: gradient }} aria-hidden="true" />
          <div className="bars grow">
            {slices.map((s) => (
              <div className="bar-row" key={s.id ?? "none"}>
                <span className="micro">
                  <i className="swatch" style={{ background: s.colour }} aria-hidden="true" />
                  {s.name}
                </span>
                <span className="bar" aria-label={`${s.name}: ${fmt.duration(s.seconds)}`}>
                  <i style={{ width: `${s.share * 100}%`, background: s.colour }} />
                </span>
                <span className="micro data">{Math.round(s.share * 100)}%</span>
                <span className="caption data">{fmt.duration(s.seconds)}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </section>
  );
}

/**
 * Focus sessions: how many, and for how long.
 *
 * Plotted *and* tracked, side by side, because a focus session is an intention
 * and the gap between the two is the same drift the rest of the app is built
 * around. "Ran its length" is reported without praise — a session cut short
 * because the work finished is a good outcome, not a failure.
 */
function FocusPanel({ report }: { report: WorkReport }) {
  const f = report.focus;
  const b = report.baseline ?? undefined;
  return (
    <section className="panel">
      <h3>Focus sessions</h3>
      {f.sessions === 0 ? (
        <p className="caption">
          No focus sessions in this range. Press <span className="kbd">F</span> on a task to start
          one — it plots the length you intend, so the overrun stays visible.
        </p>
      ) : (
        <>
          <div className="row" style={{ gap: 20, flexWrap: "wrap" }}>
            <span className="stack" style={{ gap: 2 }}>
              <strong className="display">{f.sessions}</strong>
              <span className="caption">{f.sessions === 1 ? "session" : "sessions"}</span>
              <Delta
                now={f.sessions}
                was={b?.focusSessions}
                periods={b?.periods}
                good="up"
                format={(n) => String(Math.round(n))}
              />
            </span>
            <span className="stack" style={{ gap: 2 }}>
              <strong className="display">{fmt.duration(f.trackedSec)}</strong>
              <span className="caption">tracked</span>
            </span>
            <span className="stack" style={{ gap: 2 }}>
              <strong className="display">
                {fmt.duration(Math.round(f.trackedSec / Math.max(1, f.sessions)))}
              </strong>
              <span className="caption">average session</span>
            </span>
          </div>
          <div className="bars">
            <div className="bar-row">
              <span className="micro">Plotted</span>
              <span className="bar" aria-label={`Plotted ${fmt.duration(f.plannedSec)}`}>
                <i style={{ width: "100%", background: "var(--plot)" }} />
              </span>
              <span className="micro data" />
              <span className="caption data">{fmt.duration(f.plannedSec)}</span>
            </div>
            <div className="bar-row">
              <span className="micro">Tracked</span>
              <span className="bar" aria-label={`Tracked ${fmt.duration(f.trackedSec)}`}>
                <i
                  style={{
                    width: `${Math.min(100, f.plannedSec ? (f.trackedSec / f.plannedSec) * 100 : 0)}%`,
                    background: "var(--track-graphic)",
                  }}
                />
              </span>
              <span className="micro data" />
              <span className="caption data">{fmt.duration(f.trackedSec)}</span>
            </div>
          </div>
          <p className="caption">
            Longest <span className="data">{fmt.duration(f.longestSec)}</span> ·{" "}
            <span className="data">{f.completed}</span> ran the full length they were plotted for. A
            session cut short because the work finished is not a failure.
          </p>
        </>
      )}
    </section>
  );
}

/**
 * What was actually on screen.
 *
 * The one panel here that reports **observation** rather than the confirmed
 * record, and it says so above the list. These seconds must never be added to
 * the work total — they answer a different question, and a reader who mixes
 * them has been misled by the layout.
 */
/**
 * Observed time in the words the user chose — Work · Study · Distraction · Life.
 *
 * Moved here from Activity, which could only ever show it for one day. A day's
 * split is mostly noise; the same split over a week is the line that answers
 * "where did it actually go" without any reading.
 *
 * Shares are of *observed* time, not of the range and not of the work total.
 * Two of those three would be wrong, and the caption says which one it is.
 */
function ByLabelPanel({ report }: { report: WorkReport }) {
  const total = report.byLabel.reduce((n, c) => n + c.seconds, 0);
  return (
    <section className="panel">
      <h3>By label</h3>
      <p className="caption">
        Observed time only — what the machine saw, not what you confirmed. Not part of the work
        total above; the Day view is where the two are reconciled.
      </p>
      {report.byLabel.length === 0 ? (
        <p className="caption">
          Nothing labelled in this range. Activity → What has no label yet is where labels get
          attached, and each one writes a rule that applies from then on.
        </p>
      ) : (
        <div className="bars">
          {report.byLabel.map((c) => (
            <div className="bar-row" key={c.id}>
              <span className="micro">{c.name}</span>
              <span className="bar" aria-label={`${c.name}: ${fmt.duration(c.seconds)}`}>
                <i style={{ width: `${(c.seconds / Math.max(1, total)) * 100}%`, background: c.colour }} />
              </span>
              <span className="micro data">{Math.round((c.seconds / Math.max(1, total)) * 100)}%</span>
              <span className="caption data">{fmt.duration(c.seconds)}</span>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

/**
 * §2.3 CALIBRATE — each plotted block against what was actually in front of you.
 *
 * The only place in Fruit where an intention meets an *observation* rather than
 * another record of itself, which is what lets it say "you were in Slack for the
 * hour you plotted for the refactor".
 *
 * It was day-only on Activity. Over a week it becomes the panel that shows a
 * *pattern* rather than an anecdote — one block that went to Chrome is a
 * Tuesday, five of them is a habit.
 */
function AgainstPlanPanel({ report }: { report: WorkReport }) {
  return (
    <section className="panel">
      <h3>Against the plan</h3>
      {report.correlations.length === 0 ? (
        <p className="caption">
          Nothing plotted in this range had activity recorded under it, so there is no intention to
          compare against. Plot a block in the Planner and this panel fills itself in.
        </p>
      ) : (
        <div className="stack">
          {report.correlations.map((c) => {
            const total = c.topApps.reduce((sum, a) => sum + a.seconds, 0);
            const top = c.topApps[0];
            return (
              <div key={c.blockId} className="stack" style={{ gap: 4 }}>
                <div className="row">
                  <span className="grow" style={{ minWidth: 0 }}>
                    {c.title}
                  </span>
                  {/* The date as well as the clock: over a week, "11:46 AM" on
                      its own does not say which day it was. */}
                  <span className="data micro" style={{ color: "var(--muted)" }}>
                    {report.period !== "day" && `${fmt.toLocalDate(new Date(c.startsAt)).slice(5)} · `}
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
                    Mostly <strong>{appLabel(top.appId)}</strong> · {fmt.duration(top.seconds)} of{" "}
                    {fmt.duration(c.durationSec)} plotted
                  </span>
                )}
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}

function AppsPanel({ report }: { report: WorkReport }) {
  return (
    <section className="panel">
      <h3>Apps used</h3>
      <p className="caption">
        What the machine saw in front of you — observation, not the confirmed record. These are not
        part of the work total above; the Day view is where the two are reconciled.
      </p>
      {report.apps.length === 0 ? (
        <p className="caption">
          Nothing observed in this range. Activity may be off, paused, or the range may predate it.
        </p>
      ) : (
        <div className="bars">
          {report.apps.map((a) => (
            <div className="bar-row" key={a.appId}>
              <span className="micro">{a.name}</span>
              <span className="bar" aria-label={`${a.name}: ${fmt.duration(a.seconds)}`}>
                <i style={{ width: `${a.share * 100}%`, background: "var(--muted)" }} />
              </span>
              <span className="micro data">{Math.round(a.share * 100)}%</span>
              <span className="caption data">{fmt.duration(a.seconds)}</span>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
