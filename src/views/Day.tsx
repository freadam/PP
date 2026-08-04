/**
 * The Day view (Plan Rev 3 §8.1) — the primary operational screen.
 *
 * A complete 24-hour table for one date, modelled on the client's workbook. It
 * is where reconciliation happens, and the whole product's cost to the user is
 * how long this screen takes to make honest — the design target is ninety
 * seconds a day.
 *
 * Three things this screen must do that a calendar does not:
 *
 * 1. **Show every hour, including the empty ones.** An hour nothing is known
 *    about is a row with a state, not an absence. It is the most common thing
 *    the user has to act on, so it cannot be the thing that renders as nothing.
 * 2. **Keep four layers distinguishable.** Plan, confirmed work, confirmed life
 *    and machine observation are different kinds of claim, and collapsing them
 *    visually would undo the reason they are separate tables.
 * 3. **Never imply a total it did not compute.** Every figure on this screen is
 *    summed in Rust from segments; the grid is a lens, and changing the slot
 *    size changes nothing but the number of rows.
 */

import { useEffect, useRef, useState } from "react";
import { useApp } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type { DaySlot, DayView, LifeAreaRow, SlotOwner, SlotState } from "../lib/types";
import { Empty } from "../components/chrome";

const SLOT_CHOICES = [5, 15, 30, 60] as const;

/**
 * §4.3 / M16 — every state carries a glyph and a word, never colour alone.
 * The glyph is what survives a greyscale screenshot and a printed export.
 */
const STATE_META: Record<SlotState, { glyph: string; label: string }> = {
  empty: { glyph: "·", label: "Unaccounted" },
  plannedNotStarted: { glyph: "○", label: "Planned, not started" },
  confirmedWork: { glyph: "■", label: "Work" },
  confirmedLife: { glyph: "▣", label: "Life" },
  private: { glyph: "▨", label: "Private" },
  observedOnly: { glyph: "▢", label: "Observed only" },
  idle: { glyph: "–", label: "Idle" },
};

function ownerLabel(owner: SlotOwner): string {
  switch (owner.kind) {
    case "life":
      return owner.isPrivate ? "Private" : owner.label ?? owner.areaName;
    case "work":
      return owner.taskTitle;
    case "observed":
      return owner.domain ?? owner.appId.replace(/\.(exe|app)$/i, "");
    case "idle":
      return "Away";
    case "empty":
      return "";
  }
}

function ownerColour(owner: SlotOwner): string | undefined {
  switch (owner.kind) {
    case "life":
      return owner.isPrivate ? undefined : owner.areaColour;
    case "work":
      return owner.projectColour ?? undefined;
    default:
      return undefined;
  }
}

export function Day() {
  const day = useApp((s) => s.day);
  const date = useApp((s) => s.dayDate);
  const slotMinutes = useApp((s) => s.slotMinutes);
  const setDayDate = useApp((s) => s.setDayDate);
  const setSlotMinutes = useApp((s) => s.setSlotMinutes);
  const load = useApp((s) => s.loadDay);
  const areas = useApp((s) => s.lifeAreas);
  const [fill, setFill] = useState<{ from: number; to: number } | null>(null);

  useEffect(() => {
    void load();
  }, [load]);

  if (!day) return <Empty>Loading the day…</Empty>;

  return (
    <div className="day-view">
      <div className="planner-head">
        <button className="btn" aria-label="Previous day" onClick={() => setDayDate(fmt.addDays(date, -1))}>
          ‹
        </button>
        <button className="btn" aria-label="Next day" onClick={() => setDayDate(fmt.addDays(date, 1))}>
          ›
        </button>
        <strong className="display" style={{ fontSize: "1rem" }}>
          {fmt.longDate(date)}
        </strong>
        {day.isReconciled && (
          <span className="micro" style={{ color: "var(--done)" }}>
            reconciled
          </span>
        )}
        <span className="grow" />
        <div className="row" role="group" aria-label="Slot size">
          {SLOT_CHOICES.map((n) => (
            <button
              key={n}
              className="btn"
              aria-pressed={slotMinutes === n}
              onClick={() => setSlotMinutes(n)}
              /* The tooltip states the guarantee, because a zoom control that
                 quantised the data would be indistinguishable from one that
                 doesn't until someone lost ten minutes to it. */
              title={`${n}-minute rows. Zoom changes the view, never the stored times.`}
            >
              {n}m
            </button>
          ))}
        </div>
        <button className="btn" onClick={() => setDayDate(fmt.today())}>
          Today
        </button>
      </div>

      <DayLedger totals={day.totals} />

      <div className="day-table-wrap scroll-y">
        <table className="day-table">
          <caption className="sr-only">
            {fmt.longDate(date)}, {day.slots.length} rows of {slotMinutes} minutes. Every hour of
            the day is present, including unaccounted time.
          </caption>
          <thead>
            <tr>
              <th scope="col">Time</th>
              <th scope="col">Planned</th>
              <th scope="col">Actual</th>
              <th scope="col">Observed</th>
              <th scope="col">State</th>
            </tr>
          </thead>
          <tbody>
            {day.slots.map((slot) => (
              <SlotRow
                key={slot.index}
                slot={slot}
                day={day}
                onFill={() => setFill({ from: slot.startsAt, to: slot.endsAt })}
              />
            ))}
          </tbody>
        </table>
      </div>

      {fill && (
        <FillDialog
          from={fill.from}
          to={fill.to}
          areas={areas}
          onClose={() => setFill(null)}
        />
      )}
    </div>
  );
}

/**
 * The day's arithmetic, on one line, in the order the plan asks the question:
 * how much was work, how much was life, how much is still unaccounted.
 *
 * The bar underneath is the same numbers as proportions. It sums to the day
 * exactly, because the totals behind it do — that is the counting invariant,
 * drawn.
 */
function DayLedger({ totals }: { totals: DayView["totals"] }) {
  const layers = [
    { key: "confirmedWork", label: "Work", sec: totals.confirmedWorkSec, cls: "l-work" },
    { key: "confirmedLife", label: "Life", sec: totals.confirmedLifeSec, cls: "l-life" },
    { key: "private", label: "Private", sec: totals.privateSec, cls: "l-private" },
    { key: "observedOnly", label: "Observed", sec: totals.observedOnlySec, cls: "l-observed" },
    { key: "idle", label: "Idle", sec: totals.idleSec, cls: "l-idle" },
    { key: "empty", label: "Unaccounted", sec: totals.emptySec, cls: "l-empty" },
  ].filter((l) => l.sec > 0);

  return (
    <section className="day-ledger">
      <span className="day-bar" role="img" aria-label={layers
        .map((l) => `${l.label} ${fmt.duration(l.sec)}`)
        .join(", ")}>
        {layers.map((l) => (
          <i
            key={l.key}
            className={l.cls}
            style={{ width: `${(l.sec / Math.max(1, totals.daySec)) * 100}%` }}
            title={`${l.label} · ${fmt.duration(l.sec)}`}
          />
        ))}
      </span>
      <div className="row day-ledger-figures">
        {layers.map((l) => (
          <span key={l.key} className="micro">
            <i className={`swatch ${l.cls}`} aria-hidden="true" />
            {l.label} <span className="data">{fmt.duration(l.sec)}</span>
          </span>
        ))}
        <span className="grow" />
        <span className="micro" title="Plotted, which is the plan — never actual time">
          plotted <span className="data">{fmt.duration(totals.plannedSec)}</span>
        </span>
        <span className="micro" title="Time the machine was in front of someone. Overlaps the layers above on purpose.">
          at the PC <span className="data">{fmt.duration(totals.pcSec)}</span>
        </span>
      </div>
    </section>
  );
}

function SlotRow({
  slot,
  day,
  onFill,
}: {
  slot: DaySlot;
  day: DayView;
  onFill: () => void;
}) {
  const rowRef = useRef<HTMLTableRowElement>(null);
  const isNow =
    day.isToday && day.now >= slot.startsAt && day.now < slot.endsAt;

  // Open on the current hour rather than at midnight.
  useEffect(() => {
    if (isNow) rowRef.current?.scrollIntoView({ block: "center" });
  }, [isNow]);

  const actual = slot.segments.filter((s) => s.owner.kind !== "empty");
  // A slot can hold both a record and a gap. The gap still needs its button,
  // or twenty unaccounted minutes beside a confirmed session are unreachable.
  const hasGap = slot.segments.some((s) => s.owner.kind === "empty");
  const observedEvidence = slot.segments.flatMap((s) => s.evidence);
  const meta = STATE_META[slot.state];

  return (
    <tr
      ref={rowRef}
      className="day-row"
      data-state={slot.state}
      data-now={isNow || undefined}
      data-hour={slot.index % (60 / (day.slotMinutes || 30)) === 0 || undefined}
    >
      <th scope="row" className="data day-time">
        {fmt.clock(slot.startsAt)}
      </th>

      <td className="day-planned">
        {slot.plans.map((p) => (
          <span key={p.blockId} className="day-chip" data-plan="true" title={`Plotted ${fmt.duration(p.durationSec)}`}>
            <i style={{ background: p.projectColour ?? "var(--plot)" }} aria-hidden="true" />
            {p.title}
          </span>
        ))}
      </td>

      <td className="day-actual">
        {actual.map((seg, i) => (
            <span
              key={i}
              className="day-chip"
              data-owner={seg.owner.kind}
              title={`${fmt.clock(seg.from)}–${fmt.clock(seg.to)} · ${fmt.duration(
                (seg.to - seg.from) / 1000,
              )}`}
            >
              <i
                style={{ background: ownerColour(seg.owner) ?? "var(--muted)" }}
                aria-hidden="true"
              />
              {ownerLabel(seg.owner)}
            </span>
        ))}
        {/* Empty time is offered as an action, not shown as a blank. This is
            the button that makes a day reconcilable in ninety seconds. */}
        {hasGap && (
          <button className="day-fill" onClick={onFill}>
            {actual.length === 0 ? "Unaccounted — fill" : "+ fill the gap"}
          </button>
        )}
      </td>

      <td className="day-observed data micro">
        {observedEvidence.length > 0
          ? observedEvidence
              .slice(0, 2)
              .map((a) => a.appId.replace(/\.(exe|app)$/i, ""))
              .join(", ")
          : ""}
      </td>

      <td className="day-state">
        {/* Glyph and word together: the state survives greyscale (M16). */}
        <span className="micro" title={meta.label}>
          <span aria-hidden="true" className="day-glyph">
            {meta.glyph}
          </span>
          {meta.label}
        </span>
      </td>
    </tr>
  );
}

/**
 * Filling an unaccounted slot. One click, one area, done — this is the
 * interaction the ninety-second target is spent on, so it asks for exactly one
 * decision and nothing else.
 */
function FillDialog({
  from,
  to,
  areas,
  onClose,
}: {
  from: number;
  to: number;
  areas: LifeAreaRow[];
  onClose: () => void;
}) {
  const run = useApp((s) => s.run);
  const reload = useApp((s) => s.loadDay);
  const toast = useApp((s) => s.toast);
  const [end, setEnd] = useState(to);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  const fill = async (lifeAreaId: string, isPrivate = false) => {
    const entry = await run(
      () =>
        ipc.addLifeEntry({
          lifeAreaId,
          startedAt: from,
          endedAt: end,
          tz: fmt.tz(),
          isPrivate,
          // Never destructive from here: this button exists to fill a gap, and
          // a gap has nothing to replace (M9).
          replaceExisting: false,
        }),
      "Couldn't record that.",
    );
    onClose();
    if (entry) {
      toast(`${fmt.clock(from)}–${fmt.clock(end)} recorded as ${entry.areaName}.`);
      await reload();
    }
  };

  return (
    <>
      <div className="scrim" onClick={onClose} />
      <div className="modal overlay" role="dialog" aria-modal="true" aria-label="Fill unaccounted time">
        <div className="sheet-head">
          <strong className="title">
            {fmt.clock(from)}–{fmt.clock(end)}
          </strong>
        </div>
        <div className="sheet-body stack">
          <div className="row">
            <span className="caption">Until</span>
            {[30, 60, 120, 240].map((mins) => (
              <button
                key={mins}
                className="btn"
                aria-pressed={end === from + mins * 60_000}
                onClick={() => setEnd(from + mins * 60_000)}
              >
                {fmt.duration(mins * 60)}
              </button>
            ))}
          </div>
          <div className="area-grid">
            {areas.map((a) => (
              <button key={a.id} className="btn" onClick={() => void fill(a.id)}>
                <i className="swatch" style={{ background: a.colour }} aria-hidden="true" />
                <span className="grow" style={{ textAlign: "left" }}>
                  {a.name}
                </span>
              </button>
            ))}
          </div>
          <div className="row">
            {/* Private is accounted for without being categorised: it stops the
                reconciler nagging without asking anyone to name it. */}
            <button className="btn" onClick={() => void fill(areas[0]?.id ?? "", true)} disabled={!areas.length}>
              Private — accounted for, not recorded
            </button>
            <span className="grow" />
            <button className="btn" onClick={onClose}>
              Cancel <span className="kbd">Esc</span>
            </button>
          </div>
        </div>
      </div>
    </>
  );
}
