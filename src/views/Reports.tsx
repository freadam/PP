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

import { useEffect, useMemo, useState } from "react";
import { useApp } from "../store/app";
import * as fmt from "../lib/format";
import type { MonthDay, MonthView } from "../lib/types";
import { DriftBar } from "../components/DriftRail";
import { Empty } from "../components/chrome";

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
 * The wireframe wants solid = unplanned against dashed = planned. Planned is
 * flat zero and will stay there until entertainment windows exist — which is
 * not a placeholder, it is the correct reading: with no way to plan
 * entertainment, every minute of it is unplanned by definition. The note under
 * the chart says exactly that rather than leaving a mystery line at the axis.
 */
function EntertainmentTrend({ month }: { month: MonthView }) {
  const { path, plannedPath, max } = useMemo(() => {
    const days = month.days;
    const max = Math.max(3600, ...days.map((d) => d.entertainmentSec));
    const x = (i: number) => (i / Math.max(1, days.length - 1)) * 500;
    const y = (sec: number) => 150 - (sec / max) * 140;
    return {
      max,
      path: days.map((d, i) => `${x(i)},${y(d.entertainmentSec)}`).join(" "),
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
          aria-label={`Entertainment per day across ${month.label}. Total ${fmt.duration(
            month.totals.entertainmentSec,
          )}, peak ${fmt.duration(worst?.entertainmentSec ?? 0)}.`}>
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
        Solid: unplanned · dashed: planned. Peak {fmt.duration(max)} in a day.
      </p>
      {month.plannedEntertainmentNote && (
        <p className="caption">{month.plannedEntertainmentNote}</p>
      )}
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
