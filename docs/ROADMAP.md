# Roadmap

The 12-week Windows-first MVP from Project Plan Revision 3, with what this
repository has actually delivered against each phase.

Status values: **done** · **partial** · **not started**. "Partial" always names
what is missing.

| Phase | Plan timing | Status | What exists | What is missing |
|---|---|---|---|---|
| **0. Discovery and source audit** | Week 1 | **partial** | The Fruit source exists in this repository and is being reused, which answers plan §17.7. The data model is specified in `PRODUCT-SPEC.md` §4. | Client confirmation of terminology, browsers, contribution list, and the reference month. Six of the seven §17 decisions are still open — working defaults are recorded in `PRODUCT-SPEC.md` §5.8. |
| **1. Feasibility and UX** | Week 2 | **not started** | — | The two spikes that de-risk everything downstream: Windows foreground/idle (largely proven — `frontmost.rs` works on Windows) and **browser-domain observation**, which is unproven and is the largest technical risk in the plan. Day view and month dashboard prototypes. |
| **2. Core foundation** | Weeks 3–4 | **done** | Tauri shell, Rust core with no UI dependency, SQLite schema with forward-only migrations, timer state machine, crash recovery, sleep segmentation, plan/record separation, `VACUUM INTO` backups. Time invariants and restart recovery pass automated tests. | — |
| **3. Activity capture** | Weeks 5–6 | **partial** | Foreground application observation, idle segmentation, exclusions applied before storage, retention with automatic purge, pause that survives restart, delete-all. Privacy contract enforced below the IPC boundary. | The local browser connector, and therefore domain-level classification. Without it the 90%-capture and 95%-YouTube/Twitch measures cannot be met. |
| **4. Day, Planner and dashboard** | Weeks 6–7 | **in progress** | The life-time model and precedence engine; the **Day view** at 5/15/30/60-minute resolution with empty hours, four distinguishable layers, a per-gap fill action and a day ledger. Planner at 1/3/7-day spans with drift, collisions, recurrence. | The month dashboard and the Planner's month span. Day-view editing beyond fill: split, merge, repeat, multi-select. Filters. |
| **5. Projects, tasks and life tracking** | Week 8 | **partial** | Backlog with six groups, estimates on a fixed ladder, timers, manual session correction, subtasks to three levels. Life areas and entries, and work contribution modes, both landed early in Phase 4. | Target-vs-actual reporting for life areas, life-area management UI, and the reduction of task notes from Markdown to compact plain text. |
| **6. Reconcile, calibrate, reduce entertainment** | Week 9 | **partial** | Daily reconcile over overruns, unstarted plans and unplanned work. Drift per block. Trailing 30-day calibration, median, n ≥ 5. | Reconciling **observed-only time and empty hours**. Entertainment budgets, planned entertainment windows, threshold notifications, planned-vs-unplanned reporting. |
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
3. ⬅ **The browser connector spike.** Four wireframe screens now wait on an
   unproven capability — see `WIREFRAME-GAP.md`. It was a Phase 1 item and it
   is still unspiked. *(next)*
4. ✅ `get_month`, the month dashboard, the Planner's month span, and the Excel
   export screen with its workbook writer.
5. Reconciling observed-only and empty hours (M10), then workbook import (M13)
   — the two items blocked on nothing.
6. Entertainment classification, budgets and the reconcile evidence panel.

Items 1–3 close the plan's Phase 4 exit gate: *a full day reconciles without
double-counting and the month dashboard matches source totals.*

## Wireframe coverage

`02_WIREFRAMES.html` specifies five screens. See
[`WIREFRAME-GAP.md`](WIREFRAME-GAP.md) for the line-by-line comparison.

| Screen | Coverage |
|---|---|
| Day | **built** — cards, five columns, now line, contribution in the classification, detail panel, filter, add-interval |
| Planner | **built** — month span as a calendar, in-canvas backlog, Import calendar and + Plan block |
| Month dashboard | **built** — six cards, entertainment trend, data-quality heatmap, life-area targets, findings |
| Reconcile | **partial** — no evidence panel, no observed-only or empty items |
| Excel export | **built** — preview, options, reconciliation table, three-sheet workbook with real formulas |

## Risks currently live

| Risk | Where it stands |
|---|---|
| Browser shows only `chrome.exe` instead of YouTube/Twitch | **Open and unmitigated.** The connector has not been spiked. Everything in the entertainment-reduction outcome depends on it. |
| Planned, confirmed and observed time double-counted | **Mitigated in the core**: four separate tables, explicit precedence, and a property test asserting a day sums to its own length exactly once. |
| Day view becomes visually dense or the month report slow | **Open.** Budgets are recorded (<100ms day, <250ms month) but nothing has been profiled on a packaged build. |
| Excel export looks familiar but calculates differently | **Open.** Needs the client's reference month before the export format can be fixed. |
| Sensitive app/title/domain data stored | **Mitigated** for apps and titles; **open** for domains, which the connector will introduce. |
| Scope expands into notes, finance, mobile or blocking | Recorded as out of scope in `PRODUCT-SPEC.md` §1. The existing Markdown note renderer is a live instance — it needs reducing to plain text. |
