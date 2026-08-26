# Fruit — Product Specification

**Version:** 1.0  
**Updated:** 26 August 2026  
**Owner:** Product owner  
**Status:** Build specification  
**Platforms:** Windows desktop first  
**Data model:** Offline and local-only  
**Related documents:** [`VISION.md`](VISION.md),
[`ARCHITECTURE.md`](ARCHITECTURE.md)

This document specifies the intended complete Fruit application. It is the
product contract for design, implementation, and acceptance. It does not imply
that every described requirement is already implemented. Priority labels are:

- **P0** — required for the trustworthy core product;
- **P1** — important after the core loop is dependable; and
- **P2** — optional enhancement that must not weaken the core.

---

## 1. Product summary

Fruit is a private personal productivity desktop application that unifies:

- projects and actionable tasks;
- estimates, intended dates, and time blocks;
- multi-session task timers and focus intervals;
- local foreground-application and browser-website evidence;
- explicit user classification and verification;
- a complete daily time ledger; and
- explainable weekly and monthly reports.

The product's canonical lifecycle is:

```text
PROJECT → TASK → ESTIMATE → DATE → SCHEDULE → TRACK → FOCUS
                                                │
                                                ├── OBSERVE APP/WEBSITE
                                                ▼
                                      REVIEW → VERIFY → COMPLETE
                                                │
                                                ▼
                                         WEEK / MONTH REPORT
```

Plans, timers, observations, verification, and completion remain separate
facts. Fruit helps the user reconcile them; it never edits one to make another
look correct.

---

## 2. Product objectives

### 2.1 Primary objectives

1. Make the current priority and next action obvious within five seconds of
   opening the app.
2. Make starting and stopping task tracking fast enough to become habitual.
3. Preserve an honest comparison between estimated, scheduled, tracked,
   focused, observed, and verified time.
4. Let the user understand and classify locally observed application and
   website activity without surrendering privacy.
5. Reduce daily reconciliation to a short, comprehensible review.
6. Produce week and month reports that lead to a realistic planning change.
7. Work fully offline with user-controlled local storage, backup, and export.

### 2.2 Non-goals

Fruit is not intended to provide:

- team projects, assignments, comments, shared workspaces, or manager reports;
- cloud accounts, cross-device synchronization, or a hosted dashboard;
- billing, invoicing, timesheets for clients, or payroll;
- website blocking, punishment, or enforced focus;
- screenshots, keystroke capture, page-content capture, or surveillance;
- a general calendar replacement;
- an opaque AI productivity score; or
- a mobile application in the initial product.

---

## 3. User and operating assumptions

### 3.1 Primary user

A single privacy-conscious knowledge worker on a Windows computer who manages
several personal or professional projects and wants to understand the gap
between intention and actual attention.

### 3.2 Assumptions

- One local person uses one Fruit profile.
- The system timezone and first day of week are configurable.
- The user may work away from the computer and must be able to add time manually.
- The user sometimes forgets to start a timer.
- Activity capture may be disabled, paused, incomplete, or unavailable.
- A browser application alone is not meaningfully Productive or Distracting;
  website context usually provides the more specific rule.
- The user is willing to make classification decisions when the product keeps
  them concise and remembers prospective rules.
- No network connection is required for any P0 use case.

---

## 4. Product principles translated into requirements

| Principle | Product requirement |
|---|---|
| Local ownership | Core data is stored locally; no account or remote service is required. |
| User authority | Only the user verifies sessions and completes tasks. |
| Honest layers | Planned, tracked, observed, and verified values are visually and semantically distinct. |
| Low maintenance | Common actions are available inline and from the keyboard. |
| Calm feedback | No streak loss, shame language, or coercive notifications. |
| Explainable reports | Every conclusion and chart can reveal its formula and source records. |
| Privacy before storage | Exclusions and capture-level limits run before activity is persisted. |
| Reversibility | Classification, completion, archive, and review actions can be corrected without destroying history. |

---

## 5. Information architecture

### 5.1 Primary navigation

The global navigation order is:

1. **Today** — default launch destination.
2. **Projects** — portfolio and project task management.
3. **Activity** — observed applications and websites.
4. **Reports** — Week and Month reflection.
5. **Review** — daily reconciliation queue.
6. **Settings** — privacy, capture, behavior, data, and appearance.

### 5.2 Persistent shell

The shell contains:

- product mark and navigation;
- local date/time context;
- global quick capture;
- active task timer with elapsed time and Stop;
- active focus state when applicable;
- activity-capture state;
- unresolved review count; and
- an explicit Offline / Local-only status in onboarding and Settings.

The active timer remains visible when changing routes. Navigating never stops a
timer, focus interval, or activity capture.

### 5.3 Global commands

| Command | Default shortcut | Behavior |
|---|---:|---|
| Quick capture task | `Ctrl+N` | Opens compact capture with title focused. |
| Search/jump | `Ctrl+K` | Searches projects and tasks and navigates locally. |
| Start/stop selected task | `Space` when row-focused | Starts the task or requests confirmation to switch from another running task. |
| Start/stop focus | `F` when a task is running | Toggles focus within the active session. |
| Go to Today | `G`, then `T` | Opens Today with the current date selected. |
| Go to Projects | `G`, then `P` | Opens Projects. |
| Go to Activity | `G`, then `A` | Opens Activity for today. |
| Go to Reports | `G`, then `R` | Opens the current week report. |
| Undo last reversible action | `Ctrl+Z` | Restores the most recent supported local mutation. |

Shortcuts must not fire while the user is typing in a text field unless the
shortcut explicitly includes a modifier.

---

## 6. First-run onboarding

### 6.1 Onboarding goals

Onboarding explains the local-only promise, establishes essential settings, and
gets the user to a useful Today screen without demanding activity permissions.

### 6.2 Flow

1. **Welcome and local guarantee**
   - Explain that data stays on the computer.
   - Show the local data directory and a link to change it when supported.
2. **Workweek preferences**
   - Timezone, first day of week, preferred workday start/end, daily work target.
3. **Starter planning choice**
   - Start empty or create an optional private starter project with example tasks.
4. **Activity capture choice**
   - Enable later, enable application-only capture, or begin browser connector
     setup.
   - Explicitly state what is and is not captured.
5. **Backup reminder**
   - Choose a local backup folder now or defer.
6. **Open Today**
   - Focus the first task capture action.

Activity capture is opt-in. Declining it does not reduce project, timer, review,
or task-report functionality.

### 6.3 Acceptance criteria

- The user can complete onboarding with networking disabled.
- No capture adapter starts before explicit consent.
- Closing onboarding saves progress and can resume safely.
- Every choice remains editable in Settings.

---

## 7. Today page

### 7.1 Purpose

Today answers: **What matters now, when will I do it, and am I tracking it?**

### 7.2 Layout

```text
┌────────────────────── This week ───────────────────┬─ Week allocation ─┐
│ Mon  Tue  Wed  Thu  Fri  Sat  Sun                  │ verified/pending  │
├────────────────────────────────┬────────────────────────────────────────┤
│ Tasks for selected day         │ Schedule for selected day              │
│ Top 3                          │ vertical timeline                       │
│ More for today                 │ scheduled blocks                       │
│ Completed (collapsed)          │ current-time line                      │
│ Tomorrow preview               │ active timer/focus state               │
└────────────────────────────────┴────────────────────────────────────────┘
```

### 7.3 Week strip requirements

- Defaults to the week containing today.
- Selecting a day updates both tasks and schedule.
- Each day displays independent states for:
  - distinct tasks planned;
  - distinct tasks with tracked time;
  - tasks completed on that date;
  - planned tasks with no tracked session;
  - today; and
  - selected date.
- Activity intensity uses distinct tracked task count, not session count.
- Completion uses a separate check and count.
- Status is readable without color and has an accessible label.
- Previous/next week navigation does not change the report period.

### 7.4 Task groups

**Top 3** contains at most three daily plan items. **More for today** contains
additional planned tasks. **Completed** is collapsed by default. **Tomorrow
preview** is read-only except for opening the task.

Each active task row supports:

- completion checkbox;
- title and project identity;
- estimate and tracked total;
- intended date and scheduled time, when present;
- Start or Stop action;
- focus indicator when the active task is in focus;
- overflow menu for edit, schedule, move, remove from day, and archive; and
- drag handle plus keyboard reorder for daily rank.

### 7.5 Quick capture

Minimum input is a title. Optional fields are project, estimate, intended date,
priority, and scheduled start. Capture from Today defaults the intended date to
the selected date; capture from another page defaults to Inbox with no date.

The task appears immediately after a successful local transaction. Invalid
optional syntax never discards the typed title.

### 7.6 Schedule

- Shows the same date selected in the week strip.
- Supports creating, moving, resizing, and deleting task blocks.
- Allows multiple blocks for one task across dates.
- Snaps to 15 minutes by default with a five-minute precision option.
- Shows estimate, scheduled total, and difference without forcing equality.
- A block can start its linked task but never starts automatically.
- Moving a block changes the plan only.
- Past blocks remain visible and visually distinct from tracked sessions.
- Current-time marker appears only when viewing today.
- Overlapping blocks are allowed with a warning because plans can conflict.

### 7.7 Timer and focus behavior

- Only one task session can run at a time.
- Starting another task offers **Stop and switch** or **Cancel**.
- Stopping closes the session at the host timestamp.
- Starting the same task again creates a new session.
- Focus can start only during a running task session.
- Stopping a session closes its active focus interval.
- App restart recovers a running session and asks the user to confirm or edit
  implausibly long elapsed time.

### 7.8 Empty and attention states

- No tasks: invite quick capture or selection from Projects.
- Planned but untracked day: show a neutral “No task time tracked yet” reminder.
- Timer running beyond scheduled block: show overrun without stopping it.
- Activity capture disabled: do not present the day as missing evidence; state
  that observation is off.
- Selected historical day: Start remains available but warns that a new session
  records now, not in the past.

---

## 8. Projects page

### 8.1 Purpose

Projects is the durable planning and management surface. It answers: **What
outcomes exist, what remains, and how much effort has the project consumed?**

### 8.2 Portfolio view

The left project navigation and main portfolio include:

- Inbox;
- active projects in manual order;
- archived projects in a collapsed section;
- local search;
- New project action; and
- recently archived/deleted recovery entry where applicable.

Each project card or row shows:

- name and color;
- open and completed task counts;
- remaining estimate;
- future scheduled time;
- tracked and verified time for the selected period;
- last activity date; and
- current running task indicator if relevant.

The portfolio supports List and Compact views. It does not assign a synthetic
percentage unless the denominator is explicit. Task-count progress and
estimate-weighted progress, if shown, are labeled separately.

### 8.3 Create project

Required field:

- name.

Optional fields:

- description;
- color;
- manual sort position; and
- initial tasks.

Rules:

- trim leading/trailing whitespace;
- reject a blank name;
- prevent case-insensitive duplicate active names;
- create locally in one transaction; and
- open the new project page with task capture focused.

### 8.4 Project detail

Header actions:

- edit name, description, and color;
- create task;
- choose summary period: This week, This month, All time;
- archive/restore project; and
- open project report filtered to this project.

Summary metrics:

- open tasks;
- completed in selected period;
- remaining estimate;
- scheduled future time;
- tracked time;
- verified time;
- focused time; and
- pending-review time.

Metrics clearly name the selected period. “Remaining estimate” is a current
inventory measure and is not compared directly to period actuals.

### 8.5 Project task groups

Default groups:

1. Overdue — intended date before today and not complete.
2. Today — intended today or in today's daily plan.
3. Upcoming — intended after today.
4. No date — no intended date.
5. Completed — completed tasks, newest first.

The user can switch to a flat sortable list. Filters include status, intended
date, priority, estimate range, tracked/untracked, reviewed/pending, and text.

### 8.6 Task row behavior

Each row supports:

- select and multi-select;
- complete/reopen;
- inline title edit;
- estimate edit;
- intended-date picker;
- scheduled-time summary;
- tracked and verified totals;
- Start/Stop;
- open details;
- move to another project;
- archive; and
- overflow actions.

Bulk actions: set project, date, priority, archive, complete, and add/remove from
selected day. Bulk completion asks for confirmation when any selected task has a
running session.

### 8.7 Task creation and editing

Task fields:

| Field | Required | Notes |
|---|---:|---|
| Title | Yes | Outcome-oriented plain text; maximum 500 characters. |
| Project | Yes | Defaults to current project or Inbox. |
| Notes | No | Local plain text/Markdown; no remote embeds. |
| Estimate | No | Positive duration; displayed as unset rather than zero. |
| Intended date | No | Planning date, not a deadline guarantee. |
| Priority | No | None, Low, Medium, High. |
| Parent task | No, P1 | Enables subtasks without separate timing semantics. |
| Work kind | No | User-owned classification such as Deep Work or Admin. |
| Daily rank | No | Top 3 or additional position for a specific date. |
| Scheduled blocks | No | Zero or more independent planned intervals. |

Changing a task never rewrites its sessions. Estimate changes are timestamped in
history by P1 so calibration can distinguish original and current estimates.

### 8.8 Archive, delete, and restore

- Archive is the normal reversible removal action.
- Archived projects and tasks retain all historical report contributions.
- Restore returns the entity to active state without recreating records.
- Permanent deletion is available only in Settings/Data or a deliberate danger
  flow. It previews dependent records and offers a backup first.
- Deleting a project with sessions must offer: cancel, archive, move tasks, or
  permanently delete the entire dependency graph.

### 8.9 Acceptance criteria

- A project and ten tasks can be created without leaving Projects.
- A task can be estimated, dated, scheduled, started, stopped, and completed
  from the project workbench.
- Multiple sessions appear under one task and sum correctly.
- Archiving never removes historical time from reports.
- Project totals equal the relevant task/session drill-down.
- All actions persist across restart without network access.

---

## 9. Task detail and lifecycle

### 9.1 Task detail sections

- identity: title, project, status, priority;
- plan: estimate, intended date, daily rank, scheduled blocks;
- execution: active timer, tracked total, verified total, focus total;
- sessions: chronological list with review state and edit action;
- notes and subtasks;
- history: created, moved, estimate changes, completion/reopen, archive; and
- report link filtered to the task.

### 9.2 State rules

```text
OPEN → IN_PROGRESS → COMPLETED
  └────────────────→ COMPLETED
COMPLETED → IN_PROGRESS when reopened
ANY NON-ARCHIVED → ARCHIVED
```

- First session moves Open to In progress.
- Completion is explicit and permitted with pending review.
- Completing a running task first asks whether to stop the session.
- Reopening preserves completion history and all time records.
- Scheduled blocks after completion remain history and can be removed manually.

### 9.3 Session editing

Allowed actions:

- edit start/end;
- split at a chosen time;
- merge adjacent sessions for the same task when no conflict exists;
- reassign all or part to another task;
- add a manual session;
- void accidental time; and
- verify or return to pending.

Edits touching a closed day reopen that day review. Voided sessions remain in
audit history and can be restored until retention rules remove them.

---

## 10. Activity capture

### 10.1 Capture levels

| Level | Captured | Default |
|---|---|---:|
| Off | No foreground evidence | Available |
| Applications | Foreground executable identity and active/idle boundaries | Opt-in |
| Websites | Registrable domain while a supported browser is foreground | Separate opt-in |
| Page identity | Normalized path/title label without query or fragment | Off; advanced opt-in |
| Window title | Foreground window title | Off; advanced opt-in |

Fruit never captures screenshots, keystrokes, clipboard content, page content,
form values, search queries, full browser history, or microphone/camera data.

### 10.2 Capture controls

Global states:

- Recording;
- Paused until resumed;
- Paused for a chosen duration;
- Disabled; and
- Needs attention because a connector or permission failed.

Pause and resume create explicit capture boundaries. A paused interval does not
pretend to be Unaccounted while the Activity page is presenting observation
coverage; it is shown as Capture paused.

### 10.3 Privacy exclusions

Before storage, the user can exclude:

- application identity;
- domain;
- page match pattern when enabled;
- chosen clock schedule;
- private interval started manually; and
- all capture while a specific app is foreground.

Exclusion choices are **Do not store detail**, not just hide in the interface.
The timeline may retain a generic Private interval so period totals remain
complete.

### 10.4 Browser connector

- Runs locally and communicates only with the desktop host.
- Shows Connected, Disconnected, Unsupported, or Permission needed.
- Sends an event only when active browser context changes or heartbeat/recovery
  requires it.
- Does not upload favicons or fetch remote metadata.
- If disconnected, application capture continues and browser time remains known
  only as browser-app time.

### 10.5 Reliability

- Foreground changes are reflected within a target of five seconds.
- Idle threshold is configurable, default five minutes.
- Suspend, sleep, shutdown, and clock changes close spans safely.
- A crash can lose only the bounded in-memory batch; stale spans are recovered
  conservatively and marked uncertain if necessary.
- Observer failure never stops a deliberate task timer.

---

## 11. Activity page

### 11.1 Purpose

Activity answers: **Which applications and websites were active, when were they
active, and how should that evidence be interpreted?**

### 11.2 Page structure

1. Date navigation and capture status.
2. Coverage summary: observed, active, idle, private, paused, missing.
3. Applications / Websites tabs with duration and share.
4. Classification summary: Productive, Communication, Neutral, Distraction,
   Private, Unclassified.
5. Zoomable timeline.
6. Selected-interval inspector.
7. Classification rule manager.

### 11.3 Source lists

Application rows show icon when locally available, display name, duration,
share of observed active time, effective classification mix, and rule state.

Website rows show domain, local cached icon when available without a network
request, duration, share, classification mix, and rule state.

Sorting: duration, name, Productive duration, Distraction duration, and
Unclassified duration. Filters: classification, source, minimum duration,
task-session overlap, and text.

Browser application time and website time overlap. The UI explicitly warns that
their totals are alternate breakdowns and must not be added.

### 11.4 Classification actions

Quick labels:

- Productive;
- Communication;
- Neutral;
- Distraction;
- Private; and
- Clear / Unclassified.

Every label action requires an explicit scope:

1. **Only this selected interval** — creates an interval override.
2. **Future activity from this app** — creates an application rule.
3. **Future activity on this website** — creates a domain rule.
4. **Future activity matching this page** — available only with page identity
   enabled; creates a page rule.
5. **Apply to a displayed historical range** — separate bulk action with impact
   preview; never implied by creating a rule.

Default from a timeline selection is interval-only. Default from a source-row
rule control is future rule. Button text must include the scope.

### 11.5 Rule precedence

```text
interval override > page rule > domain rule > application rule > unclassified
```

Examples:

- Chrome = Neutral, `docs.example.com` = Productive: the domain wins.
- `youtube.com` = Distraction, a selected lecture interval = Productive: the
  interval override wins without changing YouTube's future rule.
- Browser disconnected: only the browser application rule can apply.

Conflicting rules are shown in the selected span inspector with the effective
winner and reason.

### 11.6 Timeline behavior

- Default zoom shows the configured waking/work range; full 24 hours is
  available.
- Spans display application, website when known, classification, and task-timer
  overlap.
- Drag selection snaps to observed boundaries but permits manual precision.
- Applying an interval label splits source spans without changing total time.
- Multiple selected spans can be labeled together.
- Private spans hide source identity immediately.
- Idle and capture gaps cannot be labeled Productive or Distraction unless the
  user first converts them to a manual work/life record in Review.

### 11.7 Empty and error states

- Capture disabled: explain how to enable it; do not call the day empty.
- No observed activity: show whether the computer was off, capture paused, or
  data is unavailable when known.
- Browser connector missing: show application totals and connector setup.
- Unclassified sources: prioritize by total duration to reduce review work.
- Corrupt/invalid batch: isolate it, preserve valid data, and provide a local
  diagnostic export without sensitive details by default.

### 11.8 Acceptance criteria

- The user can see all observed apps for a selected day and all supported
  websites when connector data exists.
- Labeling one website does not relabel the entire browser.
- Interval-only classification does not create a future rule.
- New rules do not silently rewrite history.
- Productive observed time never becomes verified task time automatically.
- All source totals and timeline spans reconcile to observed coverage.

---

## 12. Daily Review page

### 12.1 Purpose

Review turns plans, deliberate tracking, machine evidence, and manual life time
into a record the user accepts.

### 12.2 Review queue

Prioritized items:

1. recovered or implausibly long running sessions;
2. overlapping verified work/life conflicts;
3. closed task sessions pending verification;
4. observed time that conflicts with the running task;
5. long observed-only intervals;
6. unclassified high-duration sources;
7. unstarted scheduled blocks and large overruns; and
8. unaccounted gaps.

### 12.3 Aligned lanes

- scheduled blocks;
- tracked sessions;
- focus intervals;
- application/website evidence;
- manual work and life entries;
- private intervals; and
- Day Ledger state.

### 12.4 Session review actions

- Verify as recorded.
- Edit boundaries.
- Split or merge.
- Reassign to another task.
- Add offline explanation.
- Void accidental timer.
- Classify selected evidence.
- Create a separate future rule.
- Mark sensitive interval private.

The Verify action records the decision and evidence snapshot. Evidence can be
absent; paper work and meetings remain verifiable by the user.

### 12.5 Close day

Closing the day means the user has seen the remaining gaps, not that every
minute is categorized. The close confirmation shows:

- verified work;
- verified life/sleep;
- private;
- tracked pending;
- observed unconfirmed;
- unaccounted; and
- review coverage percentage.

Editing any source record later reopens affected days.

---

## 13. Reports page

### 13.1 Purpose

Reports answers: **What did this week or month say, how reliable is the record,
and what should change next?**

### 13.2 Period controls

- Week / Month segmented control.
- Previous, next, and Current period actions.
- Date anchor picker.
- Comparison: previous equivalent period or rolling four-period baseline.
- Filters: project, task, work kind, classification, application, website, and
  reviewed-only/all tracked.
- Export visible report.

Current incomplete periods show elapsed-day context. Future periods do not show
fabricated zeros.

### 13.3 Reading order

1. What this period says.
2. Core totals and review completeness.
3. Against plan and goals.
4. Shape of work and focus.
5. Project/task allocation.
6. Activity classification and source evidence.
7. Full period allocation.
8. Arithmetic and source drill-down.

### 13.4 Core cards

P0 cards:

- verified work;
- tracked pending;
- focus time;
- reviewed days / total elapsed days;
- completed tasks;
- average verified working day; and
- full-period accounted percentage.

Every card names its state. “Work” means verified work unless the user switches
to an explicitly labeled tracked view.

### 13.5 Report metrics and formulas

| Metric | Definition |
|---|---|
| Verified work | Sum of non-overlapping verified task-session duration in period. |
| Tracked pending | Closed, non-voided session duration not yet verified after ledger overlap resolution. |
| Focus time | Focus-interval overlap within included verified sessions; never added to work total. |
| Focus ratio | Verified focus time / verified work time with zero-denominator empty state. |
| Average working day | Verified work / count of local dates with verified work. |
| Scheduled time | Sum of scheduled-block overlap in period; can overlap and therefore also exposes conflicted scheduled time. |
| Estimate variance | Verified duration − estimate for completed tasks in selected cohort. |
| Estimate ratio | Verified duration / estimate, excluding unset or zero estimates. |
| Plan adherence | Scheduled time with qualifying tracked overlap / eligible scheduled time; does not imply completion. |
| Completion count | Tasks whose `completed_at` falls inside period. |
| Longest task stretch | Longest continuous verified interval on one task after configured short-gap tolerance. |
| Short-run time | Verified session duration occurring in runs below 15 minutes by default. |
| Task switches | Transition between distinct task sessions within configured gap threshold. |
| Observed context switches | Transition between effective app/domain context, reported separately from task switches. |
| Productive observation | Active observed duration effectively classified Productive; evidence only. |
| Distraction observation | Active observed duration effectively classified Distraction; evidence only. |
| Observation coverage | Retained observed active/idle/private boundaries / eligible computer-on evidence window when measurable. |
| Review completeness | Closed reviewed local days / elapsed local days in report scope, plus ledger-state breakdown. |
| Accounted percentage | (verified work + verified life/sleep + private + intentionally accepted states) / exact period duration; component definition is displayed. |

No metric silently treats scheduled blocks, checkboxes, or productive app labels
as verified work.

### 13.6 Weekly report

The week view emphasizes actionability:

- work hours by day;
- daily target deviation;
- focus sessions and average length;
- planned vs tracked/verified by project;
- Top 3 tracked/completed consistency;
- longest uninterrupted task stretch;
- short runs and task switches;
- work timing and work outside preferred hours;
- goal pacing for the remaining days;
- Productive/Distraction evidence by day;
- top applications and websites;
- largest divergence from plan; and
- one to three explainable conclusions.

### 13.7 Monthly report

The month view emphasizes pattern and allocation:

- verified work and focus trend by week;
- active and completed projects;
- completed task volume;
- estimate-calibration cohorts and sample size;
- project and work-kind allocation;
- productive/distraction trends;
- day-of-week and time-of-day patterns;
- reviewed-day consistency without streak framing;
- life/sleep allocation where recorded;
- goal result for completed weeks/month; and
- comparison with previous month or rolling baseline.

### 13.8 Conclusions

Conclusions are generated from deterministic local rules in P0. Examples:

- “You verified 3h 20m less work than your previous four-week average.”
- “Project Atlas used 1.28× its completed-task estimates across 8 tasks.”
- “Four scheduled blocks had no tracked session; open the blocks.”
- “Distraction-classified activity was concentrated after 21:00.”

Each statement includes period, comparison, sample size where relevant, and a
drill-down. Fruit suppresses weak conclusions when data or review completeness
is insufficient.

### 13.9 Scores

A composite score is P2 and excluded from the initial release. If added later,
the complete formula, weights, completeness penalty, and source records must be
visible. Reports must remain useful without it.

### 13.10 Report drill-down

- Clicking a bar or total opens the exact tasks, sessions, observations, days,
  or gaps behind it.
- The drill-down retains the report's period and filters.
- Edited source data invalidates and refreshes affected metrics.
- A visible breadcrumb returns to the original report state.

### 13.11 Export

P0:

- CSV tables for tasks, sessions, activity summary, and metrics;
- JSON report package with period, filters, timezone, completeness, formulas,
  and source identifiers; and
- print-friendly local HTML or PDF in P1.

Export never requires upload. Private source detail is excluded by default.

### 13.12 Acceptance criteria

- Week and month totals derive from the same source records and terminology.
- A full week allocation equals its exact local duration.
- Pending and unreviewed data is visible and never merged into verified work.
- Filters update every compatible card and chart consistently.
- Every P0 headline drills down to its source.
- Partial periods and incomplete review are clearly identified.
- Exported totals match the visible report.

---

## 14. Goals and estimate calibration

### 14.1 Directed goals

Supported goal directions:

- At least duration/count.
- At most duration/count.

Initial goal measures:

- verified work;
- verified focus;
- selected project/work-kind time;
- distraction observation;
- sleep/rest life entry;
- completed task count; and
- closed review days.

Pacing shows achieved, remaining, days remaining, and required average. At-most
goals are not “behind” while below the cap.

### 14.2 Calibration

Calibration uses completed tasks with verified sessions and a positive estimate.
It groups by estimate bucket and optionally project/work kind. The default
statistic is median actual/estimate ratio. Minimum sample:

- 5 comparable tasks for a tentative observation;
- 10 for a recommendation.

The user can open every source task. Fruit proposes an estimate adjustment but
never edits future tasks automatically.

---

## 15. Settings

### 15.1 General

- timezone;
- first day of week;
- 12/24-hour time format;
- preferred day start/end;
- preferred work window;
- daily work target;
- idle threshold;
- launch at login; and
- minimize/close-to-tray behavior.

### 15.2 Focus

- default focus duration;
- optional short/long break durations;
- sound and local notification choices;
- auto-start next focus interval, off by default; and
- distraction notice, off by default and non-blocking.

### 15.3 Activity and privacy

- master capture toggle;
- application capture;
- browser connector status;
- page-level capture;
- window-title capture;
- exclusions;
- private schedules;
- application/domain/page rules;
- retention period; and
- delete activity history by range/source.

Changing capture detail affects future records. Historical deletion or
reclassification is always a separate explicit action.

### 15.4 Categories

- work kinds;
- life areas;
- classification labels and colors while preserving stable semantic kinds; and
- project colors.

### 15.5 Data

- local database location;
- database size and observation record count;
- create backup;
- automatic local backup schedule;
- restore backup;
- export data;
- retention cleanup preview; and
- permanent deletion/reset.

### 15.6 Appearance and accessibility

- dark/light/system theme;
- compact/comfortable density;
- reduced motion;
- high contrast;
- week-strip intensity scale; and
- chart pattern/label options independent of color.

---

## 16. Local data management

### 16.1 Storage

The production store is an embedded SQLite database managed by the desktop host.
The web UI never accesses the database or filesystem directly.

### 16.2 Backup

A complete backup includes source records, rules, settings, schema version, and
locally cached icons required to render saved identities. Derived caches may be
excluded because they are rebuildable.

Requirements:

- transactional snapshot;
- optional automatic daily/weekly local schedule;
- retention count;
- last successful backup visible;
- restore validation before replacing current data;
- safety backup of current data before restore; and
- checksum plus schema compatibility validation.

### 16.3 Export

- Full structured JSON for portability.
- CSV by major entity and report table.
- Private intervals export without hidden source detail.
- Observation export warns about sensitivity and supports date/source filters.
- Export contains timezone and units.

### 16.4 Retention and deletion

- Tasks and verified work persist until explicitly deleted.
- Raw observation retention is user-configurable; default proposed value is 180
  days, pending user choice.
- Aggregates derived solely from deleted raw evidence must either be rebuilt or
  marked unavailable; they must not preserve secretly deleted detail.
- Deletion preview states affected dates, durations, rules, reviews, and reports.
- Secure deletion cannot be absolutely guaranteed on SSD/storage snapshots; the
  product explains this limitation honestly.

---

## 17. Notifications and reminders

All reminders are local and configurable.

P0 notifications:

- timer still running after configurable duration;
- recovered running session after restart;
- optional scheduled-block start reminder;
- optional end-of-day review reminder; and
- capture adapter needs attention.

P1:

- optional weekly review reminder;
- optional long continuous-work break suggestion; and
- optional mismatch notice when sustained Distraction evidence overlaps a focus
  interval.

Notifications never expose website/page detail on the lock screen by default.
They do not block access or shame the user.

---

## 18. Accessibility and interaction quality

### 18.1 Keyboard

Every primary action is keyboard reachable with visible focus. Drag operations
have equivalent move/resize/reorder commands.

### 18.2 Screen readers

- Week cells announce date, selected/today state, planned count, tracked count,
  completed count, and pending count.
- Charts have a text summary and data table.
- Timer updates do not announce every second; status changes are announced.
- Classification controls announce scope and current effective rule.

### 18.3 Color and motion

- No status depends only on color.
- Productive and Distraction use text/icon/pattern in addition to hue.
- Reduced-motion setting removes non-essential transitions.
- Minimum text and control contrast follows WCAG 2.2 AA targets.

### 18.4 Responsiveness

Primary target is desktop width 1280px and above. At narrower widths, Today
stacks tasks above schedule while preserving the common selected date. No P0
action becomes hover-only.

---

## 19. Error, recovery, and diagnostic behavior

### 19.1 General rules

- Never discard typed task/project content after a recoverable error.
- State whether an operation failed, remained pending, or was rolled back.
- Use stable, non-technical messages with expandable local diagnostics.
- Do not place sensitive activity names in generic logs.

### 19.2 Recovery cases

| Case | Required behavior |
|---|---|
| App crashes with timer open | Recover the open session; ask to keep, edit, or void if duration is suspicious. |
| App sleeps with timer open | Preserve elapsed wall time and flag the sleep interval for review. |
| Observer crashes | Continue timers; mark capture gap; restart adapter when safe. |
| Browser connector disconnects | Continue app-level browser observation and show connector status. |
| Database transaction fails | Roll back fully and keep UI draft/action recoverable. |
| Migration fails | Preserve pre-migration backup and open recovery instructions. |
| Disk full | Stop writing safely, warn immediately, and avoid claiming capture continuity. |
| System clock changes | Use monotonic/runtime safeguards for running display and persisted UTC boundaries; flag anomalies. |
| Corrupt backup | Refuse restore without changing current data. |

---

## 20. Non-functional requirements

### 20.1 Offline and network

- All P0 workflows function with the network adapter disabled.
- No telemetry, analytics, crash upload, ads, remote fonts, or remote AI.
- Browser connector communicates only with the local native host.

### 20.2 Performance targets

- Warm launch to usable Today: under 2 seconds on the owner's target machine.
- Common local command feedback: under 100 ms perceived, under 300 ms committed.
- Today and project pages with 10,000 tasks: first useful render under 1 second
  after database open using pagination/virtualization where needed.
- Activity day with 50,000 raw spans: summary under 1 second from maintained
  local aggregates; timeline progressively renders.
- Week report: under 1 second when affected daily projections are cached.
- Month report: under 2 seconds under normal personal data volume.
- Idle background CPU target: below 1% average outside event bursts.
- Memory target: below 200 MB in typical Today use, subject to measurement.

### 20.3 Reliability

- Timer transaction survives UI reload and route changes.
- No single minute contributes to more than one Day Ledger accounting state.
- Backups restore to equivalent canonical totals.
- Database migrations are forward-only, versioned, and tested.

### 20.4 Security

- Least-privilege Tauri capabilities.
- UI has no arbitrary shell or filesystem access.
- Content Security Policy blocks remote content by default.
- Local IPC validates all inputs.
- Exclusion/privacy filters run before observation persistence.
- Backup/export warns when destination is not private or removable where the OS
  makes that detectable.

---

## 21. Functional requirement index

| ID | Priority | Requirement |
|---|---:|---|
| FR-TOD-001 | P0 | Open to Today with current local date selected. |
| FR-TOD-002 | P0 | Show separate weekday planned, tracked, and completed states. |
| FR-TOD-003 | P0 | Manage Top 3 and additional daily tasks. |
| FR-TOD-004 | P0 | Display and edit selected-day scheduled blocks. |
| FR-PRJ-001 | P0 | Create, edit, order, archive, and restore projects. |
| FR-PRJ-002 | P0 | Create and manage tasks inside a project or Inbox. |
| FR-PRJ-003 | P0 | Group/filter/sort tasks without duplicating entities. |
| FR-PRJ-004 | P0 | Preserve history when tasks move projects. |
| FR-TSK-001 | P0 | Store estimate, intended date, priority, notes, and project. |
| FR-TSK-002 | P0 | Support multiple scheduled blocks and sessions per task. |
| FR-TSK-003 | P0 | Require explicit task completion and allow reopening. |
| FR-TMR-001 | P0 | Permit exactly one running task session. |
| FR-TMR-002 | P0 | Recover an open session after restart. |
| FR-FOC-001 | P0 | Track focus intervals within a running session. |
| FR-ACT-001 | P0 | Opt-in capture of foreground application and idle boundaries. |
| FR-ACT-002 | P0 | Display application activity by day and timeline. |
| FR-ACT-003 | P0 | Display domain activity when local browser connector is available. |
| FR-ACT-004 | P0 | Label intervals and create separate future source rules. |
| FR-ACT-005 | P0 | Apply page > domain > app specificity with interval override highest. |
| FR-ACT-006 | P0 | Filter excluded/private detail before persistence. |
| FR-REV-001 | P0 | Review and explicitly verify task sessions. |
| FR-REV-002 | P0 | Build a complete non-double-counted Day Ledger. |
| FR-REV-003 | P0 | Close and reopen day review without fabricating time. |
| FR-RPT-001 | P0 | Provide consistent Week and Month reports. |
| FR-RPT-002 | P0 | Show completeness and pending states on all work claims. |
| FR-RPT-003 | P0 | Drill every P0 metric into source records. |
| FR-RPT-004 | P0 | Compare period values using explicit equivalent boundaries. |
| FR-RPT-005 | P0 | Export visible metrics and definitions locally. |
| FR-DAT-001 | P0 | Persist canonical data in local SQLite. |
| FR-DAT-002 | P0 | Create, validate, and restore local backups. |
| FR-DAT-003 | P0 | Export user-owned structured data. |
| FR-SET-001 | P0 | Configure capture, privacy, time, and data behavior locally. |
| FR-ACC-001 | P0 | Provide keyboard and non-color equivalents for primary actions. |
| FR-GOL-001 | P1 | Support at-least and at-most period goals. |
| FR-CAL-001 | P1 | Produce sample-backed estimate calibration. |
| FR-NTF-001 | P1 | Offer local, silenceable reminders. |

---

## 22. Critical business invariants

1. Estimate, schedule, tracked session, focus, observation, verification, and
   completion are different records.
2. One task can have many sessions and many scheduled blocks.
3. Only one task session runs at once.
4. Focus time is contained in tracked time.
5. Observation time is evidence and never automatically verified work.
6. Productive classification is not task assignment or verification.
7. The user explicitly verifies sessions and completes tasks.
8. Rule changes are prospective unless historical scope is separately chosen.
9. Interval override outranks page, domain, and application rules.
10. Website context and browser application context describe overlapping time
    and are never added together.
11. Verified work/life, private, tracked pending, observed unconfirmed, and
    unaccounted sum to the exact local period after overlap resolution.
12. Archived entities continue contributing to historical reports.
13. Source edits invalidate affected review and report projections.
14. Private/excluded detail is filtered before storage where configured.
15. No core workflow depends on a network response.

---

## 23. Delivery plan

### Slice 1 — Today foundation (implemented prototype)

- Tauri/React shell;
- Today week strip and allocation;
- starter task list and schedule;
- basic task timer, completion, and local prototype repository; and
- core projection tests.

Prototype local storage is temporary and does not satisfy production persistence.

### Slice 2 — Projects and production persistence

- SQLite repository and migrations;
- project portfolio and project detail;
- full task create/edit/move/archive flows;
- task detail and session history;
- schedule editing and stable timer recovery; and
- backup foundation.

Exit criterion: the owner can manage real projects for seven days without data
loss or a parallel task system.

### Slice 3 — Reports foundation

- metric registry and Day Ledger engine using task/session data;
- week/month period shell;
- verified/pending totals, project allocation, plan variance, focus, completion,
  completeness, and drill-down; and
- CSV/JSON report export.

Exit criterion: task and session reports reconcile exactly with source records.

### Slice 4 — Activity capture and classification

- Windows application/idle adapter;
- Activity applications view and timeline;
- local browser connector and website view;
- Productive/Communication/Neutral/Distraction/Private labels;
- interval overrides and app/domain/page rule management; and
- privacy exclusions and retention.

Exit criterion: at least 90% of eligible active PC time is observed during a
controlled day and classification scope behaves predictably.

### Slice 5 — Reconciliation and integrated reports

- aligned Daily Review;
- verification/edit/split/reassign flows;
- full 24-hour Day Ledger;
- activity/report integration;
- conclusions, goals, and calibration; and
- complete backup/restore equivalence.

Exit criterion: seven consecutive days can be reviewed in under five minutes
each and the week report is trusted.

### Slice 6 — Polish

- accessibility and keyboard completion;
- performance at long-history scale;
- optional local notifications;
- print/PDF report export; and
- installer, update policy, and recovery documentation.

---

## 24. End-to-end acceptance scenarios

### Scenario A — Project to completed verified task

1. Create project “Personal Software.”
2. Add task “Design activity labels,” estimate 2h, intended tomorrow.
3. Add 10:00–12:00 block.
4. Start at 10:08, focus 10:15–10:55, stop at 11:05.
5. Start again at 14:00 and stop at 14:40.
6. Review both sessions, inspect activity evidence, and verify them.
7. Complete task.

Expected:

- estimate remains 2h;
- schedule remains 2h;
- tracked and verified equal 1h 37m;
- focus equals 40m and is not added to tracked time;
- task has two sessions;
- completion is explicit; and
- project/week/month drill-down shows the same records.

### Scenario B — Forgotten timer discovered through Activity

1. Work in an editor and documentation website for one hour without a timer.
2. Open Activity; editor/domain evidence is visible.
3. Label the selected documentation interval Productive only for that interval.
4. In Review, create a manual session assigned to a task and verify it.

Expected:

- weekday strip originally shows planned-untracked;
- Productive label alone does not create task time;
- manual session becomes tracked/verified only after user action;
- evidence remains linked by time overlap; and
- the exact hour is counted once.

### Scenario C — Browser rule specificity

1. Set browser application to Neutral.
2. Set `github.com` to Productive.
3. Set `youtube.com` to Distraction.
4. Label one YouTube lecture interval Productive without creating a page rule.

Expected:

- GitHub is Productive;
- YouTube is Distraction generally;
- selected lecture interval is Productive;
- future YouTube remains Distraction; and
- no browser/domain durations are double-counted.

### Scenario D — Incomplete week report

1. Open current week with two reviewed days, one pending session, and three
   future days.

Expected:

- report states partial period and 2 reviewed elapsed days;
- verified work excludes pending time;
- pending time is adjacent and visible;
- future days are not treated as zero-performance failures; and
- comparison uses equivalent elapsed period when selected.

### Scenario E — Backup and restore

1. Create a backup after project, task, activity, rules, and reviews exist.
2. Restore it into a clean local profile.

Expected:

- source counts and canonical totals match;
- running session is not incorrectly resumed from backup;
- rules and privacy settings are present;
- cached report projections can rebuild; and
- no network is used.

---

## 25. Product success criteria

Because Fruit has no telemetry, these are owner-observed or locally computed:

- Today becomes the primary daily planning surface.
- The user creates and maintains real projects without a parallel task manager.
- At least 90% of eligible active PC time is observed when capture is enabled.
- At least 80% of waking time is verified, intentionally private, or
  deliberately accepted as unaccounted during a seven-day trial.
- Typical daily review approaches 90 seconds and remains below five minutes.
- Week/month totals, source drill-down, backup, and exports reconcile.
- The weekly report produces at least one accepted change to the next plan.
- The user trusts the record enough to keep it locally over multiple months.

---

## 26. Open product decisions

Decisions to validate during implementation without changing the core model:

1. Default raw-observation retention: 90, 180, or unlimited days.
2. Whether page identity uses normalized path, opt-in title, or both.
3. Default idle threshold and how sleep intervals are proposed in Review.
4. Whether Communication is a top-level quick label or a Productive subtype in
   the first Activity release.
5. Which completed-task window Projects expands by default.
6. Whether manual task sorting and priority sorting can be combined clearly.
7. Minimum sample and cohort strategy for estimate calibration.
8. Exact print/PDF report design and whether it is P1 or P2.
9. Installer/update method that preserves the no-silent-network promise.
10. Whether recurring tasks enter P1 or remain out of scope.

Until decided, implementation must preserve data compatibility and avoid
silently choosing a privacy-expansive default.

---

## 27. Glossary

| Term | Definition |
|---|---|
| Estimate | Expected effort for a task. |
| Intended date | Date the user currently expects to work on a task. |
| Scheduled block | Planned start/end time linked to a task. |
| Task session | Deliberately tracked actual interval for one task. |
| Focus interval | Deep-work interval contained inside a task session. |
| Observation | Locally captured foreground application/website evidence. |
| Classification | Productive, Communication, Neutral, Distraction, Private, or Unclassified meaning applied to evidence. |
| Interval override | Classification for a selected historical span only. |
| Source rule | Prospective application/domain/page classification. |
| Verified session | Task session explicitly accepted by the user. |
| Tracked pending | Closed session not yet verified. |
| Observed unconfirmed | Evidence not converted to verified work/life or private. |
| Unaccounted | Time with no retained or confirmed explanation. |
| Day Ledger | Non-overlapping projection accounting for the exact local day. |
| Review completeness | Visibility into which days and intervals received user judgment. |
| Productive observation | Evidence classified Productive; not automatically verified work. |
| Distraction observation | Evidence classified Distraction; not a moral judgment or blocker. |

---

## 28. Final product acceptance statement

Fruit is ready for core personal use only when the owner can, entirely offline:

1. create projects and tasks;
2. estimate, date, prioritize, and schedule them;
3. track tasks over multiple sessions and focus intervals;
4. observe opted-in applications and websites locally;
5. label evidence with clearly scoped interval decisions or future rules;
6. review and verify the day's record without double-counting;
7. complete outcomes independently of time spent;
8. understand week and month results with visible completeness and arithmetic;
9. drill every important claim into source records; and
10. back up, restore, export, and delete the data without a service account.

If any report requires hidden inference, any capture bypasses consent, or any
core workflow fails offline, the product does not yet satisfy this specification.
