# Wireframe gap analysis

Against `02_WIREFRAMES.html` (five screens: Day, Planner, Month Dashboard,
Reconcile, Excel Export).

Legend: **✅ built** · **◐ partial** · **❌ missing** · **⛔ blocked** (needs a
component that does not exist yet, named in the row).

The headline: **all five screens are built, and nothing on this list is blocked
on a missing component.** The **browser connector** — which gated four of the
five screens through every previous revision of this document — is spiked and
built; see [`SPIKE-BROWSER-CONNECTOR.md`](SPIKE-BROWSER-CONNECTOR.md). What
remains is ordinary work.

---

## Shell (all five screens)

| Wireframe | State | Note |
|---|---|---|
| Nav: Day · Planner · Projects · Activity · Reports · Settings | ✅ | Day is first and default. The rail now shows **icon over label** at 76px — six destinations is too many to learn from icons alone. "Tasks" is relabelled **Projects** to match. |
| Topbar: brand, OFFLINE, Recording / Activity-paused, Reconcile (n), timer, Focus | ✅ | The Recording pill already reports `paused` as its own state. |
| `Ctrl K` button in the topbar | ✅ | **Commands**, beside Reconcile. The hint is spelled for the platform in one place (`fmt.keys`) — the MVP is Windows-only, where a `⌘` on a button is not a stylistic choice but a key the user does not have. |
| Reconcile shows a **count** | ✅ | `unreconciled.length` was already there. |

## 1. Day — **built**

| Wireframe | State | Note |
|---|---|---|
| Context bar: ‹ date › Today, zoom segmented, Filter, + Add interval | ✅ | Zoom is 5/15/30/60 (wireframe shows 15/30/60; 5 is kept for splitting a short interval). |
| Six summary cards, Unaccounted hatched | ✅ | **Sleep is its own card**, split out of Life — a third of every month lands in one bucket otherwise. Needed `sleep_sec` in `DayTotals`. |
| Five columns: Time · Planned · Actual · PC evidence · Classification | ✅ | |
| Classification shows contribution — "Work · Own" | ✅ | |
| "Work + distraction" | ✅ | Fires. Entertainment observed *inside* confirmed work is evidence, not a second duration — the work keeps the whole interval (M8). The fixture demonstrates it from real Rust rather than asserting it. |
| "Observed Entertainment" | ✅ | The observed row names the **site**, not `chrome.exe`, which was the entire point of the connector. |
| Empty rows visible and labelled | ✅ | Hatched and labelled "Unaccounted", per plan §17.4. |
| NOW line | ✅ | A rule across the row plus a NOW tag in the gutter. |
| Right detail panel: time, tags, project/task, **contribution dropdown**, PC evidence, note, Split / Edit | ◐ | Everything except **Split**. Contribution is settable, reclassifying work → life is there (it clears the contribution by construction), and an observed segment shows its domain rather than its browser. |
| Filter | ◐ | Five presets (Everything · Unaccounted · Needs a decision · Work · Life). It filters **rows only** and says so — the totals stay the whole day, because a filter that changed the arithmetic would make the counting invariant unverifiable by eye. Per-project and per-area filters not yet. |
| + Add interval | ✅ | Opens the fill dialog with an adjustable range. |
| Split / merge / repeat / multi-select editing (plan §8.1) | ❌ | Fill is the only edit. These are the next Day-view increment. |

## 2. Planner — **built**

| Wireframe | State | Note |
|---|---|---|
| 24-hour grid, drag/resize, drift rail, fixed blocks, repeat | ✅ | Built before this plan. |
| Spans 1 day · 3 days · 7 days · **Month** | ✅ | Month is a **calendar**, not 31 hour-columns — those need ~3,400px at a legible width and no screen has it. It drops the time axis and keeps what a month horizon is for: which days are loaded, which are empty, where plan and record diverged. Clicking a day opens it at full resolution. `M` switches to it. |
| Backlog panel beside the grid | ✅ | In the canvas now, next to the grid it gets dragged onto — "what needs a slot" and "where are the slots" are one question. Capture, then Today / This week / No date. |
| "Import calendar" button in the context bar | ✅ | Same command as Settings → Data, surfaced where it is used. |
| "+ Plan block" button | ✅ | Opens the ad-hoc form at 09:00 on the first visible day. |

## 3. Month Dashboard — **built**

Reports is now month-first. `get_month` is `get_day` summed over the month, so a
figure here and the same figure on a day cannot disagree.

| Wireframe | State | Note |
|---|---|---|
| Month anchor + ‹ This month › + Day/Week/Month segmented | ✅ | Day jumps to the Day screen — it *is* the day horizon. Week keeps the calibration and project panels, which are about estimate accuracy rather than about how the month went. |
| Cards: Accounted % · Work · Life · Sleep · Entertainment · Unaccounted | ✅ | Measured against the days that have **happened**. A fresh August on the 4th is otherwise "6% accounted", which is arithmetically true and a useless headline — the missing 27 days are the future, not a gap. |
| Entertainment planned-vs-unplanned line chart | ◐ | Solid (unplanned) is real, and now has a real source — observed domains classified at write time. Dashed (planned) is still flat zero, and that remains the correct reading rather than a placeholder: with no way to plan an entertainment window, every minute is unplanned by definition. Planned windows are the next increment here, and they are no longer blocked. |
| Data-quality heatmap + "n unreconciled days · Nh observed-only" | ✅ | Shade is coverage, the numeral is always present, a corner mark is "never reviewed". Future days are outlined and never marked as a problem. |
| Life-area targets vs actual bars | ✅ | An area with a target and no time still gets a row — a zero against a target is the most actionable row there is. |
| Monthly findings + "Review source intervals" | ✅ | Findings are computed in Rust; the button opens the worst day on the Day screen. The YouTube/Twitch split now has a source — `domain_totals` — and splitting the entertainment finding into per-domain lines is a reporting change rather than a capability. |
| "Export month to Excel" | ◐ | Present and enabled — a disabled control cannot take keyboard focus, so it would be invisible to the users most likely to look for it. It explains why it cannot run. |

## 4. Reconcile — **built**

Rebuilt to the wireframe's three-column shape, and **M10 is closed**: observed-only
time and empty hours are reconcilable items now, which is what makes a day's
account trustworthy rather than merely tidy. A day with three reconciled blocks
and nine unexplained hours is not reconciled, and the sheet had no way to say so.

| Wireframe | State | Note |
|---|---|---|
| Modal over the ghosted day | ✅ | Scrim over whatever view you were on; the day stays visible behind it. |
| Three columns: queue · decision · **evidence** | ✅ | The evidence panel — Source, Subject, Confidence, Adjacent time, **Storage** — appears for observed items only, because they are the only ones asking you to accept a *machine's* claim. The Storage line is the privacy promise restated at the moment someone is looking at a record of what they did. |
| Queue with ✓ progress and "Defer remaining" | ✅ | Per-item ticks, "n of m", and a **pinned** footer — a day with twenty unaccounted hours has twenty items, and "Close the day" must never be something you scroll to find. |
| **Numbered** choices with a recommended default | ✅ | `1`–`4` pick, `Enter` takes the recommendation. The recommendation is a heavier border rather than a fill: the other choices are equally legitimate and must not read as discouraged. Each carries its own consequence line. |
| Item kinds: planned-not-started, **observed-only**, **empty**, overrun, unplanned | ✅ | Observed-only and empty come from `resolve_day`'s segments, not a query of their own — the reconciler asks about exactly the intervals the Day view shows, or the two screens disagree about what is left to decide. |
| "Apply my choice to future activity in this context" | ✅ | A real checkbox now, shown only for a claim with a **domain** behind it — an application name is not durable enough to key a rule on, and a control that is inert half the time teaches people to ignore it. Unticked by default, and it states that it applies forwards only: `activity_span.category` is stamped at write time, so a rule made today cannot rewrite a month already signed off. There is an acceptance test for exactly that, because no screen can show it. |
| "Split interval" | ❌ | Still the missing verb, here and on the Day view. |

## 5. Excel Export — **built**

A whole screen, because the workbook is the client's deliverable: handing it
over on a button press with no preview asks them to trust a file they have not
seen.

| Wireframe | State | Note |
|---|---|---|
| Full export screen with Cancel / Export .xlsx | ✅ | Reached from Reports, not the nav rail — it acts on the month you are looking at. |
| Workbook preview (mini month sheet, gaps hatched) | ✅ | **The preview *is* the sheet**: `preview_excel` and `write_excel` render from one matrix, so the screen cannot promise a layout the file doesn't have. |
| Tags: Month table · Summary · Source mapping | ✅ | Three real sheets. |
| Options: output path, observed-only markers, unaccounted slots, private labels | ✅ | Each states its own consequence. Private labels off by default — the duration is always exported, only the area is withheld, because a workbook is a file you might email. |
| **Reconciliation table** (Measure · App · Excel · Variance) | ✅ | The app's figure, the sheet's own figure, the difference. Variance is expected to be non-zero: a half-hour grid against a to-the-second record rounds, and the caption says so. The measure is no *unexplained* variance. |
| "n unreconciled days — export is allowed, the workbook will mark them" | ✅ | Refusing would be worse: the workbook exists to show the month as it stands, gaps included. |

**Totals are formulas, never pasted numbers.** The Summary sheet is `COUNTIF`s
over the month sheet, so changing a cell in Excel moves the totals — which is
the thing the old workbook could not do, and the reason its numbers drifted
from its own table.

One new dependency: `rust_xlsxwriter`. The dependency policy asks for a reason;
"offline XLSX generation" is a line in the plan's own stack, and hand-rolling a
ZIP + OPC + SharedStrings writer would have been ~600 lines of worse code.

---

## What this changes about the build order

All three gating components are now built:

1. ~~**Browser connector**~~ — **built.** It gated Day-view distraction and
   entertainment classification, the dashboard's entertainment panel and
   findings, the reconcile evidence panel and prospective rules. All four are
   live. Three field assumptions remain, and they need a Windows machine with
   Chrome rather than more code — see the spike report.
2. ~~**`get_month`**~~ — **built.** The dashboard is done.
3. ~~**XLSX writer**~~ — **built.** The export screen is done.

**Nothing left on this list is blocked on a capability that does not exist.**
What remains is work:

- ~~**Workbook import** (M13)~~ — **built.** Detection, then a mapping nobody
  can skip, then a signed per-day variance, then a commit that refuses while
  anything is unmapped or unresolved. Unproven against the client's own
  workbook, which is open question 6 rather than a gap in the code.
- ~~**Split**~~ — **built**, and this line was stale: the verb has been in
  `reconcile.rs` and offered by the sheet for some time. The original shrinks to
  what was planned and the overrun gets its own block after the tracked time
  ended.
- ~~**Entertainment budgets and planned windows**~~ — **built.** Budgets came
  with weekly goals (migration 0008); windows are migration 0009, and the
  planned-versus-unplanned split now reconciles to the intervals underneath it.
- ~~**`Ctrl K` button in the topbar**~~ — **built.**
- ~~**Per-project and per-area Day filters**~~ — **built.** The options are
  built from the day's own segments, not from every project in the database: a
  filter offering thirty projects, twenty-eight of which have no time today, is
  a menu of dead ends.
- ~~**Day-view multi-select**~~ — **built**, on the time column. A *range*, not
  a scattered set: the rows are contiguous minutes and "that whole afternoon was
  the school run" is the sentence people actually have. It lives on the time
  cell rather than the fill button because the fill button opens a modal, and a
  modal covers the table you would be shift-clicking into.
- ~~**Merge**~~ — **built.** Two adjacent records of the same thing becoming
  one, offered from the range selection when it holds two or more records of a
  single subject. Bounded at five minutes of gap, and the result says how many
  seconds it absorbed: merging asserts the gap was part of the same thing, and
  one that annexed time silently would be indistinguishable from a bug.

Everything on this page is built. What is left is not code: the client's own
workbook, which is open question 6 and settles both M12's format sign-off and
M13's proof against a real file.
