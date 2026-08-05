# Roadmap

The 12-week Windows-first MVP from Project Plan Revision 3, with what this
repository has actually delivered against each phase.

Status values: **done** · **partial** · **not started**. "Partial" always names
what is missing.

| Phase | Plan timing | Status | What exists | What is missing |
|---|---|---|---|---|
| **0. Discovery and source audit** | Week 1 | **partial** | The Fruit source exists in this repository and is being reused, which answers plan §17.7. The data model is specified in `PRODUCT-SPEC.md` §4. | Client confirmation of terminology, browsers, contribution list, and the reference month. Six of the seven §17 decisions are still open — working defaults are recorded in `PRODUCT-SPEC.md` §5.8. |
| **1. Feasibility and UX** | Week 2 | **done** | Both spikes. Windows foreground/idle was already largely proven (`frontmost.rs`). **Browser-domain observation is now spiked** — protocol, framing, storage, classification and the native-messaging host all built and covered by `cargo test`; see [`SPIKE-BROWSER-CONNECTOR.md`](SPIKE-BROWSER-CONNECTOR.md). Day view and month dashboard exist as the real screens rather than prototypes. | Three assumptions in the spike report need a Windows machine with Chrome to close — not more code. |
| **2. Core foundation** | Weeks 3–4 | **done** | Tauri shell, Rust core with no UI dependency, SQLite schema with forward-only migrations, timer state machine, crash recovery, sleep segmentation, plan/record separation, `VACUUM INTO` backups. Time invariants and restart recovery pass automated tests. | — |
| **3. Activity capture** | Weeks 5–6 | **partial** | Foreground application observation, idle segmentation, exclusions applied before storage, retention with automatic purge, pause that survives restart, delete-all. Privacy contract enforced below the IPC boundary. **The browser connector**: MV3 extension, native-messaging host, spool hand-off, domain rules and write-time classification, with its own switch and its own exclusion list. | Seven days of real capture on the client's PC, which is what the 90%-capture and 95%-YouTube/Twitch measures are actually measured over. |
| **4. Day, Planner and dashboard** | Weeks 6–7 | **in progress** | The life-time model and precedence engine; the **Day view** at 5/15/30/60-minute resolution with empty hours, four distinguishable layers, a per-gap fill action and a day ledger. Planner at 1/3/7-day spans with drift, collisions, recurrence. | The month dashboard and the Planner's month span. Day-view editing beyond fill: split, merge, repeat, multi-select. Filters. |
| **5. Projects, tasks and life tracking** | Week 8 | **partial** | Backlog with six groups, estimates on a fixed ladder, timers, manual session correction, subtasks to three levels. Life areas and entries, and work contribution modes, both landed early in Phase 4. | Target-vs-actual reporting for life areas, life-area management UI, and the reduction of task notes from Markdown to compact plain text. |
| **6. Reconcile, calibrate, reduce entertainment** | Week 9 | **partial** | Daily reconcile in the wireframe's three-column shape: queue, numbered choices with a stated recommendation, and an evidence panel for machine claims. Covers overruns, unstarted plans, unplanned work, **observed-only time and empty hours** (M10). Drift per block. Trailing 30-day calibration. | Entertainment budgets, planned entertainment windows, threshold notifications, planned-vs-unplanned reporting. The connector they were blocked on now exists, so these are ordinary work rather than a dependency. |
| **7. Excel migration and export** | Week 10 | **partial** | The `.xlsx` month export: three sheets, a preview that is the sheet, options, and a reconciliation table putting the app's figures beside the sheet's own. Totals are formulas over the month sheet, not pasted numbers. JSON round-trips exactly; CSV exists. | Workbook **import** with mapping and variance preview (M13). |
| **8. Private beta and hardening** | Week 11 | **not started** | UI checks run headless at five viewport sizes on every view. | Seven consecutive days on the client's PC. Performance profiling on a packaged build. |
| **9. Release candidate** | Week 12 | **not started** | — | Installer, user guide, release notes, acceptance sign-off. |

---

## Sequencing correction

The plan puts the Day view in Phase 4 and life entries in Phase 5. That order
cannot be built: plan §8.1 requires the Day view to show "actual project/task
**or life activity**", and MVP acceptance criteria 1, 2 and 9 all require life
entries to exist. A Day view built first would render work and empty hours
only — not the screen being specified, and it would need rebuilding a week
later.

So `life_area`, `life_entry`, work contribution modes and the precedence engine
are treated as **the first half of Phase 4**. This moves work earlier rather
than adding any, and Phase 5 keeps targets, reporting and the notes change.

## Current build order

1. ✅ **The unified time model** — `life_area`, `life_entry`, contribution
   modes, and the precedence engine that resolves four record types into one
   non-double-counted timeline, with the counting invariant as a property test.
2. ✅ **The Day view** — the 24-hour table on top of that model, with every
   hour present, four layers distinguishable, and a fill action on every gap.
3. ✅ **The browser connector.** Spiked and built: registrable-domain reduction
   enforced on both sides of the process boundary, Chrome's framing with the
   partial-read cases tested, a spool hand-off that needs no listening socket,
   and rules whose verdict is stamped at write time so a rule made today cannot
   rewrite a month already closed. See
   [`SPIKE-BROWSER-CONNECTOR.md`](SPIKE-BROWSER-CONNECTOR.md).
4. ✅ `get_month`, the month dashboard, the Planner's month span, and the Excel
   export screen with its workbook writer.
5. ✅ Reconciling observed-only and empty hours (M10), in the wireframe's
   three-column sheet.
6. ⬅ **Workbook import (M13)** — the export's inverse, and now the only
   substantial item left that is blocked on nothing. *(next)*
7. Entertainment budgets, planned entertainment windows and threshold
   notifications — the reduction half of the outcome, now that the measurement
   half exists.
8. The `Split` verb, on the Day view and in the reconciler.

Items 1–3 close the plan's Phase 4 exit gate: *a full day reconciles without
double-counting and the month dashboard matches source totals.*

## Wireframe coverage

`02_WIREFRAMES.html` specifies five screens. See
[`WIREFRAME-GAP.md`](WIREFRAME-GAP.md) for the line-by-line comparison.

| Screen | Coverage |
|---|---|
| Day | **built** — cards, five columns, now line, contribution in the classification, detail panel, filter, add-interval. "Work + distraction" and "Observed Entertainment" now fire rather than sitting dark. |
| Planner | **built** — month span as a calendar, in-canvas backlog, Import calendar and + Plan block |
| Month dashboard | **built** — six cards, entertainment trend, data-quality heatmap, life-area targets, findings |
| Reconcile | **built** — three columns, evidence panel, numbered choices, observed-only and empty items (M10), and the prospective-rule checkbox |
| Excel export | **built** — preview, options, reconciliation table, three-sheet workbook with real formulas |

## Risks currently live

| Risk | Where it stands |
|---|---|
| Browser shows only `chrome.exe` instead of YouTube/Twitch | **Largely closed.** The connector is built and its logic is covered by tests that run without a browser. What remains is three field assumptions — stdio under the Windows GUI subsystem, MV3 eviction behaviour over a real day, and Edge parity — listed in the spike report. |
| Planned, confirmed and observed time double-counted | **Mitigated in the core**: four separate tables, explicit precedence, and a property test asserting a day sums to its own length exactly once. |
| Day view becomes visually dense or the month report slow | **Open.** Budgets are recorded (<100ms day, <250ms month) but nothing has been profiled on a packaged build. |
| Excel export looks familiar but calculates differently | **Open.** Needs the client's reference month before the export format can be fixed. |
| Sensitive app/title/domain data stored | **Mitigated** for all three. Domains get their own switch (off even when apps are on), their own exclusion list matched after reduction so an entry covers every subdomain, and reduction enforced twice — in the extension and again below the IPC boundary, because the extension is the swappable part. |
| Scope expands into notes, finance, mobile or blocking | Recorded as out of scope in `PRODUCT-SPEC.md` §1. The existing Markdown note renderer is a live instance — it needs reducing to plain text. |
