# Fruit — Product Vision

**Working name:** Fruit  
**Updated:** 26 August 2026  
**Status:** Product direction. This document describes why the product should
exist and the experience it should become; it does not claim that any feature
is already implemented. [`ARCHITECTURE.md`](ARCHITECTURE.md) defines the core
system boundaries and [`PRODUCT-SPEC.md`](PRODUCT-SPEC.md) defines detailed
behavior, priority, and acceptance criteria.

---

## 1. Vision

> **Fruit is a private, offline personal productivity workspace that helps one
> person decide what matters, stay with it, and learn from where their time
> actually went — with every record stored locally and under their control.**

Fruit brings together the two halves that are usually separated:

- the planning and execution loop of a task-focused tool; and
- the automatic observation and useful reflection of a productivity tracker.

The result should feel less like being monitored and more like having an honest
instrument panel for the day and week. Fruit should make work visible without
turning life into a scoreboard.

---

## 2. Why this should exist

Personal productivity information is unusually sensitive. It can reveal which
applications, websites, documents, projects, habits, and distractions occupied
nearly every hour of a person's day. Yet the tools that capture the richest
record are commonly cloud products, while offline task managers usually know
what the user intended to do but not what actually happened.

That creates four recurring problems:

1. **Plans and reality live in different tools.** Tasks, schedules, timers, and
   automatic activity records do not form one coherent account.
2. **Manual records are expensive and incomplete.** Reconstructing a day or
   week from memory takes too long and hides the hours most worth understanding.
3. **Automatic records are not automatically true.** An app can observe that a
   browser was active, but only the user knows whether it was research, admin,
   communication, or distraction.
4. **Reports often judge instead of help.** A score or chart is only useful when
   the user can understand it and act on it without feeling manipulated.

Fruit exists for the intersection the market often misses: **useful automatic
evidence, user-confirmed meaning, practical task execution, and local ownership
of the complete record.**

---

## 3. Who Fruit is for

Fruit is designed first for its owner: a privacy-conscious solo knowledge
worker using a Windows computer who wants to manage projects and tasks, protect
focused time, reduce unplanned digital distraction, and understand work and
life together.

They are willing to review their day, but not to maintain a diary minute by
minute. They value keyboard efficiency, transparent numbers, useful weekly
reflection, and files they can back up or export themselves. They do not need a
team workspace, an employer dashboard, or a motivational coach.

The primary job is:

> **Help me choose what to do, make it easy to do it, and show me enough truth
> about the result that tomorrow's plan becomes more realistic.**

---

## 4. The product promise

Fruit should answer five questions without requiring separate tools:

1. **What matters now?** A clear task and plan for the day.
2. **Am I doing it?** A dependable timer, focus session, and gentle indication
   when activity has drifted from the plan.
3. **Where did the day go?** One timeline combining plans, confirmed work and
   life entries, and optional automatic PC observations without double-counting.
4. **What pattern is emerging?** A readable daily, weekly, and monthly account
   of focus, fragmentation, breaks, categories, projects, life areas, and
   unaccounted time.
5. **What should change next?** Small, explainable adjustments to tasks,
   estimates, goals, and the next plan.

This creates one continuous loop:

```text
PROJECT → TASK → ESTIMATE → SCHEDULE → TRACK → OBSERVE → VERIFY → COMPLETE
                                      │                   │
                                      └─ FOCUS SESSIONS ──┘
```

This task lifecycle is the app's core, not one feature among many:

1. The user creates a project and its tasks.
2. Each task receives an estimated duration and an intended date.
3. The estimate can be placed on the calendar as one or more time blocks, such
   as 10:00–12:00. A block is planned time, not proof of work.
4. Starting the task starts a timer and opens a tracked session; stopping the
   timer closes it. A task may accumulate as many sessions as the work requires.
5. A tracked session can contain zero, one, or several focus/deep-work
   intervals. Focus time is part of the session, never additional time counted
   on top of it.
6. While tracking is enabled, Fruit separately observes foreground applications
   and browser domains. Overlap between a session and Productive-classified activity
   is **observed evidence**; it does not silently become verified work.
7. During daily review, the user inspects the evidence, corrects classifications,
   and explicitly verifies, edits, reassigns, or rejects each session.
8. Completing the task is a separate user decision. A task can be complete even
   when actual time differs from its estimate or schedule.

The core comparison is therefore **estimated vs scheduled vs tracked vs focused
vs observed vs verified**. These measures stay distinct so reports can be both
useful and honest. Each step must make the next one easier. If maintaining the
record becomes a second job, the product has failed.

The task lifecycle sits inside a broader daily ledger. Task sessions explain
work; life entries explain sleep, meals, exercise, relationships, errands,
rest, and deliberate leisure; private time stays private; observation can
temporarily explain what the computer saw; and the remainder stays visibly
unaccounted. Fruit should never turn “nothing recorded” into an assumption.

### 4.1 The product surface

Fruit is one product with four primary destinations and two supporting ones:

| Destination | Primary question | Product responsibility |
|---|---|---|
| **Today** | What should I do now? | Select the day, protect the Top 3, timebox tasks, start or stop work, and notice when tracking was forgotten. |
| **Projects** | What outcomes am I managing? | Create, edit, archive, and review projects; add and organize their tasks; estimate, date, prioritize, schedule, start, and complete work. |
| **Activity** | What did the computer observe? | Show application and website activity as evidence, classify sources or intervals as Productive, Neutral, Distraction, or Private, and manage future rules. |
| **Reports** | What happened across this week or month? | Explain verified time, focus, plan accuracy, project allocation, activity patterns, goals, and review completeness with drill-down to source records. |
| **Review** | What still needs my judgment? | Reconcile tracked sessions, observed evidence, life entries, private time, and gaps into an honest Day Ledger. |
| **Settings** | What may Fruit capture and retain? | Control privacy, activity capture, working hours, categories, rules, retention, backups, exports, and appearance. |

Today is always the default launch surface. Projects, Activity, and Reports are
first-class destinations rather than panels hidden inside Today. Review is
prominent when evidence needs attention but should not interrupt starting work.

The destinations share one record rather than maintaining separate versions of
the truth. A task added in Projects appears in Today when planned there. A timer
started from either surface creates the same kind of session. Activity evidence
can support review without becoming task time automatically. Reports read the
verified record and disclose what remains pending.

---

## 5. The experience Fruit should create

### 5.1 Plan with enough structure, not ceremony

Inspired by Super Productivity, Fruit should make projects, tasks, subtasks,
estimates, priorities, scheduled blocks, timers, Pomodoro-style sessions, and
quick capture work as one lightweight system. Tasks exist to support action and
learning, not to become a complex methodology of their own.

The user should be able to open Fruit and immediately see a realistic answer to
“what now?” The plan remains an intention, never rewritten to pretend it
matched reality.

The operational home should favor action over dashboard browsing:

- a fast capture field that can create a task with project, estimate, date, and
  optional start time without opening a long form;
- task groups such as Today, No date, and Completed;
- estimated totals at group and project level;
- a visible Start action on every open task; and
- a compact Today plan beside the current timer and next scheduled block.

**Fruit opens directly to this Today workspace.** It is the first screen after
launch and the place the user can remain for most of the working day:

```text
┌──────────────────── This week ────────────────────┬── Week allocation ──┐
│ Mon · Tue · Wed · Thu · Fri · Sat · Sun           │ Work / Life / Sleep │
├───────────────────────────────┬──────────────────────────────────────────┤
│ Today's tasks                 │ Today's schedule                         │
│                               │                                          │
│ Top 3                         │ hour-by-hour timeboxing timeline          │
│ □ Task                  Start │ current-time marker                       │
│ □ Task                  Start │ scheduled task blocks                    │
│ □ Task                  Start │ active timer/focus state                 │
│                               │                                          │
│ More for today                │                                          │
│ □ Task                  Start │                                          │
│                               │                                          │
│ Tomorrow preview              │                                          │
└───────────────────────────────┴──────────────────────────────────────────┘
```

The week strip provides orientation and selects the day shown below. On launch,
Today is selected. Each weekday also acts as a memory cue for logging: its
visual intensity reflects how many distinct tasks received tracked time that
day, while a separate check marker and number show how many tasks were
completed. A planned day with no tracked task must look visibly different from
both an empty day and a day with work recorded, making a missed timer easy to
notice without opening a report. Exact counts remain available and the meaning
never depends on colour alone.

The compact allocation graphic shows how the current week is divided among
verified work, life, and sleep/rest while keeping pending, private, and
unaccounted time visible rather than hiding it.

The task column separates the three outcomes the user most wants to protect
from the rest of today's list. Each row supports completion and timer start
without navigating away. The schedule column shows the same selected date as a
vertical timeboxing canvas, including a current-time line and blocks linked to
their tasks. Planning and execution meet on this screen, but planned blocks and
tracked sessions remain different records underneath.

Long-range consistency can be shown as a calm calendar pattern, but Fruit does
not turn it into a streak that the user can “lose.” The useful question is
whether the plan is sustainable, not whether an unbroken chain survives.

The Projects surface is the durable workbench behind Today. It should let the
user move from a portfolio overview into one project without losing the ability
to capture a task quickly. A project page combines a concise project summary,
task groups, progress, estimate and tracked totals, and immediate actions. Open,
scheduled, no-date, and completed tasks remain easy to distinguish; archived
projects leave reports intact. Project administration must never be required to
start a simple task—the Inbox remains a valid capture destination.

### 5.2 Record with the lightest appropriate method

Fruit should combine:

- timers for deliberate work;
- fast manual entries for offline work, meetings, exercise, meals, rest, and
  other life activities;
- optional foreground application, idle, and browser-domain observation; and
- a visible unaccounted state for time the product cannot explain.

Automatic capture is **evidence**, not a verdict. The user can confirm, correct,
split, merge, ignore, make private, or deliberately leave a gap. Corrections
may teach local rules, but Fruit never silently rewrites history.

A running task timer is a deliberate claim that work is happening. Application
and website observations provide supporting or conflicting context. Neither one
replaces the other: tracked time answers what the user intended to work on,
while observed activity answers what the computer could see during that time.

The Activity view should make this distinction visual. A daily app/domain
summary answers “where did the computer day go?”, while a zoomable timeline
shows exactly when each context occurred. Selecting an interval may classify
that interval without changing the global rule; creating a rule for future
occurrences is a separate, explicit action.

Productive and Distraction are the fastest classification choices because they
answer the owner's recurring question, but they are not the only honest states.
Neutral covers necessary context that should not inflate productive time;
Private suppresses sensitive detail; and Unclassified remains visible until the
user decides. A browser can be productive on one domain and distracting on
another, so a website rule overrides a generic browser-app rule. When page-level
capture is explicitly enabled, a page-specific rule can override the domain for
future observations. The interface must always state whether an action labels
only the selected interval or creates a reusable rule.

### 5.3 Reconcile quickly enough to become a habit

The end-of-day review should be a short queue of the few moments that need human
judgment: observed-only time, gaps, overruns, unstarted plans, and uncertain
classifications. Familiar corrections should become faster through transparent,
on-device suggestions.

The design target is about ninety seconds per day after the system has learned
the user's recurring patterns, with five minutes as the practical ceiling. The
goal is not zero interaction; confirmation is what turns observation into a
record the user trusts.

Reconciliation should happen on a 24-hour ledger, not only a list of work
sessions. At any moment, time belongs to exactly one accounting state:

- verified work;
- verified life or sleep/rest;
- intentionally private;
- tracked work awaiting verification;
- observed but not yet confirmed; or
- unaccounted.

A time summary should show how much of the day has been reviewed and how much
still needs attention. “Observed but unconfirmed” must not be presented as
recorded work, and “unaccounted” must not be hidden inside an Other category.

### 5.4 Reflect through a concise personal report

Inspired by Rize's productivity assessments, the weekly review should lead with
a small number of useful conclusions and then allow the user to inspect the
evidence. It may include:

- work hours compared with the user's intended range and recent baseline;
- focused time, average focus-session length, and longest uninterrupted block;
- context switching and fragmented time;
- meetings and breaks, where they can be identified reliably;
- time by user-defined category, project, and life area;
- planned versus unplanned entertainment;
- sleep/rest alongside the next day's focus pattern, shown as correlation rather
  than causation; and
- changes from the previous week or a rolling four-week baseline.

The report should use clear language, compact charts, and comparisons the user
can verify. If Fruit presents a summary score, its inputs and calculation must
be visible; no opaque “AI productivity” number should become the authority on a
person's week.

Reports should follow a deliberate reading order:

1. **What the period says** — one to three plain-language conclusions.
2. **Core totals** — verified work, life, sleep/rest, focus, and review coverage.
3. **Against intention** — estimate vs actual, scheduled vs tracked, goals, and
   the largest divergence from plan.
4. **Shape of time** — work by day and hour, longest uninterrupted stretch,
   short runs, unplanned switches, breaks, and time after the preferred workday.
5. **Allocation** — project, task, work kind, life area, application, and domain.
6. **Evidence and arithmetic** — the records and formulas behind every score or
   headline.

Day, week, and month views should use the same measures and terminology. The
week should also reconcile to its full 168-hour allocation so verified work,
life, sleep/rest, private, tracked-pending, observed-unconfirmed, and
unaccounted time cannot quietly disappear between charts.

### 5.5 Turn insight into a humane next action

Fruit should prefer a useful sentence over another dashboard. Examples:

- “You planned 12 hours for Project A and confirmed 16; similar tasks have
  taken about 1.3× their estimates.”
- “Most unplanned entertainment happened after 21:30, outside scheduled leisure
  time.”
- “Your longest focus sessions occurred on mornings with fewer task switches.”

Interventions remain optional and proportionate. Fruit may offer to begin a
focus session or adjust a plan, but it should not block websites, punish leisure,
or treat the user as an adversary.

Weekly goals should support both directions: **at least** for time the user
wants to protect and **at most** for time the user wants to limit. Pacing should
translate a weekly goal into what remains per day without declaring the week a
failure before it ends.

Estimate calibration should wait for enough completed, verified tasks and show
its sample size. A statement such as “two-hour tasks usually take 1.25× your
estimate” is useful only when the user can open the tasks behind it.

---

## 6. Product principles

### Local is the default and the guarantee

Core functionality must work with the internet disconnected. Tasks, activity
history, reports, settings, and backups are stored in user-owned local files.
Fruit has no account requirement, cloud backend, telemetry, advertising, remote
fonts, or silent network dependency. Privacy controls must act before sensitive
data is stored, not merely hide it later.

### One timeline, with honest layers

Planned time, confirmed time, and observed time have different meanings and
must remain distinguishable. Overlap is resolved without counting the same
minute twice. Empty and private time are explicit states, not missing UI.

### The user is the authority

Fruit can suggest; the user decides. Automation must be reversible, explainable,
and local. A record created or corrected by the user outranks an inference made
by the machine.

### Insight must lead to agency

Every prominent metric should answer a question or support a decision. Fruit
does not optimize for maximum work time. Planned rest, relationships, sleep,
and deliberate leisure are successful uses of time, not defects in a score.

### Calm beats coercion

The interface should be direct, quiet, and factual. No streak anxiety, shame,
badges, social comparison, or artificial urgency. Notices are silenceable;
focus support is invited, not forced.

### Low maintenance is a feature

Fast capture, keyboard access, sensible defaults, and learned local rules matter
more than a long feature list. Fruit must save more effort than it creates.

### The owner's data remains portable

Backups and exports should use documented, durable formats. The user can inspect,
copy, restore, and leave with their data without asking a service for permission.

---

## 7. What Fruit is — and is not

Fruit is:

- a project/task planner built around estimate → schedule → multi-session track
  → review → completion;
- a personal desktop time and focus tracker;
- an optional, privacy-preserving observer of PC activity;
- a 24-hour daily ledger for verified work, life, sleep/rest, private,
  tracked-pending, observed-unconfirmed, and unaccounted time;
- an evidence-led daily reconciliation tool;
- a weekly and monthly reflection tool; and
- a locally owned record that connects intention, action, and learning.

Its essential first-class product capabilities are:

- **project and task management** with estimates, dates, priorities, Top 3,
  schedules, multi-session timers, focus intervals, and explicit completion;
- **local activity evidence** for foreground applications and supported browser
  websites, with interval labels and prospective app/domain/page rules;
- **daily reconciliation** that turns user-approved records into verified time;
- **week and month reporting** with period comparisons, completeness, source
  arithmetic, and drill-down; and
- **local data stewardship** through privacy controls, retention, backup,
  restore, and portable export.

Fruit is not:

- a cloud service, account platform, or subscription dependency;
- a team, employee-monitoring, invoicing, or client-billing product;
- a website blocker or anti-distraction enforcement system;
- a general notes app, wiki, document manager, or calendar replacement;
- a promise to automate every hour or decide what a “productive” life means;
- a mobile companion at the cost of the offline/local guarantee; or
- a system that sends personal activity to an external AI service.

These boundaries protect the product's central advantage: it can hold an honest,
detailed record because the record does not have to leave the user's control.

---

## 8. Product horizons

### Horizon 1 — A trustworthy daily loop

Prove that one person can plan tasks, track deliberate work, capture enough
automatic evidence, record non-work life time, reconcile the full 24-hour day
quickly, and recover or export the record without an internet connection.

The defining experience is seven consecutive real days in which the record is
useful, accurate enough to believe, and inexpensive enough to maintain.

Horizon 1 includes the complete Projects workbench and a usable Activity page.
The owner must be able to create and manage real projects, classify the apps and
websites that appear during the day, and see those decisions reflected in review
without any network dependency.

### Horizon 2 — A weekly review worth returning to

Turn the daily record into a Rize-inspired assessment that explains the week in
plain language, distinguishes planned from unplanned time, and connects the
findings directly to next week's tasks, estimates, and goals.

The weekly allocation must reconcile to the full period, and every score or
headline must expose its arithmetic and source records.

The report should become the reason to keep the daily record current—not a
collection of charts that is admired once and ignored.

Horizon 2 delivers both week and month report modes. The monthly view emphasizes
allocation, trends, consistency, completed outcomes, and estimate calibration;
the weekly view emphasizes actionability, plan adherence, focus, fragmentation,
and the next adjustment.

### Horizon 3 — A planner that learns locally

Use confirmed history to offer transparent, on-device suggestions: more
realistic estimates, likely classifications, recurring patterns, better timing
for demanding work, and proposed adjustments to the next plan.

Fruit should show the evidence behind every suggestion and remain useful when
the suggestion is rejected. “Learning” must never weaken user control or create
a network dependency.

---

## 9. How success will be judged

Because Fruit is personal and has no telemetry, success is measured inside the
product and by the owner's experience—not by engagement analytics.

| Question | Initial signal |
|---|---|
| Is the product dependable? | It works fully offline, survives restart, and restores from a local backup without unexplained loss. |
| Is capture useful? | At least 90% of active PC time is observed when optional activity capture is enabled. |
| Is the record honest? | At least 80% of waking time is confirmed, classified, made private, or deliberately left unaccounted during a seven-day trial. |
| Is upkeep light? | Daily reconciliation reaches a typical 90 seconds and stays below five minutes. |
| Do the numbers reconcile? | Day, week, month, backup, and export totals agree without double-counting. |
| Is uncertainty visible? | Pending observation and unaccounted time are shown separately from verified work and life. |
| Does reflection help? | The weekly review produces at least one concrete change the owner accepts for the next week. |
| Does behaviour improve? | Unplanned entertainment or unwanted fragmentation trends downward without driving planned rest or leisure toward zero. |
| Is it trusted? | The owner is comfortable keeping the detailed record because no core data leaves the machine. |

The strongest product signal is simple: Fruit becomes the one place the owner
uses to plan the day, understand the week, and maintain the personal time record—
without continuing a parallel system out of distrust.

---

## 10. Decision filter

When considering a feature, ask:

1. Does it strengthen the Plan → Do → Observe → Reconcile → Reflect loop?
2. Does it reduce unplanned distraction or improve the realism of the next plan?
3. Can it work completely offline with local, user-owned data?
4. Is its output understandable and correctable by the user?
5. Does it keep daily maintenance within the time budget?
6. Is it still appropriate for a single person rather than a team or platform?

If the answer to several of these is no, the feature probably belongs in a
different product.

---

## 11. Inspiration, not imitation

Fruit borrows selectively from two reference points:

- **Rize:** automatic activity evidence, focus and break analysis, category and
  project breakdowns, recent-baseline comparisons, and a readable weekly
  assessment.
- **Super Productivity:** project-based tasks, estimates, timers, scheduling,
  focus/Pomodoro support, and a practical daily execution loop.

Fruit's distinct position is the combination: these capabilities serve one
offline, local-only personal system that includes both work and life, separates
plans from records, and lets the user author the final truth.

The supplied interface references add six useful patterns to that direction:

- an action-first Today surface with quick capture and start controls;
- project task lists grouped by date and completion state;
- a timeboxing canvas paired with an accounting summary;
- an application/domain timeline where interval correction is distinct from a
  future rule;
- conclusion-first day/week/month reports with visible arithmetic; and
- goal pacing, fragmentation, and estimate calibration backed by source data.

Fruit adopts those patterns, not the reference products' assumptions. In
particular, streak pressure, unexplained scores, automatic confirmation, and
cloud dependence remain outside the vision.
