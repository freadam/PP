/**
 * The Reconcile sheet (§3.7) — the single highest-leverage feature in the spec.
 *
 * Modal, one screen, entirely keyboard-driven. It never blocks the app: `Esc`
 * defers, and a deferred day stays available for seven days before it is
 * auto-accepted (U11).
 */

import { useEffect, useMemo, useState } from "react";
import { useApp } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type { DayReview, ReconcileItem, ReconcileVerb } from "../lib/types";

const VERB_LABEL: Record<ReconcileVerb, string> = {
  accept: "Accept",
  rescheduleRemainder: "Reschedule remainder",
  split: "Split",
  drop: "Drop",
  markDone: "Mark done",
  reviseEstimate: "Revise estimate",
  moveToTomorrow: "Move to tomorrow",
  leaveUnscheduled: "Leave unscheduled",
  createRetroBlock: "Create block",
  assignToTask: "Assign to task",
  logAsBreak: "Log as break",
  ignore: "Ignore",
};

/** One-key actions (§3.7). Every verb also reaches the command palette. */
const VERB_KEY: Partial<Record<ReconcileVerb, string>> = {
  accept: "A",
  rescheduleRemainder: "R",
  split: "S",
  drop: "D",
  markDone: "M",
  moveToTomorrow: "T",
  createRetroBlock: "B",
  ignore: "I",
};

export function ReconcileSheet() {
  const close = useApp((s) => s.setOverlay);
  const unreconciled = useApp((s) => s.unreconciled);
  const explicitDate = useApp((s) => s.reconcileDate);
  const date = explicitDate ?? unreconciled[0] ?? fmt.addDays(fmt.today(), -1);

  const [items, setItems] = useState<ReconcileItem[] | null>(null);
  const [index, setIndex] = useState(0);
  const [chosen, setChosen] = useState<Record<string, ReconcileVerb>>({});
  const [review, setReview] = useState<DayReview | null>(null);

  useEffect(() => {
    void ipc
      .getReconcileItems(date, fmt.tz())
      .then((list) => {
        setItems(list);
        // Each item type has a default action; the sheet is pre-filled so
        // holding Enter is a legitimate way through.
        setChosen(Object.fromEntries(list.map((i) => [i.id, i.defaultAction])));
      })
      .catch(() => setItems([]));
  }, [date]);

  const current = items?.[index];

  const apply = async () => {
    if (!items) return;
    const { run, refresh, toast } = useApp.getState();
    const actions = items.map((item) => ({
      itemId: item.id,
      verb: chosen[item.id] ?? item.defaultAction,
      startsAt: item.suggestedSlot,
      durationSec: item.suggestedDurationSec,
      taskId: item.taskId,
    }));
    const result = await run(
      () => ipc.applyReconcile(date, actions, fmt.tz()),
      "Couldn't reconcile that day.",
    );
    if (!result) return;
    setReview(result);
    toast(result.takeaway);
    const remaining = await ipc.getUnreconciledDays(fmt.today()).catch(() => []);
    useApp.setState({ unreconciled: remaining, reconcileDate: null });
    await refresh();
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (review) return;
      if (e.key === "ArrowDown" || e.key === "j") {
        e.preventDefault();
        setIndex((i) => Math.min((items?.length ?? 1) - 1, i + 1));
      } else if (e.key === "ArrowUp" || e.key === "k") {
        e.preventDefault();
        setIndex((i) => Math.max(0, i - 1));
      } else if (e.key === "Enter") {
        e.preventDefault();
        if (items && index >= items.length - 1) void apply();
        else setIndex((i) => i + 1);
      } else if (current) {
        const hit = current.available.find(
          (v) => VERB_KEY[v]?.toLowerCase() === e.key.toLowerCase(),
        );
        if (hit) {
          e.preventDefault();
          setChosen((c) => ({ ...c, [current.id]: hit }));
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [items, index, current, review]);

  const summary = useMemo(() => {
    if (!items) return null;
    const planned = items.reduce((n, i) => n + i.plannedSec, 0);
    const tracked = items.reduce((n, i) => n + i.trackedSec, 0);
    return { planned, tracked };
  }, [items]);

  return (
    <>
      <div className="scrim" onClick={() => close(null)} />
      <div className="modal modal-wide overlay" role="dialog" aria-modal="true" aria-label="Reconcile">
        <div className="sheet-head">
          <strong className="title grow">Reconcile · {fmt.longDate(date)}</strong>
          {items && !review && (
            <span className="data caption">
              {Math.min(index + 1, items.length)} of {items.length}
            </span>
          )}
        </div>

        {review ? (
          <div className="sheet-body stack">
            <p className="display" style={{ fontSize: "1.125rem" }}>
              {review.takeaway}
            </p>
            <p className="caption data">
              plotted {fmt.duration(review.plannedSec)} · tracked{" "}
              {fmt.duration(review.trackedSec)} · unplanned{" "}
              {fmt.duration(review.unplannedSec)}
            </p>
            {/* §2.3 — the streak is stated where it is earned, once, in plain
                caption type. It is not a badge, a flame or a thing you can
                lose: closing the day is the habit, and this is the receipt. */}
            {review.streakDays > 1 && (
              <p className="caption">
                {review.streakDays} days reconciled in a row.
              </p>
            )}
            <div className="row">
              <button className="btn btn-primary" autoFocus onClick={() => close(null)}>
                Done
              </button>
            </div>
          </div>
        ) : !items ? (
          <div className="sheet-body">
            <p className="caption">Reading the day…</p>
          </div>
        ) : items.length === 0 ? (
          <div className="sheet-body stack">
            <p className="caption">
              Nothing to reconcile on {fmt.longDate(date)} — the plot and the track agree.
            </p>
            <button className="btn btn-primary" onClick={() => void apply()}>
              Close the day
            </button>
          </div>
        ) : (
          <>
            <div style={{ maxHeight: "52vh", overflow: "auto" }}>
              {items.map((item, i) => (
                <div
                  key={item.id}
                  className="reconcile-item"
                  style={{
                    background: i === index ? "var(--raised)" : undefined,
                    boxShadow: i === index ? "inset 2px 0 0 var(--plot)" : undefined,
                  }}
                  onClick={() => setIndex(i)}
                >
                  <div className="row spread">
                    <strong>{item.title}</strong>
                    <span className="micro" style={{ color: "var(--faint)" }}>
                      {item.kind === "overran"
                        ? "overran"
                        : item.kind === "neverStarted"
                          ? "never started"
                          : item.kind === "unplannedSession"
                            ? "unplanned"
                            : "gap"}
                    </span>
                  </div>

                  <p className="caption data" style={{ margin: "4px 0 8px" }}>
                    {item.plannedSec > 0 && <>plotted {fmt.duration(item.plannedSec)} · </>}
                    tracked {fmt.duration(item.trackedSec)}
                    {item.driftSec !== 0 && item.plannedSec > 0 && (
                      <> · drift {fmt.drift(item.driftSec)}</>
                    )}
                  </p>

                  {item.plannedSec > 0 && (
                    <span className="rail-bar" style={{ maxWidth: 320 }}>
                      <span className="plot" style={{ left: 0, width: "60%" }} />
                      <span
                        className="track"
                        style={{
                          width: `${Math.min(60, (item.trackedSec / Math.max(1, item.plannedSec)) * 60)}%`,
                        }}
                      />
                      {item.driftSec > 0 && (
                        <span
                          className="tail"
                          style={{
                            left: "60%",
                            width: `${Math.min(40, (item.driftSec / Math.max(1, item.plannedSec)) * 60)}%`,
                          }}
                        />
                      )}
                    </span>
                  )}

                  <div className="reconcile-verbs">
                    {item.available.map((verb) => (
                      <button
                        key={verb}
                        className="btn"
                        aria-pressed={(chosen[item.id] ?? item.defaultAction) === verb}
                        onClick={(e) => {
                          e.stopPropagation();
                          setChosen((c) => ({ ...c, [item.id]: verb }));
                        }}
                      >
                        {VERB_KEY[verb] && <span className="kbd">{VERB_KEY[verb]}</span>}
                        {VERB_LABEL[verb]}
                      </button>
                    ))}
                  </div>

                  {item.suggestedSlot &&
                    (chosen[item.id] === "rescheduleRemainder" ||
                      chosen[item.id] === "moveToTomorrow") && (
                      <p className="caption data" style={{ marginTop: 8 }}>
                        → {fmt.longDate(fmt.toLocalDate(new Date(item.suggestedSlot)))} at{" "}
                        {fmt.clock(item.suggestedSlot)} ({fmt.duration(item.suggestedDurationSec)})
                      </p>
                    )}
                </div>
              ))}
            </div>

            <div className="reconcile-foot caption">
              <span>
                <span className="kbd">↑↓</span> move
              </span>
              <span>
                <span className="kbd">Enter</span> apply
              </span>
              <span>
                <span className="kbd">Esc</span> finish later
              </span>
              <span className="grow" />
              {summary && (
                <span className="data">
                  {fmt.duration(summary.planned)} plotted · {fmt.duration(summary.tracked)} tracked
                </span>
              )}
              <button className="btn btn-primary" onClick={() => void apply()}>
                Apply
              </button>
            </div>
          </>
        )}
      </div>
    </>
  );
}
