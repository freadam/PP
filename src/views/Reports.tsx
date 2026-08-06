/**
 * Reports (wireframe screen 3) — **month-first**.
 *
 * The plan makes month the default reporting horizon, so that is what this
 * opens to: six cards and four panels, answering the four questions the client
 * asked in the order they asked them —
 *
 *   where did entertainment go · how good is the data · did life get its share
 *   · what should I look at.
 *
 * The Week horizon keeps the calibration and project panels, which are about
 * estimates rather than about how the month went. Day is the Day screen, and
 * this simply takes you there rather than building a third version of it.
 *
 * Every figure here is `get_month`, which is `get_day` summed over the month —
 * so a number on this screen and the same number on a day cannot disagree.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { useApp } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type {
  Fragmentation,
  GoalDirection,
  GoalProgress,
  GoalState,
  GoalTemplate,
  MonthDay,
  MonthView,
  WeekReview,
} from "../lib/types";
import { METRIC } from "../lib/types";
import { DriftBar } from "../components/DriftRail";
import { Empty } from "../components/chrome";
import { WeekReportPanel } from "../components/WeekReportCard";

type Horizon = "week" | "month";

export function Reports() {
  const horizon = useApp((s) => s.reportHorizon);
  const setHorizon = useApp((s) => s.setReportHorizon);
  const go = useApp((s) => s.go);
  const month = useApp((s) => s.month);
  const monthKey = useApp((s) => s.monthKey);
  const setMonthKey = useApp((s) => s.setMonthKey);
  const load = useApp((s) => s.loadMonth);

  useEffect(() => {
    void load();
  }, [load]);

  const shift = (dir: -1 | 1) => {
    const [y, m] = monthKey.split("-").map(Number);
    const d = new Date(Date.UTC(y!, m! - 1 + dir, 1));
    setMonthKey(`${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, "0")}`);
  };

  return (
    <div className="view-pad scroll-y">
      <div className="context-bar">
        <h1 className="display">{month?.label ?? monthKey}</h1>
        <button className="btn" aria-label="Previous month" onClick={() => shift(-1)}>
          ‹
        </button>
        <button className="btn" onClick={() => setMonthKey(fmt.today().slice(0, 7))}>
          This month
        </button>
        <button className="btn" aria-label="Next month" onClick={() => shift(1)}>
          ›
        </button>
        <div className="segmented" role="group" aria-label="Horizon">
          <button className="btn" onClick={() => go("day")} title="The Day screen is the day horizon">
            Day
          </button>
          {(["week", "month"] as Horizon[]).map((h) => (
            <button
              key={h}
              className="btn"
              aria-pressed={horizon === h}
              onClick={() => setHorizon(h)}
            >
              {h === "week" ? "Week" : "Month"}
            </button>
          ))}
        </div>
        <span className="grow" />
        <button className="btn" onClick={() => go("import")}>
          Import a workbook
        </button>
        <button className="btn btn-primary" onClick={() => go("export")}>
          Export month to Excel
        </button>
      </div>

      {horizon === "month" ? <MonthDashboard month={month} /> : <WeekReports />}
    </div>
  );
}

function MonthDashboard({ month }: { month: MonthView | null }) {
  if (!month) return <Empty>Loading the month…</Empty>;
  const t = month.totals;

  const cards = [
    { label: "Accounted", value: `${Math.round(month.accountedRatio * 100)}%`, cls: "l-work" },
    { label: "Work", value: fmt.duration(t.confirmedWorkSec), cls: "l-work" },
    { label: "Life", value: fmt.duration(t.confirmedLifeSec - t.sleepSec), cls: "l-life" },
    { label: "Sleep", value: fmt.duration(t.sleepSec), cls: "l-sleep" },
    { label: "Entertainment", value: fmt.duration(t.entertainmentSec), cls: "l-fun" },
    // Elapsed, not whole-month: "Accounted 40%" beside "Unaccounted 696h"
    // would be two readings of the same month, and the 696 counts days that
    // have not happened yet.
    {
      label: "Unaccounted",
      value: fmt.duration(month.elapsedEmptySec),
      cls: "l-empty",
      hatched: true,
    },
  ];

  return (
    <>
      <div className="cards">
        {cards.map((c) => (
          <div key={c.label} className="card" data-hatched={c.hatched}>
            <span className="micro">
              <i className={`swatch ${c.cls}`} aria-hidden="true" />
              {c.label}
            </span>
            <strong className="data">{c.value}</strong>
          </div>
        ))}
      </div>
      <p className="caption">
        {fmt.duration(month.elapsedSec)} of {month.label} has happened. Every figure above is
        measured against that, not against the whole month.
      </p>

      <div className="dashboard">
        <EntertainmentTrend month={month} />
        <DataQuality month={month} />
        <AreaTargets month={month} />
        <Findings month={month} />
      </div>
    </>
  );
}

/**
 * Entertainment per day across the month.
 *
 * Solid is entertainment nobody plotted a window for; dashed is the windows
 * themselves. The two reconcile against the day's total by construction —
 * `entertainmentSec` splits into the part inside a window and the part outside
 * it, and the solid line is the second of those. So the distance between the
 * lines is a real quantity and not a mood: it is how much of the month went to
 * entertainment that was never the plan.
 */
function EntertainmentTrend({ month }: { month: MonthView }) {
  const { path, plannedPath, max, unplannedSec } = useMemo(() => {
    const days = month.days;
    const unplanned = (d: MonthDay) =>
      Math.max(0, d.entertainmentSec - d.entertainmentInWindowSec);
    const max = Math.max(
      3600,
      ...days.map((d) => Math.max(unplanned(d), d.plannedEntertainmentSec)),
    );
    const x = (i: number) => (i / Math.max(1, days.length - 1)) * 500;
    const y = (sec: number) => 150 - (sec / max) * 140;
    return {
      max,
      unplannedSec:
        month.totals.entertainmentSec - month.totals.entertainmentInWindowSec,
      path: days.map((d, i) => `${x(i)},${y(unplanned(d))}`).join(" "),
      plannedPath: days.map((d, i) => `${x(i)},${y(d.plannedEntertainmentSec)}`).join(" "),
    };
  }, [month]);

  const worst = month.days.reduce<MonthDay | null>(
    (a, b) => (a && a.entertainmentSec >= b.entertainmentSec ? a : b),
    null,
  );

  return (
    <section className="panel">
      <h3>Entertainment · planned vs unplanned</h3>
      <div className="linechart">
        <svg viewBox="0 0 500 160" preserveAspectRatio="none" role="img"
          aria-label={`Unplanned entertainment per day across ${month.label}. ${fmt.duration(
            unplannedSec,
          )} unplanned in total, peak day ${fmt.duration(worst?.entertainmentSec ?? 0)}.`}>
          <polyline points={path} fill="none" stroke="var(--over)" strokeWidth="2.5" />
          <polyline
            points={plannedPath}
            fill="none"
            stroke="var(--plot)"
            strokeWidth="2"
            strokeDasharray="6 5"
          />
        </svg>
      </div>
      <p className="caption">
        Solid: unplanned · dashed: planned windows. Peak {fmt.duration(max)} in a day.
      </p>
      <p className="caption">
        {fmt.duration(month.totals.entertainmentSec)} of entertainment this month:{" "}
        {fmt.duration(month.totals.entertainmentInWindowSec)} inside a window you
        plotted, {fmt.duration(unplannedSec)} outside one.
        {month.totals.plannedEntertainmentSec === 0 &&
          " No windows plotted yet — plot one from the Planner and the dashed line" +
            " becomes the evening you meant to have."}
      </p>
    </section>
  );
}

/**
 * A day-by-day heatmap of how much of each day is accounted for.
 *
 * Shade is the ratio; the numeral is always present, so nothing here is carried
 * by colour alone (M16). An unreconciled day gets a corner mark, because "not
 * accounted" and "not reviewed" are different problems with different fixes.
 */
function DataQuality({ month }: { month: MonthView }) {
  return (
    <section className="panel">
      <h3>Data quality · {month.label.split(" ")[0]}</h3>
      <div className="heatmap">
        {month.days.map((d) => (
          <span
            key={d.localDate}
            className="heat"
            data-level={Math.min(3, Math.floor(d.accountedRatio * 4))}
            data-future={!d.hasHappened || undefined}
            data-unreconciled={(d.hasHappened && !d.isReconciled) || undefined}
            title={
              d.hasHappened
                ? `${d.localDate} · ${Math.round(d.accountedRatio * 100)}% accounted${
                    d.isReconciled ? "" : " · not reconciled"
                  }`
                : `${d.localDate} · hasn't happened yet`
            }
          >
            <span className="micro">{d.dayOfMonth}</span>
          </span>
        ))}
      </div>
      <p className="caption">
        {month.unreconciledDays} unreconciled day{month.unreconciledDays === 1 ? "" : "s"} ·{" "}
        {fmt.duration(month.totals.observedOnlySec)} observed-only. Darker is better accounted;
        a corner mark means the day was never reviewed.
      </p>
    </section>
  );
}

function AreaTargets({ month }: { month: MonthView }) {
  const withTargets = month.totals.byArea.filter((a) => a.monthlyTargetSec);
  return (
    <section className="panel">
      <h3>Life-area targets vs actual</h3>
      {withTargets.length === 0 ? (
        <p className="caption">
          No life area has a monthly target yet. Set one and this becomes the panel that says
          whether the month went where you meant it to.
        </p>
      ) : (
        <div className="bars">
          {withTargets.map((a) => {
            const pct = Math.round((a.seconds / (a.monthlyTargetSec || 1)) * 100);
            return (
              <div key={a.areaId} className="bar-row">
                <span className="micro">
                  <i className="swatch" style={{ background: a.colour }} aria-hidden="true" />
                  {a.name}
                </span>
                {/* Decorative: the percentage and the hours are both text on
                    this row already (I3). */}
                <span className="bar" aria-hidden="true">
                  <i style={{ width: `${Math.min(100, pct)}%`, background: a.colour }} />
                </span>
                <b className="data">{pct}%</b>
                <span className="micro" style={{ color: "var(--muted)" }}>
                  {fmt.duration(a.seconds)} / {fmt.duration(a.monthlyTargetSec!)}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}

function Findings({ month }: { month: MonthView }) {
  const go = useApp((s) => s.go);
  const setDayDate = useApp((s) => s.setDayDate);
  const worst = month.days.reduce<MonthDay | null>(
    (a, b) => (a && a.emptySec >= b.emptySec ? a : b),
    null,
  );

  return (
    <section className="panel">
      <h3>Monthly findings</h3>
      <div className="warning-list">
        {month.findings.map((f) => (
          <div key={f.key} data-warning={f.isWarning || undefined}>
            <span>
              {f.label}
              {f.detail && <span className="caption"> — {f.detail}</span>}
            </span>
            <b className="data">{f.value}</b>
          </div>
        ))}
      </div>
      {/* The panel's whole point is that a finding is a place to go, not a
          number to read. */}
      <button
        className="btn"
        disabled={!worst}
        onClick={() => {
          if (!worst) return;
          setDayDate(worst.localDate);
          go("day");
        }}
      >
        Review source intervals
        {worst && <span className="micro">— {worst.localDate}</span>}
      </button>
    </section>
  );
}

/** The estimate-accuracy half: about how good the plan is, not how the month went. */
function WeekReports() {
  const reports = useApp((s) => s.reports);
  const load = useApp((s) => s.loadReports);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    if (!loaded) {
      setLoaded(true);
      void load();
    }
  }, [loaded, load]);

  const max = useMemo(() => {
    if (!reports) return 0;
    return Math.max(1, ...reports.projectWeeks.map((r) => Math.max(r.plannedSec, r.trackedSec)));
  }, [reports]);

  if (!reports) return <Empty>Loading reports…</Empty>;
  const cal = reports.calibration;

  return (
    <>
      {/* The report first, then the goals it is a report on. W9 asks for the
          headline at the top and means it: everything below this is depth. */}
      <WeekReportPanel date={fmt.today()} />
      <Goals />
      <section className="panel">
        <h3>Calibration</h3>
        {cal.sampleCount < 5 ? (
          <p className="caption">
            Come back after five tracked tasks — calibration needs a few estimates to compare.
            {cal.sampleCount > 0 && <> You have {cal.sampleCount}.</>}
          </p>
        ) : (
          <>
            <p className="display" style={{ fontSize: "1.125rem", margin: "0 0 12px" }}>
              {cal.headline}
            </p>
            {cal.buckets.map((b) => (
              <div key={b.bucket} className="bucket-row" data-reportable={b.isReportable}>
                <span className="data">{b.bucket}</span>
                {b.isReportable ? (
                  <span
                    className="rail-bar"
                    role="img"
                    aria-label={`${b.bucket} estimates run ${b.medianRatio.toFixed(2)} times over, from ${b.n} samples`}
                  >
                    {/* 1.0× sits at the midpoint, so over and under read as a
                        deflection from the plot line rather than a bare length. */}
                    <span className="plot" style={{ left: 0, width: "50%" }} />
                    <span
                      className="track"
                      style={{ width: `${Math.min(100, b.medianRatio * 50)}%` }}
                    />
                  </span>
                ) : (
                  <span className="caption">needs {5 - b.n} more</span>
                )}
                <span className="data">
                  {b.isReportable ? `${b.medianRatio.toFixed(2)}× · n=${b.n}` : `n=${b.n}`}
                </span>
              </div>
            ))}
          </>
        )}
      </section>

      <section className="panel">
        <h3>Planned vs tracked, by project and week</h3>
        {reports.projectWeeks.length === 0 ? (
          <p className="caption">Nothing plotted in this range yet.</p>
        ) : (
          <div className="stack">
            {reports.projectWeeks.map((row) => (
              <div key={`${row.projectId}-${row.weekStart}`} className="row">
                <span className="dot" style={{ background: row.projectColour }} />
                <span style={{ width: 150, overflow: "hidden", textOverflow: "ellipsis" }}>
                  {row.projectName}
                </span>
                <span className="data caption" style={{ width: 84 }}>
                  {row.weekStart}
                </span>
                <span className="grow">
                  <DriftBar planned={row.plannedSec} tracked={row.trackedSec} max={max} />
                </span>
                <span className="data" style={{ width: 132, textAlign: "right" }}>
                  {fmt.duration(row.plannedSec)} → {fmt.duration(row.trackedSec)}
                </span>
              </div>
            ))}
          </div>
        )}
        <p className="caption" style={{ marginTop: 12 }}>
          Totals: plotted <span className="data">{fmt.duration(reports.totalPlannedSec)}</span>,
          tracked <span className="data">{fmt.duration(reports.totalTrackedSec)}</span>
          {reports.streakDays > 0 && <> · {reports.streakDays}-day reconcile streak</>}
        </p>
      </section>

      <section className="panel">
        <h3>Weekly targets</h3>
        {reports.weeklyTargets.length === 0 ? (
          <p className="caption">
            No project has a weekly target yet. Set one on a project to track pace.
          </p>
        ) : (
          <div className="stack">
            {reports.weeklyTargets.map((t) => {
              const pct = Math.min(100, (t.trackedSec / t.targetSec) * 100);
              const pacePct = Math.min(100, (t.paceSec / t.targetSec) * 100);
              const behind = t.trackedSec < t.paceSec;
              return (
                <div key={t.projectId} className="row">
                  <span className="dot" style={{ background: t.projectColour }} />
                  <span style={{ width: 150 }}>{t.projectName}</span>
                  <span className="grow">
                    {/* Decorative (I3): the tracked total, the target and
                        "behind pace" are all read out as text on this row. */}
                    <span className="rail-bar" aria-hidden="true">
                      <span className="track" style={{ width: `${pct}%` }} />
                      {/* The pace marker is the plot line: where you should be by now. */}
                      <span className="plot" style={{ left: 0, width: `${pacePct}%` }} />
                    </span>
                  </span>
                  <span className="data" style={{ width: 210, textAlign: "right" }}>
                    {fmt.duration(t.trackedSec)} / {fmt.duration(t.targetSec)}{" "}
                    <span className="micro" style={{ color: behind ? "var(--over)" : "var(--done)" }}>
                      {behind ? "behind pace" : "on pace"}
                    </span>
                  </span>
                </div>
              );
            })}
          </div>
        )}
      </section>
    </>
  );
}

/**
 * Goals, and pace (PLAN-WEEKLY-GOALS.md W1/W2).
 *
 * The week is the horizon where a person can still change the outcome — a month
 * dashboard is a verdict delivered too late to act on, and a day is too short a
 * window to see a habit in. So this panel is first on the week screen, above the
 * calibration and project panels, which are about estimate accuracy rather than
 * about how the week is going.
 *
 * **Pace, not a scoreboard.** Each row leads with the sentence Rust built —
 * *"3h 20m a day for the remaining 3 days"* — because that is a decision you can
 * make at breakfast, where "62% of target" is a fact. The bar behind it shows
 * where the elapsed days say you would be, so being ahead or behind is visible
 * without reading a number.
 *
 * A goal at zero on Monday morning reads **on pace**, and that is not a
 * courtesy: an app that reports the future as a failure is one whose numbers you
 * learn to discount.
 */
function Goals() {
  const run = useApp((s) => s.run);
  const [review, setReview] = useState<WeekReview | null>(null);
  const [adding, setAdding] = useState(false);

  const load = useCallback(async () => {
    const r = await ipc.getWeekReview(fmt.today(), fmt.tz()).catch(() => null);
    setReview(r);
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  if (!review) return null;

  return (
    <section className="panel">
      <div className="row">
        <h3 className="grow">This week</h3>
        <span className="data caption">
          {review.from} – {review.to}
        </span>
        <button className="btn" aria-expanded={adding} onClick={() => setAdding(!adding)}>
          {adding ? "Cancel" : "Add a goal"}
        </button>
      </div>

      {adding && (
        <>
          <Templates
            onSaved={() => {
              setAdding(false);
              void load();
            }}
          />
          <GoalForm
            onSaved={() => {
              setAdding(false);
              void load();
            }}
          />
        </>
      )}

      {review.goals.length === 0 ? (
        <p className="caption">
          No goals yet. A goal is a target with a <b>direction</b> — &ldquo;at least 20h of
          work&rdquo; or &ldquo;at most 5h of entertainment&rdquo; — and the second kind is
          one you succeed at by staying under it.
        </p>
      ) : (
        <div className="stack" style={{ gap: 10 }}>
          {review.goals.map((g) => (
            <GoalRowView
              key={g.goal.id}
              p={g}
              onEnd={async () => {
                const ok = await run(
                  () => ipc.endGoal(g.goal.id, fmt.today()),
                  "Couldn't end that goal.",
                );
                if (ok !== null) void load();
              }}
            />
          ))}
        </div>
      )}
      <FragmentationPanel now={review.fragmentation} before={review.previousFragmentation} />

      {review.calibration.length > 0 && (
        <div className="stack" style={{ gap: 4, marginTop: 8 }}>
          {review.calibration.map((c) => (
            <div key={c.goalId} className="row">
              <span className="grow caption">{c.summary}</span>
              {/* Offered, never applied. You may have a good reason the last
                  three weeks were atypical, and an app that quietly lowers your
                  ambitions is worse than one that says nothing. */}
              <button
                className="btn micro"
                onClick={async () => {
                  const ok = await run(
                    () =>
                      ipc.setGoal(
                        {
                          subjectKind: c.subjectKind,
                          subjectId: c.subjectId,
                          direction: c.direction,
                          targetSec: c.suggestedSec,
                        },
                        fmt.today(),
                      ),
                    "Couldn't change that goal.",
                  );
                  if (ok) void load();
                }}
              >
                Use {fmt.duration(c.suggestedSec)}
              </button>
            </div>
          ))}
        </div>
      )}

      {review.unreconciledDays > 0 && (
        <p className="caption">
          {review.unreconciledDays} day{review.unreconciledDays === 1 ? "" : "s"} this week
          {review.unreconciledDays === 1 ? " has" : " have"} not been reconciled, so these
          figures count only what has been recorded so far.
        </p>
      )}
    </section>
  );
}

/** The state words, in the order a person would rank them. Never colour alone. */
const GOAL_STATE: Record<GoalState, string> = {
  met: "met",
  onPace: "on pace",
  behind: "behind",
  atRisk: "at risk",
  over: "over",
};

function GoalRowView({ p, onEnd }: { p: GoalProgress; onEnd: () => void }) {
  // The bar is the *target*; the fill is what happened; the notch is where the
  // elapsed days say you would be. Three marks, so ahead-or-behind reads without
  // arithmetic.
  const pct = (n: number) => `${Math.min(100, Math.max(0, (n / p.goal.targetSec) * 100))}%`;
  const over = p.state === "over";

  return (
    <div className="stack" style={{ gap: 4 }}>
      <div className="row">
        <span className="grow">
          <b>{p.goal.subjectName}</b>{" "}
          <span className="caption">
            {p.goal.direction === "atLeast" ? "at least" : "at most"}{" "}
            <span className="data">{fmt.duration(p.goal.targetSec)}</span>
            {p.applicableDays < 7 && <> · {p.applicableDays} days a week</>}
          </span>
        </span>
        <span className="tag micro">{GOAL_STATE[p.state]}</span>
        <span className="data caption">{fmt.duration(p.actualSec)}</span>
        <button className="btn micro" aria-label={`End the ${p.goal.subjectName} goal`} onClick={onEnd}>
          End
        </button>
      </div>
      <span className="bar" aria-hidden="true" style={{ position: "relative" }}>
        <i
          style={{
            width: pct(p.actualSec),
            background: over ? "var(--over)" : "var(--track)",
          }}
        />
        {/* Where the elapsed days say you would be. Absent on Monday morning,
            when nothing is owed yet. */}
        {p.expectedSec > 0 && (
          <span
            className="bar-notch"
            style={{ left: pct(p.expectedSec) }}
            title="expected by now"
          />
        )}
      </span>
      <span className="caption">{p.summary}</span>
    </div>
  );
}

/**
 * Adding a goal. Five metrics, two directions, a number.
 *
 * Deliberately not a builder over every subject the core supports — life areas,
 * projects and labels are all valid subjects, and offering all four kinds here
 * would be exactly the "configuring the tool becomes the work" failure the plan
 * is written against. The five whole-day quantities are the ones a week is
 * actually about.
 */
function GoalForm({ onSaved }: { onSaved: () => void }) {
  const run = useApp((s) => s.run);
  const [subjectId, setSubjectId] = useState<string>(METRIC.allWork);
  const [direction, setDirection] = useState<GoalDirection>("atLeast");
  const [hours, setHours] = useState("20");
  const [weekdaysOnly, setWeekdaysOnly] = useState(false);

  const save = async () => {
    const target = Math.round(Number(hours) * 3600);
    if (!Number.isFinite(target) || target <= 0) return;
    const ok = await run(
      () =>
        ipc.setGoal(
          {
            subjectKind: "metric",
            subjectId,
            direction,
            targetSec: target,
            // Monday–Friday is 0b0011111. A weekday goal must not report you
            // behind on a Sunday morning, which is what the bitmask prevents.
            appliesDays: weekdaysOnly ? 31 : null,
          },
          fmt.today(),
        ),
      "Couldn't save that goal.",
    );
    if (ok) onSaved();
  };

  return (
    <div className="row" style={{ flexWrap: "wrap", gap: 8, marginBottom: 8 }}>
      <select value={subjectId} onChange={(e) => setSubjectId(e.target.value)} aria-label="Measure">
        {Object.values(METRIC).map((m) => (
          <option key={m} value={m}>
            {m === "allWork" ? "Work" : m[0]!.toUpperCase() + m.slice(1)}
          </option>
        ))}
      </select>
      <select
        value={direction}
        onChange={(e) => setDirection(e.target.value as GoalDirection)}
        aria-label="Direction"
      >
        <option value="atLeast">at least</option>
        <option value="atMost">at most</option>
      </select>
      <input
        type="number"
        min={0.5}
        step={0.5}
        value={hours}
        aria-label="Hours a week"
        onChange={(e) => setHours(e.target.value)}
        style={{ width: 80 }}
      />
      <span className="caption">hours a week</span>
      <button
        className="btn"
        aria-pressed={weekdaysOnly}
        onClick={() => setWeekdaysOnly(!weekdaysOnly)}
      >
        Weekdays only
      </button>
      <button className="btn btn-primary" onClick={() => void save()}>
        Save goal
      </button>
    </div>
  );
}

/**
 * How broken up the week's work was — the components, and **not** a score.
 *
 * Three numbers, each checkable against the Day view that produced it. A
 * weighted 0–100 would be the one figure in this app nobody could verify, and
 * the standing complaint about focus scores is exactly that their inputs are
 * hidden.
 *
 * Each is shown against last week, because direction of travel is the whole
 * reading: one week's longest stretch in isolation says nothing.
 */
function FragmentationPanel({ now, before }: { now: Fragmentation; before: Fragmentation }) {
  if (now.stretches === 0) return null;

  const rows = [
    {
      label: "Longest unbroken stretch",
      value: fmt.duration(now.longestStretchSec),
      was: before.longestStretchSec,
      is: now.longestStretchSec,
      // Longer is better here; for the two below, fewer is.
      better: 1,
      note: "on one task",
    },
    {
      label: "Unplanned switches",
      value: String(now.unplannedSwitches),
      was: before.unplannedSwitches,
      is: now.unplannedSwitches,
      better: -1,
      note: `${now.plannedSwitches} more landed on a block boundary — those are the plan working`,
    },
    {
      label: "Time in short runs",
      value: fmt.duration(now.fragmentedSec),
      was: before.fragmentedSec,
      is: now.fragmentedSec,
      better: -1,
      note: `runs under ${fmt.duration(now.fragmentThresholdSec)}`,
    },
  ];

  return (
    <div className="stack" style={{ gap: 6, marginTop: 12 }}>
      <h4 className="micro">How broken up the week was</h4>
      {rows.map((r) => {
        const delta = r.is - r.was;
        const word = delta === 0 ? "same as last week" : delta * r.better > 0 ? "better" : "worse";
        return (
          <div key={r.label} className="row">
            <span className="grow">
              {r.label} <span className="caption">· {r.note}</span>
            </span>
            <span className="data">{r.value}</span>
            {/* Never colour alone: the word carries it. */}
            <span className="tag micro">{word}</span>
          </div>
        );
      })}
    </div>
  );
}

/**
 * Goal templates — the numbers come from your own weeks.
 *
 * A blank goal form asks for a figure nobody has, and the figure people invent
 * is a round one they do not believe. Each template says where its number came
 * from; where there is not enough history it says that instead of guessing,
 * which is the difference between a suggestion and a wish.
 */
function Templates({ onSaved }: { onSaved: () => void }) {
  const run = useApp((s) => s.run);
  const [templates, setTemplates] = useState<GoalTemplate[]>([]);

  useEffect(() => {
    void ipc
      .getGoalTemplates(fmt.today(), fmt.tz())
      .then(setTemplates)
      .catch(() => setTemplates([]));
  }, []);

  if (templates.length === 0) return null;

  return (
    <div className="stack" style={{ gap: 6, marginBottom: 8 }}>
      {templates.map((t) => (
        <div key={t.key} className="row">
          <span className="grow">
            <b>{t.name}</b>{" "}
            <span className="caption">
              {t.direction === "atLeast" ? "at least" : "at most"}{" "}
              {t.targetSec === null ? "—" : fmt.duration(t.targetSec)} · {t.rationale}
            </span>
          </span>
          <button
            className="btn micro"
            // A template with no number cannot be applied, and says why rather
            // than offering a dead button with no explanation.
            disabled={t.targetSec === null}
            onClick={async () => {
              if (t.targetSec === null) return;
              const ok = await run(
                () =>
                  ipc.setGoal(
                    {
                      subjectKind: t.subjectKind,
                      subjectId: t.subjectId,
                      direction: t.direction,
                      targetSec: t.targetSec!,
                      appliesDays: t.appliesDays,
                    },
                    fmt.today(),
                  ),
                "Couldn't use that template.",
              );
              if (ok) onSaved();
            }}
          >
            Use
          </button>
        </div>
      ))}
    </div>
  );
}
