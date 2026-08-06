# Plan: the week you are in

Feature plans for personal productivity goals across a week, drawn from
[*2 Years with Rize*](https://freshvanroot.com/blog/rize-productivity-tracker-review/)
(Rolf Mistelbacher, Fresh Van Root, March 2025) and shaped to fit what Fruit
already is.

**Revision note.** The first version of this plan was written from search-result
summaries, because this environment's network policy refuses the host. The full
text has since been supplied, and three of those summaries turned out to describe
a *different* article. Corrections are recorded in
[Appendix A](#appendix-a--what-the-second-hand-summaries-got-wrong) rather than
quietly deleted, because a plan that hides where its premises came from is a plan
nobody can audit.

---

## The reviewer, and why he matters here

Rolf runs a marketing agency and a content business, is operationally involved in
client projects, and says plainly: *"context and task switching are part of my
job."* His stated goal is **a 30-hour work week** — an upper bound, not a target
to fill — and *"to spend as much time as possible in productive categories."*

He is not Fruit's client. But he is two years into daily use of the category of
tool Fruit is, which makes his verdict on **what turned out to matter** worth
more than a feature list. Three things stand out.

1. **The most valuable feature was the one he set up himself.** Not a report — a
   custom category. He wanted to know how much time AI chat tools were eating,
   made a bucket called *"AI Chat"*, and found out. *"AI promises to save us so
   much time, but how much time do you spend in these tools? I wanted to find
   out."*
2. **The weekly report is read at a specific moment.** *"Rize sends me a weekly
   PDF report, and I take a quick look on a Monday morning."* And: *"While this
   can all be overwhelming, the most important information is right at the
   top."*
3. **The biggest risk is the tool itself.** *"Rize is an app for productivity
   nerds. It has the potential to take too much time to configure and run — the
   settings screen can be overwhelming."* He describes actively resisting
   *"over-configuring"* and *"over-optimizing."*

## The governing constraint

Point 3 is not a footnote. It is the design budget for everything below.

Fruit's Settings screen is already long, and this plan could easily add a work
window, non-focus categories, goal durations, notice thresholds and a category
editor — and land exactly where the review warns. So:

> **Every feature here must either configure itself, or earn its configuration by
> answering a question the user already has.**

Concretely, that rules the plan in two places. Templates (W10) pick their own
numbers from your history and say where they got them. The uncategorised surface
(W8) exists so the app tells you the three things worth categorising instead of
presenting an empty taxonomy and asking you to fill it in. Nothing here ships as
a blank form.

## The gap this addresses

Fruit has three horizons and only two of them work.

- **The day** is the primary screen and has a ritual — reconcile, ninety seconds,
  every evening.
- **The month** is the dashboard: six cards, findings, targets versus actual.
- **The week** is a planner span and a calibration panel. Nothing tells you, on
  Wednesday afternoon, whether the week is going the way you meant it to.

A month dashboard is a verdict delivered too late to act on; a day is too short a
window to see a habit in. The week is the horizon where a person can still change
the outcome, and it is the one Fruit does not speak to.

## The argument for building it now

**Weekly goals are not new scope — they are the general case of an item already
in the plan.** M11 requires *"entertainment budgets and planned/unplanned totals
reconcile to the underlying intervals."* An entertainment budget is a weekly goal
with the direction *at most*. Building the general mechanism costs little more
than the specific one and closes M11 as a side effect.

The browser connector landing is what makes it possible: entertainment is
measurable now, and a budget against an unmeasurable quantity is a wish.

---

## What Fruit already matches

Recorded so it is not rebuilt. The review praises several behaviours Fruit has.

| Rize, per the review | Fruit |
|---|---|
| *"I often change how a block of work is categorized after it has been tracked… select a block on the calendar and change its project, client, or task"* | The Day view detail panel: contribution, reassignment, and work → life conversion. |
| *"Change how linkedin.com should be tracked going forward **or only for this specific session**"* | Exactly the reconciler's prospective-rule checkbox — decide this interval, or decide the domain. Built with the connector. |
| *"You can access all settings via a command shortcut (CMD + K)"* | The palette, on `⌘K`/`⌘F`. It still has **no visible affordance**, which `WIREFRAME-GAP.md` already flags. |
| Calendar integration for meeting time | `.ics` import, local file only — no CalDAV and no account, by design. Meetings are then `contribution = 'attend'`. |
| Categorised time totals per day/week/month | The month dashboard, and `domain_totals` since the connector. |

And one thing Fruit **cannot** match, for a reason worth stating rather than
logging as a gap. The review's most granular tracking rule zooms in on *"specific
window titles or URLs"* — filing one particular LinkedIn page as Client Work.
That requires storing URLs. Fruit's connector reduces every URL to a registrable
domain before it crosses the process boundary and reduces it again on the way in;
there is no code path that stores a path or a query. **Per-URL rules are not a
missing feature, they are a refused one.** Window-title rules are technically
available where titles are enabled, and are deliberately left out of scope for
the same reason: a title is the document name, the customer, the ticket.

---

## W1 · Goals

**A goal is a property of you this week, not of a category.**

Fruit already has `life_area.monthly_target_sec` and `project.weekly_target_sec`.
Those are attributes of the *thing* — "Wellbeing is meant to get 8 hours a
month". A goal is different: *"I am working a 30-hour week."* You change goals
without re-categorising your life, and two people sharing a taxonomy do not share
intentions.

```
goal
  id            TEXT PRIMARY KEY
  subject_kind  TEXT   life_area | project | contribution | category | metric
  subject_id    TEXT   area id, project id, 'own', 'entertainment', 'allWork'…
  direction     TEXT   at_least | at_most
  target_sec    INTEGER NOT NULL CHECK (target_sec > 0)
  period        TEXT   week            (month later; the shape is the same)
  applies_days  INTEGER NOT NULL       bitmask, Mon..Sun — default all seven
  starts_week   TEXT   'YYYY-Www'      when it took effect
  ends_week     TEXT   nullable        when it stopped, never deleted
  created_at, updated_at
```

**`direction` is first-class, not a sign convention.** The review is the evidence:
the reviewer's goal is *"a 30hour work week"* and the template he chose is *"the
6-hour work day"* — both **ceilings**. Rize ships these as goal templates
alongside "reach this many hours", so the two directions are peers there, and
they must be peers here. Fruit's own primary outcome is a reduction. A progress
bar that turns red when you do the right thing teaches people to ignore progress
bars.

**A goal is closed, never deleted.** `ends_week` rather than a `DELETE`, for the
same reason `activity_span.category` is stamped at write time: last month's
review has to still show the goal that was actually in force. A goal edited into
a new number retroactively rewrites how a month went, and reviews stop meaning
anything.

**`applies_days`** is Rize's per-weekday work-hours target, generalised. A goal on
a Mon–Fri bitmask must not expect progress on Saturday and — more importantly —
must not report you behind on Sunday morning.

One caution the review supplies for free: *"This feature can trick you into a
gamification mode, competing against yourself."* He means it kindly. It is still
a reason to keep goals few and to make W9's templates opinionated rather than
shipping a goal builder that invites a dozen.

---

## W2 · Pace, not a scoreboard

**The one that earns the feature.** A target you read on Friday is a report card.
The question worth answering is *where should I be right now, and where am I?*

| Figure | Why it is there |
|---|---|
| **Actual so far** | The number. |
| **Expected by now** | `target × (elapsed applicable days ÷ total applicable days)`, with today clipped to `now`. |
| **Delta** | Ahead or behind, in hours and minutes, never a percentage alone. |
| **What the rest of the week needs** | *"3h 20m a day for the remaining 3 days."* |

That last row is the one that changes behaviour. "You are at 62% of your target"
is a fact; *"3h 20m a day for the remaining 3 days"* is a decision you can make at
breakfast. For an *at most* goal it inverts: *"4h 10m left for the week"*, and
once blown, *"over by 1h 05m — the rest of the week is already spent"*, which is
information rather than a scold.

**Reuse, not new machinery.** `get_month` already clips elapsed seconds to `now`
precisely so a fresh August does not report "6% accounted" on the 4th. The same
rule applies here and for the same reason: **the future is not a shortfall.** A
goal at 0% on Monday morning is on pace, and must say so.

### What this needs from the core

The existing `get_week` is the *planner's* week — `DayColumn`s of blocks, with
planned and tracked seconds. No life time, no empty hours, no entertainment. It
cannot answer any of the above.

`get_month` has the right shape: it sums `get_day`'s `DayTotals` over a range, so
a figure on the dashboard and the same figure on a day cannot be computed two
ways. The work is to **extract that loop into a shared `aggregate_range`** and
have a new `get_week_review` call it. Not a second query — adding a weekly SQL
aggregate would quietly create a second way to total a day.

---

## W3 · Focus sessions

**Missed entirely in the first draft, and the reviewer's own headline feature:**
*"I am sharing this feature first because it is the most crucial one for me."*

A Rize focus session carries a **duration**, a category, a project, a client and
a task, started deliberately. Fruit has the pieces — a task timer, blocks with
durations, Pomodoro — but not the shape. Two specific things are missing.

**A session with an intended length.** Fruit's timer runs until stopped; a
plotted block has a duration but the timer does not know about it as a
commitment. Starting *"45 minutes on this"* is a different act from starting a
stopwatch, and it is the act the reviewer performs dozens of times a week.

**Extend in flow, in one click.** *"You can extend a focus session with a click by
clicking the + sign. Perfect if you are in a flow state."* This is the detail
worth stealing. The moment you are most productive is the moment you least want a
dialog, and the alternative — a session that expires and asks a question — is an
interruption the tool caused. One key, `+`, adds another block of the same
length.

**What Fruit adds that Rize cannot.** Extending a session is a *plan revision*,
and Fruit is the app that separates plan from record. An extension should show
in drift as what it is: you meant 45 minutes and took 90. Rize has no plan to
diverge from, so its extension is free. Here it costs something, honestly, and
that is the more useful reading.

**Automatic detection is deliberately deferred.** Rize can auto-start focus time
when you dwell in a "focus category" app. Fruit's equivalent already exists at
the *end* of the day: an observed-only stretch becomes a reconcile item you can
attach to a task. Doing it live is W4's nudge, not a silent write — an app that
invents sessions you did not start is an app whose record you stop trusting.

---

## W4 · Notices

Three notices, one Settings group, each off by default. Bundled deliberately: the
review's warning about an overwhelming settings screen means three switches under
one heading, not three sections.

**Work-hours ceiling.** The one the reviewer names: *"I also like the
notification about work hours, which reminds me when I am overworking."* A
configurable daily cap; fires once when crossed.

**Continuous work.** *(Attributed correctly: this comes from the other summaries,
not from this review — see Appendix A. It is kept on its merits.)* "2h 15m
without a break." The good idea in it is what does **not** count, and in Fruit
that is schema rather than a rule to remember:

- `life_entry` has no accumulator and never counts toward a work streak.
- `activity_span` is observed, not confirmed, and never counts either.
- `time_session` counts — **except** where `contribution = 'attend'`. Sitting in
  a two-hour review is not two hours heads-down, and Fruit already records the
  difference.

**Off-plan.** See W5.

**A notice, never a nag and never a block.** It does not interrupt a timer, it
does not dim the screen, and crossing a threshold twice in a day produces one
notice. An app that talks too much gets muted, and a muted app cannot tell you
the thing that mattered.

---

## W5 · The off-plan nudge — and why it is not blocking

This needs care, because the first draft rejected it outright on a misreading.

The review is unambiguous that this is Rize's most valuable feature in practice:

> *"I find this helpful. Of course, there are false positives — there is no way of
> knowing for Rize when I browse to a social media site to get some info needed to
> continue on a task — but very often, it keeps me from mindlessly scrolling a
> social media feed and, in fact, increases focus time."*

And crucially, **what it actually does is show a window you can dismiss**: *"this
window pops up, and I can decide if it should warn me again during that session
or be ignored."* Rize calls it a "distraction blocker". Functionally, as
described, it is a *notice*.

That distinction decides the scope question. `PRODUCT-SPEC.md` §1 puts **blocking**
out of scope, and it stays out — Fruit will not close a tab, deny a navigation or
interpose on the browser. It cannot, in fact: the connector ships with **no
`host_permissions`**, so it cannot touch a page at all, and that absence *is* the
privacy argument. Adding blocking would require exactly the permission the design
refuses.

But a notice that says *"you are on youtube.com during a block you plotted for the
auth refactor"* blocks nothing, needs no new permission, and is the behaviour the
reviewer values. Fruit is better placed to deliver it than Rize is, because Rize
must guess what counts as a distraction from a category, while **Fruit knows what
you plotted for this hour.** "Entertainment during plotted work" is a sharper
signal than "entertainment", and it produces fewer of the false positives the
review complains about.

Two rules, both taken from the reviewer's own caveat:

- **Dismissible for the session**, exactly as Rize does it — his false-positive
  case is real and common, and a nudge you cannot silence becomes a nudge you
  learn to ignore.
- **Never during unplotted time.** If you did not plan the hour, Fruit has no
  standing to have an opinion about it. This alone removes most of the false
  positives, because the ones he describes are on time nobody had claimed.

---

## W6 · Fragmentation, reported and not scored

**Reframed.** The first draft called this "Rize's focus score, done better".
There is no focus score in this review — that came from a different article
(Appendix A). What *is* in this review is the problem:

> *"I am often involved in client projects operationally, so context and task
> switching are part of my job."*

and the verdict:

> *"If you jump between apps a lot, Rize can be the app that helps you to stay
> sane and focused."*

So the need is real and stated by the user; the score was my invention. Fruit
should measure the thing and report the components.

| Measure | Definition |
|---|---|
| **Longest unbroken stretch** | The longest single run of confirmed work in a day. The figure that actually tracks deep work. |
| **Switches** | Owner changes per day from `resolve_day`, split into **planned** (falling on a block boundary — you meant to switch) and **unplanned**. |
| **Fragmented time** | Confirmed work in segments shorter than a threshold (default 15 minutes). Time that counts and accomplished little. |

**No synthesised 0–100.** This app's argument is that its arithmetic is checkable
by eye — the counting invariant, Excel totals as formulas rather than pasted
numbers, a reconciliation table putting the app's figure beside the sheet's own.
A weighted score would be the one number in Fruit nobody could verify.

**And Fruit can do something Rize structurally cannot.** Rize sees app switches.
Fruit sees the *plan* underneath them: a switch landing on a block boundary is you
executing your intention; one that does not is an interruption. For a reviewer
whose whole complaint is that switching is inherent to his job, the distinction
between *switching because the work requires it* and *switching because you got
pulled away* is the entire question.

**New capability required: none.** All three come from `resolve_day` segments
already on screen.

---

## W7 · Categories you define — **built**

**The standout, and the first draft missed it completely.** Built ahead of the
rest of this plan, on request, in migration 0007. What follows is the design as
it shipped; see `ACCEPTANCE.md` W7/W8 for what is covered.

The review's most enthusiastic passage is not about a report. It is about a
question he had and answered himself:

> *"I wanted to know how much time I spend on AI chat assistants, such as Claude,
> ChatGPT, and others, so I customized the tracking rules. I created a category in
> Rize called 'AI Chat', and all sites and apps are counted in that category."*
>
> *"AI promises to save us so much time, but how much time do you spend in these
> tools? I wanted to find out."*

That is the whole loop: *have a question about your own time → define a bucket →
get an answer.* No report Fruit could ship in advance would have answered it,
because the app cannot know what its user is curious about this month.

Fruit currently cannot do this. `DomainCategory` is a fixed three-value enum —
`core` / `entertainment` / `other` — and life areas classify confirmed time, not
observation. There is nowhere to put "AI Chat".

**The change.** Replace the fixed enum with a table of user-definable
**observation categories**, seeding the existing three so nothing migrates
badly, and let a category collect **both apps and domains** — the review is
explicit that it spans *"all sites and apps"*, which matters because Claude and
ChatGPT are a website to one person and a desktop app to another. A single bucket
that only caught one of them would answer the question wrongly.

```
observation_category
  id, name, colour
  is_builtin        core / entertainment / other seed as builtin
  counts_as         core | entertainment | other     ← keeps existing reporting working
  created_at, updated_at

-- domain_rule gains: category_id  (replacing the enum column)
-- app_rule (new, same shape): app_id → category_id
```

`counts_as` is what keeps this from being a rewrite. The month dashboard, the
entertainment trend and the Day view all key off the three-way split; a custom
category declares which of the three it rolls up into, so *"AI Chat"* can be
reported on its own **and** still land in the right bucket on the dashboard.
Adding a category never breaks a total.

Two constraints from the governing budget: categories are created **from the
uncategorised surface** (W8, below), where the user is already looking at the
thing they want to name, and never from an empty editor in Settings.

---

## W8 · What is not categorised yet — **built**

The review's setup advice, turned into a feature so it does not have to be advice:

> *"Once you have done the basic configuration of Rize, you should look in the
> 'Other' category and what comes up — this is potential for further
> categorization and optimization."*

and, on why it is worth doing:

> *"I recommend spending some time once a week or a month to confirm your gut
> feeling or surprise you — you thought you used LinkedIn only a few minutes a
> day; how can it accumulate to 8 hours over a month?"*

**The feature: a ranked list of the uncategorised, by time, with a one-click
"make this a category" on each row.** It belongs in the weekly review (W9), which
is exactly the *"once a week"* cadence he recommends.

This is the anti-configuration feature, and it is what makes W7 affordable under
the governing constraint above. The app does not present an empty taxonomy and ask you
to populate it; it says *"these three things took eleven hours between them and
have no name"*, which is a question you want to answer. Rize's own surface for
this is a category called "Other" that you have to think to go looking in. Fruit
should put it in front of you once a week and no more often than that.

---

## W9 · The weekly review, and the report

Two things, deliberately paired, because the review shows they are one habit.

### The artifact

> *"Rize sends me a weekly PDF report, and I take a quick look on a Monday
> morning."*
>
> *"While this can all be overwhelming, the most important information is right
> at the top — the Work hours from last week and the breakdown into categories."*

Three design instructions, from a user two years in:

1. **It is read at a fixed moment**, Monday morning — not whenever you happen to
   open the app. So it must exist as something waiting for you.
2. **It is skimmed, not studied.** *"A quick look."*
3. **The headline goes at the top**, and it is total hours plus the category
   breakdown. Everything else is optional depth.

Fruit already writes `.xlsx` with a preview that *is* the sheet. A weekly report
is the same machinery over seven days. Not a PDF and not an email — an offline
app has no mailer, and inventing one would be the first outbound connection in a
product badged OFFLINE. It is a file, and a Monday-morning card in the app that
opens it.

### The review

The reconcile sheet's sibling: reconcile asks about *intervals on a day*, this
asks about *goals over a week*. Same discipline — bounded, keyboard-driven,
deferrable.

1. **Each goal** — target, actual, outcome in plain language.
2. **Fragmentation** — the three W6 numbers, and how each moved against last
   week. Direction of travel is the point; one week's longest stretch in
   isolation says nothing.
3. **The uncategorised** — W8's ranked list, with naming in one click.
4. **The week's biggest divergence** — largest overrun, or largest unplanned run
   of entertainment, with a link that opens that day.
5. **Next week's goals, pre-filled from what happened.**

Point 5 is where this goes past Rize's weekly report, which reports and stops.

**Goal calibration.** Fruit already calibrates *estimates* — trailing 30 days,
median, reported at n ≥ 5 so five samples of noise cannot move it. Goals deserve
the same:

> Deep work: 20h target, 12h median over 3 weeks. A goal you miss every week
> stops being a goal. Try 13h?

Offered, never applied. The user may have a good reason the third week was
atypical, and an app that quietly lowers your ambitions is worse than one that
says nothing.

---

## W10 · Goal templates

Rize ships templates and the reviewer used one — *"What fits best to my work week
is the 6-hour work day, so I enabled that"* — which is the strongest possible
evidence that templates beat a blank goal form. Fruit's should come from its own
outcomes rather than be copied:

| Template | Shape | Where its number comes from |
|---|---|---|
| **The shorter week** | at most | Trailing median work hours, minus 10%. Rize's "6-hour work day", the one the reviewer actually chose. |
| **Cut entertainment** | at most | Trailing 4-week median, minus 20% |
| **Protect deep work** | at least | Trailing median of stretches ≥ 45 min |
| **Sleep** | at least | 8h × applicable nights, from the Sleep/Rest area |
| **Off zero** | at least | Any life area with a target and no time — already the most actionable row on the month dashboard |

**Every template states the number it chose and where it got it.** A template
that opens with an invented round number is one people dismiss, and a goal you
did not believe when you set it is one you will not honour on Thursday. Where
there is not enough history the template says so and asks, rather than guessing —
the same n ≥ 5 discipline the estimate calibration already applies.

---

## Explicitly not building

Rejected on the merits, recorded so the decisions are not relitigated.

| Rize feature | Why not |
|---|---|
| **Blocking** (as opposed to the W5 nudge) | `PRODUCT-SPEC.md` §1. Structurally impossible as designed anyway: the connector has no `host_permissions` and cannot touch a page — that absence is the privacy argument. |
| **Per-URL and per-window-title rules** | Would require storing URLs and titles. The connector reduces every URL to a domain twice over and there is no code path that stores a path. A refused feature, not a missing one. |
| **A single focus score** | See W6. It would be the only unverifiable number in the app. Also: this review never mentions one. |
| **AI descriptions and insights** | Fruit is offline. The honest offline equivalent — auto-filling a session note from the evidence already collected — is a one-line feature, not a section, and can ride along with W3. |
| **Ambient sound / Lo-Fi** | Unrelated to time accounting. Every OS and every phone already does it. |
| **Hourly rates and client billing** | `PRODUCT-SPEC.md` §1 puts finance out of scope. Rize's own reviewer does not use it. |
| **Team dashboards, manager visibility** | Not this product. |
| **Weekly streaks as motivation** | `DayReview.streakDays` exists and is enough. A streak that breaks when you take a holiday punishes rest, in an app whose premise is that rest is time worth recording. |
| **A mobile app** | The review's one structural complaint — *"Rize creates an incomplete picture"* — and Fruit shares it. Out of scope per §1, and the reviewer's own resolution is instructive: *"I treat my smartphone as a poison that needs to be kept away."* |

---

## Build order

Sequenced by what unblocks what, and by the plan's own phase order — Phase 6
(*"reconcile, calibrate, reduce entertainment"*, week 9) sits **before** Phase 7
(Excel, week 10), so goals precede workbook import.

| # | Item | Depends on | Notes |
|---|---|---|---|
| 1 | **`aggregate_range` extraction** + `get_week_review` | — | Pure refactor of `get_month`'s loop. Nothing new is computed; the point is that nothing is computed twice. |
| 2 | **W1 goals + W2 pace** | 1 | **Closes M11.** The entertainment budget is this with `direction = at_most`. |
| 3 | ~~**W7 categories + W8 uncategorised surface**~~ | — | **Built.** Migration 0007, plus a short-observation floor and the fix for the sampler and the connector both billing the same hour. |
| 4 | **W6 fragmentation** | 1 | No new capture. Derived from `resolve_day`. |
| 5 | **W9 weekly review + report** | 2, 3, 4 | Including goal calibration. |
| 6 | **W3 focus sessions** | — | Independent. The `+`-to-extend interaction is the valuable part. |
| 7 | **W4 notices + W5 off-plan nudge** | 3 | Needs categories to know what "off-plan" means. |
| 8 | **W10 templates** | 2, and trailing history | Last: the numbers need weeks of data to be worth offering. |
| 9 | **Workbook import (M13)** | — | Unchanged in scope, after the goals work per the phase order above. |

## Acceptance

Proposed as **W1–W10** in `ACCEPTANCE.md`, all testable in `fruit-core` without a
webview. The one that matters most is **W2**, for the same reason the month
dashboard's "6% accounted" bug mattered: an app that reports the future as a
failure is one whose numbers you learn to discount.

---

## Appendix A · What the second-hand summaries got wrong

The first draft of this plan was written from search-result summaries, which
conflated this review with at least two other Rize articles. Recorded because the
corrections changed the plan, not just its footnotes.

| Claimed | Actually |
|---|---|
| A **focus score** measuring app-switching, sustained stretches and interruptions | Not in this review at all. W6 was reframed: the *problem* is stated by the reviewer (*"context and task switching are part of my job"*), the score was not. |
| **AI-driven break reminders** based on activity levels | Not in this review. The reviewer mentions only the **work-hours** notification. The continuous-work notice in W4 is kept on its merits and now attributed correctly. |
| The distraction blocker is **triggered by context-switch detection** | It triggers on category during a focus session, and it is a **dismissible pop-up**, not a block. This changed W5 from a rejection to a feature. |
| Meetings are excluded from break notifications | Not stated in this review. Kept as a design principle because Fruit's schema makes it free, but no longer presented as Rize's idea. |
| Users want *"more granular categorization"* | The reviewer's actual experience is the opposite — Rize's categorisation is granular enough that **over**-configuration is the risk he warns about. This inverted the governing constraint of the whole plan. |

The last row is the one that mattered. The first draft treated "more
configurability" as the direction of travel. The review argues the reverse, and
W7/W8 are paired the way they are because of it.

## Source

- [2 Years with Rize: Can a Productivity Tracker Change Your Work Habits?](https://freshvanroot.com/blog/rize-productivity-tracker-review/) — Rolf Mistelbacher, Fresh Van Root, 24 March 2025 (updated 12 April 2025). Full text supplied by the user; the host is unreachable from this environment. The post discloses affiliate links to Rize.

---

## Appendix B · W7/W8 as built

Migration 0007, delivered ahead of the rest of this plan. Four requirements, and
three things they turned up.

### What was asked for

1. Label observed time as **Work · Study · Distraction · Life** — and anything
   else the user wants.
2. Label the **site inside the browser**, not just the browser: Instagram as a
   distraction, Coursera as study, Google Docs as work.
3. **YouTube distraction by default, changeable when it was something else.**
4. **Ignore anything under two minutes.**

### The four decisions

**One rule table, not two.** "Instagram is a distraction" is the same statement
whether Instagram is a website or an application, so `activity_rule` carries a
`match_kind` rather than splitting across two tables that would need two
commands and two lists in Settings.

**The site beats the app, always, with no setting.** A rule on `instagram.com`
outranks a rule on `chrome.exe`. The alternative — a browser labelled Work
making every site inside it work — is precisely the failure the connector was
built to fix, so it is not something to leave configurable.

**`counts_as` keeps the arithmetic still.** Every report written before this
keyed off the fixed `core`/`entertainment`/`other` split. A category declares
which of the three it rolls up into, and `activity_span` stores **both** the
specific label and the roll-up. Adding "Study" cannot move the month dashboard's
Entertainment card. There is a test that asserts exactly that.

**Unlabelled stays NULL, never "Other".** *Nobody has said* and *someone decided
it was nothing in particular* are different facts. Collapsing them would empty
the uncategorised list, which is the surface the whole feature is usable from.

### Request 3, and the honest limit

Fruit sees a registrable domain and deliberately never the URL or the page
title, so it **cannot** tell a lecture from a music video. Two mechanisms rather
than a guess:

- `youtube.com` ships as Distraction, because that is the common case;
- `set_span_category` relabels **one interval** without touching the rule, so a
  lecture is a lecture and tomorrow's video is still Distraction.

Clicking any interval on the Activity timeline does it. This is the one place
the product asks the user for something it could not work out itself, and it says
so on screen rather than pretending otherwise.

### Request 4, and why it needed a rule

Deleting short spans alone leaves a thirty-second hole in the middle of two hours
in one editor — and on untimed time that hole reads as **Unaccounted**, which is
a worse lie than the noise it was removing. So:

- a short span flanked by the **same** app-and-domain is absorbed into the run,
  because that is what it was: one stretch with a blip in it;
- anything else is dropped, and the interval falls to whatever else owns it.

**Nothing is deleted.** The floor is applied on read, so raising it to five
minutes and lowering it back recovers every row.

### Two bugs this turned up

**The sampler and the connector were both billing the same hour.** While Chrome
is frontmost, the foreground sampler writes `chrome.exe` and the connector writes
`chrome.exe` on `youtube.com`. Both are correct; both are rows. `resolve_day` was
unaffected — it picks one owner per segment — but every total that walks spans
directly would have counted the hour twice. `dedupe_browser_overlap` subtracts
the domain-bearing intervals from the app-only ones, keeping the remainder,
because Chrome on `chrome://settings` records no domain and that time is real.

**Coalescing looked at the wrong row.** `record_activity` compared each sample
against the single most recently *ended* span. With two sources interleaving,
neither ever matched, every sample became its own twenty-second row — and the new
floor then discarded all of them. The feature would have recorded a full day and
reported nothing. It now looks for the most recent span *describing the same
thing*. This was latent from the moment the connector landed and only became
visible when something depended on span length.

### Each stretch, not just the total

The first cut of the unlabelled list showed one row per app or site — *"chrome ·
8 stretches"* — with a single set of label buttons. That answers "what should
this always be" and refuses the more common question: *which of those eight was
the lecture?*

Each row now carries its stretches, longest first, with times and per-stretch
label buttons. Two verbs on one screen, and the panel says which is which:

- the buttons on the **row** write a rule, and apply from now on;
- the buttons on a **stretch** label that visit only, and touch no rule.

### Which video it was

The connector sends a registrable domain and nothing else — no URL, no page
title. That is the privacy design and it does not change. But Chrome puts the
video's name in its **window title**, and window titles are an existing,
separate, off-by-default switch the user has already been asked about.

So the detail is available, through a channel that was already consented to.
One bug was in the way: `dedupe_browser_overlap` subtracts the app-only span —
the one carrying the title — in favour of the domain span, which has none. It
now carries the title across, choosing the one that covered the most of the
interval. That is not an inference: it is the same application at the same
instant, recorded by the other of two observers.

With titles off, a stretch says which *site* and not which video, and the panel
explains that rather than showing a blank column.

### Editing rules, and typing one in

The first cut let you *make* a rule from the Activity screen and *delete* one in
Settings, and that was it. Two gaps, both blocking:

- **No edit.** Repointing `youtube.com` from Distraction to Study meant deleting
  the rule and making a new one from a stretch you happened to still have. Each
  rule row now carries a category dropdown showing its current value, which *is*
  the edit affordance — there is exactly one editable field on a rule, so hiding
  it behind a pencil would be ceremony.
- **No way to type one in.** Every path to a rule started from something already
  observed, which means **sites were unreachable until the browser extension was
  installed and had run for a day**. Settings now takes `instagram.com →
  Distraction` directly, before the site has ever been seen.

Shipped rules are editable and deletable like any other. A "shipped" rule you
cannot argue with is one that mislabels your month and tells you to live with it.

### Registering the host, on request

The first cut printed two paths and left the user to write a JSON file and an
`HKCU` registry key by hand. The reasoning was that an app which points a browser
at an executable without being asked has done something objectionable.

That reasoning was right and the conclusion was wrong. **Silence was the problem,
not automation.** Someone who cannot face regedit simply never gets domain-level
tracking, so the feature they asked for does not exist for them. A button they
press, having read what it does, is consent.

So Settings → Activity → Browser extension now takes the extension id and writes
the manifest and the per-user registry key itself. Three properties:

- **`HKCU`, never `HKLM`** — a per-user choice must not need administrator
  rights, which would turn a checkbox into a UAC prompt.
- **Every step is reported**, successes and failures alike, including the exact
  `reg.exe` command. A machine that refuses gives the user something to run
  rather than a dead end.
- **`reg.exe`, not a registry crate** — it ships with Windows, needs no `unsafe`,
  and adds no dependency to a crate that cannot be compiled where this was
  written. The command is also readable, which matters for a step that touches
  someone's registry.

The extension id still has to be pasted, and always will: Chrome only assigns it
when the extension is loaded, and `allowed_origins` must name it. The id's shape
is checked (32 letters, `a`–`p`) so a bad paste says so instead of producing a
host that silently never connects.

### Not done

- The Day view's detail panel does not yet offer relabelling; the Activity
  timeline and the unlabelled list both do. Same command either way.
- A row shows its 12 longest stretches. Past that the aggregate is the useful
  object, and a scrolling list of forty is not — that is a day for a rule.
- `counts_as` is fixed after creation. Changing it would move totals on months
  already exported, so correcting a mistake means deleting the category and
  making a new one — which is visible.
- New categories roll up as `other`. Letting the renderer choose would let a new
  label silently change the Work or Entertainment figure the moment it was
  created.
