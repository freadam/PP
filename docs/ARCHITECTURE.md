# Fruit — Core Product Architecture

**Updated:** 26 August 2026  
**Status:** Target architecture and implementation contract  
**Scope:** Complete product flow, domain model, page boundaries, state
transitions, evidence semantics, reporting projections, and local-only system
boundaries. [`PRODUCT-SPEC.md`](PRODUCT-SPEC.md) defines detailed behavior and
acceptance criteria.

---

## 1. Architecture goal

Fruit is built around one complete task lifecycle:

```text
Create project
    ↓
Create task
    ↓
Estimate effort and choose an intended date
    ↓
Place one or more time blocks on the calendar
    ↓
Start the task timer
    ↓
Accumulate one or more tracked sessions
    ├── optionally mark intervals as focus/deep work
    └── collect app and browser-domain observations in parallel
    ↓
Review and verify the session evidence
    ↓
Explicitly complete the task
    ↓
Use estimate, schedule, actual, focus, and evidence in reports and future plans
```

The architecture must preserve the meaning of every stage. A two-hour estimate,
a 10:00–12:00 calendar block, a two-hour running timer, and two hours of a work
application being visible are related facts, but they are not the same fact.

The task lifecycle is joined by a full-day accounting projection:

```text
VERIFIED WORK + VERIFIED LIFE/SLEEP + PRIVATE + TRACKED, PENDING
    + OBSERVED, UNCONFIRMED + UNACCOUNTED
    = THE COMPLETE LOCAL DAY
```

The projection does not force the user to describe every private hour. It makes
uncertainty explicit and prevents activity evidence, empty time, and confirmed
records from being mixed into one misleading total.

---

## 2. Core vocabulary

| Term | Meaning | Created by |
|---|---|---|
| **Estimate** | Expected effort required to finish a task | User |
| **Intended date** | The date on which the user currently expects to work on the task | User |
| **Daily plan item** | A task's placement and rank in one selected day's Top 3 or additional list | User |
| **Scheduled block** | A planned start and end assigned to a task | User |
| **Tracked session** | An interval during which the task timer was running | User starting/stopping the timer |
| **Focus interval** | A deep-work interval contained within a tracked session | User or an explicit focus command |
| **Observation** | Foreground application, browser domain, idle, or private context captured during time | Local observer |
| **Classification** | The meaning assigned to an observation: Productive, Communication, Neutral, Distraction, or Private | Local rule or user |
| **Source rule** | A prospective classification rule for an application, domain, or opted-in page identity | User |
| **Observed session** | A tracked session with overlapping machine evidence relevant to work | Derived by the app |
| **Verified session** | A tracked session the user has reviewed and explicitly accepted, with any necessary corrections | User |
| **Tracked, pending** | A closed, non-voided task session that has not yet been verified | Derived by the app |
| **Completed task** | A task whose intended outcome the user declares finished | User |
| **Life entry** | Confirmed non-task time such as sleep, exercise, meals, family, errands, or deliberate leisure | User |
| **Day review** | The user's statement that a local day has been reconciled as far as intended | User |
| **Observed, unconfirmed** | Machine-observed time not yet converted to verified work/life or made private | Derived by the app |
| **Unaccounted** | Time with no confirmed record and no retained observation | Derived by the app |
| **Directed goal** | A weekly or monthly target expressed as at least or at most a duration/count | User |

“Observed” and “verified” are deliberately different:

- **Observed** means Fruit has machine evidence for at least part of the interval.
- **Verified** means the user reviewed the interval and accepted it as an
  accurate task record.

Classification can help Fruit prepare a recommendation, but it can never verify
a task session automatically.

---

## 3. Canonical user flow

### 3.0 Open into Today

The default route after launch is the Today workspace. It composes planning,
execution, and a small amount of weekly context without requiring the user to
visit a reporting dashboard before starting work.

```text
┌────────────────────── Week strip ──────────────────────┬─ Week allocation ─┐
│ Mon  Tue  Wed  Thu  Fri  Sat  Sun                     │ donut + exact totals│
├───────────────────────────────────┬─────────────────────────────────────────┤
│ TASKS FOR SELECTED DAY            │ SCHEDULE FOR SELECTED DAY               │
│                                   │                                         │
│ Top 3                             │ vertical time grid                      │
│ □ task · estimate · Start         │ scheduled blocks linked to tasks        │
│ □ task · estimate · Start         │ current-time marker                     │
│ □ task · estimate · Start         │ active task/focus state                 │
│                                   │                                         │
│ More for today                    │                                         │
│ □ task · estimate · Start         │                                         │
│                                   │                                         │
│ Tomorrow preview                  │                                         │
└───────────────────────────────────┴─────────────────────────────────────────┘
```

Launch behavior:

1. The selected date defaults to the current local date.
2. The week strip shows the seven-day week containing the selected date and
   allows any day to be selected without leaving the workspace. Every day cell
   also shows a tracking-memory cue: fill intensity represents distinct tasks
   tracked, a separate check marker shows tasks completed, and a hollow pending
   marker identifies planned tasks with no tracked session.
3. The weekly allocation summarizes the exact Day Ledger states. Work, life,
   and sleep/rest are prominent; private, tracked-pending,
   observed-unconfirmed, and unaccounted time remain visible in muted segments
   or the adjacent legend.
4. The task column shows the selected date's Top 3, additional planned tasks,
   and a small next-day preview. Completed tasks remain available without
   dominating the working list.
5. The schedule column shows the same selected date as a vertical timeboxing
   canvas. It scrolls through the complete local day, opens near the user's
   configured day start or current time, and keeps a current-time marker visible
   when viewing today.
6. Any recovered running task session appears immediately in the workspace with
   its timer and focus state.

Primary interactions stay in place:

- select a day from the week strip;
- promote, demote, or reorder the Top 3;
- complete or reopen a task;
- start a task session from its row or scheduled block;
- create, move, resize, or remove a scheduled block;
- drag a task to the schedule, with a keyboard equivalent; and
- open the full task, expanded schedule, Daily Review, or Reports only when more
  detail is needed.

### 3.1 Create a project and tasks

The user creates a project, then adds the tasks that contribute to it. A task
starts open and may exist without a date, estimate, or schedule so quick capture
remains cheap.

Minimum project data:

- name;
- optional colour and description; and
- active or archived state.

Minimum task data:

- project;
- title;
- status;
- estimate in minutes;
- intended date; and
- optional priority, notes, and subtasks.

### 3.2 Estimate and schedule

The user estimates the task's total effort, then can place that effort on the
calendar. A task may have:

- no scheduled block;
- one block matching the estimate; or
- several blocks that divide the work across sessions or days.

The application displays the difference between estimated and scheduled time
but does not force them to match. Moving or resizing a block changes the plan,
not the estimate and never the tracked record.

Example:

```text
Task: Draft project proposal
Estimate: 2 hours
Intended date: 2026-08-27
Scheduled block: 10:00–12:00
```

### 3.3 Start and track work

Clicking the task's Start action opens a tracked session and makes the task the
active task. The timer records elapsed time independently of the scheduled
block, so starting late, stopping early, or working outside the plan remains
visible.

Stopping the timer closes the session. Starting the same task again creates a
new session; it does not extend or rewrite the earlier one.

The initial architecture permits only one running task session at a time. If
the user starts another task, Fruit asks to stop the current session and start
the new one. This prevents ambiguous double-counting.

### 3.4 Track focus inside the task session

The user may begin a focus/deep-work interval while a task session is running.
A session may contain zero, one, or several focus intervals.

```text
Tracked session  10:07 ───────────────────────────── 11:42
Focus interval          10:15 ─────────── 10:58
Focus interval                                11:10 ─ 11:35
```

Focus time is nested within tracked time:

```text
0 ≤ focus duration ≤ tracked-session duration
```

It is never added to session duration. Stopping a task session automatically
stops any open focus interval.

### 3.5 Observe applications and websites in parallel

When activity tracking is enabled, a local background observer records the
foreground application. When a supported browser is foreground and the local
browser connector is enabled, the same observation may also contain its active
domain.

```text
Task session:     10:07 ───────────────────────────── 11:42
Observed context: VS Code ───── Browser/docs ─ Slack ─ VS Code
Classification:  Productive ─── Productive ─── Comms ─ Productive
```

Observations are not attached permanently to whichever timer happened to be
running. They remain independent time spans and are related to sessions by
overlap. This makes corrections possible and preserves observations made when
no task timer was active.

### 3.6 Review and verify

At the end of the day or the next day, the review screen aligns four lanes:

1. scheduled task blocks;
2. tracked task sessions and focus intervals;
3. observed applications/domains; and
4. classifications and coverage gaps.

For each pending session, Fruit calculates a recommendation and shows its
evidence. The user can:

- verify the session as recorded;
- change its start or end;
- split it;
- reassign all or part to another task;
- classify an observation for this interval;
- create an optional future rule for that application or domain;
- mark part of the observation private;
- record an offline/unobserved explanation; or
- void an accidental timer session.

After corrections, the user explicitly chooses **Verify**. Verification records
the decision and time of review. A session can be verified even if the computer
could not observe it—for example, work performed on paper or in a meeting.

### 3.7 Complete the task

The user marks the task complete when its outcome is finished. Completion is
not inferred from any of the following:

- the estimate being reached;
- all scheduled blocks ending;
- a certain amount of tracked time;
- full observed coverage; or
- every session being verified.

Fruit may warn about pending sessions before completion, but it must allow the
user to complete the task. Reopening a completed task preserves all estimates,
blocks, sessions, observations, and reviews.

### 3.8 Account for the rest of the day

Task sessions explain work but cannot explain sleep, exercise, meals, family,
errands, rest, or deliberate leisure. The user records these as life entries
against user-defined life areas. A life entry is confirmed actual time; it is
not a task and does not need an estimate, timer, or completion checkbox.

The Day view projects every interval into exactly one accounting state:

```text
verified work
verified life or sleep/rest
intentionally private
tracked work awaiting verification
observed but unconfirmed
unaccounted
```

If a life entry conflicts with a task session, Fruit asks the user to resolve
the overlap. It does not choose silently. Observations may enrich either record
but do not add duration on top of it.

When the review queue is resolved as far as the user intends, they can close the
day review. Closing a review records its coverage and remaining deliberate gaps;
it does not fabricate entries for them. Later edits reopen the affected day.

### 3.9 Reflect and adjust

Day, week, and month reports are generated from the same interval projection.
They begin with plain-language conclusions, then expose their arithmetic and
source records.

The user may create directed goals such as:

```text
Work: at least 20h per week
Focus: at least 8h per week
Unplanned entertainment: at most 5h per week
Sleep/rest: at least 49h per week
```

Pacing shows what remains over the remaining days. Estimate calibration uses
only completed tasks with verified sessions and displays the sample size before
offering an adjustment.

---

## 4. Domain model

```text
WORK RECORDS                              LIFE RECORDS
┌─────────┐ 1  * ┌──────┐               ┌───────────┐ 1  * ┌──────────┐
│ Project │──────│ Task │               │ Life Area │──────│Life Entry│
└─────────┘      └──┬───┘               └───────────┘      └──────────┘
                    │
          ┌─────────┴──────────┐
          │                    │
┌─────────▼────────┐  ┌────────▼───────┐ 1  * ┌──────────────┐
│ Scheduled Block │  │  Task Session  │──────│Focus Interval│
│ planned time    │  │  tracked time  │      │ nested time  │
└──────────────────┘  └────────┬───────┘      └──────────────┘
                               │ 1  0..*
                       ┌───────▼───────┐
                       │Session Review│
                       └───────────────┘

MACHINE EVIDENCE                         USER INTENT & REVIEW
┌──────────────────┐                    ┌───────────────────┐
│ Observation Span │◀──classification──│Classification Rule│
│ app + domain     │                    │ prospective       │
└────────┬─────────┘                    └───────────────────┘
         │                               ┌──────────┐ ┌─────────────┐
         └── Observation Review          │Day Review│ │Directed Goal│
                                         └──────────┘ └─────────────┘

Task sessions, life entries, private intervals, and observations are joined by
time overlap in the Day Ledger projection. No projection takes ownership of or
rewrites its source records.

```text
Task 1 ─── * Daily Plan Item ─── 1 Local Date
```
```

### 4.1 Project

Owns task organization but does not own time directly. Project totals are
derived from its tasks and their sessions.

Key fields:

```text
id, name, description, colour, status, sort_order,
created_at, updated_at, archived_at
```

### 4.2 Task

Represents an outcome to complete.

Key fields:

```text
id, project_id, parent_task_id, title, notes, status,
estimate_sec, intended_local_date, priority,
created_at, completed_at, archived_at
```

The task stores one current estimate. If estimate history becomes important,
changes should be recorded separately rather than overwriting the ability to
compare the original estimate with actual time.

### 4.3 Scheduled block

Represents planned calendar time.

Key fields:

```text
id, task_id, starts_at, ends_at, local_date, timezone,
created_at, updated_at
```

A block may optionally be associated with the session started from it, but the
session is not constrained to the block's time.

### 4.4 Task session

Represents tracked actual time for one task.

Key fields:

```text
id, task_id, scheduled_block_id?, started_at, ended_at?,
work_kind_id?, source, review_status, reviewed_at?, voided_at?, created_at
```

`ended_at = null` identifies the one running session. Duration is derived from
timestamps, with paused periods represented explicitly if pause/resume is later
supported.

### 4.5 Focus interval

Represents deliberate deep work inside a task session.

Key fields:

```text
id, task_session_id, started_at, ended_at?, interruption_reason?
```

The interval must be fully contained by its parent session after the session is
closed.

### 4.6 Observation span

Represents locally observed computer context.

Key fields:

```text
id, started_at, ended_at, application_id,
website_id?, page_identity_id?, activity_state,
classification_id?, classification_source, created_at
```

`activity_state` distinguishes active, idle, and private/untracked boundaries.
Full URLs, page contents, and browser history are not required. Window titles
should be a separate opt-in because they can contain sensitive content.

### 4.6a Application identity

Represents the stable local identity of an observed executable without treating
each process instance as a different source.

```text
id, executable_key, display_name, publisher?, icon_cache_key?,
first_seen_at, last_seen_at
```

The executable key is normalized locally. Paths that contain a username or
other sensitive directory segments are not exposed in reports.

### 4.6b Website and page identity

A website identity represents a registrable domain such as `github.com`. An
optional page identity may be retained only when page-level capture is enabled.

```text
Website: id, registrable_domain, display_name?, favicon_cache_key?,
         first_seen_at, last_seen_at

PageIdentity: id, website_id, normalized_match_key, display_label?,
              capture_level, first_seen_at, last_seen_at
```

The default capture level is domain only. Full URLs, query strings, fragments,
page contents, form values, and browser history are not stored. A page match key
must remove queries and fragments before persistence and is disabled by default.

### 4.7 Classification rule

Maps an application, domain, or opted-in page identity to a classification for
future observations.

Key fields:

```text
id, match_type, match_value, classification_id,
specificity, effective_from, enabled, created_at, updated_at
```

Creating a rule during review does not silently rewrite older observations.
The user may separately choose to apply it to a selected historical interval.

### 4.8 Session review

Records the user's verification decision.

Key fields:

```text
id, task_session_id, supersedes_review_id?, decision, reviewed_at,
observed_work_sec, observed_other_sec, idle_sec, unobserved_sec,
note?
```

Coverage fields are an audit snapshot of what the user saw when verifying. The
underlying observations remain available, and current reports can clearly
distinguish review-time evidence from later reclassification. A new decision
supersedes rather than deletes the prior review.

### 4.9 Life area

Defines a user-owned category for non-task time, such as Sleep/Rest, Wellbeing,
Family, Personal Development, Errands, or Fun.

Key fields:

```text
id, name, kind, colour, active, created_at, archived_at?
```

`kind` provides a stable report roll-up while allowing the user to name and
organize life areas freely.

### 4.10 Life entry

Represents confirmed non-task actual time.

Key fields:

```text
id, life_area_id, label?, started_at, ended_at,
local_date, timezone, source, reviewed_at?, created_at
```

A manually created life entry is user-confirmed at creation. Fruit may propose
a life entry from observation, but no entry is created until the user accepts it.

### 4.11 Private interval

Marks time the user intentionally excludes without storing application, domain,
task, or life detail.

Key fields:

```text
id, started_at, ended_at, local_date, timezone, created_at
```

Private intervals contribute only duration to the day ledger.

### 4.12 Day review

Records the state and snapshot of one local day's reconciliation.

Key fields:

```text
id, local_date, timezone, status, closed_at?,
verified_work_sec, verified_life_sec, private_sec,
observed_unconfirmed_sec, unaccounted_sec, reviewed_gap_sec
```

The stored summary is an audit snapshot; authoritative totals remain derived
from source intervals. Any later edit touching that day sets the review back to
open.

### 4.13 Directed goal

Represents a target whose direction is part of its meaning.

Key fields:

```text
id, metric, scope_type, scope_id?, direction,
target_value, period, starts_on, ends_on?, active
```

`direction` is `at_least` or `at_most`. A goal can target verified work, focus,
a project, a life area, an observation category, review coverage, or another
explicitly supported metric.

### 4.14 Work kind

Optionally describes the user's mode or contribution during a task session,
such as Own, Support, Attend, or Assist.

Key fields:

```text
id, name, colour, active, created_at, archived_at?
```

The field belongs to the session because one task may involve different kinds
of work across different sessions. A task may provide a default that the user
can override when reviewing a session.

### 4.15 Daily plan item

Places a task in one local day's ordered working list without changing the task,
its estimate, or its scheduled blocks.

Key fields:

```text
id, task_id, local_date, section, rank, created_at, updated_at
```

`section` is `top` or `more`. At most three active items may be in `top` for a
date. A task may appear only once per local date. The Tomorrow preview is a
projection of the next date's daily plan items, not a second copy of the task.

Tasks with an intended date or scheduled block on the selected date but no
explicit daily plan item appear in **More for today** with a deterministic
default order. Promoting or reordering one creates or updates its daily plan
item.

---

## 5. State models

### 5.1 Task state

```text
OPEN ──start first session──▶ IN_PROGRESS ──complete──▶ COMPLETED
  │                               │                       │
  └────────complete───────────────┘                       │
                                                          └──reopen──▶ IN_PROGRESS

Any non-archived state ──archive──▶ ARCHIVED
```

Scheduling is not a task state. An open or in-progress task can have zero or
many blocks, and a completed task retains its blocks for history.

### 5.2 Session state

```text
RUNNING ──stop──▶ CLOSED / PENDING REVIEW ──verify──▶ VERIFIED
   │                         │
   └──recover after crash────┘
                             ├──edit/split/reassign──▶ PENDING REVIEW
                             └──void─────────────────▶ VOIDED
```

Editing a previously verified session returns the changed result to pending
review unless the edit and re-verification happen in one explicit operation.

### 5.3 Focus state

```text
INACTIVE ──start focus during running task──▶ ACTIVE
ACTIVE ──stop focus or stop task session────▶ CLOSED
```

Focus cannot run without a task session in the initial product.

### 5.4 Observation classification state

```text
UNCLASSIFIED ──local rule──▶ RULE-CLASSIFIED
UNCLASSIFIED ──user review─▶ USER-CLASSIFIED
RULE-CLASSIFIED ──user correction─▶ USER-CLASSIFIED
Any state ──privacy action─▶ PRIVATE
```

User classification outranks a rule for the selected interval.

Classification precedence is deterministic:

```text
interval override > page rule > domain rule > application rule > unclassified
```

A website rule therefore overrides the generic browser application rule. A
Productive or Distraction quick action can either label the selected interval or
create a future source rule, but the command and confirmation text must make the
scope explicit.

### 5.5 Day review state

```text
OPEN ──resolve or deliberately accept remaining questions──▶ CLOSED
  ▲                                                            │
  └────────edit a source interval touching this day─────────────┘
```

Closing a day does not require zero unaccounted time. It requires that the user
has seen the remaining gaps and chosen to leave them. Reports distinguish a
closed day with deliberate gaps from a day that has never been reviewed.

---

## 6. Time and evidence invariants

These rules belong in the domain core and must be protected by automated tests.

1. All intervals use half-open boundaries: `[start, end)`.
2. A closed interval must have `end > start`.
3. Only one task session may be running at a time.
4. A task can have multiple sessions across any number of dates.
5. Scheduled blocks never become sessions automatically.
6. Moving a scheduled block never changes a session.
7. Focus intervals must be contained within their task session and may not
   overlap each other.
8. Focus duration is a subset of tracked duration and is never added to it.
9. Observation spans do not become task sessions.
10. App and browser context describing the same foreground interval count as
    one observation span, not two durations.
11. Overlap associates observations with a session at query time; it does not
    transfer ownership of the observation to the task.
12. A classification may produce an observed-work recommendation but never a
    verified session.
13. Only an explicit user action can set a session to verified.
14. Only an explicit user action can set a task to completed.
15. Voided sessions do not contribute to tracked totals but remain recoverable
    for audit and undo.
16. Stored timestamps use UTC; the originating timezone and local calendar date
    are retained for day/week grouping.
17. Verified task sessions, verified life entries, and private intervals are
    mutually exclusive in the Day Ledger projection. Source overlap creates a
    review conflict and never double-counts.
18. After overlap resolution, verified work, verified life/sleep, private,
    tracked-pending, observed-unconfirmed, and unaccounted durations sum to
    exactly the local day's length, including 23- or 25-hour daylight-saving
    days.
19. Tracked-pending time fills closed, non-voided task sessions not yet verified.
    Observed-unconfirmed fills only the remaining intervals with retained
    observations. Unaccounted time is the final complement.
20. Closing a day records that it was reviewed; it does not change unaccounted
    time into another category.
21. Editing a session, life entry, private interval, or classification reopens
    every closed day review touched by that edit.
22. A goal's direction is immutable for historical evaluation. Changing from
    at-least to at-most creates a new goal version.
23. A task may have at most one daily plan item per local date.
24. A local date may have at most three active daily plan items in the Top 3.
25. Moving, promoting, or removing a daily plan item never changes the task's
    estimate, scheduled blocks, sessions, or completion history.
26. The Today workspace's task column and schedule column always use the same
    selected local date.
27. A weekday's tracked-task count uses distinct task ids, not session count or
    raw duration; multiple sessions on one task still count as one tracked task.
28. Observation without a task session never increments the tracked-task count.
29. Tracked, verified, and completed remain independent: the weekday strip may
    show any valid combination and must not infer one from another.
30. Today, selected-day, tracked-volume, planned-untracked, and completion states
    each have independent non-colour indicators.

### 6.1 Derived measures

| Measure | Calculation |
|---|---|
| Estimated time | Current task estimate, with estimate history when available |
| Scheduled time | Union or sum of the task's valid scheduled blocks, with overlaps flagged |
| Tracked time | Sum of closed, non-voided task sessions |
| Verified time | Sum of sessions explicitly verified by the user |
| Pending time | Tracked time not yet verified |
| Focus time | Sum of valid focus intervals inside task sessions |
| Observed work coverage | Union of Productive-classified observation overlap inside a session |
| Observed other coverage | Union of non-work/communication/neutral observation overlap inside a session |
| Unobserved time | Session duration not covered by an active observation, after idle/private handling |
| Estimate variance | Verified time minus estimate, while pending time is shown separately |
| Schedule variance | Actual session boundaries compared with associated or nearest scheduled blocks |
| Verified life time | Union of confirmed life entries after overlap resolution |
| Tracked-pending time | Union of closed, non-voided task sessions awaiting explicit verification |
| Observed-unconfirmed time | Observation coverage remaining after verified and private records take precedence |
| Unaccounted time | The complement of verified, private, and retained observed time within the local period |
| Accounting coverage | `(verified work + verified life/sleep + private) / period duration` |
| Review coverage | Portion of the period belonging to closed day reviews, reported separately from accounting coverage |
| Goal pace | Remaining target divided across remaining eligible days, respecting at-least/at-most direction |
| Estimate calibration | Median verified-time/estimate ratio for completed tasks in a comparable estimate band, with sample size |

Reports must label these measures rather than collapsing them into one “actual”
number without explanation.

---

## 7. Review and verification logic

### 7.1 Recommendation, not decision

For a pending session, Fruit may calculate:

```text
work_coverage_ratio = observed_work_overlap / session_duration
other_coverage_ratio = observed_other_overlap / session_duration
idle_ratio = idle_overlap / session_duration
unobserved_ratio = remaining_duration / session_duration
```

It can then display a neutral recommendation such as:

- **Strong supporting evidence** — most observed time matches work categories.
- **Mixed evidence** — work and other categories both overlap the session.
- **Little machine evidence** — much of the session was idle, private, or
  unobserved.
- **Conflicting evidence** — most observed time is classified as distraction or
  unrelated activity.

No threshold changes the session to verified. The recommendation only orders
the review queue and explains what needs attention.

### 7.2 Classification during review

When the user says that an application or domain was work during a session,
Fruit offers two distinct actions:

1. **Classify this interval** — correct only the selected historical evidence.
2. **Classify future activity like this** — additionally create a prospective
   application or domain rule.

Keeping these actions separate avoids rewriting history by surprise.

### 7.3 Verification granularity

The default unit of verification is one task session. Splitting a session
creates smaller sessions so each can be assigned and verified independently.
Observation spans can be classified at a finer interval without fragmenting the
task session unless the user chooses to split it.

### 7.4 Day Ledger projection

The Day Ledger is a read model built from source intervals, not another editable
timesheet. For the requested local date, the projection:

1. loads task sessions, life entries, private intervals, and observations that
   intersect the day;
2. splits the day at every source boundary;
3. flags any overlap among verified work, verified life, or private intervals
   for user resolution;
4. assigns each conflict-free segment to verified work, verified life/sleep,
   private, tracked-pending, observed-unconfirmed, or unaccounted;
5. attaches app/domain context to verified or pending segments as evidence
   without adding duration; and
6. unions segments before producing totals, so parallel sources cannot
   double-count time.

The Daily Review UI uses the same projection with multiple visual lanes. The
Time Summary uses its mutually exclusive accounting states. They must never
compute separate totals.

### 7.5 Today Workspace projection

The launch screen is a composition read model, not a new source of truth. Given
one selected local date, it returns:

```text
week_days[]
  date, is_today, is_selected, planned_task_count,
  tracked_task_count, completed_task_count,
  untracked_planned_count, day_review_status

week_allocation
  verified_work, verified_life, sleep_rest, private,
  tracked_pending, observed_unconfirmed, unaccounted

selected_day_tasks
  top_three[], more[], completed[]

tomorrow_preview[]

selected_day_schedule
  scheduled_blocks[], current_time?, visible_day_start

active_execution
  running_task_session?, active_focus_interval?
```

The week allocation comes from Day Ledger projections for the week containing
the selected date. It never derives confirmed totals from scheduled blocks or
task checkboxes.

The task list comes from daily plan items plus open tasks whose intended date or
scheduled block intersects the selected day. The schedule comes only from
scheduled blocks. Starting a task creates a task session; checking it complete
changes task state; dragging it onto the timeline creates or moves a scheduled
block. These controls may be adjacent in the UI but remain different commands.

### 7.6 Weekday tracking-memory cue

Each cell in the Monday–Sunday strip is derived from task records for that local
date:

```text
planned_task_count
  distinct open or completed tasks present through a daily plan item,
  intended date, or scheduled block

tracked_task_count
  distinct tasks with positive-duration, non-voided task-session overlap
  on the local date

completed_task_count
  tasks currently completed whose completed_at falls on the local date

untracked_planned_count
  planned tasks with no qualifying task-session overlap on the local date
```

Machine observation does not increase `tracked_task_count`; the cue exists
specifically to remind the user to start or add a task session. Verification is
also separate: the strip may show that a task was tracked while the Daily Review
still shows its session as pending.

Visual state contract:

- **No planned or tracked task** — neutral cell.
- **Planned but none tracked** — hollow pending marker; on today, this is a
  gentle reminder rather than an error state.
- **Tasks tracked** — activity fill with intensity bucketed by the distinct
  tracked-task count; the exact count is available in text.
- **Tasks completed** — independent check marker with the completed-task count.
- **Current date** — persistent today indicator.
- **Selected date** — focus/selection outline independent of all activity marks.

Colour is supplementary. Shape, marker, count, tooltip/focus text, and an
accessible label communicate the same state. There is no streak total and no
penalty when a day remains empty.

---

## 8. Application component architecture

```text
┌──────────────────────────────── Desktop UI ────────────────────────────────┐
│ Today │ Projects │ Activity │ Reports │ Review │ Settings                 │
└───────────────────────────────┬────────────────────────────────────────────┘
                                │ typed commands and queries
┌───────────────────────────────▼────────────────────────────────────────────┐
│                         Application Services                              │
│ Project/Task │ Scheduling │ Timer/Focus │ Observation │ Review │ Reporting │
└───────────────────────────────┬────────────────────────────────────────────┘
                                │ domain operations
┌───────────────────────────────▼────────────────────────────────────────────┐
│                            Domain Core                                    │
│ Entities │ State machines │ Interval math │ Evidence rules │ Invariants    │
└───────────────┬───────────────────────────────┬────────────────────────────┘
                │ repositories                  │ local adapter interfaces
┌───────────────▼────────────────┐  ┌───────────▼────────────────────────────┐
│ Local Database & Files        │  │ Local Capture Adapters                 │
│ SQLite │ backups │ exports    │  │ OS observer │ browser connector │ idle │
└────────────────────────────────┘  └────────────────────────────────────────┘
```

### 8.1 Desktop UI

The UI presents state and sends user intent. It does not compute authoritative
time totals, manipulate the database directly, or decide verification.

Primary surfaces:

- **Today** — the default launch route: week selector and honest week-allocation
  summary above a two-column workspace with Top 3 / More / Tomorrow tasks on the
  left and the selected day's timeboxing schedule on the right; the active
  timer/focus state remains visible throughout;
- **Projects** — project portfolio and project-detail workbench with grouped
  tasks, progress, estimate/tracked totals, quick add, bulk organization, and
  immediate schedule, Start, Complete, Archive, and Restore actions;
- **Activity** — daily application and website totals above a selectable
  observation timeline; Productive, Neutral, Distraction, and Private interval
  labels remain separate from prospective app/domain/page rules;
- **Daily Review** — aligned plan, session, focus, life, private, observation,
  and classification lanes paired with the authoritative Day Ledger summary;
- **Reports** — day/week/month conclusions, task/project variance, goals,
  estimate calibration, focus/fragmentation, full-period allocation, and app/site
  evidence clearly separated from confirmed records; and
- **Settings** — capture, privacy, classification rules, backup, and export.

### 8.2 Application services

Application services coordinate transactions and expose explicit use cases:

```text
CreateProject
UpdateProject / ReorderProjects / ArchiveProject / RestoreProject
CreateTask / UpdateTask / MoveTask / ReorderTasks
UpdateTaskEstimate / SetTaskIntendedDate
ParseQuickCapture
AddTaskToDay / RemoveTaskFromDay / SetTopThree / ReorderDayTasks
ScheduleTask / MoveScheduledBlock / DeleteScheduledBlock
StartTaskSession / StopTaskSession / RecoverRunningSession
StartFocus / StopFocus
RecordObservationBatch
ClassifyObservationInterval / ClearObservationClassification
CreateClassificationRule / UpdateClassificationRule / DisableClassificationRule
EditSession / SplitSession / ReassignSession / VoidSession
VerifySession
CreateLifeEntry / EditLifeEntry / DeleteLifeEntry
CreatePrivateInterval / DeletePrivateInterval
BuildDayLedger / CloseDayReview / ReopenDayReview
CreateDirectedGoal / UpdateDirectedGoal / EndDirectedGoal
CompleteTask / ReopenTask
BuildTodayWorkspace / BuildProjectPortfolio / BuildProjectDetail
BuildActivitySummary / BuildDailyReview / BuildPeriodReport
BuildEstimateCalibration
CreateBackup / RestoreBackup / ExportData
```

Each command validates in the domain core and commits atomically.

### 8.3 Domain core

The domain core contains no UI or operating-system code. It owns:

- task and session transitions;
- interval validation and overlap calculations;
- focus containment;
- evidence coverage;
- classification precedence;
- verification rules;
- full-day interval projection;
- directed-goal pacing;
- estimate calibration; and
- report inputs and time totals.

This boundary makes the most important rules testable without launching the
desktop application.

### 8.4 Local persistence

An embedded SQLite database is the production persistence mechanism:

- transactions protect timer and review operations;
- indexes support time-range queries;
- migrations make the local record durable across versions; and
- the database can be backed up as a user-owned file.

The application should also provide an exact structured backup format and a
human-readable export. Backups must include projects, tasks, blocks, sessions,
focus intervals, daily plan items, life areas and entries, private intervals,
observations, classifications, work kinds, session/day reviews, goals, rules,
and settings.

### 8.5 Capture adapters

The operating-system observer and browser connector are replaceable adapters,
not part of the domain model.

The Windows adapter records foreground application and idle boundaries. A
browser extension sends the active registrable domain to the local desktop
process through a local-only channel such as native messaging. Page identity is
an additional explicit opt-in and is normalized before storage. Neither adapter
has permission to send data to a remote service.

---

## 9. Local-only security and privacy boundary

```text
Allowed boundary
┌──────────────────────────────────────────────────────────┐
│ Desktop UI                                               │
│ Domain and application services                          │
│ SQLite database, backups, and exports                    │
│ Windows foreground/idle observer                         │
│ Browser extension ↔ local native connector               │
└──────────────────────────────────────────────────────────┘

No core-data path crosses this boundary.
```

Rules:

1. The app works with networking disabled.
2. No account, cloud database, telemetry, analytics, crash upload, remote font,
   advertising, or remote AI is required.
3. Activity capture is opt-in and can be paused independently of timers.
4. Excluded applications and domains are filtered before storage.
5. Browser capture stores a registrable domain by default, not a full URL, page
   content, search query, fragment, form value, or browsing history. Optional
   page matching removes query strings and fragments before storage.
6. Window-title capture is separately opt-in and off by default.
7. Private intervals retain only the minimum information needed to show that
   time was intentionally not captured.
8. Deletion, retention, backup, and restore controls are available locally.
9. Any future on-device learning reads only local confirmed history and writes
   explainable local suggestions.

---

## 10. Reporting architecture

Reports are projections derived from source records; they do not maintain a
second truth. Every headline can be opened to the tasks, sessions, focus
intervals, life entries, and observations behind it.

Every day/week/month report follows the same hierarchy:

```text
What this period says
    ↓
Core totals and review completeness
    ↓
Against estimates, schedule, and directed goals
    ↓
Shape, allocation, focus, and fragmentation
    ↓
Observation evidence
    ↓
Arithmetic and source records
```

Initial report projections:

- estimate vs scheduled vs tracked vs verified by task and project;
- planned start/end vs actual session timing;
- verified and pending time;
- focus time and average focus interval;
- observed work coverage and conflicting evidence;
- application and domain categories during tracked sessions;
- verified work/life/sleep, private, tracked-pending,
  observed-unconfirmed, and unaccounted allocation across the exact local
  period length;
- directed-goal progress and remaining daily pace;
- longest uninterrupted task stretch, short runs, and unplanned switches;
- busiest work hours and work outside the user's preferred window;
- project, task, work-kind, and life-area breakdowns;
- completed tasks and estimate accuracy;
- daily and weekly context switching; and
- weekly change against a recent local baseline.

Reports should default to verified time for claims about completed work while
showing pending tracked time alongside it. Observation-only time is never
reported as task time unless the user converts or assigns it during review.

### 10.1 Completeness contract

Every report states how many days are closed, open, or missing review. Partial
periods are allowed, but the interface must not present them as complete.
Typically a week accounts for 168 hours; the projection uses the exact local
period length so daylight-saving transitions remain correct.

### 10.2 Score contract

A summary score is optional. If included, it must provide:

- the complete arithmetic;
- the inputs and their weights;
- the period and comparison baseline;
- the effect of unreviewed days or insufficient data; and
- direct navigation to the contributing records.

Fruit's reports remain useful without a score. Plain-language conclusions and
source totals are the primary interface.

### 10.3 Goal and calibration contract

Goal evaluation respects `at_least` and `at_most` direction. Pacing never labels
an at-most goal as behind merely because the user is currently below its cap.

Estimate calibration groups comparable completed tasks, uses verified duration,
prefers a robust statistic such as the median ratio, and withholds a recommendation
until the minimum sample is met. The sample size and source tasks are always
visible.

### 10.4 Consistency without streak pressure

A calendar heatmap may reveal when planned tasks, focus time, or closed reviews
happened over months. It is a descriptive projection, not a reward mechanism:
there is no streak loss, freeze, badge, or escalating reminder attached to it.

---

## 11. Example end to end

```text
Project: Personal Software
Task: Design task lifecycle
Estimate: 2h
Intended date: 2026-08-27
Scheduled block: 10:00–12:00

Session 1: 10:08–10:54     tracked 46m
  Focus:   10:12–10:47     focus 35m
  Evidence: editor + documentation domains, mostly Productive-classified

Session 2: 14:05–15:31     tracked 1h 26m
  Focus:   14:10–15:02     focus 52m
  Evidence: editor, documentation, and 12m communication

Daily review:
  - classify the documentation domain as Productive for this interval;
  - optionally create a prospective Productive rule for that domain;
  - accept the communication interval as related coordination;
  - verify both sessions.

Task completion:
  - user marks task Complete;
  - estimated: 2h;
  - scheduled: 2h;
  - tracked and verified: 2h 12m;
  - focus: 1h 27m;
  - observed evidence coverage remains inspectable.

Day Ledger after review:
  - verified work: 2h 12m;
  - verified life: 3h 18m;
  - sleep/rest: 7h 30m;
  - private: 1h;
  - tracked-pending: 0m;
  - observed-unconfirmed: 2h;
  - unaccounted: 8h;
  - total: 24h exactly.
```

No duration is overwritten to make the plan look accurate. The twelve-minute
variance becomes useful input for future estimation.

---

## 12. Architecture decisions

| Decision | Reason |
|---|---|
| Keep estimate, schedule, session, focus, observation, verification, and completion separate | They answer different questions and must not overwrite one another. |
| Allow multiple sessions per task | Real work is interrupted and often spans days. |
| Nest focus intervals inside task sessions | Focus describes the quality/mode of tracked work, not extra time. |
| Relate observations to sessions by time overlap | Evidence remains correctable and useful even when no timer was running. |
| Require explicit user verification | A Productive-classified app cannot prove which task was performed. |
| Require explicit task completion | Time spent does not prove the outcome is finished. |
| Permit one active task session initially | Prevents ambiguous and double-counted tracked time. |
| Apply new classification rules prospectively | Preserves historical truth and avoids surprise rewrites. |
| Model non-task life with life entries rather than fake tasks | Sleep, health, family, and leisure are actual time but not work outcomes. |
| Build one Day Ledger projection for review, summaries, and reports | Prevents different screens from calculating different versions of the day. |
| Keep tracked-pending separate from observed-unconfirmed | A deliberate timer claim is stronger than passive observation but still awaits verification. |
| Make goal direction part of the domain | At-least and at-most targets have opposite success and pacing semantics. |
| Expose report completeness and arithmetic | Partial data and opaque scores should not look authoritative. |
| Open directly into the Today workspace | The primary daily job is choosing, scheduling, starting, and completing work—not reading reports. |
| Keep the task list and schedule side by side | It makes assigning a task to time and starting it possible without navigation. |
| Model the Top 3 with dated daily plan items | Daily emphasis can change without mutating the task or pretending a calendar block exists. |
| Mark weekdays by distinct tasks tracked and completed | An empty or partially marked day reminds the user to log work without introducing streak pressure. |
| Keep weekday navigation state separate from activity marks | Today and selection must stay legible regardless of tracking volume. |
| Keep authoritative logic outside the UI | Makes time and evidence rules testable and consistent. |
| Use local persistence and local adapters only | Protects the detailed activity record and guarantees offline operation. |

---

## 13. Deferred decisions

These choices do not change the core lifecycle and can be made during product
design:

- exact task statuses beyond Open, In Progress, Completed, and Archived;
- pause/resume versus closing and starting a new session;
- whether focus intervals are exclusively manual or may be suggested;
- default classification categories and user customization;
- default life areas and work kinds;
- the exact quick-capture syntax and whether natural-language parsing is used;
- the Today workspace's responsive stacking order and minimum supported width;
- the configurable visible-day start and default timeline zoom;
- whether the first release includes any composite score;
- recurring tasks and recurring scheduled blocks;
- calendar import, without account-based sync;
- exact browser-extension packaging and supported browser list;
- export formats beyond exact backup and basic CSV/JSON; and
- how much estimate history to expose in the first release.

The core rule is not deferred: regardless of the UI or stack, Fruit must keep
what was planned, tracked, focused, observed, verified, recorded as life, made
private, left unaccounted, and completed as distinct, locally owned facts.

---

## 14. Route and page architecture

The desktop shell owns global navigation, the single active timer, capture
status, review-attention count, and selected date/period. Pages consume typed
read models and send commands; they do not share mutable component state.

```text
/
└── /today                         default launch route
    /projects                      project portfolio
    /projects/:projectId           project detail and task workbench
    /activity?date=YYYY-MM-DD       observed applications and websites
    /reports?period=week&anchor=…   week or month reports
    /review?date=YYYY-MM-DD         daily reconciliation
    /settings/:section?             local configuration
```

The Planner is part of Today in the initial information architecture instead of
a separate top-level route. The complete timeboxing canvas may be opened in an
expanded Today mode without creating a second scheduling model.

Global shell contract:

- Today is first and receives the default focus after launch.
- A persistent running-timer control shows task, elapsed time, focus state, and
  Stop from every route.
- Capture state shows Recording, Paused, Disabled, or Needs attention without
  implying that a task timer is running.
- Review attention shows unresolved days or intervals, not a punitive streak.
- Navigation and commands work without network access.
- Route changes never stop a task session, focus interval, or capture adapter.

Deep links resolve only local entity identifiers. Deleted or missing entities
show a recoverable not-found state and a route back to the relevant collection.

---

## 15. Projects architecture

Projects has two projections: a portfolio and a project detail. Both are built
from source entities plus period totals; neither stores its own progress value.

### 15.1 Project portfolio read model

```text
ProjectPortfolio
  active_projects[]
    id, name, colour, open_task_count, completed_task_count,
    estimated_open_sec, scheduled_future_sec,
    tracked_period_sec, verified_period_sec, last_activity_at?
  archived_projects[]
  inbox
    open_task_count, estimated_open_sec
  active_session
```

The portfolio defaults to active projects in user-defined order. Search matches
project names and task titles locally. Archived projects are collapsed and do
not appear in task pickers unless explicitly requested.

### 15.2 Project detail read model

```text
ProjectDetail
  project
    id, name, description, colour, status
  summary
    open_count, completed_count, estimated_open_sec,
    scheduled_open_sec, tracked_period_sec, verified_period_sec
  task_groups
    overdue[], today[], upcoming[], no_date[], completed[]
  active_session
  selected_period
```

Grouping is a projection. A task remains one entity when its date changes or it
moves between open and completed groups. Default completed scope is recent
completion; older tasks remain searchable and expandable.

### 15.3 Project command rules

- Project creation requires a non-blank unique display name among active
  projects after case-insensitive normalization. Archived names may be reused.
- Archiving a project does not archive, delete, stop, or void its historical
  sessions. Open tasks become hidden from default planning surfaces until the
  project is restored or the task is moved.
- A project with a running task session cannot be archived without first
  stopping the session or moving the task.
- Moving a task changes `project_id` atomically and preserves all sessions,
  blocks, focus intervals, reviews, and completion history.
- Deleting a project is not a normal command. Permanent deletion is a separate
  privacy/data-management operation with a dependency preview and backup prompt.
- Bulk task edits operate in one transaction and report individual validation
  failures before committing.

Task creation from a project page defaults `project_id` to that project. Quick
capture can omit every field except title. Estimate, intended date, priority,
Top 3 placement, and scheduled time remain independently editable.

---

## 16. Activity architecture

Activity presents machine evidence; it is never the authority for verified
work. It has three related but separate projections.

### 16.1 Daily source summary

```text
ActivityDaySummary
  date, observed_sec, active_sec, idle_sec, private_sec, capture_gap_sec
  applications[]
    application_id, display_name, duration_sec, share,
    productive_sec, distraction_sec, neutral_sec, unclassified_sec
  websites[]
    website_id, domain, duration_sec, share,
    productive_sec, distraction_sec, neutral_sec, unclassified_sec
```

Application and website totals are two views of overlapping evidence. Their
grand totals must not be added together. When a browser is active, its
application span and website identity describe the same duration.

### 16.2 Observation timeline

```text
ActivityTimeline
  spans[]
    id, start, end, application, website?, page_label?,
    active_or_idle, effective_classification,
    classification_source, overlaps_task_session_ids[]
  capture_boundaries[]
  private_intervals[]
```

Adjacent spans with identical effective context may be visually coalesced, but
editing targets exact stored boundaries. The UI can split a span at the
selection boundary before applying an interval override.

### 16.3 Rule library

```text
ActivityRuleLibrary
  application_rules[]
  domain_rules[]
  page_rules[]
  conflicts[]
```

Rules apply prospectively from `effective_from`. Editing a rule changes future
classification. Applying a classification to historical spans is a separate
batch command with a displayed date range and affected duration.

### 16.4 Capture pipeline

```text
Windows foreground event ─┐
Idle state event ──────────┼─▶ normalize ─▶ privacy filter ─▶ coalesce
Browser local event ───────┘                                  │
                                                              ▼
                                                     SQLite observation batch
                                                              │
                                                              ▼
                                               classify at query/review time
```

The capture adapter emits changes, not screenshots or input content. The
privacy filter runs before persistence. Coalescing closes the previous span and
opens a new span when the foreground identity, website identity, idle state, or
capture state changes. A heartbeat safely closes stale spans after a crash.

The initial sampling objective is an effective boundary within five seconds of
a foreground change while keeping continuous idle CPU usage low. Events are
batched transactionally to reduce disk writes without risking more than a small
bounded amount of recent evidence.

### 16.5 Classification semantics

Stable classification kinds are:

```text
PRODUCTIVE     contributes to productive-observation evidence
COMMUNICATION  necessary coordination, reported separately when useful
NEUTRAL        observed context with no productivity claim
DISTRACTION    activity the user wants reported as distracting
PRIVATE        detail suppressed and excluded from source breakdowns
UNCLASSIFIED   no user decision or applicable rule
```

Classification is contextual evidence, not moral judgment. Productive time is
not automatically verified work; Distraction can overlap a valid task session;
Communication may be productive for one project and incidental for another.
Reports preserve these distinctions.

---

## 17. Reports architecture

Reports offers exactly two primary period modes in the initial release: Week and
Month. Day detail belongs to Today, Activity, and Review. Both report modes use
the same metric registry and drill-down protocol.

### 17.1 Period boundaries

- Week uses the configured first day of week, default Monday, in the user's
  stored timezone.
- Month uses local calendar month boundaries.
- An active period may be partial; the report displays elapsed and remaining
  days and does not compare a partial month directly with a completed month
  without an equivalent elapsed-period option.
- Period changes are deterministic when the system timezone later changes
  because source records retain their originating local date and timezone.

### 17.2 Report read model

```text
PeriodReport
  period, completeness, comparison_period
  conclusions[]
  core_totals
  allocation
  plan_vs_actual
  focus_and_fragmentation
  projects_and_tasks
  activity_classification
  applications_and_websites
  goals
  estimate_calibration
  completion_and_consistency
  metric_definitions[]
```

Each metric carries:

```text
metric_id, value, unit, numerator?, denominator?,
source_state_filter, completeness, comparison_value?,
delta?, sample_size?, drilldown_query
```

The UI formats but does not recompute these values.

### 17.3 Query strategy

Source tables remain canonical. Read models are built through indexed range
queries and interval projections. Derived daily aggregates may be cached locally
when observation volume grows, subject to these rules:

- caches are replaceable and can be rebuilt from source records;
- a source edit invalidates every affected local day;
- cache schema versions are independent of source migrations;
- cached totals never become backup authority; and
- a diagnostic command can compare cached and fresh calculations.

### 17.4 Drill-down contract

Every headline and chart element supports a source query appropriate to its
meaning. Project time opens tasks and verified sessions. Productive-observation
time opens observation spans and the rules that classified them. Unaccounted
time opens Day Ledger gaps. A comparison delta exposes both period definitions.

Report export includes visible filters, period boundaries, timezone,
completeness, metric definitions, and generated time so a saved report remains
interpretable.

---

## 18. Target implementation architecture

The selected desktop implementation is:

```text
React + TypeScript UI
        │ typed Tauri commands and events
        ▼
Tauri 2 desktop host
        │
        ├── Rust application services and domain interval engine
        ├── SQLite repository and migrations
        ├── Windows foreground/idle capture adapter
        ├── browser native-messaging host
        └── local backup/export services
```

Tauri is chosen for the low-overhead, continuously running Windows utility and
its narrow native capability boundary. React components never receive direct
filesystem, process-list, or database access. Tauri capabilities allow only the
commands required by the application.

The current prototype repository may use browser local storage while UI flows
are being proved. That store is non-authoritative and must be replaced by the
SQLite repository before real activity capture or migration guarantees are
claimed. The repository interface allows this replacement without changing
page behavior.

### 18.1 Local IPC

Commands use versioned serializable request and response types. Long-running
capture and export operations emit progress events and support cancellation.
Errors use stable codes with safe user-facing messages; sensitive paths or page
identities are not included in generic logs.

### 18.2 SQLite organization

Source tables follow the entities in section 4. Required indexes include:

- task by project/status/intended date;
- scheduled block by start/end and task;
- session by start/end, task, and review status;
- observation by start/end, application, website, and classification source;
- classification rule by match type/value/effective date;
- day review by local date; and
- goal by period and active version.

Foreign keys are enabled. Migrations run transactionally after a pre-migration
backup. The app refuses to open a database created by a newer incompatible
version and offers a clear recovery path rather than attempting a downgrade.

### 18.3 Runtime ownership

- Rust owns the authoritative running-session and running-focus records.
- The foreground observer continues independently of UI route and window focus.
- Closing the window follows the configured background behavior; it never
  silently terminates a timer.
- On startup, stale open sessions and observation spans enter recovery flow.
- The UI clock is presentational; persisted durations always use source
  timestamps from the host.

### 18.4 Verification strategy

Automated tests are layered:

1. domain tests for state transitions, precedence, interval math, period
   boundaries, allocation completeness, and report formulas;
2. repository tests against temporary SQLite databases and every migration;
3. application-service tests for atomic commands and recovery;
4. adapter contract tests with synthetic foreground/browser event streams;
5. UI tests for keyboard flow, empty/loading/error states, and accessible names;
6. end-to-end desktop tests for create → plan → track → observe → classify →
   verify → complete → report; and
7. backup/restore equivalence tests comparing canonical totals before and after.

The release gate requires offline operation, crash recovery, no report
double-counting, and a successful backup restore on a clean local profile.
