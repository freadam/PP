/**
 * Today — the default launch route (ARCHITECTURE §3.0, §7.5; PRODUCT-SPEC §7).
 *
 * The screen the wireframe draws, and the one the user can stay on for most of
 * a working day: the week strip and the week's honest allocation above, the
 * day's tasks on the left, the same day's schedule on the right.
 *
 * Four things it has to hold together:
 *
 * 1. **One selected date drives both columns** (invariant 26). They are two
 *    lenses on one day, not two screens that happen to sit beside each other.
 * 2. **The allocation is the Day Ledger, not the plan.** A plotted day with
 *    nothing tracked confirms nothing, and the donut says so — pending,
 *    observed and unaccounted time stay visible instead of being rounded into
 *    the confirmed slice.
 * 3. **Adjacent controls are still different commands** (§7.5). Ticking a task
 *    completes it, pressing Start opens a session, dragging it onto the
 *    timeline plots a block. None of the three implies either other one.
 * 4. **The weekday cue is a memory aid, not a streak.** It counts distinct
 *    tasks tracked and completed, an empty day costs nothing, and every state
 *    is carried by shape and text as well as colour (§7.6).
 */

import { useEffect, useRef, useState } from "react";
import { useApp } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type { BlockView, DaySection, PlannedTask, WeekAllocation, WeekDayCell } from "../lib/types";
import { Empty } from "../components/chrome";
import { useStartTaskDrag, taskDragDuration } from "../lib/useTaskDrag";
import { CaptureBar } from "./Tasks";

/* The timeboxing canvas's vertical scale is the shell's own `hourHeight`
   (`--hour-h`, 56px, Ctrl +/- scales it and it persists). Today and the Planner
   are two views of one canvas, so an invented constant here would mean the
   same hour was two different heights on two screens — and would ignore a zoom
   the user had already set. */
const SNAP_MIN = 15;

export function Today() {
  const today = useApp((s) => s.today);
  const loadToday = useApp((s) => s.loadToday);

  useEffect(() => {
    void loadToday();
  }, [loadToday]);

  if (!today) {
    return (
      <div className="today">
        <Empty>Loading today.</Empty>
      </div>
    );
  }

  return (
    <div className="today">
      <div className="today-head">
        <WeekStrip days={today.weekDays} />
        <AllocationCard allocation={today.weekAllocation} />
      </div>
      <div className="today-body">
        <TaskColumn />
        <ScheduleColumn />
      </div>
    </div>
  );
}

/* ─── the week strip (§7.3, §7.6) ───────────────────────────────────────── */

/**
 * Tracked volume in four buckets rather than a continuous ramp.
 *
 * A continuous ramp asks the eye to compare shades across a row, which nobody
 * can do; four steps are distinguishable, and the exact count is in the text
 * either way. The count is **distinct tasks**, so a day with one task and six
 * sessions reads as one task tracked — which is what it is.
 */
function intensity(trackedTasks: number): 0 | 1 | 2 | 3 {
  if (trackedTasks === 0) return 0;
  if (trackedTasks === 1) return 1;
  if (trackedTasks <= 3) return 2;
  return 3;
}

/**
 * What the cell says out loud. Every state named here is also carried by a
 * glyph and a numeral, because colour alone is not a status (§7.3, §18.3).
 */
function cellLabel(d: WeekDayCell): string {
  const parts = [fmt.longDate(d.localDate)];
  if (d.isToday) parts.push("today");
  if (d.isSelected) parts.push("selected");
  parts.push(`${d.plannedTaskCount} planned`);
  parts.push(`${d.trackedTaskCount} tracked`);
  parts.push(`${d.completedTaskCount} completed`);
  if (d.untrackedPlannedCount > 0) {
    parts.push(`${d.untrackedPlannedCount} planned with no tracked session`);
  }
  if (d.isReconciled) parts.push("reviewed");
  return parts.join(", ");
}

function WeekStrip({ days }: { days: WeekDayCell[] }) {
  const setTodayDate = useApp((s) => s.setTodayDate);
  const shiftTodayWeek = useApp((s) => s.shiftTodayWeek);
  const selected = useApp((s) => s.todayDate);

  return (
    <section className="week-strip" aria-label="This week">
      <header className="week-strip-head">
        <h2 className="label">This week</h2>
        <span className="grow" />
        <button className="btn" aria-label="Previous week" onClick={() => shiftTodayWeek(-1)}>
          ‹
        </button>
        <button className="btn" onClick={() => setTodayDate(fmt.today())} title="Today (T)">
          Today
        </button>
        <button className="btn" aria-label="Next week" onClick={() => shiftTodayWeek(1)}>
          ›
        </button>
      </header>
      <div className="week-days" role="group" aria-label="Select a day">
        {days.map((d) => (
          <button
            key={d.localDate}
            className="week-day"
            aria-pressed={d.isSelected}
            aria-label={cellLabel(d)}
            title={cellLabel(d)}
            data-today={d.isToday}
            data-selected={d.isSelected}
            data-intensity={intensity(d.trackedTaskCount)}
            onClick={() => setTodayDate(d.localDate)}
          >
            <span className="micro wd">{d.weekday}</span>
            <span className="num data">{d.dayOfMonth}</span>
            {/* Three independent marks, never inferred from one another
                (invariant 29): tracked volume, completions, and the reminder
                that something planned has no session against it yet. */}
            <span className="marks" aria-hidden="true">
              <span className="mark fill" data-on={d.trackedTaskCount > 0}>
                {d.trackedTaskCount > 0 ? d.trackedTaskCount : ""}
              </span>
              <span className="mark check" data-on={d.completedTaskCount > 0}>
                {d.completedTaskCount > 0 ? `✓${d.completedTaskCount}` : ""}
              </span>
              {/* A hollow ring, not a warning. On today it is a nudge; on a
                  past day it is a fact. Nothing is lost by leaving it. */}
              <span className="mark pending" data-on={d.untrackedPlannedCount > 0}>
                {d.untrackedPlannedCount > 0 ? "○" : ""}
              </span>
            </span>
            {selected === d.localDate && <span className="sel-rule" aria-hidden="true" />}
          </button>
        ))}
      </div>
    </section>
  );
}

/* ─── the week allocation (§7.5, invariant 18) ──────────────────────────── */

interface Slice {
  key: string;
  label: string;
  sec: number;
  colour: string;
  /** Confirmed states carry the headline; the rest stay legible but quiet. */
  prominent: boolean;
  note: string;
}

function slices(a: WeekAllocation): Slice[] {
  return [
    {
      key: "work",
      label: "Work",
      sec: a.verifiedWorkSec,
      colour: "var(--track-graphic)",
      prominent: true,
      note: "Sessions you reviewed and accepted.",
    },
    {
      key: "life",
      label: "Life",
      sec: a.verifiedLifeSec,
      colour: "var(--done)",
      prominent: true,
      note: "Confirmed non-task time.",
    },
    {
      key: "sleep",
      label: "Sleep",
      sec: a.sleepRestSec,
      colour: "var(--plot)",
      prominent: true,
      note: "Confirmed rest.",
    },
    {
      key: "pending",
      label: "Pending",
      sec: a.trackedPendingSec,
      colour: "var(--over)",
      prominent: false,
      note: "A timer ran. Nobody has verified it yet.",
    },
    {
      key: "private",
      label: "Private",
      sec: a.privateSec,
      colour: "var(--muted)",
      prominent: false,
      note: "Accounted for, deliberately unrecorded.",
    },
    {
      key: "observed",
      label: "Observed",
      sec: a.observedUnconfirmedSec,
      colour: "var(--line-strong)",
      prominent: false,
      note: "The machine saw something. It is evidence, not work.",
    },
    {
      key: "unaccounted",
      label: "Unaccounted",
      sec: a.unaccountedSec,
      colour: "var(--sunken)",
      prominent: false,
      note: "No record of any kind. A real state, not a gap in the screen.",
    },
  ];
}

/**
 * The wireframe's donut, drawn as a stroked circle so the arithmetic is the
 * geometry: each slice's arc length is its seconds, and they close the ring
 * exactly because the six states tile the week (invariant 18). Nothing is
 * normalised or rounded to make it meet.
 */
function AllocationCard({ allocation }: { allocation: WeekAllocation }) {
  const parts = slices(allocation);
  const total = Math.max(1, allocation.periodSec);
  const R = 34;
  const C = 2 * Math.PI * R;

  let offset = 0;
  const arcs = parts.map((p) => {
    const length = (p.sec / total) * C;
    const arc = { ...p, dash: `${length} ${C - length}`, offset: -offset };
    offset += length;
    return arc;
  });

  const accounted = Math.round(allocation.accountedRatio * 100);

  return (
    <section className="alloc" aria-label="This week's allocation">
      <div className="alloc-ring">
        <svg viewBox="0 0 80 80" role="img" aria-label={`${accounted}% of elapsed time accounted for`}>
          <circle cx="40" cy="40" r={R} fill="none" stroke="var(--sunken)" strokeWidth="10" />
          {arcs.map((a) =>
            a.sec > 0 ? (
              <circle
                key={a.key}
                cx="40"
                cy="40"
                r={R}
                fill="none"
                stroke={a.colour}
                strokeWidth="10"
                strokeDasharray={a.dash}
                strokeDashoffset={a.offset}
                transform="rotate(-90 40 40)"
              />
            ) : null,
          )}
        </svg>
      </div>
      <ul className="alloc-legend">
        {parts.map((p) => (
          <li key={p.key} data-quiet={!p.prominent} title={p.note}>
            <span className="swatch" style={{ background: p.colour }} aria-hidden="true" />
            <span className="name">{p.label}</span>
            <span className="data val">{p.sec > 0 ? fmt.duration(p.sec) : "—"}</span>
          </li>
        ))}
      </ul>
      {/* §10.1 — a report that does not say how complete it is looks
          authoritative when it is not. */}
      <p className="micro alloc-foot">
        {accounted}% of elapsed time accounted for · {allocation.reviewedDays} of{" "}
        {allocation.elapsedDays} elapsed days reviewed
      </p>
    </section>
  );
}

/* ─── the task column (§7.4) ────────────────────────────────────────────── */

function TaskColumn() {
  const today = useApp((s) => s.today)!;
  const reorderDay = useApp((s) => s.reorderDay);
  const [showDone, setShowDone] = useState(false);

  const topFull = today.topThree.length >= 3;

  /* §7.4 — daily rank has a keyboard path, not only a drag. The row knows its
     own id; only this component knows its siblings, so the move lives here and
     the row calls it. Moving off either end is a no-op rather than a wrap:
     wrapping makes "hold the key" unpredictable. */
  const moveRank = (section: DaySection, taskId: string, delta: -1 | 1) => {
    const list = section === "top" ? today.topThree : today.moreToday;
    const ids = list.map((p) => p.task.id);
    const from = ids.indexOf(taskId);
    const to = from + delta;
    if (from < 0 || to < 0 || to >= ids.length) return;
    ids.splice(to, 0, ids.splice(from, 1)[0]!);
    void reorderDay(section, ids);
  };

  return (
    <section className="today-tasks" aria-label="Tasks for the selected day">
      <header className="today-col-head">
        <h2 className="display">
          {today.selectedDate === today.today ? "Today's tasks" : fmt.longDate(today.selectedDate)}
        </h2>
      </header>

      {/* §7.5 — capture here means "this day". A task typed on Today that
          landed in Inbox with no date would not appear on the screen it was
          typed on, which is the one place it was obviously meant to go. */}
      <CaptureBar defaultDueDate={today.selectedDate} />

      <h3 className="group-head label">
        Top 3
        <span className="data">{today.topThree.length}/3</span>
      </h3>
      {today.topThree.length === 0 ? (
        <p className="micro today-hint">
          Nothing chosen yet. Promote a task below to protect it for today.
        </p>
      ) : (
        today.topThree.map((p) => (
          <TaskLine key={p.task.id} planned={p} topFull={topFull} onMoveRank={moveRank} />
        ))
      )}

      <h3 className="group-head label">
        More for today
        <span className="data">{today.moreToday.length}</span>
      </h3>
      {today.moreToday.length === 0 ? (
        <p className="micro today-hint">
          Nothing else planned. Press <span className="kbd">C</span> to capture a task.
        </p>
      ) : (
        today.moreToday.map((p) => (
          <TaskLine key={p.task.id} planned={p} topFull={topFull} onMoveRank={moveRank} />
        ))
      )}

      {today.completedToday.length > 0 && (
        <>
          {/* Collapsed by default (§7.4): finished work is history the moment
              it is finished, and it should not crowd the working list. */}
          <button
            className="group-head label today-fold"
            aria-expanded={showDone}
            onClick={() => setShowDone((v) => !v)}
          >
            {showDone ? "▾" : "▸"} Completed
            <span className="data">{today.completedToday.length}</span>
          </button>
          {showDone &&
            today.completedToday.map((p) => (
              <TaskLine key={p.task.id} planned={p} topFull={topFull} />
            ))}
        </>
      )}

      <h3 className="group-head label">
        Tomorrow
        <span className="data">{today.tomorrowPreview.length}</span>
      </h3>
      {today.tomorrowPreview.length === 0 ? (
        <p className="micro today-hint">Nothing planned for tomorrow yet.</p>
      ) : (
        <ul className="tomorrow">
          {today.tomorrowPreview.slice(0, 5).map((p) => (
            <li key={p.task.id}>
              {/* Read-only except for opening the task (§7.4). Tomorrow is
                  orientation, not a second list to keep. */}
              <button className="btn btn-quiet tomorrow-open" onClick={() => useApp.getState().openDetail(p.task.id)}>
                {p.task.title}
              </button>
              {p.task.estimateSec != null && (
                <span className="data micro">{fmt.duration(p.task.estimateSec)}</span>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function TaskLine({
  planned,
  topFull,
  onMoveRank,
}: {
  planned: PlannedTask;
  topFull: boolean;
  onMoveRank?: (section: DaySection, taskId: string, delta: -1 | 1) => void;
}) {
  const { task, section } = planned;
  const timer = useApp((s) => s.timer);
  const toggleTimer = useApp((s) => s.toggleTimer);
  const setDaySection = useApp((s) => s.setDaySection);
  const removeFromDay = useApp((s) => s.removeFromDay);
  const openDetail = useApp((s) => s.openDetail);
  const startDrag = useStartTaskDrag();

  const done = task.status === "done";
  const running =
    timer.phase !== "idle" && (timer.session?.taskId ?? timer.runTaskId) === task.id;
  const inFocus = running && timer.focusEndsAt != null;

  const promote = section === "more";
  const canPromote = promote && !topFull;

  return (
    <div className="today-task" data-done={done} data-running={running}>
      {/* Three adjacent controls, three different commands (§7.5). */}
      <input
        type="checkbox"
        checked={done}
        aria-label={done ? `Reopen ${task.title}` : `Complete ${task.title}`}
        onChange={async () => {
          await useApp
            .getState()
            .run(() => ipc.setTaskStatus(task.id, done ? "open" : "done"));
          await useApp.getState().refresh();
        }}
      />

      <button
        className="today-task-title"
        onPointerDown={(e) => !done && startDrag(e, task)}
        onClick={() => void openDetail(task.id)}
        /* Alt+Arrow reorders within the section. Alt, not bare arrows, so the
           row stays scrollable and Tab-navigable — and so the shortcut cannot
           fire while someone is only moving between rows. */
        onKeyDown={(e) => {
          if (!onMoveRank || !e.altKey) return;
          if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
          e.preventDefault();
          onMoveRank(section, task.id, e.key === "ArrowUp" ? -1 : 1);
        }}
        title={`${task.title} — open, drag onto the schedule to plot ${fmt.duration(
          taskDragDuration(task),
        )}, or Alt+↑/↓ to reorder`}
      >
        <span className="t">{task.title}</span>
        <span className="meta micro">
          {planned.projectName && (
            <span className="proj">
              <span
                className="dot"
                style={{ background: planned.projectColour ?? "var(--muted)" }}
                aria-hidden="true"
              />
              {planned.projectName}
            </span>
          )}
          {task.estimateSec != null && (
            <span className="data">{fmt.duration(task.estimateSec)} est</span>
          )}
          {/* Tracked *today*, beside the estimate, because the row is asking
              whether this is being tracked now — not what it has ever cost. */}
          {planned.trackedTodaySec > 0 && (
            <span className="data tracked">{fmt.duration(planned.trackedTodaySec)} tracked</span>
          )}
          {planned.scheduledStartsAt != null && (
            <span className="data">
              {fmt.clock(planned.scheduledStartsAt)}
              {planned.scheduledBlockCount > 1 && ` +${planned.scheduledBlockCount - 1}`}
            </span>
          )}
          {inFocus && <span className="focus-tag">focus</span>}
        </span>
      </button>

      {!done && (
        <button
          className={running ? "btn btn-primary" : "btn"}
          onClick={() => void toggleTimer(task.id)}
          title={running ? "Stop (Space)" : "Start (Space)"}
        >
          {running ? "Stop" : "Start"}
        </button>
      )}

      {/* Promote / demote, with a keyboard path — drag is never the only way
          to rank a day (§18.1). */}
      {!done &&
        (promote ? (
          <button
            className="btn btn-quiet btn-icon"
            disabled={!canPromote}
            aria-label={`Move ${task.title} into the Top 3`}
            title={canPromote ? "Move into the Top 3" : "Top 3 already holds three tasks"}
            onClick={() => void setDaySection(task.id, "top" as DaySection)}
          >
            ▲
          </button>
        ) : (
          <button
            className="btn btn-quiet btn-icon"
            aria-label={`Move ${task.title} out of the Top 3`}
            title="Move out of the Top 3"
            onClick={() => void setDaySection(task.id, "more" as DaySection)}
          >
            ▼
          </button>
        ))}

      {planned.planItemId && (
        <button
          className="btn btn-quiet btn-icon"
          aria-label={`Take ${task.title} off this day`}
          title="Take off this day. The task, its estimate and its history are untouched."
          onClick={() => void removeFromDay(task.id)}
        >
          ×
        </button>
      )}
    </div>
  );
}

/* ─── the schedule column (§7.6) ────────────────────────────────────────── */

function ScheduleColumn() {
  const today = useApp((s) => s.today)!;
  const taskDrag = useApp((s) => s.taskDrag);
  const setTaskDrag = useApp((s) => s.setTaskDrag);
  const hourHeight = useApp((s) => s.hourHeight);
  const scrollRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<HTMLDivElement>(null);
  const [drop, setDrop] = useState<{ startMin: number; durationMin: number } | null>(null);
  const [nowMin, setNowMin] = useState(today.schedule.currentMinute);

  const { schedule } = today;

  /* The canvas opens where the day starts, not at midnight (§3.0). Keyed on
     the date so switching days re-opens rather than holding yesterday's
     scroll. */
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    // A few pixels of lead-in, so whatever sits at the opening hour is not
    // jammed flush against the fold and half-legible.
    el.scrollTop = Math.max(0, (schedule.visibleDayStartMinute / 60) * hourHeight - 8);
  }, [schedule.localDate, schedule.visibleDayStartMinute]);

  /* Minute-aligned, and only while the marker is on screen at all — a
     one-second interval keeps the CPU awake for a line that moves 0.9px a
     minute. */
  useEffect(() => {
    if (schedule.currentMinute == null) {
      setNowMin(null);
      return;
    }
    setNowMin(fmt.minutesIntoDay(Date.now()));
    let timer: number;
    const tick = () => {
      setNowMin(fmt.minutesIntoDay(Date.now()));
      timer = window.setTimeout(tick, 60_000 - (Date.now() % 60_000) + 50);
    };
    timer = window.setTimeout(tick, 60_000 - (Date.now() % 60_000) + 50);
    return () => clearTimeout(timer);
  }, [schedule.currentMinute, schedule.localDate]);

  const minuteAt = (clientY: number, fine: boolean) => {
    const grid = gridRef.current;
    if (!grid) return 0;
    const raw = ((clientY - grid.getBoundingClientRect().top) / hourHeight) * 60;
    const snap = fine ? 5 : SNAP_MIN;
    return Math.max(0, Math.min(24 * 60 - snap, Math.round(raw / snap) * snap));
  };

  /* Dropping a task plots a block. It does **not** start a timer and does not
     complete anything — plotting time and spending it stay different acts
     (§7.6, invariant 5). */
  const commitDrop = async (startMin: number, durationSec: number, taskId: string) => {
    const startsAt = new Date(`${schedule.localDate}T00:00:00`).getTime() + startMin * 60_000;
    const ok = await useApp
      .getState()
      .run(
        () =>
          ipc.scheduleBlock({
            taskId,
            label: null,
            startsAt,
            durationSec,
            tz: fmt.tz(),
            isFixed: false,
            rrule: null,
          }),
        "Couldn't plot that block.",
      );
    setTaskDrag(null);
    setDrop(null);
    if (ok) await useApp.getState().refresh();
  };

  const diff = schedule.plannedSec;

  return (
    <section className="today-schedule" aria-label="Schedule for the selected day">
      <header className="today-col-head">
        <h2 className="display">Schedule</h2>
        <span className="grow" />
        <span className="micro">
          {diff > 0 ? `${fmt.duration(diff)} plotted` : "Nothing plotted"}
          {schedule.unplannedSec > 0 && ` · ${fmt.duration(schedule.unplannedSec)} unplanned`}
        </span>
        {/* ARCHITECTURE §14 — the complete timeboxing canvas opens as an
            expanded Today mode rather than as a second scheduling model. Same
            blocks, same commands, more days on screen. */}
        <button
          className="btn btn-quiet"
          onClick={() => {
            useApp.getState().setAnchor(today.selectedDate);
            useApp.getState().go("planner");
          }}
          title="Open the full multi-day canvas (G then L)"
        >
          Expand
        </button>
      </header>

      <div className="today-scroll" ref={scrollRef}>
        <div
          className="today-grid"
          ref={gridRef}
          style={{ height: 24 * hourHeight }}
          onPointerMove={(e) => {
            if (!taskDrag) return;
            setDrop({
              startMin: minuteAt(e.clientY, e.altKey),
              durationMin: Math.round(taskDrag.durationSec / 60),
            });
          }}
          onPointerUp={(e) => {
            if (!taskDrag) return;
            void commitDrop(minuteAt(e.clientY, e.altKey), taskDrag.durationSec, taskDrag.taskId);
          }}
        >
          {Array.from({ length: 24 }, (_, h) => (
            <div key={h} className="today-hour" style={{ height: hourHeight }}>
              <span className="today-hour-label micro">{fmt.clock(hourMs(schedule.localDate, h))}</span>
            </div>
          ))}

          {schedule.blocks.map((b) => (
            <BlockPlate key={b.block.id} block={b} hourHeight={hourHeight} />
          ))}

          {nowMin != null && (
            <div
              className="today-now"
              style={{ top: (nowMin / 60) * hourHeight }}
              aria-hidden="true"
            />
          )}

          {drop && taskDrag && (
            <div
              className="today-drop"
              style={{
                top: (drop.startMin / 60) * hourHeight,
                height: (drop.durationMin / 60) * hourHeight,
              }}
            >
              <span className="micro">{taskDrag.title}</span>
            </div>
          )}
        </div>
      </div>
    </section>
  );
}

function hourMs(date: string, hour: number): number {
  return new Date(`${date}T00:00:00`).getTime() + hour * 3_600_000;
}

/**
 * A plotted block. It can start its linked task; it never starts one by itself
 * (§7.6), and the tracked bar underneath is the record beside the plan rather
 * than a redrawing of it.
 */
function BlockPlate({ block, hourHeight }: { block: BlockView; hourHeight: number }) {
  const toggleTimer = useApp((s) => s.toggleTimer);
  const startMin = fmt.minutesIntoDay(block.block.startsAt);
  const height = Math.max(18, (block.plannedSec / 3600) * hourHeight);
  const filled = block.plannedSec > 0 ? Math.min(1, block.trackedSec / block.plannedSec) : 0;

  return (
    <div
      className="today-block"
      data-drift={block.driftState}
      data-running={block.isRunning}
      style={{ top: (startMin / 60) * hourHeight, height }}
    >
      <span
        className="edge"
        style={{ background: block.projectColour ?? "var(--plot)" }}
        aria-hidden="true"
      />
      <span className="today-block-body">
        <span className="t">{block.title}</span>
        <span className="data micro">
          {fmt.clockRange(block.block.startsAt, block.plannedSec)}
        </span>
      </span>
      {/* Tracked against plotted, in the plate, so the gap is visible where the
          plan is rather than on another screen. */}
      <span className="tracked-bar" aria-hidden="true">
        <span style={{ width: `${filled * 100}%` }} />
      </span>
      {block.block.taskId && (
        <button
          className="btn btn-quiet btn-icon start"
          aria-label={block.isRunning ? `Stop ${block.title}` : `Start ${block.title}`}
          onClick={() => void toggleTimer(block.block.taskId!, block.block.id)}
        >
          {block.isRunning ? "■" : "▶"}
        </button>
      )}
    </div>
  );
}
