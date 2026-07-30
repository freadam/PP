/**
 * The Planner (§3.1) — plot the course, and see the drift on it.
 *
 * 24 hours, scrollable, because a 07:00–21:00 window excludes night workers,
 * who are a real slice of this audience. Drag is an *alternative* to the
 * keyboard, never the only path (§4.3).
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useApp } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type { BlockView, CollisionPolicy, DayColumn } from "../lib/types";
import { DriftBadge, DriftRail, driftLabel } from "../components/DriftRail";
import { Empty } from "../components/chrome";

const SNAP_MIN = 15;
const FINE_SNAP_MIN = 5;
const DRAG_THRESHOLD_PX = 8;
const AUTOSCROLL_EDGE_PX = 40;
const AUTOSCROLL_MAX_PX_S = 600;

interface DragState {
  kind: "move" | "resize";
  blockId: string;
  originX: number;
  originY: number;
  grabOffsetMin: number;
  live: { date: string; startMin: number; durationMin: number } | null;
  armed: boolean;
}

export function Planner() {
  const week = useApp((s) => s.week);
  const span = useApp((s) => s.span);
  const hourHeight = useApp((s) => s.hourHeight);
  const selectedBlockId = useApp((s) => s.selectedBlockId);
  const setSpan = useApp((s) => s.setSpan);
  const setAnchor = useApp((s) => s.setAnchor);
  const shiftPeriod = useApp((s) => s.shiftPeriod);
  const selectBlock = useApp((s) => s.selectBlock);

  const scrollRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<DragState | null>(null);
  const [adHoc, setAdHoc] = useState<{ date: string; startMin: number } | null>(null);
  const [nowMin, setNowMin] = useState(() => fmt.minutesIntoDay(Date.now()));

  /* §6.9 — the now cursor is minute-aligned and paused when the Planner is
     unmounted. A 1-second interval keeps the CPU awake permanently for a line
     that moves 0.6px a second. */
  useEffect(() => {
    let timer: number;
    const tick = () => {
      setNowMin(fmt.minutesIntoDay(Date.now()));
      const delay = 60_000 - (Date.now() % 60_000) + 50;
      timer = window.setTimeout(tick, delay);
    };
    const delay = 60_000 - (Date.now() % 60_000) + 50;
    timer = window.setTimeout(tick, delay);
    return () => clearTimeout(timer);
  }, []);

  // Open on the working day rather than at midnight.
  useEffect(() => {
    const el = scrollRef.current;
    if (el && el.scrollTop === 0) el.scrollTop = (8 * hourHeight) - 8;
  }, [hourHeight]);

  const days = week?.days ?? [];
  const columns = `repeat(${Math.max(1, days.length)}, minmax(110px, 1fr))`;

  const minuteToY = useCallback((min: number) => (min / 60) * hourHeight, [hourHeight]);

  const pointToSlot = useCallback(
    (clientX: number, clientY: number, fine: boolean) => {
      const grid = gridRef.current;
      if (!grid || !days.length) return null;
      const rect = grid.getBoundingClientRect();
      const gutter = 48;
      const colWidth = (rect.width - gutter) / days.length;
      const index = Math.min(
        days.length - 1,
        Math.max(0, Math.floor((clientX - rect.left - gutter) / colWidth)),
      );
      const rawMin = ((clientY - rect.top) / hourHeight) * 60;
      const snap = fine ? FINE_SNAP_MIN : SNAP_MIN;
      const startMin = Math.round(rawMin / snap) * snap;
      return { date: days[index]!.localDate, startMin, index };
    },
    [days, hourHeight],
  );

  /* ─── drag and drop (§4.3) ─────────────────────────────────────────── */

  const beginDrag = (e: React.PointerEvent, block: BlockView, kind: "move" | "resize") => {
    e.preventDefault();
    (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
    const startMin = fmt.minutesIntoDay(block.block.startsAt);
    const grabMin = ((e.clientY - gridRef.current!.getBoundingClientRect().top) / hourHeight) * 60;
    setDrag({
      kind,
      blockId: block.block.id,
      originX: e.clientX,
      originY: e.clientY,
      grabOffsetMin: grabMin - startMin,
      live: null,
      // 8px of movement before a drag begins, so taps and scrolls aren't misread.
      armed: false,
    });
    selectBlock(block.block.id);
  };

  useEffect(() => {
    if (!drag) return;

    const onMove = (e: PointerEvent) => {
      const moved =
        Math.abs(e.clientX - drag.originX) + Math.abs(e.clientY - drag.originY);
      if (!drag.armed && moved < DRAG_THRESHOLD_PX) return;

      // Auto-scroll within 40px of an edge, ramping to 600px/s.
      const el = scrollRef.current;
      if (el) {
        const r = el.getBoundingClientRect();
        const above = e.clientY - r.top;
        const below = r.bottom - e.clientY;
        let dy = 0;
        if (above < AUTOSCROLL_EDGE_PX) dy = -(1 - above / AUTOSCROLL_EDGE_PX);
        else if (below < AUTOSCROLL_EDGE_PX) dy = 1 - below / AUTOSCROLL_EDGE_PX;
        if (dy !== 0) el.scrollTop += (dy * AUTOSCROLL_MAX_PX_S) / 60;
      }

      const slot = pointToSlot(e.clientX, e.clientY, e.altKey);
      if (!slot) return;
      const block = days.flatMap((d) => d.blocks).find((b) => b.block.id === drag.blockId);
      if (!block) return;
      const durationMin = block.block.durationSec / 60;

      setDrag((d) =>
        d
          ? {
              ...d,
              armed: true,
              live:
                d.kind === "move"
                  ? {
                      date: slot.date,
                      startMin: Math.max(0, slot.startMin - Math.round(d.grabOffsetMin)),
                      durationMin,
                    }
                  : {
                      date: block.block.localDate,
                      startMin: fmt.minutesIntoDay(block.block.startsAt),
                      durationMin: Math.max(
                        5,
                        slot.startMin - fmt.minutesIntoDay(block.block.startsAt),
                      ),
                    },
            }
          : d,
      );
    };

    const onUp = async (e: PointerEvent) => {
      const current = drag;
      setDrag(null);
      if (!current?.armed || !current.live) return;

      const { run, refresh, toast } = useApp.getState();
      if (current.kind === "resize") {
        await run(
          () => ipc.resizeBlock(current.blockId, Math.round(current.live!.durationMin) * 60),
          "Couldn't resize that block.",
        );
      } else {
        // Shift pushes subsequent non-fixed blocks down; Alt shortens to fit;
        // the default is overlap with a warning tint (§4.3).
        const policy: CollisionPolicy = e.shiftKey ? "push" : e.altKey ? "shrink" : "overlap";
        const startsAt =
          new Date(`${current.live.date}T00:00:00`).getTime() + current.live.startMin * 60_000;
        const moved = await run(
          () => ipc.moveBlock(current.blockId, startsAt, policy),
          "Couldn't move that block.",
        );
        if (moved && policy === "push" && moved.length > 1) {
          toast(`Pushed ${moved.length - 1} block${moved.length === 2 ? "" : "s"} down.`);
        }
      }
      await refresh();
    };

    // Escape cancels and restores the original position — v1 had no cancel (U3).
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        setDrag(null);
      }
    };

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("keydown", onKey, true);
    };
  }, [drag, days, pointToSlot]);

  const onGridPointerDown = (e: React.PointerEvent) => {
    if (e.target !== e.currentTarget) return;
    const slot = pointToSlot(e.clientX, e.clientY, e.altKey);
    if (slot) setAdHoc({ date: slot.date, startMin: slot.startMin });
  };

  const rangeLabel = useMemo(
    () => (week ? fmt.rangeLabel(week.from, week.to) : ""),
    [week],
  );

  if (!week) {
    return <Empty>Loading the week…</Empty>;
  }

  return (
    <div className="planner">
      <div className="planner-head">
        <button className="btn" aria-label="Previous period" onClick={() => shiftPeriod(-1)}>
          ‹
        </button>
        <button className="btn" aria-label="Next period" onClick={() => shiftPeriod(1)}>
          ›
        </button>
        <strong className="display" style={{ fontSize: "1rem" }}>
          {rangeLabel}
        </strong>
        <span className="grow" />
        <div className="row" role="group" aria-label="Day span">
          {([1, 3, 7] as const).map((n) => (
            <button
              key={n}
              className="btn"
              aria-pressed={span === n}
              onClick={() => setSpan(n)}
              title={`${n}-day view (${n})`}
            >
              {n}
            </button>
          ))}
        </div>
        <button className="btn" onClick={() => setAnchor(fmt.today())} title="Today (T)">
          Today
        </button>
      </div>

      <div className="planner-daynames" style={{ gridTemplateColumns: columns }}>
        {days.map((d) => (
          <div key={d.localDate} className="dayname" data-today={d.isToday}>
            <span className="micro" style={{ color: "var(--muted)" }}>
              {fmt.weekdayShort(d.localDate)}
            </span>
            <span className="num data">{fmt.dayOfMonth(d.localDate)}</span>
            <span className="totals data micro">
              {d.plannedSec > 0 ? fmt.duration(d.plannedSec) : ""}
            </span>
          </div>
        ))}
      </div>

      <div className="planner-scroll" ref={scrollRef}>
        <div
          className="planner-grid"
          ref={gridRef}
          role="grid"
          aria-label="Planner"
          aria-rowcount={24}
          aria-colcount={days.length}
          style={{ gridTemplateColumns: columns, height: 24 * hourHeight }}
          onPointerDown={onGridPointerDown}
        >
          <HourRules hourHeight={hourHeight} />

          {days.map((day, colIndex) => (
            <DayColumnView
              key={day.localDate}
              day={day}
              colIndex={colIndex}
              minuteToY={minuteToY}
              selectedBlockId={selectedBlockId}
              onDragStart={beginDrag}
              dragBlockId={drag?.armed ? drag.blockId : null}
            />
          ))}

          {days.some((d) => d.isToday) && (
            <div
              className="now-cursor"
              style={{
                top: minuteToY(nowMin),
                gridColumn: `1 / -1`,
                position: "absolute",
                left: 48,
                right: 0,
              }}
              aria-hidden="true"
            />
          )}

          {drag?.live && (
            <DropPlate live={drag.live} days={days} minuteToY={minuteToY} />
          )}

          {adHoc && (
            <AdHocForm
              slot={adHoc}
              days={days}
              minuteToY={minuteToY}
              onClose={() => setAdHoc(null)}
            />
          )}
        </div>
      </div>

      {days.every((d) => d.blocks.length === 0) && (
        <div style={{ padding: "0 0 16px" }}>
          <p className="caption" style={{ textAlign: "center" }}>
            Nothing plotted this week. Drag a task from the left, or press{" "}
            <span className="kbd">C</span> to capture one.
          </p>
        </div>
      )}
    </div>
  );
}

/** §5.1 — heavy rule at the hour, hairline at the quarter, numerals in the gutter. */
function HourRules({ hourHeight }: { hourHeight: number }) {
  return (
    <div className="hour-rows" style={{ left: 48 }} aria-hidden="true">
      {Array.from({ length: 24 }, (_, h) => (
        <div key={h} className="hour-row" style={{ height: hourHeight }}>
          <span className="hour-label micro" aria-hidden="true">
            {String(h).padStart(2, "0")}
          </span>
          {[1, 2, 3].map((q) => (
            <span key={q} className="q" style={{ top: (hourHeight / 4) * q }} />
          ))}
        </div>
      ))}
    </div>
  );
}

function DayColumnView({
  day,
  colIndex,
  minuteToY,
  selectedBlockId,
  onDragStart,
  dragBlockId,
}: {
  day: DayColumn;
  colIndex: number;
  minuteToY: (m: number) => number;
  selectedBlockId: string | null;
  onDragStart: (e: React.PointerEvent, b: BlockView, kind: "move" | "resize") => void;
  dragBlockId: string | null;
}) {
  const openDetail = useApp((s) => s.openDetail);
  const selectBlock = useApp((s) => s.selectBlock);
  const toggleTimer = useApp((s) => s.toggleTimer);

  // Collision groups: equal-width side by side, 2px gutter, max 3 visible.
  const MAX_VISIBLE_LANES = 3;

  return (
    <div
      className="day-col"
      data-today={day.isToday}
      data-past={day.isPast}
      role="row"
      aria-label={`${day.localDate}, ${day.blocks.length} block${day.blocks.length === 1 ? "" : "s"}`}
    >
      {day.blocks.map((b, i) => {
        const startMin = fmt.minutesIntoDay(b.block.startsAt);
        const top = minuteToY(startMin);
        const height = Math.max(12, minuteToY(b.block.durationSec / 60));
        const lanes = Math.min(b.lanes, MAX_VISIBLE_LANES);
        if (b.lane >= MAX_VISIBLE_LANES) return null;
        const widthPct = 100 / lanes;

        return (
          <button
            key={b.block.id}
            role="gridcell"
            className="plate"
            data-selected={selectedBlockId === b.block.id}
            data-fixed={b.block.isFixed}
            data-running={b.isRunning}
            data-tiny={height < 28}
            data-warn={b.lanes > 1}
            style={{
              top,
              height,
              left: `calc(${b.lane * widthPct}% + 1px)`,
              width: `calc(${widthPct}% - 2px)`,
              ["--project-colour" as string]: b.projectColour ?? "transparent",
              opacity: dragBlockId === b.block.id ? 0.4 : 1,
            }}
            /* §4.7 — each block is a gridcell whose accessible name reads
               "Refactor auth module, 9 to 10 AM, planned 60 minutes, tracked 74
               minutes, 14 minutes over." */
            aria-label={`${b.title}, ${fmt.clockRange(b.block.startsAt, b.block.durationSec)}, ${driftLabel(
              {
                plannedSec: b.plannedSec,
                trackedSec: b.trackedSec,
                driftSec: b.driftSec,
                state: b.driftState,
              },
            )}`}
            onPointerDown={(e) => onDragStart(e, b, "move")}
            onClick={() => selectBlock(b.block.id)}
            onDoubleClick={() => b.block.taskId && void openDetail(b.block.taskId)}
            onKeyDown={(e) => {
              if (e.key === " " && b.block.taskId) {
                e.preventDefault();
                void toggleTimer(b.block.taskId, b.block.id);
              }
            }}
          >
            <DriftRail
              planned={b.plannedSec}
              tracked={b.trackedSec}
              driftSec={b.driftSec}
              state={b.driftState}
              height={height}
              isPast={day.isPast}
              settleIndex={colIndex * 4 + i}
            />
            {height >= 28 && (
              <span className="plate-title" style={{ fontSize: "var(--t-body)" }}>
                {b.title}
              </span>
            )}
            {height >= 68 && (
              <span className="plate-time data">
                {fmt.clockRange(b.block.startsAt, b.block.durationSec)} · plotted{" "}
                {fmt.duration(b.plannedSec)}
              </span>
            )}
            {height >= 40 && (
              <span className="plate-footer">
                <DriftBadge driftSec={b.driftSec} state={b.driftState} />
              </span>
            )}
            {/* Resize handle — drag is an alternative; Shift+↑/↓ does the same. */}
            <span
              onPointerDown={(e) => {
                e.stopPropagation();
                onDragStart(e, b, "resize");
              }}
              style={{
                position: "absolute",
                left: 12,
                right: 0,
                bottom: 0,
                height: 6,
                cursor: "ns-resize",
              }}
              aria-hidden="true"
            />
          </button>
        );
      })}

      {day.blocks.some((b) => b.lane >= MAX_VISIBLE_LANES) && (
        <span className="micro" style={{ position: "absolute", bottom: 4, left: 12, color: "var(--muted)" }}>
          +{day.blocks.filter((b) => b.lane >= MAX_VISIBLE_LANES).length} more
        </span>
      )}
    </div>
  );
}

/** The insertion plate: live time range and duration label (§4.3). */
function DropPlate({
  live,
  days,
  minuteToY,
}: {
  live: { date: string; startMin: number; durationMin: number };
  days: DayColumn[];
  minuteToY: (m: number) => number;
}) {
  const index = days.findIndex((d) => d.localDate === live.date);
  if (index < 0) return null;
  const widthPct = 100 / days.length;
  const startsAt = new Date(`${live.date}T00:00:00`).getTime() + live.startMin * 60_000;
  return (
    <div
      className="drop-plate"
      style={{
        top: minuteToY(live.startMin),
        height: Math.max(12, minuteToY(live.durationMin)),
        left: `calc(48px + ${index * widthPct}% - ${(index * widthPct * 48) / 100}px)`,
        width: `calc(${widthPct}% - ${(widthPct * 48) / 100}px)`,
      }}
    >
      <span className="data micro">
        {fmt.clockRange(startsAt, live.durationMin * 60)} · {fmt.duration(live.durationMin * 60)}
      </span>
    </div>
  );
}

/** Clicking empty grid creates an ad-hoc block inline — you need this for meetings. */
function AdHocForm({
  slot,
  days,
  minuteToY,
  onClose,
}: {
  slot: { date: string; startMin: number };
  days: DayColumn[];
  minuteToY: (m: number) => number;
  onClose: () => void;
}) {
  const [label, setLabel] = useState("");
  const index = days.findIndex((d) => d.localDate === slot.date);
  const widthPct = 100 / days.length;

  const submit = async () => {
    if (!label.trim()) return onClose();
    const { run, refresh } = useApp.getState();
    const startsAt = new Date(`${slot.date}T00:00:00`).getTime() + slot.startMin * 60_000;
    await run(
      () =>
        ipc.scheduleBlock({
          label: label.trim(),
          startsAt,
          durationSec: 3600,
          tz: fmt.tz(),
        }),
      "Couldn't create that block.",
    );
    await refresh();
    onClose();
  };

  return (
    <div
      className="drop-plate"
      style={{
        top: minuteToY(slot.startMin),
        height: minuteToY(60),
        left: `calc(48px + ${index * widthPct}% - ${(index * widthPct * 48) / 100}px)`,
        width: `calc(${widthPct}% - ${(widthPct * 48) / 100}px)`,
        pointerEvents: "auto",
      }}
    >
      <input
        autoFocus
        value={label}
        placeholder="Meeting…"
        onChange={(e) => setLabel(e.target.value)}
        onBlur={() => void submit()}
        onKeyDown={(e) => {
          if (e.key === "Enter") void submit();
          if (e.key === "Escape") onClose();
        }}
        style={{ width: "100%", height: 24 }}
      />
    </div>
  );
}
