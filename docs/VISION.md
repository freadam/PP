# Fruit — Vision

**What this document is.** The argument for why Fruit should exist, what it is
trying to become, and what is still missing to get there. `PRODUCT-SPEC.md`
says what to build; this says *why that is worth building* and *what the finished
thing looks like from a distance*. Where the two disagree about a fact, the
specification wins. Where they disagree about a direction, this one is the one
to argue with.

**Status.** Written August 2026, against a build that passes 307 automated
tests and has a working Plan → Track → Reconcile → Calibrate loop end to end.
§6 of this document is the gap list, and it is the part that dates fastest.

---

## 1. The sentence

> **Fruit is a local-first desk instrument that makes an honest account of
> where your day went — including the hours you would rather not look at — and
> costs ninety seconds a day to keep true.**

Everything below is an unpacking of that sentence. The three load-bearing words
are **honest**, **local-first**, and **ninety seconds**.

---

## 2. The problem, stated properly

The user Fruit is built for is not disorganised. They already keep a record —
a monthly Excel workbook with a 24-hour grid per day, life areas down one axis,
targets versus actuals at the bottom. They are *more* rigorous than the market
assumes. The workbook is not the problem.

The problems are:

1. **The record costs too much to keep.** Filling a 24-hour grid by memory at
   the end of a week is an hour of work and produces a document that is roughly
   half fiction, because nobody remembers Tuesday afternoon.
2. **The hours that matter most are the ones least likely to be written down.**
   Nobody voluntarily types "2h 40m — YouTube" into a spreadsheet. The
   unaccounted column is where the truth lives, and a manual system makes it
   the easiest column to leave blank.
3. **Automatic trackers solve (1) and make (2) worse.** They produce a
   beautiful timeline that the user did not author, cannot correct cheaply, and
   therefore does not believe. An unbelieved record does not change behaviour.
4. **Every good tool in this category ships the data off the machine.** For a
   record of literally everything you looked at all day, that is a hard sell —
   and for the user in question, a non-starter.

The gap in the market is the intersection: **automatic capture, user-authored
truth, entirely local.** Nobody is standing there.

---

## 3. What the category actually does

Seven products and reviews were read for this document (sourcing note in §8).
Between them they cover the four live strategies in personal time tracking. All
four are good at something Fruit should learn from, and all four leave the same
thing on the table.

### 3.1 Rize — automatic capture plus a quality score

Rize tracks apps, websites and documents with no timers at all, and layers a
**Focus Quality Score** over the result, computed from more than twenty
attributes of how you work. It categorises into focus work, meetings and
context switching, supports meeting and focus **keywords** (mark "standup" as a
meeting, "interview" as focus), blocks distracting sites during focus sessions,
and mails daily and weekly reports.

Reviewers rate it around 4.1/5 and are consistent about the weaknesses:
**no offline story** ("don't get Rize if you do most of your real work
offline"), no mobile app, activity **processed in Rize's cloud** on AWS, and a
pricing model that grew to four tiers plus an AI credit balance — one reviewer's
line, that "working out whether 500 AI credits covers your month is not a
productivity activity," is the whole critique of the category's drift in one
sentence. It also "falls apart if you need accurate client, project or team
tracking."

**What Fruit takes:** a single legible quality number is worth more than ten
charts. Keyword-driven classification is the cheapest rule a user will
actually write. Automatic capture with zero timers is the right default.

**What Fruit refuses:** the cloud, the credit balance, and a score whose
derivation the user cannot see.

### 3.2 Clockk — reconstructing the past

Clockk's entire pitch is the one Fruit cares most about: **reconstruct precisely
what you did days, weeks, or months later.** It records apps, sites and files,
uses a "deterministic AI" that learns how *you* attribute activity to clients
and projects, and then presents suggested entries in a timeline you accept,
adjust or reject before anything becomes a timesheet. Its **Timesheet
Cheatsheet** is a daily digest of what you spent most time on, explicitly
framed as something you can pull up "to defend your invoice to a client."

**What Fruit takes:** this is the correct shape for reconciliation, and it is
almost exactly what Fruit's reconciler already does — a queue of claims, a
recommendation, and a human verdict. Two things Fruit lacks: a **learned**
suggestion (Clockk watches how you attributed things before and proposes the
same next time), and the framing of the daily digest as **evidence you can
defend**, not just a summary.

### 3.3 Magicflow — deep work, context switching, and the body

Magicflow measures deep work and **context switching**, shows a live flow timer
with distraction warnings, and — the interesting part — **integrates Apple
Health to correlate productivity with sleep and exercise.** Its reports ask
whether last night's sleep explains today's fragmentation.

**What Fruit takes:** the correlation question. Fruit already stores sleep as a
first-class life entry and already computes fragmentation. It has both halves of
Magicflow's headline insight and has never joined them. That is a report waiting
to be written, with no new capture required.

### 3.4 RescueTime Focus — goals, alerts, and automatic intervention

RescueTime's Focus product is the most behaviourally aggressive of the four.
Focus Sessions block distracting sites *and* apps *and* communications at a
chosen **blocking level**, silence notifications, and can play focus music.
Users set unlimited **focus goals** with daily targets, get **instant alerts**
when a distraction pulls them off track, and — the sharpest idea — configure
**alerts that automatically start a Focus Session** when a threshold is crossed:
after thirty minutes on distracting sites, or at the start of every workday.

**What Fruit takes:** the automation trigger. Fruit has thresholds, notices and
focus sessions as three separate things and never wires them together. "After
30 minutes of unplanned entertainment, offer a focus session" is the single
highest-leverage behavioural feature in this whole document, and Fruit already
owns every part of it.

**What Fruit refuses:** blocking. See §4.4.

### 3.5 Tickkl — the billing baseline

Browser-first manual tracking with projects, descriptions, budgets,
forecasting, invoicing and Slack/Jira/Calendar integrations. It is the
professional-services baseline: what a tracker looks like when the output is an
invoice rather than a behaviour change.

**What Fruit takes:** almost nothing, and that is the useful finding. Fruit's
Excel export is its invoice equivalent, and it should not grow an invoicing
module. Knowing which adjacent product you are *not* is worth a section.

### 3.6 The shared blind spot

Every product above is:

- **cloud-first** — the record leaves the machine by design;
- **work-only** — sleep, family, errands and rest are outside the model, so the
  totals never add to twenty-four hours;
- **subscription-priced**, increasingly with usage meters;
- **and none of them reconcile to a document the user already keeps.**

Fruit's position is the complement of all four.

---

## 4. Convictions

These are the beliefs that decide arguments. If one of them turns out to be
wrong, the product changes shape — so they are worth stating where they can be
attacked.

### 4.1 A record you did not author is a record you do not believe

Automatic capture produces *evidence*, never *truth*. The machine saw Chrome;
only you know it was research. Fruit therefore keeps a hard line between
**observed** and **confirmed**, shows both, and never silently promotes one to
the other. Reconciliation is not friction to be optimised away — it is the step
that makes the number yours. The goal is to make it *cheap*, not to remove it.

This is why Fruit will never ship an "AI cleaned up your week for you" button.
It will ship a "here are eleven claims, decide each in one keystroke" queue.

### 4.2 Twenty-four hours, or the totals are lies

Every competitor models the working day. Fruit models the day. Sleep, meals,
the school run and two hours of television are all first-class entries, and
every local date's layers sum to that date's length exactly once — the
**counting invariant**, which is enforced in the core rather than trusted.

This is the structural reason Fruit can answer "where did the month go?" and a
work tracker cannot. It is also what makes the unaccounted column meaningful:
in a work-only model, unaccounted means "not working"; in Fruit it means
"nobody knows", which is a question with an answer.

### 4.3 Local-first is a feature, not a compromise

Fruit makes no network request of any kind. Not for fonts, not for telemetry,
not for crash reports — verified in CI, not promised in a policy. The database
is plain SQLite with a documented schema, sitting in a folder the user owns, and
the export round-trips losslessly with ids preserved.

The reasoning is not ideological. It is that the honest version of this product
requires recording *everything you looked at all day*, and the only version of
that anyone should accept is one that cannot leave the machine. Privacy is what
buys the completeness that makes the record useful.

### 4.4 Visibility, not restriction

RescueTime blocks. Fruit does not, and will not.

A blocker treats the user as an adversary to be contained, which is a bet that
loses the moment they turn it off — and they always turn it off. Fruit's bet is
that a person who can *see*, accurately and without flinching, that they spent
eleven hours on YouTube this month will change that on their own, and the change
will hold because it was theirs. The intervention Fruit ships is the sentence
and the number, delivered at the moment they are still actionable.

The strongest form of this: an entertainment window you *planned* is not a
failure. Two hours of television you plotted on Saturday evening is a plan being
kept. The same two hours unplanned on a Tuesday is the thing this product
exists to surface. Fruit is the only tool in this survey that can tell those
apart, because it is the only one that holds a plan and a record side by side.

### 4.5 The record must be cheaper than the workbook it replaces

The competing product is not Rize. It is the spreadsheet the user already
maintains. If keeping Fruit true costs more than an hour a month, Fruit loses,
however good the reports are.

Hence the ninety-second budget for a day's reconciliation, and hence the Excel
export existing at all: the user does not have to abandon the artefact their
reporting is built on. Fruit earns the right to replace the workbook by first
agreeing to feed it.

---

## 5. What Fruit is not

Naming these prevents a hundred future arguments:

**Not a team tool.** No accounts, no sync, no manager dashboard, no seats.
The moment a second person can see your day, you start editing it for them, and
the record stops being honest. This is the one exclusion that is a *conviction*
rather than a scope decision.

**Not a blocker.** §4.4.

**Not an invoicing system.** The Excel export is the boundary.

**Not a note-taking app, wiki, or Markdown editor.** Notes are plain text
attached to tasks. A rich editor is a different product that eats this one.

**Not a phone app.** The observation Fruit is built on is desk observation.
A mobile companion that cannot see what you did is a widget showing yesterday's
number, and it would cost the local-first guarantee to build.

**Not gamified.** No streaks, no points, no badges, no coach voice. Verdicts
are stated as neutral facts. "1h 51m observed, never confirmed" — not "You can
do better!"

---

## 6. Where it is going

Three horizons. Horizon 1 is the honest MVP; Horizon 2 is where Fruit becomes
better than the tools in §3 at their own game; Horizon 3 is the thing nobody
else can build because they gave up locality.

### Horizon 1 — *The record is true and cheap* (now → release)

The loop works end to end and the client's month closes in Fruit rather than in
Excel. Success is a single measurement: **seven consecutive days on the client's
own machine with ≥90% of waking hours accounted for and under ninety seconds a
day spent doing it.** Nothing in Horizon 2 matters until that number exists.

### Horizon 2 — *The record explains itself* (next)

Fruit stops reporting and starts answering. The month dashboard says *why*
February was worse than January, not just that it was. This is where §3's
lessons land: learned reconciliation suggestions (Clockk), the sleep ↔
fragmentation correlation (Magicflow), threshold-triggered focus (RescueTime),
one legible quality number whose derivation you can open (Rize, done honestly).

### Horizon 3 — *The record changes the plan by itself* (later)

Calibration closes the loop. Fruit has thirty days of drift per project, per
task shape, per time of day. It should be *proposing* next week's plan from
what actually happened, defending each proposal with the history behind it, and
being visibly wrong in a way the user can correct. A planner that learns your
real velocity is something no cloud tracker in §3 attempts, because none of them
hold the plan in the first place.

---

## 7. How we would know it worked

Deliberately few, and mostly not usage metrics — a local-first app cannot
measure its users, and would not want to.

| Question | The measure | Where it comes from |
|---|---|---|
| Is the record true? | ≥90% of waking hours accounted, over 7 consecutive days | The Day view's own unaccounted total |
| Is it cheap? | <90 seconds/day of reconciliation | Timed, on the client's machine |
| Is it believed? | The client stops maintaining the parallel workbook | Ask them |
| Does it change behaviour? | Unplanned entertainment down month over month, with planned entertainment *not* driven to zero | The month dashboard |
| Is it trusted? | Zero network requests, verified per build | `check-ui.mjs` I7 |

The fourth row matters most and is the easiest to get wrong. A product that
drove all entertainment to zero would have failed: the goal is that leisure
becomes **chosen** rather than **defaulted into**. A month with eight hours of
plotted, deliberate television and no unplanned drift is a *success*.

---

## 8. Sourcing note

Seven URLs were provided for this document. **The egress policy on this machine
blocks direct fetches to all seven**, so §3 is built from web-search summaries
of those pages and their surrounding coverage rather than from the pages
themselves. Direct quotes are short and drawn from those summaries; specific
numbers (Rize's 4.1/5, its pricing tiers, RescueTime's blocking levels) should
be re-verified before any of them appear in customer-facing material.

Nothing in §4–§7 depends on a competitor detail being exactly right. The
strategic reads — cloud-first, work-only, subscription-metered, no reconciliation
to an existing artefact — are consistent across every source and are the part
worth acting on.

Sources: [The Process Hacker](https://theprocesshacker.com/blog/rize-review) ·
[Fresh Van Root](https://freshvanroot.com/blog/rize-productivity-tracker-review/) ·
[The Business Dive](https://thebusinessdive.com/rize-review) ·
[Magicflow](https://magicflow.com/) ·
[Clockk](https://clockk.com/product) ·
[Tickkl](https://www.tickkl.com/) ·
[RescueTime Focus Solo](https://www.rescuetime.com/features/focus/solo)

---

# 9. What is missing to make this real

Everything below is **not built**. Each item names the conviction or competitor
lesson it serves, so the list can be cut from the top by argument rather than by
guesswork. Items already tracked in `BACKLOG.md` keep their identifier; new ones
introduced by this document are numbered `V*`.

Ordered by leverage, not by effort.

---

## Tier 0 — Without these, the vision is unproven

These are not features. They are the evidence that the thing works at all, and
nothing below them should be started first.

### V1 · Seven days of real capture on the client's machine
**Serves §6 Horizon 1 · the only measure that matters.**
Every acceptance number that matters — 90% capture, 95% YouTube/Twitch
detection, ninety seconds a day — is measured over a real week on a real
Windows PC with the real browser. The build cannot produce this; only the
client can. Everything else in this document is a hypothesis until it exists.

### A13 · The counting invariant is not tested the way the spec requires
**Serves §4.2 — the belief the whole data model rests on.**
The spec calls for a property test over generated overlapping records proving
that a date's layers sum to its length exactly once. What exists is
example-based tests over hand-written fixtures. The invariant is the load-bearing
claim of §4.2; asserting it on six examples is not asserting it.

### A12 · Five of six performance budgets are unmeasured
**Serves §4.5.** Day view under 100ms, week load, cold start, idle CPU, and
data loss on forced close are all specified and none are measured. The ninety-
second budget is a *latency* claim before it is a UX claim, and a budget nobody
measures degrades in increments nobody notices.

---

## Tier 1 — The vision's distinctive claims, currently unsupported

### V2 · Threshold-triggered focus sessions
**Serves §4.4 · lesson from RescueTime.**
Fruit has entertainment thresholds, it has notices, and it has focus sessions.
They are three unconnected features. The behavioural product is the wire between
them: *after 30 minutes of unplanned entertainment, offer — never force — a
focus session, pre-loaded with the task you were last on.* One rule, one
setting, and it turns a passive tracker into the thing §1 claims Fruit is. This
is the highest-leverage missing item in the document.

### V3 · The daily defensible digest
**Serves §4.1 · lesson from Clockk's Timesheet Cheatsheet.**
Fruit has a Monday-morning weekly report. It has no daily one. Clockk's framing
is the insight: a digest is not a summary, it is *evidence you can defend* —
here is the day, here is what the machine saw, here is what you confirmed, here
is what nobody accounted for. Delivered in-app (no email; §4.3), it is also the
natural home for the ninety-second reconciliation prompt.

### V4 · Learned reconciliation suggestions
**Serves §4.1 · lesson from Clockk's deterministic attribution.**
Today the reconciler recommends by rule. It should recommend by *history*: this
domain, at this hour, on a weekday, was filed as Research nine times out of ten
— so that is the pre-selected choice, with the count shown as its justification.
Strictly local, strictly deterministic, and always visibly explained. This is
the single largest reduction available in the ninety-second budget.

### V5 · Sleep ↔ fragmentation correlation
**Serves §6 Horizon 2 · lesson from Magicflow.**
Fruit stores sleep as a first-class life entry and computes fragmentation per
day. It has never joined them. "Your four most fragmented days this month
followed under six hours of sleep" requires no new capture, no new permission
and no cloud — only a query and a panel. Magicflow needs Apple Health for this;
Fruit already has the data.

### V6 · One legible quality number
**Serves §3.1, done honestly.**
Rize's Focus Quality Score is the right idea executed opaquely — twenty-plus
attributes, no derivation shown. Fruit should ship one number per day, from at
most four inputs (accounted share, plan adherence, fragmentation, unplanned
entertainment), with a panel that shows the arithmetic and lets you disagree
with the weighting. A score you can open is a score you can trust; §4.1 applies
to derived numbers as much as to records.

### A11 · Three of the ten specified slot states are missing
**Serves §4.2.** The Day view renders seven. Until every state the model can
produce is drawable, "the grid is a lens over the model" is not true, and the
counting invariant has states with nowhere to appear.

### A3 · No YouTube/Twitch trend
**Serves §1 · the primary outcome.**
The stated primary outcome is reducing unplanned PC entertainment, and the
month dashboard has no panel dedicated to the two services that dominate it.
The entertainment chart aggregates; the behaviour change needs the specific.

---

## Tier 2 — The record stays cheap as it grows

### A5 · The Day view has no drag and no keyboard editing
**Serves §4.5.** Correcting an interval costs a dialog. At scale — a week's
backlog of reconciliation — that is the difference between ninety seconds and
five minutes. Drag to adjust boundaries; arrow keys to walk and edit rows.

### A10 · Entertainment rules match only apps and domains
**Serves §3.1's keyword lesson.** Rize lets you say "anything titled *standup*
is a meeting". Fruit cannot express a rule about a window title, a project, a
recurring pattern or a time of day. Title-pattern rules are the cheapest
classification a user will actually write, and they are the rules that stop the
reconciler asking the same question every week.

### A7 · Two of the five specified Day filters are missing
Work-contribution and entertainment filters. **Serves §4.4** — "show me only
the unplanned entertainment" is the exact query the primary outcome needs, and
it cannot currently be asked.

### A6 · Day totals do not break out life areas
**Serves §4.2.** The day ledger shows aggregate life time. Per-area totals are
what make the twenty-four-hour model legible on the screen where it is used.

### A8 · No delete-recent for observed activity
**Serves §4.3.** Delete-all exists. "That last hour was private, forget it"
does not. Trust in continuous observation depends on cheap, precise retraction,
not on a nuclear option.

### A4 · Projects have no note and no monthly target
**Serves §6 Horizon 3.** Life areas have monthly targets and appear on the
dashboard's target-vs-actual bars. Projects have neither. Calibration cannot
propose a plan against a target that does not exist.

### A9 · Settings has no Excel group
**Serves §4.5.** Export options are set at export time and not remembered,
so the monthly ritual re-answers the same four questions every month.

---

## Tier 3 — Horizon 3 groundwork

### V7 · Plan proposal from history
**Serves §6 Horizon 3 · the thing no competitor attempts.**
Thirty days of drift per project and per task shape already exist. The planner
should propose next week — "you plot 45m for these and they take 70m; here is a
week that reflects that" — with each proposal defending itself with the history
behind it, and every one of them editable. Calibration currently *reports* a
correction factor and never *applies* it.

### V8 · Recurring-pattern detection
**Serves §4.5 and V4.** Repeating life entries exist but must be declared. The
data to notice "you sleep 23:30–07:00 on weekdays" or "Friday afternoons are
always admin" is already recorded. Proposing the pattern, and letting the user
accept it once, removes a whole category of daily entry.

### V9 · The estimate ladder should learn
**Serves §6 Horizon 3.** Estimates sit on a fixed ladder. Which rung a task
lands on should be informed by what tasks of that shape actually cost — the
narrow, honest version of the "AI suggestion" the category sells, computed
locally from your own history with the sample size shown.

### V10 · A second machine, without a cloud
**Serves §4.3 under pressure.**
The most common real objection to §4.3 is a laptop *and* a desktop. Fruit's
answer today is "two databases". A file-based, user-carried merge — export from
one, import into the other, with the existing id-preserving round-trip and a
real conflict story — keeps the local-first guarantee while answering the
objection. This is the hardest item here and the one most likely to be cut; it
is listed because the objection is real, not because the feature is certain.

---

## What is deliberately absent from this list

Team features, a mobile app, distraction blocking, invoicing, a rich note
editor, cloud sync, telemetry, AI credits, streaks and scoring gamification.
Each is a reasonable product decision for someone else's product. Each is
excluded here by §4 or §5, and the exclusions are the reason Fruit has a shape
at all.

---

## Related documents

| Document | What it holds |
|---|---|
| [`PRODUCT-SPEC.md`](PRODUCT-SPEC.md) | The specification of record. Wins on any factual disagreement. |
| [`BACKLOG.md`](BACKLOG.md) | The A-numbered gaps above, with reproduction detail and acceptance criteria |
| [`ROADMAP.md`](ROADMAP.md) | The 12-week plan versus what is actually delivered, phase by phase |
| [`FRONTEND-PRD.md`](FRONTEND-PRD.md) | Every screen: purpose, data contract, wireframe, states, interactions |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | The three layers and why the split is load-bearing |
| [`ACCEPTANCE.md`](ACCEPTANCE.md) | What is signed off, criterion by criterion, with the test that proves it |
