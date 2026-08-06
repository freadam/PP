/**
 * Workbook import (M13, §4.8) — mapping, then variance, then commit.
 *
 * > *"Import starts with a mapping and variance preview, never alters the
 * > source file, and requires the user to resolve duplicate or inconsistent
 * > periods before commit."*
 *
 * The screen is those three steps in that order, and none of them can be
 * skipped — not because the buttons are disabled (they are), but because the
 * core refuses. The renderer is not a trusted caller; what you see here is a
 * restatement of a rule that is enforced in Rust.
 *
 * The one thing worth stating plainly: **every label the sheet contains has to
 * be given a meaning, and "ignore" is one of the meanings.** An importer that
 * quietly drops what it does not recognise is how a historical month arrives
 * eighty per cent complete and nobody notices for a year.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { useApp } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type {
  ImportBatch,
  ImportMapping,
  ImportPreview,
  ImportTarget,
  LifeAreaRow,
  WorkbookInspection,
} from "../lib/types";
import { Empty } from "../components/chrome";

export function Import() {
  const go = useApp((s) => s.go);
  const run = useApp((s) => s.run);
  const toast = useApp((s) => s.toast);
  const refresh = useApp((s) => s.refresh);

  const [path, setPath] = useState("");
  const [inspection, setInspection] = useState<WorkbookInspection | null>(null);
  const [mapping, setMapping] = useState<ImportMapping | null>(null);
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [areas, setAreas] = useState<LifeAreaRow[]>([]);
  const [batches, setBatches] = useState<ImportBatch[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadBatches = useCallback(async () => {
    setBatches(await ipc.getImportBatches().catch(() => []));
  }, []);

  useEffect(() => {
    void ipc
      .getLifeAreas(fmt.tz(), false)
      .then(setAreas)
      .catch(() => setAreas([]));
    void loadBatches();
  }, [loadBatches]);

  const inspect = async () => {
    setError(null);
    setPreview(null);
    setMapping(null);
    setBusy(true);
    try {
      const insp = await ipc.inspectWorkbook(path.trim());
      setInspection(insp);
      const first = insp.sheets.find((s) => s.dayColumns.length > 0) ?? insp.sheets[0];
      if (first) await chooseSheet(path.trim(), first.name);
    } catch (e) {
      setInspection(null);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const chooseSheet = async (p: string, sheet: string) => {
    const m = await ipc.suggestImportMapping(p, sheet, fmt.tz()).catch(() => null);
    setMapping(m);
    setPreview(null);
  };

  // The preview is recomputed on every mapping change: the variance is the
  // thing being decided, so it has to move when the decision does.
  useEffect(() => {
    if (!mapping || !path.trim()) return;
    let live = true;
    void ipc
      .previewImport(path.trim(), mapping)
      .then((p) => live && setPreview(p))
      .catch(() => live && setPreview(null));
    return () => {
      live = false;
    };
  }, [mapping, path]);

  const setRule = (label: string, target: ImportTarget) => {
    setMapping((m) =>
      m
        ? {
            ...m,
            rules: [...m.rules.filter((r) => r.label !== label), { label, target }],
          }
        : m,
    );
  };

  const commit = async () => {
    if (!mapping) return;
    setBusy(true);
    const result = await run(
      () => ipc.commitImport(path.trim(), mapping),
      "Couldn't import that workbook.",
    );
    setBusy(false);
    if (result) {
      toast(
        `Imported ${fmt.duration(result.totalSec)} into ${result.month} — ` +
          `${result.sessionsWritten} session${result.sessionsWritten === 1 ? "" : "s"}, ` +
          `${result.lifeEntriesWritten} life ${result.lifeEntriesWritten === 1 ? "entry" : "entries"}.`,
        {
          action: {
            label: "Undo",
            run: async () => {
              await ipc.undoImport(result.batchId).catch(() => {});
              await refresh();
              void loadBatches();
            },
          },
        },
      );
      await refresh();
      void loadBatches();
      setPreview(null);
    }
  };

  const sheet = useMemo(
    () => inspection?.sheets.find((s) => s.name === mapping?.sheet) ?? null,
    [inspection, mapping],
  );

  return (
    <div className="view-pad scroll-y">
      <div className="context-bar">
        <h1 className="display">Import a workbook</h1>
        <span className="grow" />
        <button className="btn" onClick={() => go("reports")}>
          Back to Reports
        </button>
      </div>

      <section className="panel">
        <h3>The file</h3>
        <p className="caption">
          Fruit reads the file and never writes to it. Nothing is imported until you have said
          what every label in it means and looked at the variance.
        </p>
        <div className="row">
          <label htmlFor="import-path" className="micro">
            Workbook
          </label>
          <input
            id="import-path"
            className="grow"
            value={path}
            placeholder="C:\\Users\\you\\Documents\\2025.xlsx"
            onChange={(e) => setPath(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void inspect();
            }}
          />
          <button className="btn btn-primary" onClick={() => void inspect()} disabled={busy || !path.trim()}>
            {busy ? "Reading…" : "Read it"}
          </button>
        </div>
        {error && <p className="caption" role="alert">{error}</p>}
      </section>

      {inspection && sheet && mapping && (
        <>
          <section className="panel">
            <h3>What Fruit found</h3>
            <div className="row">
              <label htmlFor="import-sheet" className="micro">
                Sheet
              </label>
              <select
                id="import-sheet"
                value={mapping.sheet}
                onChange={(e) => void chooseSheet(path.trim(), e.target.value)}
              >
                {inspection.sheets.map((s) => (
                  <option key={s.name} value={s.name}>
                    {s.name} — {s.dayColumns.length} day column
                    {s.dayColumns.length === 1 ? "" : "s"}
                  </option>
                ))}
              </select>
              <label htmlFor="import-month" className="micro">
                Month
              </label>
              <input
                id="import-month"
                type="month"
                value={mapping.month}
                onChange={(e) => setMapping({ ...mapping, month: e.target.value })}
              />
            </div>
            <p className="caption">
              {/* Stated rather than assumed: the headers say "1 … 31" and
                  nothing in the file says which month that is. */}
              Time in column {sheet.timeCol === null ? "—" : sheet.timeCol + 1}, days across row{" "}
              {sheet.headerRow === null ? "—" : sheet.headerRow + 1}, {mapping.slotMinutes} minutes
              a row. The sheet doesn't say which month it covers, so that one is yours to pick.
            </p>
          </section>

          <section className="panel">
            <h3>What each label means</h3>
            <p className="caption">
              Every label has to be given a meaning before anything is imported, and{" "}
              <b>Ignore</b> is a meaning. An importer that quietly drops what it doesn't
              recognise is how a month arrives eighty per cent complete and nobody notices.
            </p>
            <table className="table compact">
              <thead>
                <tr>
                  <th scope="col">Label</th>
                  <th scope="col" className="data">
                    Time
                  </th>
                  <th scope="col">Import as</th>
                </tr>
              </thead>
              <tbody>
                {sheet.labels.map((l) => {
                  const rule = mapping.rules.find((r) => r.label === l.label);
                  return (
                    <tr key={l.label} data-warn={!rule || undefined}>
                      <th scope="row">{l.label}</th>
                      <td className="data">
                        {fmt.duration(l.slots * mapping.slotMinutes * 60)}
                      </td>
                      <td>
                        <select
                          aria-label={`Import "${l.label}" as`}
                          value={targetKey(rule?.target)}
                          onChange={(e) => setRule(l.label, targetFrom(e.target.value))}
                        >
                          <option value="">— not decided —</option>
                          <option value="ignore">Ignore</option>
                          <option value="work">Work</option>
                          <option value="private">Private</option>
                          {areas.map((a) => (
                            <option key={a.id} value={`life:${a.id}`}>
                              Life · {a.name}
                            </option>
                          ))}
                        </select>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </section>

          {preview && <Variance preview={preview} mapping={mapping} onChange={setMapping} />}

          <div className="row" style={{ marginTop: 12 }}>
            <span className="grow" />
            <button
              className="btn btn-primary"
              disabled={busy || !preview || preview.blocking.length > 0}
              onClick={() => void commit()}
            >
              {busy ? "Importing…" : "Import"}
            </button>
          </div>
        </>
      )}

      {batches.length > 0 && (
        <section className="panel">
          <h3>Imports so far</h3>
          <p className="caption">
            Undo removes exactly what an import added. Records it replaced are not brought back —
            replacing them was a decision, and quietly resurrecting them would be a second one
            nobody made.
          </p>
          {batches.map((b) => (
            <div key={b.id} className="row">
              <span className="grow">
                {b.month} · {b.sheet}{" "}
                <span className="micro" style={{ color: "var(--muted)" }}>
                  {b.sourcePath}
                </span>
              </span>
              <span className="data micro">
                {b.sessionsWritten} session{b.sessionsWritten === 1 ? "" : "s"} ·{" "}
                {b.lifeEntriesWritten} life
              </span>
              <button
                className="btn"
                onClick={async () => {
                  const token = await run(() => ipc.undoImport(b.id), "Couldn't undo that import.");
                  if (token) {
                    toast(token.label);
                    await refresh();
                    void loadBatches();
                  }
                }}
              >
                Undo
              </button>
            </div>
          ))}
        </section>
      )}

      {!inspection && !error && <Empty>Point Fruit at an .xlsx file to begin.</Empty>}
    </div>
  );
}

/**
 * The variance preview — §4.8's precondition, and the part worth reading.
 *
 * Two columns beside each other: what the sheet says, and what Fruit already
 * holds. The difference is signed, because "12h of variance" without a
 * direction is a number nobody can act on.
 */
function Variance({
  preview,
  mapping,
  onChange,
}: {
  preview: ImportPreview;
  mapping: ImportMapping;
  onChange: (m: ImportMapping) => void;
}) {
  return (
    <section className="panel">
      <h3>Variance</h3>

      {preview.blocking.length > 0 ? (
        <ul className="stack" style={{ gap: 4 }}>
          {preview.blocking.map((b) => (
            <li key={b} className="caption" role="alert">
              {b}
            </li>
          ))}
        </ul>
      ) : (
        <p className="caption">
          Nothing outstanding. {fmt.duration(preview.totalSec)} will be imported
          {preview.ignoredSec > 0 && <>, and {fmt.duration(preview.ignoredSec)} deliberately not</>}
          .
        </p>
      )}

      {preview.conflictingDates.length > 0 && (
        <label className="row" style={{ marginTop: 8 }}>
          <input
            type="checkbox"
            checked={mapping.replaceExisting}
            onChange={(e) => onChange({ ...mapping, replaceExisting: e.target.checked })}
          />
          <span className="grow">
            Replace what Fruit already holds on those {preview.conflictingDates.length} day
            {preview.conflictingDates.length === 1 ? "" : "s"}. Left unticked, the import will not
            run — two records of the same hour are a disagreement, not evidence twice.
          </span>
        </label>
      )}

      <table className="table compact" style={{ marginTop: 8 }}>
        <thead>
          <tr>
            <th scope="col">Day</th>
            <th scope="col" className="data">
              In the sheet
            </th>
            <th scope="col" className="data">
              In Fruit
            </th>
            <th scope="col" className="data">
              Difference
            </th>
          </tr>
        </thead>
        <tbody>
          {preview.days.map((d) => (
            <tr key={d.localDate} data-warn={d.conflicts || undefined}>
              <th scope="row">{d.localDate}</th>
              <td className="data">{fmt.duration(d.seconds)}</td>
              <td className="data">{fmt.duration(d.existingSec)}</td>
              <td className="data">
                {/* The sign is carried in text, never by colour alone (M16). */}
                {d.varianceSec === 0
                  ? "same"
                  : `${d.varianceSec > 0 ? "+" : "−"}${fmt.duration(Math.abs(d.varianceSec))}`}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

function targetKey(t: ImportTarget | undefined): string {
  if (!t) return "";
  switch (t.kind) {
    case "life":
      return `life:${t.lifeAreaId}`;
    default:
      return t.kind;
  }
}

function targetFrom(key: string): ImportTarget {
  if (key.startsWith("life:")) return { kind: "life", lifeAreaId: key.slice(5) };
  if (key === "work") return { kind: "work", taskId: null };
  if (key === "private") return { kind: "private" };
  return { kind: "ignore" };
}
