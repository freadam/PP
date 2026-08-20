# Backlog

Work that is known and agreed. Items are struck through when built, with what
building them turned up — a backlog that only ever shrinks silently teaches
nobody anything. Two sources feed it:

1. **An audit of `PRODUCT-SPEC.md` against the code** — features the
   specification asks for that turned out not to exist. These are numbered
   `A1…` for *audit*.
2. **Using the built app** — things found by running it on a real machine
   against real days, which is evidence no test in this repository can produce.
   Numbered `U1…` for *use*.

Each item says what the spec asks for, what exists today, why it matters, and
roughly how big it is. Nothing here is a guess about priority order — that is a
conversation, not a field.

For the priority *argument* — which of these matter most and why, plus ten
`V*` items that come from the product's direction rather than from an audit —
see [`VISION.md`](VISION.md) §9. This file stays the reference; that one stays
the ranking.

`ACCEPTANCE.md` remains the record of what is *signed off*. This file is the
record of what is *known to be missing*. An item leaves here by being built, or
by the specification being changed on purpose — and if the second, the change
gets written down with its reasoning, the way §3.2's 1-day span was.

---

## ~~U1 · Filling an interval cannot record work — only life~~ — **built**

**Was:** `FillDialog` took a list of life areas and called `addLifeEntry`. There
was no path from a gap on the Day view to a work record at all. Recording work
by hand meant Task detail → Sessions → "Add a session by hand", so you had to
know the task already and had to leave the screen where you noticed the gap.

**Now:** the dialog has a Life/Work switch. Work offers a filterable task list
and a contribution dropdown, and writes a `time_session` through the same
`add_session` the rest of the app uses.

Three things came out of building it that were not in the original note:

- **Contribution moved into the write.** It used to be settable only afterwards,
  on a record that already existed. But the case manual entry exists for is very
  often a meeting, and "I attended two hours" versus "I did two hours" is the
  whole distinction contribution was added to draw — so `ManualSession` carries
  it and the dialog offers it at the point of entry.
- **`replace_existing` came too**, matching `NewLifeEntry`. Recording by hand is
  usually filling a gap, but it is sometimes correcting an hour the app got
  wrong, and the second case needs the old record gone or the day holds both. It
  clears *before* the insert — clearing after would delete the row just written,
  since that row is itself inside the interval. There is a test for exactly that.
- **A zero-length session is now refused.** `ended_at == started_at` used to be
  allowed and produced a row that is invisible on the Day view, counted nowhere,
  and impossible to select in order to delete.

Private stays life-only: a work session names a task by definition, so
"accounted for, nothing recorded about it" has nowhere to attach.

---

## ~~U2 · Start and end times cannot be typed, only nudged~~ — **built**

**Was:** four buttons, start and end ±30m. A meeting that ran 14:20–16:05 could
not be entered at all, because the steps were half-hours from wherever the
dialog happened to open.

**Now:** a typed field per endpoint *beside* the steppers. The stepper is still
the right control for trimming what the app guessed; it was only ever the wrong
one for entering an interval from scratch.

Two bugs found by driving it in a browser rather than by reading it:

- **Clamping was quietly wrong.** Typing `14:20` into a 05:30–06:00 span first
  produced `05:59` — the value clamped to "just before the end", which is a time
  nobody asked for and reads as the app ignoring the keyboard. Typing a start
  later than the end means *relocating* the interval, so it now moves and keeps
  its length, which is what a calendar does.
- **The End "+30m" fell outside the dialog.** Both endpoints shared one row,
  which fitted before two time inputs joined it. The button was still tabbable
  and never visible. One row per endpoint now.

A third, cosmetic: `.area-grid` sized its buttons for one- and two-word life
areas, so task titles wrapped over the row beneath. Rows now size to their
tallest cell.

---

## ~~A1 · The Excel export is missing seven of its specified contents~~ — **built**

**Was:** the month matrix, day columns, a four-measure summary (Work ·
Unaccounted · Observed only · Private), a life-area block, and a source-mapping
sheet. Seven of the twelve contents §4.8 enumerates were absent.

**Now**, all seven: weekly totals, work by project and by task, work by
contribution, core, entertainment split into planned and unplanned, and
YouTube/Twitch.

Three things came out of building it:

- **Provenance is a column now.** §4.8 requires totals to be formulas over the
  sheet, and most are — a week is a contiguous run of day columns, and a work
  slot carries its task's title, so both stay real `COUNTIF`s. Some genuinely
  cannot be: a slot label cannot say whether an hour of entertainment fell
  inside a window you plotted, or whether a session was one you attended rather
  than owned. Those come from the record, and each row says which it is rather
  than leaving the reader to work it out. A figure you cannot trace is a figure
  you cannot check.
- **Formulas carry cached results.** Found by dumping the written file rather
  than by reading the code: `calamine` — which is what *Fruit's own importer*
  uses — never evaluates a formula, so every total read back as zero. Excel
  recomputes and discards the cache; a library reads the cache. A file whose
  numbers depend on which program opens it is not an export.
- **A cache that disagrees with its formula is worse than none.** The first
  version cached the record's exact seconds in cells whose formula is a
  `COUNTIF`, so a task read 2.47 where Excel would recompute 3. Caches are now
  counted from the same cells the formula counts.
  `cached_results_are_what_the_formula_will_recompute` is the guard.

Also `escape_criteria`: a `"` in a task title would end a `COUNTIF` criteria
early and produce a formula Excel refuses to open — a file that looks written
and cannot be opened.

---

## ~~A2 · Contribution is captured everywhere and reported nowhere~~ — **built**

Closed with A1, as predicted — it lands in the same summary sheet.

`DayTotals` gained `by_contribution`, aggregated across the month the same way
`by_app` already was, so the month gets it by summing days rather than by a
second query that could disagree.

Two distinctions the block preserves:

- **`Contribution::None` and "not set" are different rows.** The first is *work
  recorded with no mode*, which §5.8 explicitly separates from the second,
  *never asked*. Collapsing them would answer a question nobody asked.
- **Life can never appear.** `life_entry` has no contribution column, so
  "contribution never applies to personal time" holds however the export is
  written. `the_contribution_block_reports_work_and_only_work` asserts the
  split accounts for all confirmed work and nothing else.

---

## ~~U3 · The app opens on a table, not on an answer~~ — **built**

**Was:** launching landed on the Day view — twenty-four rows, every hour of the
date, most of them empty. That is the right screen for *reconciling* a day and
the wrong one for starting one. Interview 1 has asked the same question since
before there was code: "top 3 tasks scheduled for the day, tasks I logged time
on yesterday but didn't mark complete." Nothing in the build answered it in one
glance, so the answer was assembled by hand every morning out of Planner and
Projects.

**Now:** a `Today` view is the landing route, with Day one keystroke below it
(`G` `Y` / `G` `D`). Three sections: today's plotted work, work left hanging
before today, and the day's totals with a way through to reconcile. Every row's
primary action is start or resume, because the point of a landing screen in a
capture tool is to put the next honest trace one keystroke away.

It composes existing DTOs and adds no logic below the UI. Two display fields
and one thin read were needed, and it is worth being precise about which:

- `DayPlan.task_id` — a plan could name a task but not identify it, so no
  caller could start a timer from one. `null` for a bare label ("Standup"), and
  that row withdraws the offer rather than making it and then refusing.
- `SlotOwner::Work.source` — see C1 below.
- `Store::unfinished_before(date, tz)` — the "still open" clause. Not a list
  anyone maintains: it is inferred from sessions, which is only possible
  because time attaches to tasks. Bounded to seven days and five rows so it
  stays a set of loose ends rather than a standing accusation, and it excludes
  work already picked back up today, which is current work rather than a thing
  you forgot. Covered by `still_open_is_yesterdays_work_that_was_never_finished`.

**Found by driving it:** `G` `D` had never been wired. The rail's own tooltip
promised it and the key map had no `d` entry — invisible for as long as Day was
the landing route, because the view it failed to reach was already on screen.
`check-ui.mjs` now walks Today as well, so the same class of gap fails loudly.

---

## ~~U4 · The 90% capture target was a promise nobody could check~~ — **built**

**Was:** `time_session.source` recorded whether an interval was captured by the
timer or typed in afterwards, and nothing ever showed the ratio. The plan's
headline number — ≥90% of confirmed work captured live — could not be read off
the app that was supposed to be achieving it.

**Now:** a **Captured live** card on the Day summary strip and in the Today
header, with the 90% gate marked on its bar and a `✓` that survives greyscale.

The decision worth recording is what it counts. The obvious implementation sums
`SessionRow.elapsed_sec` grouped by source; it is wrong in the one place being
wrong matters most. A session row is the whole session, and the card sits beside
a figure for one date: a session crossing midnight belongs to two days, and a
session outranked by a confirmed life entry is not counted as work at all. So
`source` is carried on the resolved **segment**, and the split is a partition of
exactly the seconds `confirmedWorkSec` is summed from — the two agree by
construction rather than by luck (`every_confirmed_work_segment_says_how_it_was_captured`).

Recovered time is shown and deliberately left out of the ratio: its provenance
is the crash-recovery machinery, not a choice anyone made about honesty. An
empty day reads `—`, not 0%.

See `ACCEPTANCE.md` § *Capture honesty (C)*. What the number reads on a real
week is still V1's job; this item is that the number exists and is derived from
the right seconds.

---

## A3 · No YouTube/Twitch trend in reports

**What the spec asks for.** §3.5, "Planned vs unplanned entertainment with
YouTube/Twitch trends". Success measure 5 names those two sites specifically.

**What exists.** The planned-vs-unplanned chart (built with migration 0009), and
`get_domain_totals`, which returns seconds per domain and already feeds the
Activity screen.

**What does not.** Any per-domain series on Reports.

**Size.** Small — one panel over an existing query.

---

## A4 · Projects cannot have a note, and have no monthly target

**What the spec asks for.** §3.3: projects have "one compact plain-text note",
the same as tasks. §4.5's entity table lists a project's key fields as `name`,
`colour`, `weekly_target_sec`, **`monthly_target_sec`**, **`note`**,
`is_archived`.

**What exists.** `CREATE TABLE project` has `weekly_target_sec` and neither of
the other two. A `life_area` *does* have `monthly_target_sec`, which is what the
month dashboard's target-vs-actual bars read — so projects are the only
targetable thing in the app that cannot be measured against a month, on the one
screen the plan makes the default reporting horizon.

**What exists.** The `note` table's primary key is `task_id`, so it can
physically only hold notes for tasks. This is not a missing screen; it is
missing storage.

**Size.** Moderate — the only item on this list needing a migration. A column on
`project`, a save function with the same 2,000-character cap tasks have, a
command, a text box, tests.

---

## A5 · The Day view has no drag and no keyboard editing

**What the spec asks for.** §3.1: "Editing: drag, keyboard, split, merge, fill,
repeat, multi-select." Five of the seven are built; drag and keyboard are not.

**Why keyboard matters more than it looks.** §2 describes a user who "uses the
keyboard heavily and will learn shortcuts", and criterion **U10** requires every
action be reachable by keyboard. This is an accessibility commitment, not a
preference.

**Size.** The largest item here. Drag on a table needs edge hit-testing, a live
preview, snapping, a clean Escape, and correct behaviour when a record spans
rows scrolled out of view. Worth agreeing the exact gestures before starting —
"drag on the Day table" could mean several different things and building the
wrong one wastes the effort.

---

## A6 · Day totals do not break out life areas

**What the spec asks for.** §3.1: "Selected-day totals: work, **each life
area**, sleep/rest, entertainment, PC use, and gaps."

**What exists.** Six aggregate cards. `DayTotals.by_area` already holds the
per-area breakdown and already crosses to the renderer; nothing reads it.

**Size.** The smallest item on this list — renderer only, no core change.

---

## A7 · Two of the five specified Day filters are missing

**What the spec asks for.** §3.1: "Filters: project, life area, work
contribution, entertainment, confidence state."

**What exists.** The state presets (which cover confidence), plus per-project and
per-area. Work-contribution and entertainment filters are missing.

**Design note for whoever builds it.** The existing subject filters are built
from *the day's own segments*, not from a master list — a dropdown offering
thirty projects when two had time today is a menu of dead ends. Any new filter
should follow that rule.

**Size.** Small.

---

## A8 · No delete-recent for observed activity

**What the spec asks for.** §3.4: "Exclusions applied before storage, retention
choices, a pause that survives restart, **delete-recent** and delete-all."

**What exists.** Exclusions, retention with automatic purge, pause across
restart, and `clear_activity` (delete-all).

**Why it matters.** Delete-recent is the control people actually reach for,
because it is the one wanted *immediately* — right after something on the
machine they would rather the app had not observed. Today the only option is
deleting the entire history, which is drastic enough that most people will not,
so they keep data they did not want kept. A privacy control that only exists in
its most destructive form is one that does not get used.

**Size.** Small — one function taking a number of minutes, a bounded `DELETE`, a
confirmation in Settings.

---

## A9 · Settings has no Excel group

**What the spec asks for.** §3.6 lists ten groups, including Excel.

**What exists.** Nine equivalents — "Entertainment rules" appears as **Labels**
and "Activity privacy" as **Activity**, which are naming differences rather than
gaps. Excel options live on the export screen instead of in Settings.

**Size.** Small, and arguably fine as-is: the options are where they are used.
Either move them or record the deviation deliberately.

---

## A10 · Entertainment rules match only apps and domains

**What the spec asks for.** §4.7: *"The user can override any interval, domain,
application, project, task or recurring pattern."*

**What exists.** `MatchKind` has exactly two variants, `App` and `Domain`.
Per-interval override exists too — that is what the reconciler's verbs do — so
three of the six are covered.

**Missing:** rules keyed on a **project**, on a **task**, or on a **recurring
pattern**.

**Why the three missing ones are not interchangeable with the two that exist.**
A domain rule says "youtube.com is entertainment, always". A project rule says
something a domain rule cannot: *"time on the Client X project is never
entertainment, whatever site it happened on"* — research on YouTube for a
client is the exact case a domain rule gets wrong, and the reviewer's own
complaint about false positives is this case. A recurring-pattern rule says
*"Friday 18:00–20:00 is entertainment"*, which is a statement about time rather
than about software, and nothing in the current model can express it.

**Size.** Moderate. The storage generalises — `activity_rule` already keys on
`(match_kind, match_value)`, so `project`/`task` are two more variants and a
lookup through the session that owns the interval. A recurring pattern is
bigger: it needs a time-of-week matcher, which is closer to the `rrule` engine
than to the rule table.

---

## A11 · The Day view renders seven of the ten specified slot states

**What the spec asks for.** §4.3 enumerates ten states every Day slot can
visibly be: *planned and completed as intended · planned with overrun · planned
with underrun · planned but never started · unplanned confirmed activity ·
observed but unconfirmed · idle/away · sleep/rest · intentionally
private/untracked · empty/unaccounted*.

**What exists.** `SlotState` has seven: `empty`, `plannedNotStarted`,
`confirmedWork`, `confirmedLife`, `private`, `observedOnly`, `idle`.

**The three that are not distinct, and how they differ in severity:**

- **planned + completed / + overrun / + underrun** collapse into
  `confirmedWork`. The information exists — `DriftState` carries exactly these
  distinctions — but it lives on a *block*, and the Planner is where it is
  drawn. A Day slot knows a block was planned there and knows work happened;
  it does not say whether the two matched. Given the Day view is the primary
  operational screen and drift is the product's signature reading, this is the
  most substantive of the three. The Today screen (U3) does not close this
  either, and deliberately: it reports totals, and plan-versus-actual on the
  landing screen is the harder redesign. Carried, not cut.
- **unplanned confirmed activity** is not distinguished from planned confirmed
  activity: both are `confirmedWork`. The Planned column being empty beside a
  filled Actual column carries it visually, which is arguably enough — but it
  is carried by absence rather than stated.
- **sleep/rest** collapses into `confirmedLife`. There is a Sleep summary card
  and `sleepSec` in the totals, so the *figure* is broken out; the row is not.

**Size.** Small to moderate, and mostly a decision rather than code: the data
for all three already reaches the renderer. The question is whether the
classification column gains states or the Planned column gains a drift mark,
and that is a design call worth making deliberately rather than by whichever is
easier to write.

---

## A12 · Five of the six performance budgets are unmeasured

**What the spec asks for.** §5.6 sets six budgets: cold start < 1.5s, Day view
< 100ms, month dashboard < 250ms, week load with 500 blocks < 100ms, idle CPU
~0%, and no data loss after a forced close.

**What exists.** One test: `month_load_stays_inside_its_budget`.

**Missing:** the Day view, the week load, cold start, idle CPU, and the
data-loss guarantee — which is criterion **D4** and already listed under Test
gaps below.

**Why it is worth more than it looks.** The Day view is the screen the product
is organised around and the one the ninety-second reconciliation target runs
on; it is also the screen that gained the most work this year — filters,
multi-select, split, merge, the plan overlay. A budget nobody measures is a
budget that degrades in increments nobody notices.

**Size.** Small for the two core ones. The Day and week loads are pure
`fruit-core` calls and can be timed exactly the way the month already is. Cold
start and idle CPU need the packaged binary and belong with the Windows work.

---

## A13 · The counting invariant is not tested the way §4.2 says it is

**What the spec asks for.** §4.2, describing the app's central promise: *"This
is enforced in the core as a property test over random overlapping records, not
asserted in the UI."*

**What exists.** Two things, neither of which is that:

- `a_day_accounts_for_every_second_exactly_once` — a real assertion that the
  layers tile the day, over **hand-written fixed data**: one sleep entry, one
  session, the rest empty. It would not catch a bug that only appears when a
  life entry partly overlaps a session which partly overlaps an observation.
- `d1_d7_d11_fuzz_leaves_the_database_consistent` — a *scripted* fuzz with a
  deterministic RNG, which asserts foreign keys, cache agreement and the
  one-running-session rule. It does **not** assert the counting invariant.

**Why this is the most important item on this page.** §4.2 calls the invariant
"the technical form of the product's promise", and notes that acceptance
criteria 2, 4 and 8 all reduce to it. Overlap is exactly where it can break,
and overlap is exactly what the current test does not generate. The claim in
the specification is currently stronger than the evidence.

**Size.** Small, and the machinery exists — `Lcg` is already in the test file.
Generate a few hundred random life entries, sessions and observations on one
day, resolve it, and assert the layers sum to the day's length every time. The
fuzz that exists is the template; it needs one more assertion and a different
generator.

---

## Test gaps

Distinct from missing features. Nothing is failing and nothing is skipped —
these are criteria resting on argument rather than evidence.

| Criterion | State | Could it be automated? |
|---|---|---|
| **U1** every action reachable from palette *and* a key | "structural" — one `COMMANDS` registry feeds palette, key handler and shortcut sheet, so an unreachable command cannot be written | The argument is sound but is an argument |
| **U3** `Esc` cancels a drag and restores position | "manual" | **Yes**, straightforwardly — the headless browser already runs and can drag and press keys |
| **U9** purposeful empty states; failed writes say what/why/next | "visual" | Partly — that every view renders *something* when empty is checkable; whether the copy is good is human |
| **I2** contrast ≥4.5:1 body, ≥3:1 graphics, both themes, including Focus text over four gradients | ❌ not measured | **Yes** — contrast ratio is a defined formula and the headless browser can read rendered colours |
| **D4** force-kill mid-write stays consistent, loses ≤500ms | ❌ not fuzzed | **Yes** — spawn a writer, `SIGKILL` it, reopen, `PRAGMA quick_check` |

---

## Blocked on someone else

Not backlog — nothing here is closed by writing code.

- **The client's reference workbook** (spec open question 6). Blocks M12's format
  sign-off *and* gives M13's importer a real file to be held against. One
  question, two criteria.
- **A Windows desktop.** I8 (tray icon at 16px), D13 (second launch focuses the
  window), the Windows foreground-sampling path, and the three field assumptions
  in `SPIKE-BROWSER-CONNECTOR.md`.
- **Seven consecutive days of real capture**, which is what success measures 1
  and 5 are actually measured over.
- **Vendoring the woff2 font files.** Deliberately uncommitted — third-party
  binaries with their own licence texts, so a release decision rather than a
  build one. Until then the app falls back to system faces, which is a visual
  regression and never a network call.
