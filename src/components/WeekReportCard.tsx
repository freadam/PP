/**
 * The Monday-morning card, and the report behind it (W9).
 *
 * The habit this is built for was described precisely by the reviewer the plan
 * is drawn from: *"Rize sends me a weekly PDF report, and I take a quick look
 * on a Monday morning."* Two things follow.
 *
 * **It has to be waiting.** Not somewhere you can navigate to if you remember —
 * `MondayCard` appears on its own once a week has finished, and keeps appearing
 * until it has been read. It is the one card in Fruit that is not a response to
 * something the user just did.
 *
 * **It is skimmed.** The headline is one sentence and a seven-row table, and
 * everything below it is optional depth the reader is free to ignore. The card
 * shows only the sentence; the panel shows the rest.
 *
 * The file is the third thing. Fruit cannot email a report — an offline app has
 * no mailer, and inventing one would be the first outbound connection in a
 * product badged OFFLINE — so it writes a workbook and shows you where it went.
 */

import { useCallback, useEffect, useState } from "react";
import { useApp } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type { WeekReport, WeekReportDue } from "../lib/types";

/** Appears on its own once a week has finished. Not a toast: a toast is for
 *  something you just did, and this is about a week you have not looked at. */
export function MondayCard() {
  const go = useApp((s) => s.go);
  const run = useApp((s) => s.run);
  const toast = useApp((s) => s.toast);
  const setHorizon = useApp((s) => s.setReportHorizon);
  const [due, setDue] = useState<WeekReportDue | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    // A failure here is silence, not an error banner: "no report waiting" and
    // "couldn't check" look the same to someone at breakfast, and only one of
    // them is worth interrupting them for.
    void ipc
      .dueWeekReport(fmt.tz())
      .then(setDue)
      .catch(() => setDue(null));
  }, []);

  if (!due) return null;

  const dismiss = async () => {
    await ipc.markWeekReportSeen(due.week).catch(() => {});
    setDue(null);
  };

  const write = async () => {
    setBusy(true);
    const result = await run(
      () => ipc.exportWeekReport(due.from, fmt.tz()),
      "Couldn't write the report.",
    );
    setBusy(false);
    if (result) {
      toast(`Wrote ${result.path}`, {
        action: { label: "Show it", run: () => void ipc.revealPath(result.path).catch(() => {}) },
      });
      void dismiss();
    }
  };

  return (
    <div className="monday-card" role="status" aria-label={`Report for ${due.week}`}>
      <div className="row">
        <strong className="grow">Last week is ready</strong>
        <span className="data micro" style={{ color: "var(--muted)" }}>
          {due.from} – {due.to}
        </span>
      </div>
      <p className="headline">{headline(due)}</p>
      <div className="row">
        <button
          className="btn btn-primary"
          onClick={() => {
            setHorizon("week");
            go("reports");
            void dismiss();
          }}
        >
          Read it
        </button>
        {due.writtenTo ? (
          <button
            className="btn"
            onClick={() => void ipc.revealPath(due.writtenTo!).catch(() => {})}
          >
            Show the file
          </button>
        ) : (
          <button className="btn" onClick={() => void write()} disabled={busy}>
            {busy ? "Writing…" : "Save as .xlsx"}
          </button>
        )}
        <span className="grow" />
        <button className="btn" onClick={() => void dismiss()}>
          Dismiss
        </button>
      </div>
    </div>
  );
}

/** The sentence, built the same way Rust builds it for the file. Two figures,
 *  because two is what gets read. */
function headline(due: WeekReportDue): string {
  const ent = due.headline.categories.find((c) => c.name === "Entertainment")?.seconds ?? 0;
  return `${fmt.duration(due.headline.workSec)} of work, ${fmt.duration(ent)} of entertainment.`;
}

/**
 * The report itself. Headline first, always — everything else is depth.
 *
 * `date` is any day in the week to report on.
 */
export function WeekReportPanel({ date }: { date: string }) {
  const go = useApp((s) => s.go);
  const goDay = useApp((s) => s.setDayDate);
  const run = useApp((s) => s.run);
  const toast = useApp((s) => s.toast);
  const [report, setReport] = useState<WeekReport | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    setReport(await ipc.getWeekReport(date, fmt.tz()).catch(() => null));
  }, [date]);

  useEffect(() => {
    void load();
  }, [load]);

  if (!report) return null;
  const h = report.headline;
  const ent = h.categories.find((c) => c.name === "Entertainment")?.seconds ?? 0;

  const save = async () => {
    setBusy(true);
    const result = await run(
      () => ipc.exportWeekReport(report.review.from, fmt.tz()),
      "Couldn't write the report.",
    );
    setBusy(false);
    if (result) {
      toast(`Wrote ${result.path}`, {
        action: { label: "Show it", run: () => void ipc.revealPath(result.path).catch(() => {}) },
      });
    }
  };

  return (
    <section className="panel">
      <div className="row">
        <h3 className="grow">{h.isComplete ? "Last week" : "This week so far"}</h3>
        <span className="data caption">
          {report.review.from} – {report.review.to} · {report.review.week}
        </span>
        <button className="btn" onClick={() => void save()} disabled={busy}>
          {busy ? "Writing…" : "Save as .xlsx"}
        </button>
      </div>

      {/* The top. One sentence, then the breakdown — the order the reviewer
          asked for, and the reason this panel exists rather than a link to the
          month dashboard. */}
      <p className="headline">
        {fmt.duration(h.workSec)} of work, {fmt.duration(ent)} of entertainment.
      </p>

      <table className="table compact">
        <thead>
          <tr>
            <th scope="col">Category</th>
            <th scope="col" className="data">
              Time
            </th>
            <th scope="col" className="data">
              Share
            </th>
          </tr>
        </thead>
        <tbody>
          {h.categories.map((c) => (
            <tr key={c.name}>
              <th scope="row">{c.name}</th>
              <td className="data">{fmt.duration(c.seconds)}</td>
              <td className="data">{Math.round(c.share * 100)}%</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="caption">Every hour of the week appears in exactly one row above.</p>

      {report.divergence && (
        <div className="stack" style={{ gap: 2, marginTop: 10 }}>
          <strong className="micro">Biggest divergence</strong>
          <button
            className="btn"
            style={{ justifyContent: "flex-start" }}
            onClick={() => {
              goDay(report.divergence!.localDate);
              go("day");
            }}
          >
            <span className="grow" style={{ textAlign: "left" }}>
              {report.divergence.localDate} ·{" "}
              {report.divergence.kind === "overrun"
                ? "work ran past the plan"
                : "entertainment nobody plotted"}
            </span>
            <span className="micro" style={{ color: "var(--muted)" }}>
              {report.divergence.detail}
            </span>
          </button>
        </div>
      )}

      {report.unlabelled.length > 0 && (
        <div className="stack" style={{ gap: 2, marginTop: 10 }}>
          <strong className="micro">Not categorised yet</strong>
          {report.unlabelled.map((u) => (
            <div key={`${u.matchKind}:${u.matchValue}`} className="row micro">
              <span className="grow">{u.matchValue}</span>
              <span className="data">{fmt.duration(u.seconds)}</span>
            </div>
          ))}
          <button className="btn" onClick={() => go("activity")}>
            Name them in Activity
          </button>
        </div>
      )}
    </section>
  );
}
