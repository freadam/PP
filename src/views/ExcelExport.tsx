/**
 * Excel export (wireframe screen 5) — a whole screen, not a menu item.
 *
 * It is a screen because the export is the client's deliverable: the workbook
 * replaces one they maintain by hand, and handing it over on a button press
 * with no preview asks them to trust a file they have not seen.
 *
 * Three things fill it, in the order of the question they answer:
 *
 * 1. **Does it look like my workbook?** The preview *is* the sheet — the same
 *    matrix renders here and into the file, so this cannot promise a layout the
 *    workbook doesn't have.
 * 2. **What will it contain?** Options, each stating its own consequence rather
 *    than leaving it to be discovered.
 * 3. **Do the numbers agree?** The reconciliation table: the app's figure, the
 *    sheet's own figure, and the difference. That is success measure 7, made
 *    checkable rather than asserted.
 */

import { useEffect, useState } from "react";
import { useApp } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type { ExcelCell, ExcelOptions, ExcelPreview } from "../lib/types";
import { Empty } from "../components/chrome";

/** How many columns and rows the on-screen sample shows. The file has them all. */
const PREVIEW_DAYS = 7;
const PREVIEW_FROM_SLOT = 12; // 06:00 — where the workbook's own sample starts
const PREVIEW_SLOTS = 10;

export function ExcelExport() {
  const monthKey = useApp((s) => s.monthKey);
  const go = useApp((s) => s.go);
  const run = useApp((s) => s.run);
  const toast = useApp((s) => s.toast);

  const [options, setOptions] = useState<ExcelOptions>({
    includeObserved: true,
    includeUnaccounted: true,
    includePrivateLabels: false,
  });
  const [preview, setPreview] = useState<ExcelPreview | null>(null);
  const [path, setPath] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let live = true;
    void ipc
      .previewExcel(monthKey, fmt.tz(), options)
      .then((p) => {
        if (!live) return;
        setPreview(p);
        // Show where it will land before anyone commits to writing it — an
        // export you can't find is an export you don't trust (§6.12).
        void ipc
          .suggestExcelPath(p.fileName)
          .then((s) => live && setPath(s))
          .catch(() => live && setPath(p.fileName));
      })
      .catch(() => live && setPreview(null));
    return () => {
      live = false;
    };
  }, [monthKey, options]);

  const write = async () => {
    setBusy(true);
    const result = await run(
      () => ipc.exportExcel(monthKey, fmt.tz(), path, options),
      "Couldn't write that workbook.",
    );
    setBusy(false);
    if (result) {
      toast(`Wrote ${result.sheets.length} sheets to ${result.path}`, {
        action: { label: "Back to Reports", run: () => go("reports") },
      });
    }
  };

  if (!preview) {
    return <Empty>Preparing the workbook preview…</Empty>;
  }

  return (
    <div className="view-pad scroll-y">
      <div className="context-bar">
        <h1 className="display">Export {preview.label} to Excel</h1>
        <span className="grow" />
        <button className="btn" onClick={() => go("reports")}>
          Cancel
        </button>
        <button className="btn btn-primary" onClick={() => void write()} disabled={busy}>
          {busy ? "Writing…" : "Export .xlsx"}
        </button>
      </div>

      <div className="export-layout">
        <section className="panel sheet-preview">
          <h3>Workbook preview · {preview.fileName}</h3>
          <div className="row" style={{ gap: 4, marginBottom: 8 }}>
            <span className="tag">Month table</span>
            <span className="tag">Summary</span>
            <span className="tag">Source mapping</span>
          </div>

          <div className="mini-sheet-wrap">
            <table className="mini-sheet">
              <thead>
                <tr>
                  <th>Time</th>
                  {preview.dayHeaders.slice(0, PREVIEW_DAYS).map((h) => (
                    <th key={h}>{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {preview.slotLabels
                  .slice(PREVIEW_FROM_SLOT, PREVIEW_FROM_SLOT + PREVIEW_SLOTS)
                  .map((label, i) => (
                    <tr key={label}>
                      <th scope="row" className="data">
                        {label}
                      </th>
                      {preview.rows[PREVIEW_FROM_SLOT + i]!
                        .slice(0, PREVIEW_DAYS)
                        .map((cell, c) => (
                          <SheetCell key={c} cell={cell} />
                        ))}
                    </tr>
                  ))}
              </tbody>
            </table>
          </div>

          <p className="caption">
            Showing {PREVIEW_SLOTS} of {preview.slotLabels.length} rows and {PREVIEW_DAYS} of{" "}
            {preview.dayHeaders.length} days. The file has all of them.
          </p>
          <p className="caption">
            <strong>Colours communicate categories; structured values and formulas produce
            totals.</strong>{" "}
            The Summary sheet counts the cells on this one, so changing a cell in Excel moves the
            totals — which is the thing the old workbook could not do.
          </p>
        </section>

        <aside className="stack">
          <section className="panel">
            <h3>Export options</h3>
            <div className="field">
              <label htmlFor="excel-path">Output</label>
              <input
                id="excel-path"
                className="fake-input data"
                value={path}
                onChange={(e) => setPath(e.target.value)}
              />
            </div>
            <Option
              checked={options.includeObserved}
              onChange={(includeObserved) => setOptions((o) => ({ ...o, includeObserved }))}
              label="Include observed-only markers"
              hint="Slots the machine saw but nobody confirmed, marked “Observed: …”."
            />
            <Option
              checked={options.includeUnaccounted}
              onChange={(includeUnaccounted) => setOptions((o) => ({ ...o, includeUnaccounted }))}
              label="Include unaccounted slots"
              hint="Blank time exported as a visible “Gap” rather than an empty cell."
            />
            <Option
              checked={options.includePrivateLabels}
              onChange={(includePrivateLabels) =>
                setOptions((o) => ({ ...o, includePrivateLabels }))
              }
              label="Include private labels"
              hint="Private duration is included either way; this only names the area. Off by default, because a workbook is a file you might email."
            />
          </section>

          <section className="panel">
            <h3>Reconciliation</h3>
            <table className="variance">
              <thead>
                <tr>
                  <th>Measure</th>
                  <th>App</th>
                  <th>Excel</th>
                  <th>Variance</th>
                </tr>
              </thead>
              <tbody>
                {preview.variances.map((v) => (
                  <tr key={v.measure}>
                    <td>{v.measure}</td>
                    <td className="data">{fmt.duration(v.appSec)}</td>
                    <td className="data">{fmt.duration(v.sheetSec)}</td>
                    <td className="data" data-variance={v.varianceSec !== 0 || undefined}>
                      {v.varianceSec === 0 ? "0" : fmt.drift(v.varianceSec)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="caption">
              A non-zero variance is rounding, not error: the sheet is a half-hour grid and the
              record is to the second, so a 20-minute session fills one slot and reads as 30
              minutes. The measure is no <em>unexplained</em> variance.
            </p>
          </section>

          {preview.unreconciledDays > 0 && (
            <section className="panel callout">
              <strong>
                {preview.unreconciledDays} unreconciled day
                {preview.unreconciledDays === 1 ? "" : "s"}
              </strong>
              {/* Refusing to export would be worse: the point of the workbook is
                  to show the month as it stands, gaps included. */}
              <p className="caption">
                Export is allowed. The workbook marks those days and preserves their observed and
                unaccounted states rather than quietly filling them in.
              </p>
            </section>
          )}

          <p className="caption">{preview.sourceNote}</p>
        </aside>
      </div>
    </div>
  );
}

function Option({
  checked,
  onChange,
  label,
  hint,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  hint: string;
}) {
  return (
    <div className="field">
      <label className="row" style={{ gap: 8, alignItems: "flex-start" }}>
        <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
        <span>
          {label}
          <span className="caption" style={{ display: "block" }}>
            {hint}
          </span>
        </span>
      </label>
    </div>
  );
}

function SheetCell({ cell }: { cell: ExcelCell }) {
  return (
    <td
      data-kind={cell.kind}
      title={cell.label}
      style={
        cell.colour
          ? { boxShadow: `inset 3px 0 0 ${cell.colour}` }
          : undefined
      }
    >
      {cell.label}
    </td>
  );
}
