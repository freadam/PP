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

## A1 · The Excel export is missing seven of its specified contents

**What the spec asks for.** §4.8 enumerates the export's contents.

**What exists** (`crates/fruit-core/src/store/excel.rs`): the month matrix, day
columns, a four-measure summary (Work · Unaccounted · Observed only · Private),
a life-area target-vs-actual block, and a source-mapping sheet.

**Missing:**

| §4.8 requires | Present |
|---|---|
| weekly totals | ✗ nothing groups by week |
| work by project/task | ✗ |
| work by contribution | ✗ |
| core totals | ✗ |
| planned-entertainment totals | ✗ |
| unplanned-entertainment totals | ✗ |
| YouTube/Twitch totals | ✗ |

**Why it matters.** M12 is currently marked ✅ and should not be. The export is
the client's primary exchange format and the artefact the product will be judged
by — and the specific missing rows include planned-versus-unplanned
entertainment, which is *the outcome the product exists to move*.

**Size.** Moderate, and entirely presentational: every figure already exists.
`entertainment_in_window_sec` (migration 0009) gives the planned/unplanned
split, `get_domain_totals` gives YouTube and Twitch, `by_project` and `by_area`
are already on `DayTotals`, and `contribution` is on every `time_session`. The
work is adding blocks to the summary sheet and a weekly grouping — as formulas,
not pasted numbers, per §4.8's rule that totals must recompute in Excel.

---

## A2 · Contribution is captured everywhere and reported nowhere

**What the spec asks for.** §3.5 "Work contribution summaries — which never
apply to personal time", and §4.8 lists contribution in the export.

**What exists.** The column on `time_session`; a dropdown on the Day view that
sets it; a deliberate structural guarantee that `life_entry` has no such column,
so "never on personal time" cannot be violated.

**What does not.** Any aggregation at all. Searching the repository for
`by_contribution` / `byContribution` returns nothing.

**Why it matters.** A field that asks for a judgement and returns nothing is a
field people stop filling in. Either report it or remove it; the present state
is the worst of the two.

**Size.** Small once A1 is under way — it is one more grouping over data already
loaded, and it lands in the same summary sheet.

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

## A4 · Projects cannot have a note

**What the spec asks for.** §3.3: projects have "one compact plain-text note",
the same as tasks.

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
