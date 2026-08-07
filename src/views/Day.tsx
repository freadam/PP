/**
 * The Day view (Plan Rev 3 §8.1, wireframe screen 1) — the primary screen.
 *
 * A complete 24-hour table for one date, modelled on the client's workbook. It
 * is where reconciliation happens, and the product's whole cost to the user is
 * how long this screen takes to make honest — the design target is ninety
 * seconds a day.
 *
 * Layout follows the wireframe: context bar, six summary cards, the five-column
 * grid, and a detail panel for whatever interval is selected. Three things it
 * must do that a calendar does not:
 *
 * 1. **Show every hour, including the empty ones.** An hour nothing is known
 *    about is a row with a state, not an absence. It is the most common thing
 *    the user has to act on, so it cannot be the thing that renders as nothing.
 * 2. **Keep four layers distinguishable.** Plan, confirmed work, confirmed life
 *    and machine observation are different kinds of claim, and collapsing them
 *    visually would undo the reason they are separate tables.
 * 3. **Never imply a total it did not compute.** Every figure here is summed in
 *    Rust from segments; the grid is a lens, and changing the slot size changes
 *    nothing but the number of rows.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { useApp } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import { CONTRIBUTIONS } from "../lib/types";
import type {
  Contribution,
  DaySegment,
  DaySlot,
  DayView,
  LifeAreaRow,
  LifeEntryRow,
  RrulePreset,
  SlotOwner,
  SlotState,
} from "../lib/types";
import { Empty } from "../components/chrome";

/** The wireframe offers 15/30/60; 5 is kept for splitting a short interval. */
const SLOT_CHOICES = [5, 15, 30, 60] as const;

/**
 * M16 — every state carries a glyph *and* a word, never colour alone. The glyph
 * is what survives a greyscale screenshot and a printed Excel export.
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

/** Rows the filter can isolate. Presentation only — see `Filter` below. */
const FILTERS: { key: string; label: string; match: (s: DaySlot) => boolean }[] = [
  { key: "all", label: "Everything", match: () => true },
  { key: "unaccounted", label: "Unaccounted", match: (s) => s.state === "empty" },
  {
    key: "needsAction",
    label: "Needs a decision",
    match: (s) =>
      s.state === "empty" || s.state === "observedOnly" || s.state === "plannedNotStarted",
  },
  { key: "work", label: "Work", match: (s) => s.state === "confirmedWork" },
  { key: "life", label: "Life", match: (s) => s.state === "confirmedLife" },
];

/**
 * The per-project and per-area filters (wireframe, Day).
 *
 * Built from **the day's own segments**, not from every project in the
 * database. A filter that offers thirty projects, twenty-eight of which have no
 * time today, is a menu of dead ends — and the two that do have time are the
 * ones you came to isolate.
 *
 * The key carries its kind (`project:…`, `area:…`) so one `<select>` can hold
 * three groups without a second piece of state deciding which one is live.
 */
function subjectFilters(day: DayView | null): { key: string; label: string; match: (s: DaySlot) => boolean }[] {
  if (!day) return [];
  const projects = new Map<string, string>();
  const areas = new Map<string, string>();
  for (const seg of day.segments) {
    if (seg.owner.kind === "work") {
      // A session with no project still needs a row, or "no project" becomes
      // the one thing the filter cannot show you.
      projects.set(seg.owner.projectId ?? "", seg.owner.projectName ?? "No project");
    } else if (seg.owner.kind === "life" && !seg.owner.isPrivate) {
      areas.set(seg.owner.areaId, seg.owner.areaName);
    }
  }
  const byName = (a: [string, string], b: [string, string]) => a[1].localeCompare(b[1]);
  return [
    ...[...projects].sort(byName).map(([id, label]) => ({
      key: `project:${id}`,
      label,
      match: (s: DaySlot) =>
        s.segments.some((g) => g.owner.kind === "work" && (g.owner.projectId ?? "") === id),
    })),
    ...[...areas].sort(byName).map(([id, label]) => ({
      key: `area:${id}`,
      label,
      match: (s: DaySlot) =>
        s.segments.some((g) => g.owner.kind === "life" && g.owner.areaId === id),
    })),
  ];
}

function appName(id: string): string {
  return id.replace(/\.(exe|app)$/i, "");
}

function ownerLabel(owner: SlotOwner): string {
  switch (owner.kind) {
    case "life":
      return owner.isPrivate ? "Private" : owner.label ?? owner.areaName;
    case "work":
      return owner.taskTitle;
    case "observed":
      return owner.domain ?? appName(owner.appId);
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

/**
 * The Classification column. "Work · Own" rather than "Work", because the
 * contribution mode is the one field the monthly work summary groups by and it
 * is invisible everywhere else.
 */
function classification(slot: DaySlot): string {
  const seg = slot.segments.find((s) => s.owner.kind !== "empty");
  if (seg?.owner.kind === "work") {
    const mode = seg.owner.contribution;
    const label = mode ? CONTRIBUTIONS.find((c) => c.value === mode)?.label : null;
    // The one classification you cannot reach by reading either layer alone.
    const suffix = seg.hasDistraction ? " + distraction" : "";
    return label ? `Work · ${label}${suffix}` : `Work${suffix}`;
  }
  if (seg?.owner.kind === "life" && !seg.owner.isPrivate) {
    return seg.owner.areaKind === "entertainment" ? "Entertainment" : "Life";
  }
  if (seg?.owner.kind === "observed") {
    return seg.owner.category === "entertainment" ? "Observed Entertainment" : "Observed";
  }
  return STATE_META[slot.state].label;
}

export function Day() {
  const day = useApp((s) => s.day);
  const date = useApp((s) => s.dayDate);
  const slotMinutes = useApp((s) => s.slotMinutes);
  const setDayDate = useApp((s) => s.setDayDate);
  const setSlotMinutes = useApp((s) => s.setSlotMinutes);
  const load = useApp((s) => s.loadDay);
  const run = useApp((s) => s.run);
  const toast = useApp((s) => s.toast);
  const areas = useApp((s) => s.lifeAreas);

  const [filter, setFilter] = useState("all");
  const [selected, setSelected] = useState<DaySegment | null>(null);
  const [fill, setFill] = useState<{ from: number; to: number } | null>(null);
  /**
   * Multi-select, on the time column (wireframe, Day).
   *
   * A *range*, not a scattered set. The rows are contiguous minutes and "that
   * whole afternoon was the school run" is the sentence people actually have;
   * a set of disconnected slots filled with one label would write four records
   * that read as one.
   *
   * It lives on the time cell rather than on the fill button because the fill
   * button opens a modal, and a modal covers the table you would be
   * shift-clicking into. Click a time to anchor, shift-click another to reach
   * — the same gesture as a spreadsheet, on the column that looks like one.
   */
  const [range, setRange] = useState<{ anchor: number; head: number } | null>(null);

  useEffect(() => {
    void load();
  }, [load]);

  // A selection is a pointer into a payload that gets replaced on every reload,
  // so it is re-resolved by time rather than held by reference.
  const liveSelection = useMemo(() => {
    if (!selected || !day) return null;
    return day.segments.find((s) => s.from === selected.from && s.to === selected.to) ?? null;
  }, [selected, day]);

  const fillFrom = (slot: DaySlot, _extend: boolean) => {
    setFill({ from: slot.startsAt, to: slot.endsAt });
  };

  const markRange = (slot: DaySlot, extend: boolean) => {
    setRange((r) =>
      extend && r ? { ...r, head: slot.index } : { anchor: slot.index, head: slot.index },
    );
  };

  /** The selected span, in real times — `null` when it is a single row, which
   *  is a click that has not become a selection yet. */
  const span = useMemo(() => {
    if (!day || !range) return null;
    const lo = Math.min(range.anchor, range.head);
    const hi = Math.max(range.anchor, range.head);
    if (lo === hi) return null;
    const first = day.slots.find((s) => s.index === lo);
    const last = day.slots.find((s) => s.index === hi);
    if (!first || !last) return null;
    return { from: first.startsAt, to: last.endsAt, rows: hi - lo + 1 };
  }, [day, range]);

  /**
   * What the selection could be merged into (wireframe, Day — the last item).
   *
   * Offered only when the selection covers two or more records of **one**
   * subject. Two life areas inside the selection is not a merge, it is a
   * question, and answering it on the user's behalf is how someone loses an
   * afternoon.
   */
  const mergeable = useMemo(() => {
    if (!day || !span) return null;
    const subjects = new Map<string, { kind: "life" | "work"; name: string; ids: string[] }>();
    for (const g of day.segments) {
      if (g.from < span.from || g.to > span.to) continue;
      const entry =
        g.owner.kind === "life"
          ? { key: `life:${g.owner.areaId}`, kind: "life" as const, name: g.owner.areaName, id: g.owner.entryId }
          : g.owner.kind === "work"
            ? { key: `work:${g.owner.taskId}`, kind: "work" as const, name: g.owner.taskTitle, id: g.owner.sessionId }
            : null;
      if (!entry) continue;
      const at = subjects.get(entry.key) ?? { kind: entry.kind, name: entry.name, ids: [] };
      if (!at.ids.includes(entry.id)) at.ids.push(entry.id);
      subjects.set(entry.key, at);
    }
    const candidates = [...subjects.values()].filter((v) => v.ids.length > 1);
    return candidates.length === 1 ? candidates[0]! : null;
  }, [day, span]);

  const subjects = useMemo(() => subjectFilters(day), [day]);
  const rule =
    [...FILTERS, ...subjects].find((f) => f.key === filter) ?? FILTERS[0]!;
  const rows = day ? day.slots.filter(rule.match) : [];

  if (!day) return <Empty>Loading the day…</Empty>;

  return (
    <div className="day-view">
      <div className="context-bar">
        <button className="btn" aria-label="Previous day" onClick={() => setDayDate(fmt.addDays(date, -1))}>
          ‹
        </button>
        <h1 className="display">{fmt.longDate(date)}</h1>
        <button className="btn" aria-label="Next day" onClick={() => setDayDate(fmt.addDays(date, 1))}>
          ›
        </button>
        <button className="btn" onClick={() => setDayDate(fmt.today())}>
          Today
        </button>
        {day.isReconciled && (
          <span className="micro" style={{ color: "var(--done)" }}>
            reconciled
          </span>
        )}
        <div className="segmented" role="group" aria-label="Row size">
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
        <span className="grow" />
        <label className="row micro" style={{ gap: 6 }}>
          Filter
          <select value={filter} onChange={(e) => setFilter(e.target.value)} aria-label="Filter rows">
            {FILTERS.map((f) => (
              <option key={f.key} value={f.key}>
                {f.label}
              </option>
            ))}
            {/* Only what this day actually contains — see `subjectFilters`. */}
            {subjects.some((f) => f.key.startsWith("project:")) && (
              <optgroup label="Project">
                {subjects
                  .filter((f) => f.key.startsWith("project:"))
                  .map((f) => (
                    <option key={f.key} value={f.key}>
                      {f.label}
                    </option>
                  ))}
              </optgroup>
            )}
            {subjects.some((f) => f.key.startsWith("area:")) && (
              <optgroup label="Life area">
                {subjects
                  .filter((f) => f.key.startsWith("area:"))
                  .map((f) => (
                    <option key={f.key} value={f.key}>
                      {f.label}
                    </option>
                  ))}
              </optgroup>
            )}
          </select>
        </label>
        <button
          className="btn btn-primary"
          onClick={() => setFill({ from: day.startsAt + 9 * 3_600_000, to: day.startsAt + 10 * 3_600_000 })}
        >
          + Add interval
        </button>
      </div>

      <SummaryCards totals={day.totals} />

      {filter !== "all" && (
        /* A filtered table shows fewer rows but the same day. Saying so is the
           difference between a filter and a screen that appears to have lost
           twenty hours. */
        <p className="caption">
          Showing {rows.length} of {day.slots.length} rows
          {filter.includes(":") && <> for {rule.label}</>}. The totals above are always the whole
          day.
        </p>
      )}

      {span && (
        /* Nothing happens until it is asked for: a selection that acted on
           its own would be a four-hour edit made by a stray shift-click. */
        <div className="row selection-bar" role="status">
          <span className="grow">
            <span className="data">
              {fmt.clock(span.from)}–{fmt.clock(span.to)}
            </span>{" "}
            selected · {span.rows} rows · {fmt.duration((span.to - span.from) / 1000)}
          </span>
          {mergeable && (
            <button
              className="btn"
              onClick={async () => {
                const result = await run(
                  () =>
                    mergeable.kind === "life"
                      ? ipc.mergeLifeEntries(mergeable.ids)
                      : ipc.mergeSessions(mergeable.ids),
                  "Couldn't merge those.",
                );
                if (result) {
                  // The absorbed seconds are the claim the merge just made.
                  // Saying it is the difference between an edit and a surprise.
                  toast(
                    `${result.merged} records became one · ${fmt.duration(result.seconds)}` +
                      (result.absorbedSec > 0
                        ? `, including ${fmt.duration(result.absorbedSec)} between them that nothing had claimed.`
                        : "."),
                  );
                  setRange(null);
                  await load();
                }
              }}
            >
              Merge {mergeable.ids.length} · {mergeable.name}
            </button>
          )}
          <button
            className="btn btn-primary"
            onClick={() => setFill({ from: span.from, to: span.to })}
          >
            Record all of it
          </button>
          <button className="btn" onClick={() => setRange(null)}>
            Clear
          </button>
        </div>
      )}

      <div className="day-body">
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
                <th scope="col">PC evidence</th>
                <th scope="col">Classification</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((slot) => (
                <SlotRow
                  key={slot.index}
                  slot={slot}
                  day={day}
                  selected={liveSelection}
                  onSelect={setSelected}
                  onFill={(shift) => fillFrom(slot, shift)}
                  onMark={(extend) => markRange(slot, extend)}
                  inRange={
                    range != null &&
                    slot.index >= Math.min(range.anchor, range.head) &&
                    slot.index <= Math.max(range.anchor, range.head)
                  }
                />
              ))}
            </tbody>
          </table>
        </div>

        <IntervalDetail
          segment={liveSelection}
          areas={areas}
          onClose={() => setSelected(null)}
        />
      </div>

      {fill && (
        <FillDialog from={fill.from} to={fill.to} areas={areas} onClose={() => setFill(null)} />
      )}
    </div>
  );
}

/**
 * Six cards, in the order the plan asks the question. Sleep is broken out of
 * Life because a third of every month lands in one bucket otherwise and makes
 * the rest unreadable — the workbook reports it on its own line for the same
 * reason.
 *
 * Unaccounted is hatched, like the rows it counts, so the eye connects the
 * figure to the thing it can act on.
 */
function SummaryCards({ totals }: { totals: DayView["totals"] }) {
  const cards = [
    { label: "Work", sec: totals.confirmedWorkSec, cls: "l-work" },
    { label: "Life", sec: totals.confirmedLifeSec - totals.sleepSec, cls: "l-life" },
    { label: "Sleep", sec: totals.sleepSec, cls: "l-sleep" },
    { label: "Entertainment", sec: totals.entertainmentSec, cls: "l-fun" },
    { label: "Observed only", sec: totals.observedOnlySec, cls: "l-observed" },
    { label: "Unaccounted", sec: totals.emptySec, cls: "l-empty", hatched: true },
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
            <strong className="data">{fmt.duration(c.sec)}</strong>
          </div>
        ))}
      </div>
      {/* The same numbers as proportions. It fills the width exactly because
          the totals behind it sum to the day — the counting invariant, drawn. */}
      <span
        className="day-bar"
        role="img"
        aria-label={`Of ${fmt.duration(totals.daySec)}: ${cards
          .filter((c) => c.sec > 0)
          .map((c) => `${c.label} ${fmt.duration(c.sec)}`)
          .join(", ")}`}
      >
        {[
          { sec: totals.confirmedWorkSec, cls: "l-work" },
          { sec: totals.confirmedLifeSec - totals.sleepSec, cls: "l-life" },
          { sec: totals.sleepSec, cls: "l-sleep" },
          { sec: totals.privateSec, cls: "l-private" },
          { sec: totals.observedOnlySec, cls: "l-observed" },
          { sec: totals.idleSec, cls: "l-idle" },
          { sec: totals.emptySec, cls: "l-empty" },
        ]
          .filter((l) => l.sec > 0)
          .map((l) => (
            <i
              key={l.cls}
              className={l.cls}
              style={{ width: `${(l.sec / Math.max(1, totals.daySec)) * 100}%` }}
            />
          ))}
      </span>
      <div className="row day-ledger-figures">
        <span className="micro" title="Plotted, which is the plan — never actual time">
          plotted <span className="data">{fmt.duration(totals.plannedSec)}</span>
        </span>
        <span
          className="micro"
          title="Time the machine was in front of someone. Overlaps the layers above on purpose — it answers a different question."
        >
          at the PC <span className="data">{fmt.duration(totals.pcSec)}</span>
        </span>
        {totals.privateSec > 0 && (
          <span className="micro">
            private <span className="data">{fmt.duration(totals.privateSec)}</span>
          </span>
        )}
      </div>
    </>
  );
}

function SlotRow({
  slot,
  day,
  selected,
  onSelect,
  onFill,
  onMark,
  inRange,
}: {
  slot: DaySlot;
  day: DayView;
  selected: DaySegment | null;
  onSelect: (s: DaySegment) => void;
  onFill: (extend: boolean) => void;
  onMark: (extend: boolean) => void;
  inRange: boolean;
}) {
  const rowRef = useRef<HTMLTableRowElement>(null);
  const isNow = day.isToday && day.now >= slot.startsAt && day.now < slot.endsAt;

  // Open on the current hour rather than at midnight.
  useEffect(() => {
    if (isNow) rowRef.current?.scrollIntoView({ block: "center" });
  }, [isNow]);

  const actual = slot.segments.filter((s) => s.owner.kind !== "empty");
  // A slot can hold both a record and a gap. The gap still needs its button, or
  // twenty unaccounted minutes beside a confirmed session are unreachable.
  const hasGap = slot.segments.some((s) => s.owner.kind === "empty");
  const evidence = slot.segments.flatMap((s) => s.evidence);
  const meta = STATE_META[slot.state];
  const rowsPerHour = 60 / (day.slotMinutes || 30);

  return (
    <tr
      ref={rowRef}
      className="day-row"
      data-state={slot.state}
      data-now={isNow || undefined}
      data-hour={slot.index % rowsPerHour === 0 || undefined}
      data-in-range={inRange || undefined}
      data-selected={
        selected && actual.some((s) => s.from === selected.from && s.to === selected.to)
          ? true
          : undefined
      }
    >
      <th scope="row" className="data day-time">
        {/* The whole cell is the selection handle, labelled or not — a
            half-hour row you cannot click is a half-hour you cannot include. */}
        <button
          className="day-time-mark"
          onClick={(e) => onMark(e.shiftKey)}
          aria-label={`Select from ${fmt.clock(slot.startsAt)}`}
          title="Click to start a selection · shift-click to extend it"
        >
          {/* Only the hour is labelled; the half-hours between are rhythm, not
              information, and repeating ":30" 24 times is noise. */}
          {slot.index % rowsPerHour === 0 ? fmt.clock(slot.startsAt) : ""}
        </button>
      </th>

      <td className="day-planned">
        {slot.plans.map((p) => (
          <span
            key={p.blockId}
            className="day-chip"
            data-plan="true"
            data-intent={p.intent}
            title={`Plotted ${fmt.duration(p.durationSec)} · tracked ${fmt.duration(p.trackedSec)}${
              p.intent === "entertainment"
                ? " · an entertainment window, not work"
                : p.intent === "life"
                  ? " · life, not work"
                  : ""
            }`}
          >
            <i style={{ background: p.projectColour ?? "var(--plot)" }} aria-hidden="true" />
            {/* An entertainment window sitting in the plan column has to say
                so, or the row reads as an hour of work nobody did. */}
            {p.intent !== "work" && (
              <span className="intent-mark micro">
                {p.intent === "entertainment" ? "Window" : "Life"} ·
              </span>
            )}
            {p.title}
          </span>
        ))}
      </td>

      <td className="day-actual">
        {actual.map((seg, i) => (
          <button
            key={i}
            className="day-chip"
            data-owner={seg.owner.kind}
            onClick={() => onSelect(seg)}
            title={`${fmt.clock(seg.from)}–${fmt.clock(seg.to)} · ${fmt.duration(
              (seg.to - seg.from) / 1000,
            )}`}
          >
            <i style={{ background: ownerColour(seg.owner) ?? "var(--muted)" }} aria-hidden="true" />
            {ownerLabel(seg.owner)}
          </button>
        ))}
        {/* Empty time is offered as an action, not shown as a blank. This is the
            button the ninety-second reconciliation target is actually spent on. */}
        {hasGap && (
          <button className="day-fill" onClick={() => onFill(false)}>
            {actual.length === 0 ? "Unaccounted — fill" : "+ fill the gap"}
          </button>
        )}
      </td>

      <td className="day-observed data micro">
        {evidence.slice(0, 2).map((a) => appName(a.appId)).join(", ")}
      </td>

      <td className="day-state">
        <span className="micro" title={meta.label}>
          <span aria-hidden="true" className="day-glyph">
            {meta.glyph}
          </span>
          {classification(slot)}
        </span>
      </td>
    </tr>
  );
}

/**
 * The selected interval (wireframe, right column). Everything about one
 * segment, and the one place a contribution mode can be set — which matters,
 * because contribution is what the monthly work summary groups by and it is
 * invisible on every other screen.
 */
function IntervalDetail({
  segment,
  areas,
  onClose,
}: {
  segment: DaySegment | null;
  areas: LifeAreaRow[];
  onClose: () => void;
}) {
  const run = useApp((s) => s.run);
  const reload = useApp((s) => s.loadDay);
  const toast = useApp((s) => s.toast);
  const openDetail = useApp((s) => s.openDetail);
  const entries = useApp((s) => s.dayEntries);

  if (!segment) {
    return (
      <aside className="day-detail">
        <p className="caption">
          Select an interval to see what it is made of, set a contribution mode, or reclassify it.
        </p>
      </aside>
    );
  }

  const { owner } = segment;
  const seconds = (segment.to - segment.from) / 1000;
  const entry =
    owner.kind === "life" ? entries.find((e: LifeEntryRow) => e.id === owner.entryId) : undefined;

  const setContribution = async (value: Contribution | null) => {
    if (owner.kind !== "work") return;
    const next = await run(
      () => ipc.setSessionContribution(owner.sessionId, value),
      "Couldn't set that contribution mode.",
    );
    if (next) await reload();
  };

  const convert = async (lifeAreaId: string) => {
    if (owner.kind !== "work") return;
    const moved = await run(
      () => ipc.convertSessionToLife(owner.sessionId, lifeAreaId, fmt.tz()),
      "Couldn't reclassify that.",
    );
    if (moved) {
      // Said out loud, because the contribution mode did not travel and the
      // user chose it deliberately a moment ago.
      toast(`Moved to ${moved.areaName}. Contribution modes don't apply to personal time.`);
      onClose();
      await reload();
    }
  };

  return (
    <aside className="day-detail">
      <div className="row">
        <h2 className="title grow">
          {fmt.clock(segment.from)}–{fmt.clock(segment.to)}
        </h2>
        <button aria-label="Close" onClick={onClose} style={{ color: "var(--muted)" }}>
          ✕
        </button>
      </div>
      <div className="row" style={{ flexWrap: "wrap", gap: 4 }}>
        <span className="tag">
          {owner.kind === "work"
            ? "Confirmed work"
            : owner.kind === "life"
              ? owner.isPrivate
                ? "Private"
                : "Confirmed life"
              : owner.kind === "observed"
                ? "Observed only"
                : owner.kind === "idle"
                  ? "Idle"
                  : "Unaccounted"}
        </span>
        <span className="tag data">{fmt.duration(seconds)}</span>
        {segment.hasDistraction && <span className="tag">Distraction inside</span>}
      </div>

      {owner.kind === "work" && (
        <>
          <div className="field">
            <label>Task</label>
            <button className="fake-input" onClick={() => void openDetail(owner.taskId)}>
              {owner.taskTitle}
            </button>
          </div>
          <div className="field">
            <label>Contribution</label>
            <select
              value={owner.contribution ?? ""}
              onChange={(e) => void setContribution((e.target.value || null) as Contribution | null)}
            >
              <option value="">Not recorded</option>
              {CONTRIBUTIONS.map((c) => (
                <option key={c.value} value={c.value}>
                  {c.label} — {c.hint}
                </option>
              ))}
            </select>
          </div>
          <div className="field">
            <label>Reclassify as life time</label>
            <select
              defaultValue=""
              onChange={(e) => e.target.value && void convert(e.target.value)}
              aria-label="Move this to a life area"
            >
              <option value="">Keep as work…</option>
              {areas.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.name}
                </option>
              ))}
            </select>
            <span className="caption">
              The session is replaced by a life entry. Its contribution mode does not travel —
              personal time has no such field.
            </span>
          </div>
        </>
      )}

      {(owner.kind === "work" || owner.kind === "life") && (
        <SplitControl segment={segment} onDone={reload} />
      )}

      {owner.kind === "life" && (
        <>
          <div className="field">
            <label>Life area</label>
            <div className="fake-input">
              <i className="swatch" style={{ background: owner.areaColour }} aria-hidden="true" />
              {owner.areaName}
              {owner.isPrivate && " · private"}
            </div>
          </div>
          {owner.label && (
            <div className="field">
              <label>Label</label>
              <div className="fake-input">{owner.label}</div>
            </div>
          )}
          {entry?.note && (
            <div className="field">
              <label>Note</label>
              <div className="fake-input note">{entry.note}</div>
            </div>
          )}
          {entry && <RepeatLife entry={entry} onDone={reload} />}
        </>
      )}

      {owner.kind === "observed" && (
        <div className="field">
          <label>Application</label>
          <div className="fake-input data">{owner.domain ?? appName(owner.appId)}</div>
          <span className="caption">
            Nobody has said what this time was. Reconcile turns it into a record — until then it is
            an observation, and it counts as observed, not as work.
          </span>
        </div>
      )}

      {segment.evidence.length > 0 && (
        <div className="field">
          <label>PC evidence</label>
          <div className="fake-input">
            {segment.evidence.map((a) => (
              <div key={a.appId} className="row micro">
                <span className="grow">{appName(a.appId)}</span>
                <span className="data">{fmt.duration(a.seconds)}</span>
              </div>
            ))}
          </div>
          <span className="caption">
            What the machine saw during this interval. Evidence, not duration — it is already
            counted once, above.
          </span>
        </div>
      )}
    </aside>
  );
}

/**
 * Split — one record becoming two, at a moment inside it (wireframe, Day).
 *
 * Merge's inverse, and the pair matters: an afternoon divided by mistake costs
 * one more click rather than an evening. The core guarantees the halves account
 * for exactly what the whole did, so this control's only job is choosing where.
 *
 * It opens at the midpoint and steps in quarter hours. A time input would be
 * more precise and worse: the point of a split is almost never a minute you
 * could name, it is "somewhere around there", and two buttons get you there
 * without a keyboard.
 */
function SplitControl({ segment, onDone }: { segment: DaySegment; onDone: () => Promise<void> }) {
  const run = useApp((s) => s.run);
  const toast = useApp((s) => s.toast);
  const [at, setAt] = useState<number | null>(null);

  const length = segment.to - segment.from;
  // Under two minutes there is no inside to pick — the core refuses anything
  // that would leave less than a minute on either side, and offering a control
  // that can only fail is worse than not offering one.
  if (length < 2 * 60_000) return null;
  const point = at ?? segment.from + Math.round(length / 2);

  const shift = (minutes: number) =>
    setAt(
      Math.min(
        segment.to - 60_000,
        Math.max(segment.from + 60_000, point + minutes * 60_000),
      ),
    );

  return (
    <div className="field">
      <label>Split</label>
      {at === null ? (
        <button className="btn" onClick={() => setAt(point)}>
          Split this in two
        </button>
      ) : (
        <>
          <div className="row">
            <button className="btn" onClick={() => shift(-15)}>
              −15m
            </button>
            <span className="grow data" style={{ textAlign: "center" }}>
              {fmt.clock(point)}
            </span>
            <button className="btn" onClick={() => shift(15)}>
              +15m
            </button>
          </div>
          <div className="row">
            <button
              className="btn btn-primary"
              onClick={async () => {
                const owner = segment.owner;
                const done = await run<unknown>(
                  () =>
                    owner.kind === "life"
                      ? ipc.splitLifeEntry(owner.entryId, point)
                      : owner.kind === "work"
                        ? ipc.splitSession(owner.sessionId, point)
                        : Promise.reject(new Error("Only records can be split.")),
                  "Couldn't split that.",
                );
                setAt(null);
                if (done) {
                  toast(
                    `Split at ${fmt.clock(point)} — ${fmt.duration(
                      (point - segment.from) / 1000,
                    )} then ${fmt.duration((segment.to - point) / 1000)}.`,
                  );
                  await onDone();
                }
              }}
            >
              Split at {fmt.clock(point)}
            </button>
            <button className="btn" onClick={() => setAt(null)}>
              Cancel
            </button>
          </div>
          <span className="caption">
            {fmt.duration((point - segment.from) / 1000)} then{" "}
            {fmt.duration((segment.to - point) / 1000)}. Nothing is added or lost — the two halves
            account for exactly what the one did.
          </span>
        </>
      )}
    </div>
  );
}

/**
 * Making a life entry repeat, and unmaking it (M9).
 *
 * Sleep is the case this exists for: the largest block of anyone's week, the
 * one that is genuinely the same every night, and — before this — the one that
 * had to be typed in daily.
 *
 * The presets come from Rust, the same code that parses them, so the picker can
 * never offer a rule the engine would refuse. Removing asks for a scope rather
 * than guessing: "just tonight" and "three months of sleep" are different
 * enough that inferring which was meant is a data-loss bug with a friendly name.
 */
function RepeatLife({ entry, onDone }: { entry: LifeEntryRow; onDone: () => Promise<void> }) {
  const run = useApp((s) => s.run);
  const toast = useApp((s) => s.toast);
  const [presets, setPresets] = useState<RrulePreset[]>([]);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    void ipc
      .getRrulePresets()
      .then(setPresets)
      .catch(() => setPresets([]));
  }, []);

  if (entry.seriesId) {
    return (
      <div className="field">
        <label>Repeats</label>
        <div className="row">
          <span className="grow caption">
            One of a repeating set. Each night is a real entry — edit or remove any of them on
            its own.
          </span>
        </div>
        <div className="row">
          {(
            [
              ["instance", "Just this one"],
              ["future", "This and later"],
              ["all", "All of them"],
            ] as const
          ).map(([scope, label]) => (
            <button
              key={scope}
              className="btn"
              onClick={async () => {
                const token = await run(
                  () => ipc.deleteLifeSeries(entry.id, scope),
                  "Couldn't remove that.",
                );
                if (token) {
                  toast(token.label, {
                    action: {
                      label: "Undo",
                      run: async () => {
                        await ipc.restore(token).catch(() => {});
                        await onDone();
                      },
                    },
                  });
                  await onDone();
                }
              }}
            >
              Remove · {label}
            </button>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="field">
      <label>Repeats</label>
      {!open ? (
        <button className="btn" onClick={() => setOpen(true)}>
          Make this repeat
        </button>
      ) : presets.length === 0 ? (
        <p className="caption">Repeat presets aren't available in browser preview.</p>
      ) : (
        <div className="stack" style={{ gap: 4 }}>
          {presets.map((p) => (
            <button
              key={p.rrule}
              className="btn"
              onClick={async () => {
                const rows = await run(
                  () => ipc.repeatLifeEntry(entry.id, p.rrule),
                  "Couldn't make that repeat.",
                );
                setOpen(false);
                if (rows) {
                  // Say how many: a rule that silently produces nothing — a
                  // monthly 31st, say — would otherwise look like it worked.
                  toast(`${p.description} — ${rows.length - 1} more over the next 90 days.`);
                  await onDone();
                }
              }}
            >
              <span className="grow" style={{ textAlign: "left" }}>
                {p.label}
              </span>
              <span className="micro" style={{ color: "var(--muted)" }}>
                {p.description}
              </span>
            </button>
          ))}
          <button className="btn" onClick={() => setOpen(false)}>
            Cancel
          </button>
        </div>
      )}
    </div>
  );
}

/**
 * Filling an interval. One decision and nothing else — this is where the
 * ninety-second target is spent, so the area grid is the whole dialog.
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
  const backlog = useApp((s) => s.backlog);
  const [start, setStart] = useState(from);
  const [end, setEnd] = useState(to);
  const [replace, setReplace] = useState(false);
  /* Which kind of confirmed time this is. Life opens first because filling a
     gap on the Day view is most often life — but work has to be here at all,
     which it was not: the observer sees one machine, so an offline meeting or
     an hour on a second computer produces no observation and can *only* be
     entered by hand. Sending someone to Task detail to do it means leaving the
     screen where they noticed the gap. */
  const [kind, setKind] = useState<"life" | "work">("life");
  const [taskId, setTaskId] = useState("");
  const [taskFilter, setTaskFilter] = useState("");
  const [contribution, setContribution] = useState<Contribution | "">("");

  const tasks = useMemo(() => {
    const all = backlog?.tasks ?? [];
    const needle = taskFilter.trim().toLowerCase();
    const matching = needle
      ? all.filter((t) => t.title.toLowerCase().includes(needle))
      : all;
    // Capped, because this is a picker inside a dialog and not the backlog.
    // The filter is how you reach anything past the cap.
    return matching.slice(0, 40);
  }, [backlog, taskFilter]);

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
          startedAt: start,
          endedAt: end,
          tz: fmt.tz(),
          isPrivate,
          replaceExisting: replace,
        }),
      "Couldn't record that.",
    );
    onClose();
    if (entry) {
      toast(`${fmt.clock(start)}–${fmt.clock(end)} recorded as ${entry.areaName}.`);
      await reload();
    }
  };

  const recordWork = async () => {
    if (!taskId) return;
    const session = await run(
      () =>
        ipc.addSession({
          taskId,
          startedAt: start,
          endedAt: end,
          contribution: contribution || null,
          replaceExisting: replace,
        }),
      "Couldn't record that.",
    );
    onClose();
    if (session) {
      toast(
        `${fmt.clock(start)}–${fmt.clock(end)} recorded against ${session.taskTitle}.`,
      );
      await reload();
    }
  };

  const shift = (which: "start" | "end", minutes: number) => {
    if (which === "start") setStart((s) => Math.min(end - 5 * 60_000, s + minutes * 60_000));
    else setEnd((e) => Math.max(start + 5 * 60_000, e + minutes * 60_000));
  };

  /* Typing a time keeps the *date* of the interval and replaces only the clock,
     so editing the end of a 23:40 → 00:20 span cannot silently move it a day.
     An empty or half-typed value is ignored rather than treated as midnight —
     `<input type="time">` fires on every keystroke, and the "1" of "14" is not
     a request to jump to 01:00.

     When the typed time would cross the other end, the interval **moves and
     keeps its length** rather than the value being clamped. Clamping was the
     first thing tried and it is quietly wrong: typing 14:20 into a 05:30–06:00
     span produced 05:59, a time nobody asked for and which looks like the app
     ignoring the keyboard. Someone typing a start later than the end is
     relocating the interval, which is what a calendar does too. */
  const setClock = (which: "start" | "end", value: string) => {
    const [h, m] = value.split(":").map(Number);
    if (!Number.isFinite(h) || !Number.isFinite(m)) return;
    const d = new Date(which === "start" ? start : end);
    d.setHours(h!, m!, 0, 0);
    const next = d.getTime();
    const length = end - start;

    if (which === "start") {
      setStart(next);
      if (next >= end) setEnd(next + length);
    } else {
      setEnd(next);
      if (next <= start) setStart(next - length);
    }
  };

  return (
    <>
      <div className="scrim" onClick={onClose} />
      <div className="modal overlay" role="dialog" aria-modal="true" aria-label="Record an interval">
        <div className="sheet-head">
          <strong className="title">
            {fmt.clock(start)}–{fmt.clock(end)} · {fmt.duration((end - start) / 1000)}
          </strong>
        </div>
        <div className="sheet-body stack">
          {/* Typed *and* nudged. The stepper is the right control for trimming
              what the app already guessed; it is the wrong one for entering a
              meeting that ran 14:20 to 16:05, which no number of half-hour
              steps can reach. */}
          {/* One row per endpoint. Both on one line fitted before the time
              inputs existed; with them the End "+30m" was pushed outside the
              dialog, where it could still be tabbed to but never seen. */}
          {(
            [
              ["start", "Start", start] as const,
              ["end", "End", end] as const,
            ]
          ).map(([which, label, value]) => (
            <div className="row" key={which}>
              <label className="caption" htmlFor={`fill-${which}`} style={{ width: 44 }}>
                {label}
              </label>
              <input
                id={`fill-${which}`}
                type="time"
                className="data"
                value={fmt.clockValue(value)}
                onChange={(e) => setClock(which, e.target.value)}
              />
              <span className="grow" />
              <button
                className="btn"
                aria-label={`${label} 30 minutes earlier`}
                onClick={() => shift(which, -30)}
              >
                −30m
              </button>
              <button
                className="btn"
                aria-label={`${label} 30 minutes later`}
                onClick={() => shift(which, 30)}
              >
                +30m
              </button>
            </div>
          ))}

          {/* Both kinds of confirmed time are reachable from the gap, because
              both kinds of time happen in one. */}
          <div className="segmented" role="group" aria-label="What kind of time">
            <button
              className="btn"
              aria-pressed={kind === "life"}
              onClick={() => setKind("life")}
            >
              Life
            </button>
            <button
              className="btn"
              aria-pressed={kind === "work"}
              onClick={() => setKind("work")}
            >
              Work
            </button>
          </div>

          {kind === "life" ? (
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
          ) : (
            <div className="stack" style={{ gap: 6 }}>
              <input
                aria-label="Find a task"
                placeholder="Find a task…"
                value={taskFilter}
                onChange={(e) => setTaskFilter(e.target.value)}
              />
              <div className="area-grid" role="listbox" aria-label="Task">
                {tasks.map((t) => (
                  <button
                    key={t.id}
                    className="btn"
                    role="option"
                    aria-selected={taskId === t.id}
                    data-chosen={taskId === t.id || undefined}
                    onClick={() => setTaskId(t.id)}
                  >
                    <span className="grow" style={{ textAlign: "left" }}>
                      {t.title}
                    </span>
                  </button>
                ))}
              </div>
              {tasks.length === 0 && (
                <p className="caption">
                  {backlog
                    ? "No task matches that. Clear the filter, or create the task first — a session has to belong to something."
                    : "Tasks aren't available in browser preview."}
                </p>
              )}

              <label className="row caption">
                <span style={{ width: 96 }}>Contribution</span>
                <select
                  aria-label="Contribution"
                  value={contribution}
                  onChange={(e) => setContribution(e.target.value as Contribution | "")}
                >
                  <option value="">Not recorded</option>
                  {CONTRIBUTIONS.map((c) => (
                    <option key={c.value} value={c.value}>
                      {c.label} — {c.hint}
                    </option>
                  ))}
                </select>
              </label>
              {/* Said here rather than in a tooltip, because this is the field
                  people fill in when the entry is a meeting, and a meeting is
                  the commonest thing entered by hand. */}
              <p className="caption">
                Work away from this PC — a second machine, an offline meeting, a
                task done on paper — is never observed, so this is the only way
                it is ever recorded.
              </p>
            </div>
          )}

          <label className="row caption">
            <input
              type="checkbox"
              checked={replace}
              onChange={(e) => setReplace(e.target.checked)}
            />
            {/* Never the default: replacing deletes a confirmed record, so the
                user has to have seen the sentence saying so (M9). */}
            Replace anything already recorded in this interval
          </label>

          <div className="row">
            {/* Private is a life state — "accounted for, nothing recorded about
                it". There is no private work: a work session names a task by
                definition, so the concept has nowhere to attach. */}
            {kind === "life" && (
              <button
                className="btn"
                onClick={() => void fill(areas[0]?.id ?? "", true)}
                disabled={!areas.length}
                title="Accounted for, so the reconciler stops asking — but nothing is recorded about it"
              >
                Private
              </button>
            )}
            <span className="grow" />
            <button className="btn" onClick={onClose}>
              Cancel <span className="kbd">Esc</span>
            </button>
            {/* Life commits by picking an area — one click, which is the
                ninety-second budget. Work needs a second step because a task
                has to be chosen first, so it gets an explicit button. */}
            {kind === "work" && (
              <button
                className="btn btn-primary"
                disabled={!taskId}
                onClick={() => void recordWork()}
              >
                Record {fmt.duration((end - start) / 1000)} of work
              </button>
            )}
          </div>
        </div>
      </div>
    </>
  );
}
