/**
 * The work reports — five questions over one range.
 *
 * *How many hours did I work? On what kind of work? On which projects? How much
 * of it was deliberate? What was actually on screen?*
 *
 * Every panel except the last reads **confirmed** work. Observed time is not
 * work until somebody says it was, and a work-hours graph that quietly included
 * "Chrome was in front" would be the exact dishonesty this app exists to avoid.
 * The apps panel is the one that reports observation, and it says so on screen
 * rather than in a tooltip.
 *
 * All five come from one `get_work_report` call, so the panels on this screen
 * can never disagree about which week it is.
 */

import { useEffect, useMemo, useState } from "react";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type { WorkPeriod, WorkReport, WorkSlice } from "../lib/types";

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
          <WorkHours report={report} />
          <SplitPanel
            title="By kind of work"
            slices={report.byCategory}
            total={report.totalWorkSec}
            empty="No categorised work in this range. Settings → Kinds of work sets the list; a task's category is on its detail panel."
          />
          <SplitPanel
            title="By project"
            slices={report.byProject}
            total={report.totalWorkSec}
            empty="No work recorded against a project in this range."
          />
          <FocusPanel report={report} />
          <AppsPanel report={report} />
        </>
      )}
    </div>
  );
}

/**
 * Hours worked per day, against the daily target.
 *
 * The target is a line across the chart rather than a number in a corner, so
 * "which days missed it" is a glance instead of five subtractions. It is drawn
 * only across the days it applies to — a Mon–Fri target that appeared to run
 * through Sunday would report a shortfall on a day off.
 */
function WorkHours({ report }: { report: WorkReport }) {
  const max = Math.max(
    report.targetSec ?? 0,
    ...report.days.map((d) => d.workSec),
    3600,
  );
  const today = fmt.today();

  return (
    <section className="panel">
      <h3>Work hours</h3>
      <div className="row" style={{ gap: 20, flexWrap: "wrap" }}>
        <span>
          <strong className="display">{fmt.duration(report.totalWorkSec)}</strong>{" "}
          <span className="caption">confirmed in this {report.period}</span>
        </span>
        {report.targetSec !== null && report.targetDaysApplicable !== null && (
          <span className="caption">
            Target <span className="data">{fmt.duration(report.targetSec)}</span> a day ·{" "}
            <span className="data">
              {report.targetDaysMet} of {report.targetDaysApplicable}
            </span>{" "}
            {report.targetDaysApplicable === 1 ? "day" : "days"} met
            {/* Only days that have *happened* are counted. A Friday that has not
                arrived is not a day you failed to work six hours on. */}
          </span>
        )}
      </div>

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
              {/* The target line, drawn only where the target applies. */}
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
          .map((d) => `${d.date} ${fmt.duration(d.workSec)}`)
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

/**
 * A named split of the work total — the same component for kinds of work and
 * for projects, because it is the same question asked of a different column.
 *
 * The uncategorised bucket is always shown. Hiding it would make the shares add
 * to less than the total with nothing on screen explaining where the rest went,
 * which is the one thing a report must never do.
 */
function SplitPanel({
  title,
  slices,
  total,
  empty,
}: {
  title: string;
  slices: WorkSlice[];
  total: number;
  empty: string;
}) {
  const accounted = useMemo(() => slices.reduce((n, s) => n + s.seconds, 0), [slices]);

  return (
    <section className="panel">
      <h3>{title}</h3>
      {slices.length === 0 ? (
        <p className="caption">{empty}</p>
      ) : (
        <>
          <span className="day-bar" aria-hidden="true">
            {slices.map((s) => (
              <i
                key={s.id ?? "none"}
                style={{ width: `${s.share * 100}%`, background: s.colour }}
              />
            ))}
          </span>
          <div className="bars">
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
          {/* Said out loud rather than left to arithmetic: the split has to
              account for the total exactly once, and if it ever does not, the
              screen should be the thing that tells you. */}
          {accounted !== total && (
            <p className="caption">
              These add to {fmt.duration(accounted)} of {fmt.duration(total)} recorded.
            </p>
          )}
        </>
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
            <span>
              <strong className="display">{f.sessions}</strong>{" "}
              <span className="caption">{f.sessions === 1 ? "session" : "sessions"}</span>
            </span>
            <span>
              <strong className="display">{fmt.duration(f.trackedSec)}</strong>{" "}
              <span className="caption">tracked</span>
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
            <span className="data">{f.completed}</span> ran the full length they were plotted for.
            A session cut short because the work finished is not a failure.
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
