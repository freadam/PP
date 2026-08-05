# Plan: the week you are in

Feature plans for personal productivity goals across a week, prompted by a
review of [Rize](https://rize.io) and shaped to fit what Fruit already is.

**Provenance, stated honestly.** The requested source —
[*2 Years with Rize*, Fresh Van Root](https://freshvanroot.com/blog/rize-productivity-tracker-review/)
— could not be fetched: this environment's network policy refuses the host
(`connect_rejected`, 403 at the proxy). What follows is built from search-result
summaries of that review and of Rize's own feature and changelog pages, listed
under [Sources](#sources). Every Rize behaviour attributed below is therefore
**second-hand**. Where a design decision here turns on what Rize actually does,
it is flagged, and it should be checked against the app before that decision is
treated as settled.

---

## The gap this addresses

Fruit currently has two horizons that work and one that does not.

- **The day** is the primary screen and has a ritual — reconcile, ninety
  seconds, every evening.
- **The month** is the dashboard: six cards, findings, targets versus actual.
- **The week** is a planner span and a calibration panel. There is nothing that
  tells you, on Wednesday afternoon, whether the week is going the way you meant
  it to.

That is the gap. A month dashboard is a verdict delivered too late to act on; a
day view is too short a window to see a habit in. The week is the horizon where
a person can still change the outcome, and Fruit does not currently speak to it.

Rize's answer is work-hour targets per weekday, a focus score, break reminders,
overwork notifications and a weekly report. Some of that is directly useful
here. Some of it is the opposite of what this app is.

## The argument for building it now

**Weekly goals are not new scope — they are the general case of an item already
in the plan.** M11 requires *"entertainment budgets and planned/unplanned totals
reconcile to the underlying intervals."* An entertainment budget is a weekly goal
with the direction *at most*. Building the general mechanism costs little more
than building the specific one and closes M11 as a side effect.

The browser connector landing is what makes this possible: entertainment is
measurable now, and a budget against an unmeasurable quantity is a wish.

---

## W1 · Goals

**A goal is a property of you this week, not of a category.**

Fruit already has `life_area.monthly_target_sec` and `project.weekly_target_sec`.
Those are attributes of the *thing* — "Wellbeing is meant to get 8 hours a
month". A goal is different: "*I* am cutting entertainment *this month*". You
change goals without re-categorising your life, and two people sharing a
category taxonomy do not share intentions. Conflating them means editing a
project to change a personal commitment, which is the wrong shape.

So: a new `goal` table, and the existing target columns stay where they are and
keep feeding the month dashboard's target-versus-actual bars.

```
goal
  id            TEXT PRIMARY KEY
  subject_kind  TEXT   life_area | project | contribution | category | metric
  subject_id    TEXT   area id, project id, 'own', 'entertainment', 'deepWork'…
  direction     TEXT   at_least | at_most
  target_sec    INTEGER NOT NULL CHECK (target_sec > 0)
  period        TEXT   week            (month later; the shape is the same)
  applies_days  INTEGER NOT NULL       bitmask, Mon..Sun — default all seven
  starts_week   TEXT   'YYYY-Www'      when it took effect
  ends_week     TEXT   nullable        when it stopped, never deleted
  created_at, updated_at
```

Two decisions worth defending.

**`direction` is first-class, not a sign convention.** Rize's targets are all
"reach this many hours". Fruit's primary outcome is a *reduction*, so "at most
5h of entertainment" has to be a goal you are succeeding at by being under it —
not a target you are failing. A progress bar that turns red when you do the right
thing teaches people to ignore progress bars.

**A goal is closed, never deleted.** `ends_week` rather than a `DELETE`, for the
same reason `activity_span.category` is stamped at write time: last month's
review has to still show the goal that was actually in force. A goal edited into
a new number retroactively rewrites how a month went, and reviews stop meaning
anything.

**`applies_days`** is Rize's per-weekday work-hours target, generalised. A "deep
work" goal on a Mon–Fri bitmask must not expect progress on Saturday, and — more
importantly — must not report you behind on Sunday morning.

---

## W2 · Pace, not a scoreboard

**The one that earns the feature.** A target you read on Friday is a report card.
The question worth answering is *where should I be right now, and where am I?*

Per goal, computed against elapsed time only:

| Figure | Why it is there |
|---|---|
| **Actual so far** | The number. |
| **Expected by now** | `target × (elapsed applicable days ÷ total applicable days)`, with today clipped to `now`. |
| **Delta** | Ahead or behind, in hours and minutes, never a percentage alone. |
| **What the rest of the week needs** | *"3h 20m a day for the remaining 3 days."* |

That last row is the one that changes behaviour. "You are at 62% of your target"
is a fact; "3h 20m a day for the remaining 3 days" is a decision you can make at
breakfast. For an *at most* goal it inverts: *"4h 10m left for the week"*, and
once blown, *"over by 1h 05m — the rest of the week is already spent"*, which is
information rather than a scold.

**Reuse, not new machinery.** `get_month` already clips elapsed seconds to `now`
precisely so a fresh August does not report "6% accounted" on the 4th. The same
rule applies here and for the same reason: **the future is not a shortfall.** A
goal at 0% on Monday morning is on pace, and must say so.

### What this needs from the core

The existing `get_week` is the *planner's* week — `DayColumn`s of blocks, with
planned and tracked seconds. It has no life time, no empty hours and no
entertainment, so it cannot answer any of the above.

`get_month` has the right shape: it sums `get_day`'s `DayTotals` over a range, so
a figure on the dashboard and the same figure on a day cannot be computed two
ways. The work is to **extract that loop into a shared `aggregate_range`** and
have a new `get_week_review` call it. Not a second query — the invariant the
codebase already prizes is that there is one way to total a day, and adding a
weekly SQL aggregate would quietly create a second.

---

## W3 · Fragmentation, reported and not scored

Rize's focus score watches app-switching, sustained stretches and interruptions,
and returns a number.

**Fruit should report the components and deliberately not synthesise a score.**

| Measure | Definition |
|---|---|
| **Longest unbroken stretch** | The longest single run of confirmed work in a day. The figure that actually tracks deep work. |
| **Switches** | Owner changes per day from `resolve_day`, split into **planned** (falling on a block boundary — you meant to switch) and **unplanned**. |
| **Fragmented time** | Confirmed work sitting in segments shorter than a threshold (default 15 minutes). Time that counts and accomplished little. |

Two reasons for no score.

First, character. This app's whole argument is that its arithmetic is checkable
by eye — the counting invariant, Excel totals as formulas rather than pasted
numbers, a reconciliation table putting the app's figure beside the sheet's own.
A weighted 0–100 would be the single number in Fruit that nobody could verify.

Second, evidence. The recurring criticism of Rize in the summaries is that users
*"want fuller summary views, more granular categorization"* — which is the shape
of a score whose inputs are hidden. Reporting three legible numbers is the fix,
not a fourth derived one.

**Fruit can also do something Rize structurally cannot.** Rize sees app switches;
Fruit sees the *plan* underneath them. A switch that lands on a block boundary is
you executing your intention, and a switch that does not is an interruption.
Counting them as the same event throws away the thing Fruit knows and Rize does
not.

**New capability required: none.** All three come from `resolve_day` segments
already on screen.

---

## W4 · Continuous-work and ceiling notices

Rize's break reminders, with the good idea kept and the framing changed.

The good idea, per the summaries: Rize *"doesn't count meetings or non-work time
toward break notifications — it only tracks actual focused work."* In Fruit that
is not a rule to remember, it is the schema:

- `life_entry` has no accumulator and never counts toward a work streak.
- `activity_span` is observed, not confirmed, and never counts either.
- `time_session` counts — **except** where `contribution = 'attend'`, which is a
  meeting. Sitting in a two-hour review is not two hours heads-down, and Fruit
  already records the difference.

Two notices, both off by default like everything observational:

- **Continuous work** — "2h 15m without a break." One per crossing, dismissible.
- **Daily ceiling** — Rize's overwork notification. A configurable cap; the
  notice fires once when crossed.

**A notice, never a nag and never a block.** It does not interrupt a timer, it
does not dim the screen, and crossing the ceiling twice in a day produces one
notice, not two. An app that talks too much gets muted, and a muted app cannot
tell you the thing that mattered.

---

## W5 · The weekly review

The reconcile sheet's sibling. Reconcile asks about *intervals on a day*; this
asks about *goals over a week*. Same discipline: bounded, keyboard-driven,
deferrable.

1. **Each goal** — target, actual, and the outcome in plain language.
2. **Fragmentation** — the three W3 numbers, and how each moved against last
   week. Direction of travel is the whole point; one week's longest stretch in
   isolation says nothing.
3. **The week's biggest divergence** — the largest overrun, or the largest
   unplanned run of entertainment, with a link that opens that day.
4. **Next week's goals, pre-filled from what happened.**

Point 4 is where this goes past Rize's weekly report, which reports and stops.

**Goal calibration.** Fruit already calibrates *estimates* — trailing 30 days,
median, reported at n ≥ 5 so five samples of noise cannot move it. Goals deserve
the same treatment. Set 20 hours of deep work, hit 12 for three weeks running,
and the review should say so and offer 13:

> Deep work: 20h target, 12h median over 3 weeks. A goal you miss every week
> stops being a goal. Try 13h?

Offered, never applied. The user may have a good reason the third week was
atypical, and an app that quietly lowers your ambitions is worse than one that
says nothing.

---

## W6 · Goal templates

Rize ships eight, each aimed at a common problem — *"The 6-Hour Work Day"* for
people who work too much. Fruit's should come from its own outcomes rather than
be copied across:

| Template | Shape | Where its number comes from |
|---|---|---|
| **Cut entertainment** | at most | Trailing 4-week median, minus 20% |
| **Protect deep work** | at least | Trailing median of stretches ≥ 45 min |
| **Sleep** | at least | 8h × applicable nights, from the Sleep/Rest area |
| **Off zero** | at least | Any life area with a target and no time — already the most actionable row on the month dashboard |
| **The shorter week** | at most | Trailing median work hours, minus 10% |

**Every template states the number it chose and where it got it.** A template
that opens with an invented round number is a template people dismiss, and a
goal you did not believe when you set it is one you will not honour on Thursday.
Where there is not enough history, the template says so and asks, rather than
guessing — the same n ≥ 5 discipline the estimate calibration already applies.

---

## Explicitly not building

Rejected on the merits, recorded so the decisions are not relitigated.

| Rize feature | Why not |
|---|---|
| **Distraction blocker** | `PRODUCT-SPEC.md` §1 puts blocking out of scope. It is also structurally impossible as designed: the connector ships with **no `host_permissions`**, so it cannot touch a page — that absence *is* the privacy argument. Blocking would require exactly the permission the design refuses. |
| **A single focus score** | See W3. It would be the only unverifiable number in the app. |
| **"AI-powered" anything** | Fruit is offline and badged OFFLINE. These are heuristics; the docs should call them heuristics and state their thresholds. |
| **Focus sounds, background audio** | Unrelated to time accounting. Every OS and every phone already does it. |
| **Team dashboards, manager visibility** | Not this product. `PRODUCT-SPEC.md` §1. |
| **Weekly streaks as motivation** | `DayReview.streakDays` already exists and is enough. A streak that breaks when you take a holiday punishes rest — in an app whose premise is that rest is time worth recording. |

---

## Build order

Sequenced by what unblocks what, and by the plan's own phase order — Phase 6
(*"reconcile, calibrate, reduce entertainment"*, week 9) sits **before** Phase 7
(Excel, week 10), so goals precede workbook import.

| # | Item | Depends on | Notes |
|---|---|---|---|
| 1 | **`aggregate_range` extraction** and `get_week_review` | — | Pure refactor of `get_month`'s loop. Nothing new is computed; the point is that nothing is computed twice. |
| 2 | **W1 goals + W2 pace** | 1 | **Closes M11.** The entertainment budget is this with `direction = at_most`. |
| 3 | **W3 fragmentation** | 1 | No new capture. Derived from `resolve_day`. |
| 4 | **W5 weekly review** | 2, 3 | Including goal calibration. |
| 5 | **W4 notices** | — | Independent and small; can land any time after 2. |
| 6 | **W6 templates** | 2, and trailing history | Last, because its numbers need weeks of data to be worth offering. |
| 7 | **Workbook import (M13)** | — | Unchanged in scope, now after the goals work per the phase order above. |

## Acceptance

Proposed as **W1–W6** in `ACCEPTANCE.md`, all testable in `fruit-core` without a
webview:

- **W1** — a goal in force during a week is the goal that week's review reports,
  after the goal has since been edited.
- **W2** — a goal at 0 on Monday morning reports *on pace*, not *behind*; and
  expected progress never counts a day that has not happened.
- **W3** — planned and unplanned switches are counted separately, and a day of
  one unbroken session reports one stretch and zero unplanned switches.
- **W4** — a two-hour meeting (`contribution = 'attend'`) does not accrue toward
  the continuous-work notice; two hours of `own` work does.
- **W5** — calibration reports at n ≥ 5 weeks and uses the median, matching the
  estimate calibration's discipline.
- **W6** — a template with insufficient history says so instead of guessing.

The one that matters most is **W2**, for the same reason the month dashboard's
"6% accounted" bug mattered: an app that reports the future as a failure is an
app whose numbers you learn to discount.

---

## Sources

The requested review was unreachable from this environment (403 at the proxy).
These are what the feature descriptions above are drawn from:

- [2 Years with Rize: Can a Productivity Tracker Change Your Work Habits?](https://freshvanroot.com/blog/rize-productivity-tracker-review/) — the requested source, **not fetched**; referenced via search-result summaries only
- [Rize Productivity — AI-Powered Focus Time Tracking](https://rize.io/features/productivity)
- [Set custom work hours target per weekday | Rize Changelog](https://rize.io/changelog/set-custom-work-hours-target-per-weekday)
- [Rize review 2026: Should you use this AI time tracker?](https://dhruvirzala.com/rize-review/)
- [Rize: A time-tracker with breaks](https://www.todayonmac.com/rize-the-ai-that-knows-when-you-need-a-break-even-if-you-dont/)
- [Rize Reviews (2026) | Product Hunt](https://www.producthunt.com/products/rizeio/reviews)
