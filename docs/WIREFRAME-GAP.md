# Wireframe gap analysis

Against `02_WIREFRAMES.html` (five screens: Day, Planner, Month Dashboard,
Reconcile, Excel Export).

Legend: **✅ built** · **◐ partial** · **❌ missing** · **⛔ blocked** (needs a
component that does not exist yet, named in the row).

The headline: **Day and Month Dashboard are built to the wireframe.** Planner
and Reconcile are partial; Excel Export is untouched. Every ❌ below is blocked
on one of exactly two remaining components — the **browser connector** and the
**XLSX writer**. Nothing is blocked on design.

---

## Shell (all five screens)

| Wireframe | State | Note |
|---|---|---|
| Nav: Day · Planner · Projects · Activity · Reports · Settings | ✅ | Day is first and default. The rail now shows **icon over label** at 76px — six destinations is too many to learn from icons alone. "Tasks" is relabelled **Projects** to match. |
| Topbar: brand, OFFLINE, Recording / Activity-paused, Reconcile (n), timer, Focus | ✅ | The Recording pill already reports `paused` as its own state. |
| `Ctrl K` button in the topbar | ❌ | The palette exists on `⌘K`/`⌘F` but has no visible affordance. One button. |
| Reconcile shows a **count** | ✅ | `unreconciled.length` was already there. |

## 1. Day — **built**

| Wireframe | State | Note |
|---|---|---|
| Context bar: ‹ date › Today, zoom segmented, Filter, + Add interval | ✅ | Zoom is 5/15/30/60 (wireframe shows 15/30/60; 5 is kept for splitting a short interval). |
| Six summary cards, Unaccounted hatched | ✅ | **Sleep is its own card**, split out of Life — a third of every month lands in one bucket otherwise. Needed `sleep_sec` in `DayTotals`. |
| Five columns: Time · Planned · Actual · PC evidence · Classification | ✅ | |
| Classification shows contribution — "Work · Own" | ✅ | |
| "Work + distraction" | ◐ ⛔ | The state, the DTO field (`hasDistraction`) and the rendering are built. It can never fire until the **browser connector** classifies a domain as entertainment. |
| "Observed Entertainment" | ◐ ⛔ | Same: rendering built, classification blocked on the connector. |
| Empty rows visible and labelled | ✅ | Hatched and labelled "Unaccounted", per plan §17.4. |
| NOW line | ✅ | A rule across the row plus a NOW tag in the gutter. |
| Right detail panel: time, tags, project/task, **contribution dropdown**, PC evidence, note, Split / Edit | ◐ | Everything except **Split**. Contribution is settable, and reclassifying work → life is there too (it clears the contribution by construction). |
| Filter | ◐ | Five presets (Everything · Unaccounted · Needs a decision · Work · Life). It filters **rows only** and says so — the totals stay the whole day, because a filter that changed the arithmetic would make the counting invariant unverifiable by eye. Per-project and per-area filters not yet. |
| + Add interval | ✅ | Opens the fill dialog with an adjustable range. |
| Split / merge / repeat / multi-select editing (plan §8.1) | ❌ | Fill is the only edit. These are the next Day-view increment. |

## 2. Planner — **partial**

| Wireframe | State | Note |
|---|---|---|
| 24-hour grid, drag/resize, drift rail, fixed blocks, repeat | ✅ | Built before this plan. |
| Spans **3 days · 7 days · Month** | ◐ | 1/3/7 exist; **Month is missing.** 31 columns at a legible width does not fit the current grid — it needs a different layout (a month is a calendar, not 31 day columns), which is a real design decision, not a parameter change. |
| Backlog panel beside the grid | ◐ | The backlog is in the global sidebar rather than inside the canvas. Functionally the same; visually not the wireframe. |
| "Import calendar" button in the context bar | ❌ | The command exists (Settings → Data). It just isn't surfaced here. |
| "+ Plan block" button | ❌ | Blocks are created by clicking the grid or pressing `S`. No button. |

## 3. Month Dashboard — **built**

Reports is now month-first. `get_month` is `get_day` summed over the month, so a
figure here and the same figure on a day cannot disagree.

| Wireframe | State | Note |
|---|---|---|
| Month anchor + ‹ This month › + Day/Week/Month segmented | ✅ | Day jumps to the Day screen — it *is* the day horizon. Week keeps the calibration and project panels, which are about estimate accuracy rather than about how the month went. |
| Cards: Accounted % · Work · Life · Sleep · Entertainment · Unaccounted | ✅ | Measured against the days that have **happened**. A fresh August on the 4th is otherwise "6% accounted", which is arithmetically true and a useless headline — the missing 27 days are the future, not a gap. |
| Entertainment planned-vs-unplanned line chart | ◐ | Solid (unplanned) is real. Dashed (planned) is flat zero, and that is the correct reading rather than a placeholder: with no way to plan an entertainment window, every minute is unplanned by definition. The chart says so underneath. |
| Data-quality heatmap + "n unreconciled days · Nh observed-only" | ✅ | Shade is coverage, the numeral is always present, a corner mark is "never reviewed". Future days are outlined and never marked as a problem. |
| Life-area targets vs actual bars | ✅ | An area with a target and no time still gets a row — a zero against a target is the most actionable row there is. |
| Monthly findings + "Review source intervals" | ✅ | Findings are computed in Rust; the button opens the worst day on the Day screen. The YouTube/Twitch split needs the connector, so the entertainment finding is currently one line rather than two. |
| "Export month to Excel" | ◐ | Present and enabled — a disabled control cannot take keyboard focus, so it would be invisible to the users most likely to look for it. It explains why it cannot run. |

## 4. Reconcile — **partial**

The existing sheet handles overruns, never-started blocks and unplanned
sessions. The wireframe's version is a different, better shape.

| Wireframe | State | Note |
|---|---|---|
| Modal over the ghosted day | ◐ | Modal exists; no ghosted day behind it. |
| Three columns: queue · decision · **evidence** | ❌ | The evidence panel — Source, Domain, Confidence, Adjacent time, and the **Storage** line ("Domain only · no full URL/title") — does not exist. That last line is the privacy promise stated at the moment of use, which is the only moment it counts. |
| Queue with ✓ progress and "Defer remaining" | ◐ | Progress is "n of m"; no per-item ticks, no defer-all. |
| **Numbered** choices with a recommended default | ◐ | Verbs exist with one-key alternatives, but not numbered `1..4 / S`, and no "recommended" marking. |
| Item kinds: recovered timer, planned-not-started, **observed-only**, **empty**, overrun | ◐ ⛔ | The first, second and last exist. **Observed-only and empty hours do not** — they are plan acceptance M10, and both now have the data behind them (`observedOnlySec`, `emptySec`) but no reconcile item. |
| "Apply my choice to future Twitch activity in this context" | ❌ ⛔ | Prospective rule creation. Needs the connector and a rules table. |
| "Split interval" | ❌ | Same missing verb as the Day view's Split. |

## 5. Excel Export — **missing** ⛔

Nothing of this screen exists. It is a whole view.

| Wireframe | State |
|---|---|
| Full export screen with Cancel / Export .xlsx | ❌ |
| Workbook preview (mini month sheet, gaps hatched) | ❌ |
| Tags: Month table · Summary · Source mapping | ❌ |
| Options: output path, include observed-only markers, include unaccounted slots, include private labels | ❌ |
| **Reconciliation table** (Measure · App · Excel · Variance) | ❌ |
| "7 unreconciled days — export is allowed, the workbook will mark them" | ❌ |

All blocked on the **XLSX writer**. The reconciliation table is worth calling
out separately: it is the mechanism that makes "Excel exports reconcile to
application totals with no unexplained variance" (success measure 7) checkable
rather than asserted, and it is a *design* feature of the export, not a
by-product of writing one.

---

## What this changes about the build order

Three components gate almost everything left:

1. **Browser connector** — gates Day-view distraction and entertainment
   classification, the dashboard's entertainment panel and findings, the
   reconcile evidence panel and prospective rules. Still unspiked, still the
   largest risk in the plan, and it now has **four** screens waiting on it
   rather than one.
2. ~~**`get_month`**~~ — **built.** The dashboard is done.
3. **XLSX writer** — gates the export screen entirely.

One item is blocked on **nothing** and is the cheapest work left:

- **Reconciling observed-only and empty hours** — M10, and both figures already
  come out of `get_day`.

Recommended order from here: the connector spike (three screens still depend on
an unproven capability), then M10 reconcile items, then the export.
