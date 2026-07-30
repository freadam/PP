/**
 * Reports (§3.6). Three panels, no more.
 *
 * Panel 2 uses the same plot/track encoding as the drift rail, rotated
 * horizontal. Consistency of encoding across scales is what makes a visual
 * language rather than a set of charts.
 */

import { useMemo } from "react";
import { useApp } from "../store/app";
import * as fmt from "../lib/format";
import { DriftBar } from "../components/DriftRail";
import { Empty } from "../components/chrome";

export function Reports() {
  const reports = useApp((s) => s.reports);

  const max = useMemo(() => {
    if (!reports) return 0;
    return Math.max(
      1,
      ...reports.projectWeeks.map((r) => Math.max(r.plannedSec, r.trackedSec)),
    );
  }, [reports]);

  if (!reports) return <Empty>Loading reports…</Empty>;

  const cal = reports.calibration;

  return (
    <div className="view-pad scroll-y">
      {/* ── 1. Calibration — the payoff for the entire drift concept ── */}
      <section className="panel">
        <h2>Calibration</h2>
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
                    {b.medianRatio > 1 && (
                      <span
                        className="tail"
                        style={{
                          left: "50%",
                          width: `${Math.min(50, (b.medianRatio - 1) * 50)}%`,
                        }}
                      />
                    )}
                  </span>
                ) : (
                  <span className="caption">
                    {b.n} of 5 samples — not enough to report
                  </span>
                )}
                <span className="data" style={{ textAlign: "right" }}>
                  {b.isReportable ? `${b.medianRatio.toFixed(2)}×` : "—"}{" "}
                  <span className="micro" style={{ color: "var(--faint)" }}>
                    n={b.n}
                  </span>
                </span>
              </div>
            ))}
            <p className="caption" style={{ marginTop: 8 }}>
              Median, not mean — one abandoned task would ruin a mean. Trailing 30 days.
            </p>
          </>
        )}
      </section>

      {/* ── 2. Planned vs tracked, per project per week ── */}
      <section className="panel">
        <h2>Planned vs tracked</h2>
        {reports.projectWeeks.length === 0 ? (
          <p className="caption">Nothing plotted or tracked in this range.</p>
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
          {reports.streakDays > 0 && (
            <> · {reports.streakDays}-day reconcile streak</>
          )}
        </p>
      </section>

      {/* ── 3. Weekly targets, with pace-to-date rather than a bare total ── */}
      <section className="panel">
        <h2>Weekly targets</h2>
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
                    <span className="rail-bar">
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
    </div>
  );
}
