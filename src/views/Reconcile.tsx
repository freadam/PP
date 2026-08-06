/**
 * The Reconcile sheet (wireframe screen 4) — the single highest-leverage
 * feature in the spec, and the one the ninety-second daily target is spent in.
 *
 * Three columns, and the split is the argument:
 *
 *   **queue** — how many decisions are left, and which are done. Reconciling is
 *     a finite job, and a screen that hides how finite is a screen people quit.
 *   **decision** — one item, its explanation, and numbered choices with one
 *     recommended. Numbers because the fastest path through seven items is
 *     seven keystrokes.
 *   **evidence** — where the claim came from. Only observed items have it, and
 *     they are the only ones asking the user to accept a *machine's* assertion
 *     rather than their own. Its last line is the privacy promise, restated at
 *     the moment someone is looking at a record of what they did.
 *
 * Modal, entirely keyboard-driven, and it never blocks the app: `Esc` defers,
 * and a deferred day stays available for seven days before auto-accepting (U11).
 */

import { useEffect, useMemo, useState } from "react";
import { useApp } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type { DayReview, ReconcileItem, ReconcileKind, ReconcileVerb } from "../lib/types";

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
  assignToTask: "Attach to a work task",
  logAsBreak: "Log as break",
  ignore: "Leave it unaccounted",
  recordAsLife: "Record as life time",
  markPrivate: "Mark private / intentionally untracked",
};

/** A one-line consequence, where the verb's name doesn't carry it. */
const VERB_HINT: Partial<Record<ReconcileVerb, string>> = {
  assignToTask: "Preserves the observation as evidence rather than replacing it",
  recordAsLife: "Pick a life area — this is what makes the hour count",
  markPrivate: "Accounted for, nothing recorded about it",
  ignore: "It stays a gap, and the month's account says so",
  accept: "Record what happened, and move on",
  reviseEstimate: "Makes the next estimate better",
};

/**
 * The verbs that are a verdict about a *website* rather than about an interval.
 *
 * "Record as life time" says this domain is not work; "attach to a work task"
 * says it is how work gets done. The rest — accept, ignore, mark private — are
 * decisions about one hour, and putting a rule behind a shrug is how someone
 * ends up with a classifier they did not mean to train.
 */
const RULE_VERBS = new Set<ReconcileVerb>(["recordAsLife", "assignToTask"]);

const KIND_TAG: Record<ReconcileKind, string> = {
  overran: "Overrun",
  neverStarted: "Planned, not started",
  untrackedGap: "Untracked gap",
  unplannedSession: "Unplanned work",
  observedOnly: "Observed only",
  empty: "Unaccounted",
};

export function ReconcileSheet() {
  const close = useApp((s) => s.setOverlay);
  const unreconciled = useApp((s) => s.unreconciled);
  const explicitDate = useApp((s) => s.reconcileDate);
  const areas = useApp((s) => s.lifeAreas);
  const date = explicitDate ?? unreconciled[0] ?? fmt.addDays(fmt.today(), -1);

  const [items, setItems] = useState<ReconcileItem[] | null>(null);
  const [index, setIndex] = useState(0);
  const [chosen, setChosen] = useState<Record<string, ReconcileVerb>>({});
  const [areaFor, setAreaFor] = useState<Record<string, string>>({});
  const [done, setDone] = useState<Set<string>>(new Set());
  /** Which items should also write a prospective domain rule. */
  const [applyRule, setApplyRule] = useState<Set<string>>(new Set());
  const [review, setReview] = useState<DayReview | null>(null);

  useEffect(() => {
    void ipc
      .getReconcileItems(date, fmt.tz())
      .then((list) => {
        setItems(list);
        // Every item has a default; the sheet is pre-filled so holding Enter is
        // a legitimate way through a day you have nothing to say about.
        setChosen(Object.fromEntries(list.map((i) => [i.id, i.defaultAction])));
      })
      .catch(() => setItems([]));
  }, [date]);

  const current = items?.[index];
  const ruleDomain = current?.evidence?.domain ?? null;
  const summary = useMemo(() => {
    if (!items) return null;
    return {
      planned: items.reduce((n, i) => n + i.plannedSec, 0),
      tracked: items.reduce((n, i) => n + i.trackedSec, 0),
    };
  }, [items]);

  const choose = (verb: ReconcileVerb) => {
    if (!current) return;
    setChosen((c) => ({ ...c, [current.id]: verb }));
    setDone((d) => new Set(d).add(current.id));
    if (index < (items?.length ?? 0) - 1) setIndex(index + 1);
  };

  const apply = async () => {
    if (!items) return;
    const actions = items.map((item) => ({
      itemId: item.id,
      verb: chosen[item.id] ?? item.defaultAction,
      startsAt: item.suggestedSlot,
      durationSec: item.suggestedDurationSec,
      taskId: item.taskId,
      lifeAreaId: areaFor[item.id] ?? areas[0]?.id ?? null,
      ruleForDomain: applyRule.has(item.id) ? (item.evidence?.domain ?? null) : null,
    }));
    const result = await useApp
      .getState()
      .run(() => ipc.applyReconcile(date, actions, fmt.tz()), "Couldn't close the day.");
    if (result) {
      setReview(result);
      await useApp.getState().refresh();
    }
  };

  /* One key per choice, plus Enter for the recommended one. Seven items should
     take seven keystrokes (§3.7). */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (review || !current) return;
      if (e.key === "Enter") {
        e.preventDefault();
        choose(chosen[current.id] ?? current.defaultAction);
        return;
      }
      const n = Number(e.key);
      if (n >= 1 && n <= current.available.length) {
        e.preventDefault();
        choose(current.available[n - 1]!);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [current, chosen, review, index, items]);

  return (
    <>
      <div className="scrim" onClick={() => close(null)} />
      <div
        className="reconcile-modal overlay"
        role="dialog"
        aria-modal="true"
        aria-label="Reconcile"
      >
        {review ? (
          <div className="decision">
            <p className="display" style={{ fontSize: "1.125rem" }}>
              {review.takeaway}
            </p>
            <p className="caption data">
              plotted {fmt.duration(review.plannedSec)} · tracked{" "}
              {fmt.duration(review.trackedSec)} · unplanned{" "}
              {fmt.duration(review.unplannedSec)}
            </p>
            {review.streakDays > 1 && (
              <p className="caption">{review.streakDays} days reconciled in a row.</p>
            )}
            <button className="btn btn-primary" autoFocus onClick={() => close(null)}>
              Done
            </button>
          </div>
        ) : (
          <>
            <aside className="queue">
              <h3>
                Reconcile · {items ? Math.min(index + 1, items.length) : 0} of{" "}
                {items?.length ?? 0}
              </h3>
              <p className="caption">{fmt.longDate(date)}</p>
              <div className="queue-list stack" style={{ gap: 4 }}>
                {(items ?? []).map((item, i) => (
                  <button
                    key={item.id}
                    className="queue-item"
                    aria-current={i === index || undefined}
                    data-done={done.has(item.id) || undefined}
                    onClick={() => setIndex(i)}
                  >
                    <span className="micro">
                      {done.has(item.id) ? "✓ " : ""}
                      {KIND_TAG[item.kind]}
                    </span>
                    <span className="queue-when data micro">
                      {item.startsAt ? fmt.clock(item.startsAt) : ""}
                      {item.driftSec !== 0 && ` · ${fmt.drift(item.driftSec)}`}
                    </span>
                  </button>
                ))}
              </div>
              {/* Pinned, not pushed to the bottom of a list: a day with
                  twenty unaccounted hours has twenty items, and "Close the day"
                  must never be something you scroll to find. Deferring is a
                  first-class outcome — a sheet that cannot be left is a sheet
                  people stop opening (U11). */}
              <div className="queue-foot stack" style={{ gap: 4 }}>
                <button className="btn" onClick={() => close(null)}>
                  Defer remaining <span className="kbd">Esc</span>
                </button>
                <button className="btn btn-primary" onClick={() => void apply()}>
                  Close the day
                </button>
              </div>
            </aside>

            <section className="decision">
              {!items ? (
                <p className="caption">Reading the day…</p>
              ) : items.length === 0 ? (
                <>
                  <p className="caption">
                    Nothing to reconcile on {fmt.longDate(date)} — the plot, the record and the
                    clock all agree.
                  </p>
                  <button className="btn btn-primary" onClick={() => void apply()}>
                    Close the day
                  </button>
                </>
              ) : current ? (
                <>
                  <span className="tag">{KIND_TAG[current.kind]}</span>
                  <h2 className="title">{current.title}</h2>
                  <p className="caption">{current.explanation}</p>

                  <div className="stack" style={{ gap: 6, marginTop: 12 }}>
                    {current.available.map((verb, i) => {
                      const isDefault = verb === current.defaultAction;
                      return (
                        <button
                          key={verb}
                          className="choice"
                          data-recommended={isDefault || undefined}
                          data-chosen={chosen[current.id] === verb || undefined}
                          onClick={() => choose(verb)}
                        >
                          <b>
                            <span className="data">{i + 1}</span> · {VERB_LABEL[verb]}
                          </b>
                          {isDefault && current.recommendation ? (
                            <span className="caption">{current.recommendation}</span>
                          ) : (
                            VERB_HINT[verb] && <span className="caption">{VERB_HINT[verb]}</span>
                          )}
                        </button>
                      );
                    })}
                  </div>

                  {/* The area picker only appears for the verb that needs one —
                      a dropdown that is inert for four of five choices is noise. */}
                  {chosen[current.id] === "recordAsLife" && (
                    <div className="field" style={{ marginTop: 12 }}>
                      <label htmlFor="rec-area">Life area</label>
                      <select
                        id="rec-area"
                        value={areaFor[current.id] ?? areas[0]?.id ?? ""}
                        onChange={(e) =>
                          setAreaFor((a) => ({ ...a, [current.id]: e.target.value }))
                        }
                      >
                        {areas.map((a) => (
                          <option key={a.id} value={a.id}>
                            {a.name}
                          </option>
                        ))}
                      </select>
                    </div>
                  )}

                  {/* The wireframe's "apply my choice to future activity in this
                      context". It appears only for a claim with a domain behind
                      it: an application name is not durable enough to key a rule
                      on, and a checkbox that is inert half the time teaches
                      people to ignore it.

                      Unticked by default, and it says which way it applies. A
                      rule made by accident is a rule that quietly mislabels a
                      month, and one that reached backwards would rewrite days
                      already signed off. */}
                  {ruleDomain && RULE_VERBS.has(chosen[current.id] ?? current.defaultAction) ? (
                    <label className="row" style={{ marginTop: 12, gap: 8 }}>
                      <input
                        type="checkbox"
                        checked={applyRule.has(current.id)}
                        onChange={(e) =>
                          setApplyRule((prev) => {
                            const next = new Set(prev);
                            if (e.target.checked) next.add(current.id);
                            else next.delete(current.id);
                            return next;
                          })
                        }
                      />
                      <span className="stack" style={{ gap: 2 }}>
                        <span>
                          Apply this to future activity on{" "}
                          <span className="data">{ruleDomain}</span>
                        </span>
                        <span className="caption">
                          Also fills in any stretch of it that has no label yet. What you
                          decided on a day you have closed is untouched.
                        </span>
                      </span>
                    </label>
                  ) : (
                    <p className="caption" style={{ marginTop: 12 }}>
                      {ruleDomain
                        ? "This choice says nothing about the site itself, so there is no rule to make from it."
                        : "This interval is decided on its own — there is no website behind it to make a rule from."}
                    </p>
                  )}
                </>
              ) : null}
            </section>

            <aside className="evidence">
              <h3>Evidence</h3>
              {current?.evidence ? (
                <>
                  <Field label="Source">{current.evidence.source}</Field>
                  <Field label="Subject">{current.evidence.subject}</Field>
                  <Field label="Confidence">{current.evidence.confidence}</Field>
                  {current.evidence.adjacent.length > 0 && (
                    <Field label="Adjacent time">
                      {current.evidence.adjacent.map((a) => (
                        <div key={a} className="data micro">
                          {a}
                        </div>
                      ))}
                    </Field>
                  )}
                  <Field label="Storage">{current.evidence.storage}</Field>
                </>
              ) : (
                <p className="caption">
                  {current
                    ? "Nothing observed here — this item is about your own record, not the machine's."
                    : ""}
                </p>
              )}
              {summary && (
                <p className="caption" style={{ marginTop: 12 }}>
                  This day: plotted <span className="data">{fmt.duration(summary.planned)}</span>,
                  tracked <span className="data">{fmt.duration(summary.tracked)}</span>.
                </p>
              )}
            </aside>
          </>
        )}
      </div>
    </>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="field">
      <label>{label}</label>
      <div className="fake-input">{children}</div>
    </div>
  );
}
