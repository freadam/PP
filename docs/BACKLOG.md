# Backlog

Work that is known, agreed and not yet built. Two sources feed it:

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

## U1 · Filling an interval cannot record work — only life

**Found by:** using the app.

**What the spec asks for.** §3.1, now stated explicitly: filling an interval
records either kind of confirmed time — a life area, or work on a project/task.

**What exists.** `FillDialog` in `src/views/Day.tsx` takes a list of life areas
and calls `addLifeEntry`. There is no path from a gap on the Day view to a work
record. Recording work by hand is possible, but only from Task detail → Sessions
→ "Add a session by hand" — which means you must already know which task, and
must leave the screen where you noticed the gap.

**Why it matters, and why it is not a small convenience.** The whole premise of
the product is that the observer does most of the recording and the human
confirms it. That premise has a hole in it exactly where the observer cannot
see: work done on a second machine, an offline meeting, a task done on paper or
at a whiteboard. None of it produces an `activity_span`, so none of it is ever
offered by the reconciler — it can only ever be entered by hand. Making the
manual path longer than the automatic one puts the friction precisely where the
app is already weakest, and unrecorded work is the failure that makes the
monthly account untrustworthy.

**Size.** Moderate. The core already has `add_session`, so no new storage and no
migration. The work is in the dialog: a mode switch between *life* and *work*, a
task picker for the work side, and passing the chosen task through. Both write
paths already exist and are tested.

---

## U2 · Start and end times cannot be typed, only nudged

**Found by:** using the app.

**What exists.** `FillDialog` offers four buttons — start −30m/+30m, end
−30m/+30m. There is no way to type a time.

**Why it matters.** The stepper is the right control for *trimming* an interval
the app already proposed, which is what it was built for. It is the wrong
control for *entering* one from scratch: a meeting that ran 14:20 to 16:05 takes
a dozen clicks and still cannot land on 14:20, because the steps are half-hours
from wherever the dialog opened. Combined with U1, the two together are why
recording offline work is currently unpleasant enough to skip — and a record
that gets skipped is the one that breaks the counting invariant.

**Size.** Small. Two `<input type="time">` fields beside the existing steppers,
both writing the same state. The core's validation is unchanged.

**Note.** U1 and U2 are one piece of work in practice. They are listed
separately because they are separately true — either could be fixed without the
other — but the dialog should be opened once and both done.

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
