/**
 * The landing screen the user actually opens the app to see.
 *
 * Interview 1, unchanged for the whole project: "top 3 tasks scheduled for the
 * day, tasks I logged time on yesterday but didn't mark complete." That is not
 * the Projects list and it is not the raw Day grid — it is a small,
 * forward-looking surface that answers "what am I doing now?" in one glance and
 * then gets out of the way.
 *
 * It invents no data. It composes three things the store already holds:
 *
 *   • today's plotted work            → `day.slots[].plans`, deduplicated
 *   • still open from before today    → `unfinished_before`, an inference over
 *                                        sessions, not a list anyone maintains
 *   • the day so far, and its honesty → `day.totals` + `HonestyCard`
 *
 * Every row's primary action is **start**, because the point of a landing
 * screen in a *capture* tool is to put the next honest trace one keystroke
 * away. This screen is a launcher for the timer, not a place to manage a
 * backlog — anything that would need a second decision belongs on Planner or
 * Projects, and links there rather than growing a control here.
 *
 * What it deliberately does not do is show drift (A11). Today reports totals;
 * plan-versus-actual is the harder redesign and it is carried, not cut.
 */

import { useMemo } from "react";
import { useApp } from "../store/app";
import * as fmt from "../lib/format";
import { HonestyCard } from "../components/HonestyCard";
import { captureSplit } from "../lib/honesty";
import { Empty } from "../components/chrome";
import type { DayPlan, DayView, TaskRow } from "../lib/types";

/** Interview 1 says three. Three is the whole point: a fourth is a backlog. */
const TOP_N = 3;

/**
 * The day's plotted work, in clock order, one entry per block.
 *
 * `day.slots` repeats a plan in every row it crosses — the grid is a lens, so a
 * two-hour block appears in four thirty-minute rows. Deduplicating by block id
 * is the only thing this does; the ordering, the drift state and the tracked
 * seconds all arrive computed.
 *
 * Entertainment and life windows are filtered out: an evening plotted for a
 * film is a plan, but it is not the answer to "what am I working on now", and
 * putting a Start button on it would be an offer to time your own dinner.
 */
function plottedWork(day: DayView): DayPlan[] {
  const seen = new Set<string>();
  const plans: DayPlan[] = [];
  for (const slot of day.slots) {
    for (const plan of slot.plans) {
      if (plan.intent !== "work") continue;
      if (seen.has(plan.blockId)) continue;
      seen.add(plan.blockId);
      plans.push(plan);
    }
  }
  return plans.sort((a, b) => a.startsAt - b.startsAt);
}

export function Today() {
  const day = useApp((s) => s.day);
  const stillOpen = useApp((s) => s.stillOpen);
  const timer = useApp((s) => s.timer);
  const toggleTimer = useApp((s) => s.toggleTimer);
  const openDetail = useApp((s) => s.openDetail);
  const setDayDate = useApp((s) => s.setDayDate);
  const go = useApp((s) => s.go);

  const top = useMemo(() => (day ? plottedWork(day).slice(0, TOP_N) : []), [day]);

  if (!day) return <Empty>Loading today…</Empty>;

  const t = day.totals;
  const capture = captureSplit(day.segments);

  // The one fact that decides whether the day is on track: how much of it is
  // accounted for at all. Against the whole day, not a working window — the
  // counting invariant is over 24 hours, and quietly changing the denominator
  // here would put a number on screen that no other screen agrees with.
  const accountedSec = t.daySec - t.emptySec;
  const coverage = Math.round((accountedSec / Math.max(1, t.daySec)) * 100);

  const openDay = () => {
    setDayDate(day.localDate);
    go("day");
  };

  return (
    <div className="view-pad today">
      <header className="today-head">
        <div className="today-head-text">
          <h1 className="display">{fmt.longDate(day.localDate)}</h1>
          <p className="caption today-sub">
            <span className="data">{coverage}%</span> of the day accounted
            {capture.livePct !== null && (
              <>
                <span className="today-dot" aria-hidden="true">
                  ·
                </span>
                <span className="data">{Math.round(capture.livePct * 100)}%</span> of it
                captured live
              </>
            )}
          </p>
        </div>
        <HonestyCard segments={day.segments} />
      </header>

      {/* ── TOP 3 ───────────────────────────────────────────────────────
          What was plotted for today, in the order it was plotted. Not a
          priority ranking the app invented — the plan is the user's, and the
          first three of it are what the morning is actually about. */}
      <section className="today-block">
        <h2 className="title">Top {TOP_N}</h2>
        {top.length === 0 ? (
          <p className="caption today-empty">
            Nothing plotted for today.{" "}
            <button className="btn btn-quiet" onClick={() => go("planner")}>
              Plan the day →
            </button>
          </p>
        ) : (
          <ul className="today-list">
            {top.map((plan) => (
              <PlanRow
                key={plan.blockId}
                plan={plan}
                running={timer.runTaskId != null && timer.runTaskId === plan.taskId}
                onToggle={() => plan.taskId && void toggleTimer(plan.taskId, plan.blockId)}
                onOpen={() => plan.taskId && void openDetail(plan.taskId)}
              />
            ))}
          </ul>
        )}
      </section>

      {/* ── STILL OPEN ──────────────────────────────────────────────────
          The clause that made the whole thing one product: "logged time
          yesterday but didn't mark complete." It exists only because time
          attaches to tasks — an inference, not a list. It is capped and
          time-boxed in Rust, so it stays a set of loose ends rather than
          becoming a standing accusation. */}
      {stillOpen.length > 0 && (
        <section className="today-block">
          <h2 className="title">Still open</h2>
          <ul className="today-list">
            {stillOpen.map((task) => (
              <LooseEndRow
                key={task.id}
                task={task}
                running={timer.runTaskId === task.id}
                onToggle={() => void toggleTimer(task.id, null)}
                onOpen={() => void openDetail(task.id)}
              />
            ))}
          </ul>
        </section>
      )}

      {/* ── THE DAY SO FAR ──────────────────────────────────────────────
          A quiet read-out, not a dashboard. The partition the counting
          invariant guarantees, in one line, so the figure is legible without
          opening the full Day grid — and a way through to the grid, because
          the only thing to *do* with an unaccounted hour is reconcile it. */}
      <section className="today-block">
        <h2 className="title">The day so far</h2>
        <dl className="today-totals">
          <Stat label="Work" sec={t.confirmedWorkSec} />
          <Stat label="Life" sec={t.confirmedLifeSec - t.sleepSec} />
          <Stat label="Sleep" sec={t.sleepSec} />
          <Stat label="Entertainment" sec={t.entertainmentSec} />
          <Stat label="Observed only" sec={t.observedOnlySec} />
          <Stat label="Unaccounted" sec={t.emptySec} />
        </dl>
        <button className="btn btn-quiet today-reconcile" onClick={openDay}>
          Open the day to reconcile →
        </button>
      </section>
    </div>
  );
}

/** A block plotted for today. The whole row opens the task; the button times it. */
function PlanRow({
  plan,
  running,
  onToggle,
  onOpen,
}: {
  plan: DayPlan;
  running: boolean;
  onToggle: () => void;
  onOpen: () => void;
}) {
  const started = plan.trackedSec > 0;
  return (
    <li className="today-row" data-running={running}>
      <button className="today-row-main" onClick={onOpen} disabled={!plan.taskId}>
        <span className="today-row-title">{plan.title}</span>
        <span className="caption today-row-meta">
          <span className="data">{fmt.clockRange(plan.startsAt, plan.durationSec)}</span>
          {started && (
            <>
              <span className="today-dot" aria-hidden="true">
                ·
              </span>
              <span className="data">{fmt.duration(plan.trackedSec)}</span> tracked
            </>
          )}
        </span>
      </button>
      <div className="today-row-actions">
        {/* A block plotted with a bare label ("Standup") has no task to time.
            The row still renders — it is on the day and pretending otherwise
            would make the plan look emptier than it is — but the offer is
            withdrawn rather than made and then refused. */}
        {plan.taskId ? (
          <button className="btn btn-primary" onClick={onToggle}>
            {running ? "Stop" : started ? "Resume" : "Start"}
          </button>
        ) : (
          <span className="caption today-untimeable">no task</span>
        )}
      </div>
    </li>
  );
}

/** Work with time against it and no completion. Resume is the only sane verb. */
function LooseEndRow({
  task,
  running,
  onToggle,
  onOpen,
}: {
  task: TaskRow;
  running: boolean;
  onToggle: () => void;
  onOpen: () => void;
}) {
  return (
    <li className="today-row" data-running={running}>
      <button className="today-row-main" onClick={onOpen}>
        <span className="today-row-title">{task.title}</span>
        <span className="caption today-row-meta">
          <span className="data">{fmt.duration(task.trackedSec)}</span> logged
          {task.lastSessionAt != null && (
            <>
              <span className="today-dot" aria-hidden="true">
                ·
              </span>
              {/* Which day, not "yesterday": the list reaches back a week, and
                  a row that says yesterday about Tuesday is a small lie in the
                  one place the app is arguing for accuracy. */}
              last worked {fmt.weekdayShort(fmt.toLocalDate(new Date(task.lastSessionAt)))}
            </>
          )}
          <span className="today-dot" aria-hidden="true">
            ·
          </span>
          not marked done
        </span>
      </button>
      <div className="today-row-actions">
        <button className="btn btn-secondary" onClick={onToggle}>
          {running ? "Stop" : "Resume"}
        </button>
      </div>
    </li>
  );
}

function Stat({ label, sec }: { label: string; sec: number }) {
  return (
    <div className="today-stat">
      <dt className="micro">{label}</dt>
      {/* An em dash rather than 0m: nothing recorded and nothing spent are
          different facts, and only one of them is worth a number. */}
      <dd className="data">{sec > 0 ? fmt.duration(sec) : "—"}</dd>
    </div>
  );
}
