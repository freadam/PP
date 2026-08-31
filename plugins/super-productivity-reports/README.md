# Fruit Reports — a Super Productivity plugin

Fruit's reporting screens, rebuilt against Super Productivity's data.

Fruit reports on the gap between what you plotted and what you did. Super
Productivity records something narrower — how long you spent on a task on a
given day — so some of Fruit's readings come across exactly, some come across
with their inputs redefined, and four cannot come across at all. This document
says which is which, per figure, so you can tell at a glance whether a number
here means what the same number means in Fruit.

Nothing that cannot be computed is estimated or filled in. A plausible-looking
figure that is not the one it claims to be is the worst outcome available to a
report whose whole purpose is being trustworthy, so the missing panels are
absent and the report says so on screen.

```
plugins/super-productivity-reports/
  manifest.json     what the app installs and what it is allowed to read
  plugin.js         host-side: the header button and the shortcut, nothing else
  src/engine.js     all the arithmetic — no DOM, so it can be unit-tested
  src/render.js     all the drawing — no arithmetic, so figures cannot disagree
  src/app.js        one read of app state, handed to the engine
  src/styles.css    Fruit's report layout in Super Productivity's theme tokens
  build.mjs         inlines the above into one index.html, and zips it
  test/             45 engine tests, 22 checks in a real browser
```

## Installing it

```bash
cd plugins/super-productivity-reports
npm run build          # → dist/fruit-reports-1.0.0.zip
```

or, from the repository root:

```bash
npm run sp-plugin:build
```

In Super Productivity: **Settings → Plugins → Load plugin from file**, pick the
ZIP, enable it. A **Fruit Reports** button appears in the header.

Requires Super Productivity 14.0.0 or newer. It reads tasks, the archive,
projects and tags; it writes nothing except its own settings, and makes no
network requests — the manifest declares no `http` permission and no
`allowedHosts`, so the host would refuse one.

## What it reports

Four tabs. **Work report** (day / week / month), **Estimates**, **Targets**,
**Settings**.

### Ported exactly

These read the same inputs and run the same arithmetic as `fruit-core`. Where
Fruit's Rust rounds with integer division, so does this; where Fruit takes a
median rather than a mean, so does this.

| Figure | Fruit | Here |
|---|---|---|
| **Calibration** — trailing 30-day `tracked ÷ estimate`, bucketed at 15m/30m/1h/2h/3h+, median not mean, silent below 5 samples, worst bucket named | `store/reports.rs::calibration` | identical; `completed_at` → `doneOn`, task tracked total → `timeSpent` |
| **Calibration headline** — "You run 1.24× your estimates. Your 3h estimates are the least accurate." | `reports.rs::headline` | identical wording |
| **Weekly targets with pace-to-date** — `target × days_elapsed ÷ 7`, archived projects excluded | `reports.rs::weekly_targets` | identical; the target is stored in plugin settings |
| **Baseline** — the previous four comparable periods, averaged, skipping periods with nothing recorded | `work_report.rs::baseline_for` | identical |
| **Work by day** — every date present, a day with no work is a zero | `work_report.rs::report_for` | identical |
| **By project / by kind of work** — shares to 3dp, longest first, the empty bucket always kept | `work_report.rs::work_by_project` | identical, on tags instead of categories |
| **Target days met** — only days that have already happened, and only weekdays the target applies to | `work_report.rs::report_for` | identical |
| **Findings** — baseline comparison at ≥10%, target, dominating day, mostly-unlabelled | `work_report.rs::findings_for` | identical wording |
| **CSV export** — `week_start,project,planned_sec,tracked_sec,drift_sec` | `reports.rs::report_csv` | identical columns |

### Ported with an input redefined

Two figures have no direct equivalent, and are stated in terms of what Super
Productivity does record. Both say so on screen, next to the panel.

**Kind of work → tags.** Fruit gives a task one category. A Super Productivity
task has any number of tags, and the split has to partition the total or the
shares stop adding to 100%. So a task's **first** tag is read as its kind of
work. Super Productivity's synthetic `TODAY` tag is ignored, and a subtask with
no tags of its own inherits its parent's.

**Planned vs tracked → estimated vs actual.** Fruit compares the time you
*plotted in blocks* for a week against the time you tracked in it. Super
Productivity has no plotted blocks, and deriving a plan from due dates would put
an invented number in the panel whose entire job is honesty. What survives is
the like-for-like: over the tasks **completed** in a week, what you said they
would take against what they took. Both bars describe one population, which is
what makes the difference a real quantity — the same `drift = tracked − planned`
the Fruit CSV carries.

### Not here, and why

| Fruit panel | Why it cannot be computed |
|---|---|
| **Hour-of-day histogram** | Needs session start times. `timeSpentOnDay` is a daily total; there is no "when". |
| **Working-day span** (first start to last stop) and the span-density finding | Same — a day's total does not say when it began or ended. |
| **Focus sessions**, and the *Deliberate* component of the score | Super Productivity has no timed block plotted in advance, so there is nothing to be inside of. |
| **Apps panel** and the plan-vs-observation correlation | These read Fruit's screen-observation record. Super Productivity has no such data, and this plugin makes no attempt to collect any. |
| **Reconcile streak**, life entries, day reviews | No equivalent record. |

The work report always carries a finding saying these four are missing, so a
reader who knows the Fruit screen is not left wondering where they went.

### The score, and what changed about it

Fruit scores a period out of 100 from four inputs, each printed with its own
weight: Against target (0.35), Deliberate (0.25), Depth (0.25), Accounted
(0.15). **Deliberate cannot be computed here.** Leaving the other three at their
Fruit weights would cap every score at 75 and quietly relabel a good week as a
mediocre one, so the surviving three are rescaled by `1 / 0.75` to sum to one
again — 46.7%, 33.3%, 20.0%. The panel prints the rescaled weights and says
which component is missing.

A score you cannot open is a score you cannot argue with, so the panel still
prints every input, its value and its weight, and the sum is the arithmetic you
can check by hand.

Nothing recorded means **not scored**, not zero.

## Two things worth knowing about the data

**Parent tasks are not counted.** Super Productivity maintains a parent task's
`timeSpentOnDay` as the sum of its subtasks'. Counting both doubles every total
on the screen, so the engine drops any task that has subtasks and counts the
leaves. This is why the totals here match the ones the app itself shows.

**The archive is read too.** Super Productivity moves finished tasks out of the
live set. A 30-day calibration window read from live tasks alone would be nearly
empty and would tell someone who has finished fifty tasks to come back after
five.

## Settings

Two things Fruit stores on its own entities and Super Productivity has nowhere
to put:

- **Daily work target**, with the weekdays it applies to. A Monday-to-Friday
  target does not report a shortfall on a Sunday. Leave it empty and the score's
  first component measures how *even* the days were instead — with no target
  there is nothing to be behind.
- **Weekly target per project**, in hours, which drives the Targets tab.

Both are saved through `persistDataSynced`, so they travel with your sync.

## Developing

```bash
npm run build                 # → dist/, and dist/fruit-reports-<version>.zip
node --test test/engine.test.js   # 45 tests: the arithmetic
node test/check-ui.mjs            # 22 checks: the built page in real Chromium
```

Edit the files under `src/`, never `dist/index.html` — it is generated. Super
Productivity serves plugin UI through an iframe `srcdoc`, so no second file from
the ZIP is reachable from the page and everything has to be inlined; `build.mjs`
does that, and fails loudly if a source ever contains a literal `</script`.

`test/engine.test.js` holds the engine to numbers transcribed from the Rust it
was ported from — the median of `[1.0, 1.1, 1.2, 1.05, 11.2]`, the bucket
boundaries, the score weights — rather than to a recording of its own output. A
port that agrees with itself proves nothing.

`test/check-ui.mjs` drives the built page in headless Chromium against a stubbed
Plugin API: that the figures reach the screen, that a project titled
`<img src=x onerror=…>` is displayed rather than executed, that no component
hard-codes a colour outside a `var()` fallback, and that the layout holds at
420px.
