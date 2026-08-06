# Product Specification

**Working name:** Fruit (repository name; the plan calls it *Harmonized Offline
Time Planner and Tracker*)
**Baseline:** Project Plan Revision 3 — 4 August 2026
**Release target:** Windows-first MVP, 12 weeks
**Status of this document:** the specification of record. Where it disagrees
with anything else in this repository, this document wins and the other file
needs updating.

---

## 0. How this document relates to what is already built

The repository currently contains a working implementation of *Fruit —
Technical Product Specification v2*: a planner, timer, reconciler and
calibrator with opt-in application-level activity observation. That build is
**one of four inputs** to this product, not the product itself.

| Input | What it contributes |
|---|---|
| **Fruit v2** (built, in this repo) | The Plan→Track→Reconcile→Calibrate loop, plan/record separation, drift, the timer state machine, privacy architecture, the testable core |
| **The workbook** (client's Excel) | The 24-hour day table, life areas, targets vs actual, monthly reporting, the Excel export format |
| **Rize** | Automatic PC activity observation, including browser domains |
| **Super Productivity** | Projects, tasks, estimates, timers |

The plan reorders the product around the workbook's day table. That is the
single biggest change: **the Planner stops being the primary screen.**

---

## 1. What does the app do?

> **A local-first Windows application that shows how the user planned to spend
> the month, what they actually did across work and life, where PC
> entertainment displaced intention, and how to make the next plan more
> realistic.**

### The primary outcome

**Reduce unplanned PC entertainment**, while producing a trustworthy monthly
account of work, personal activities, sleep/rest, and unaccounted time.

That is a behaviour-change goal, not a reporting goal, and it is what
distinguishes this product from a time tracker. The reports exist to make the
behaviour visible; the Day view exists to make the record cheap enough to keep.

### Secondary outcomes

- More accurate project estimates, via planned-versus-tracked drift.
- Less daily effort to maintain the monthly record than the workbook costs today.
- One offline replacement for tracking currently spread across several tools.
- The user's existing Excel reporting workflow preserved.

### The loop

```
PLAN ──▶ TRACK ──▶ RECONCILE ──▶ CALIBRATE ──▶ PLAN BETTER
  │        │           │             │
  │        │           │             └─ estimate accuracy, recurring patterns
  │        │           └─ confirm gaps, overruns, unplanned and observed-only time
  │        └─ timers, manual life entries, and automatic PC observation
  └─ projects, tasks, life targets, scheduled blocks
```

### What it deliberately does not do

No sync, accounts, cloud, web, mobile, macOS or Linux. No collaboration, team
workspaces, or manager reporting. No personal-notes system, wiki, Markdown
editor, or Obsidian integration. No role/KPI/value scoring. No expense, loan or
income tracking. No AI scheduling. No telemetry or crash uploads. No calendar
write-back, plugin API, website blocking, or tamper prevention.

---

## 2. Who uses it?

A **privacy-conscious solo knowledge worker on Windows** who plans and reports
their own time, and who wants both project tracking and a broader account of
where their life's hours go.

They:

- want automatic evidence of PC use, but the final say on every classification;
- use the keyboard heavily and will learn shortcuts;
- need tasks only to support planning and tracking — not GTD, not kanban;
- need a **detailed day** for correction and a **month-first dashboard** for
  seeing patterns;
- expect every byte of activity data to stay on the machine.

### The primary task

> **Plan the month. During each day, let the timer and the observer do most of
> the recording. At the end of the day, spend ninety seconds confirming what
> the app got right and filling in what it could not see.**

The design target for daily reconciliation is **90 seconds**; the accepted
ceiling after the learning period is **five minutes**. If reconciliation stops
happening, the monthly account stops being trustworthy and the product's whole
claim fails — so every feature is judged partly on whether it makes those
ninety seconds shorter.

### Non-users

Teams, managers reporting on someone else's time, anyone wanting automated
tracking with no confirmation step. Observation never becomes a confirmed
record on its own.

---

## 3. What are the key screens?

Navigation order, which is also priority order:

**Day · Planner · Projects/Tasks · Activity · Reports · Settings**

Persistent shell throughout: timer chip, OFFLINE indicator, Recording
indicator, Reconcile action, Focus action.

### 3.1 Day view — the primary operational screen

A complete 24-hour table for one date, modelled on the workbook's time grid.
This is where the user spends their reconciliation time, and it is the screen
the product is organised around.

- **One row per 30-minute slot** by default; zoom to 5, 15, 30 or 60 minutes
  **without changing stored precision**. The slot size is a lens, never a
  quantisation of the data.
- **Aligned layers per row**: planned block · confirmed actual (project/task or
  life activity) · observed PC app/domain · classification.
- **All 24 hours are always present, including empty ones.** Empty time is a
  real state with its own visual treatment. It is never silently rendered as
  "None", and it never disappears because nothing happened.
- Non-colour indicators for planned, confirmed, observed, idle, private and
  empty — the states must survive a greyscale screenshot.
- Current-day and current-time marker.
- Editing: drag, keyboard, split, merge, fill, repeat, multi-select.
- Selected-day totals: work, each life area, sleep/rest, entertainment, PC use,
  and gaps.
- Filters: project, life area, work contribution, entertainment, confidence
  state.
- Previous/next date and "go to today".

### 3.2 Planner — secondary

The existing 24-hour planning grid, at **3-day, 7-day and month** spans. Blocks
are intentions; actual sessions draw a drift rail against them. Drag and resize
with 15-minute snap and full keyboard equivalents. Fixed and repeating blocks.
Planned-but-unstarted and unplanned-but-tracked both stay visible.

*(The 1-day span is dropped: the Day view replaces it and does more.)*

### 3.3 Projects and tasks

Projects with colour, archive state, weekly/monthly target and **one compact
plain-text note**. Tasks with title, project, status, estimate, priority, due
date/time, tags and **one compact plain-text note**. Subtasks to three levels,
tracked independently, rolled up for display. Backlog groups: Overdue · Today ·
This week · No date · Someday · Completed. Quick capture, timer, manual session
correction, Pomodoro and Focus.

No wiki, no Markdown editor, no attachments, no general notes area.

### 3.4 Activity

Opt-in foreground application and idle observation. **Window titles are a
separate control, off by default.** A local browser-domain connector for
supported browsers, communicating only with the local application. An
against-the-plan view showing which apps and domains appeared during a planned
block. Per-app and per-domain totals on the same time axis as the Planner.
Exclusions applied before storage, retention choices, a pause that survives
restart, delete-recent and delete-all.

### 3.5 Reports and reconciliation

**Dashboards and reports open to the month horizon by default**, with day and
week drill-down. Daily reconciliation covering overruns, unstarted plans,
unplanned work, observed-only PC time and empty hours. Trailing 30-day estimate
calibration (median ratio, minimum sample threshold). Planned vs tracked by
project and week. Life-area and sleep/rest target vs actual. Planned vs
unplanned entertainment with YouTube/Twitch trends. Work contribution
summaries — **which never apply to personal time**.

**The week horizon** (planned — [`PLAN-WEEKLY-GOALS.md`](PLAN-WEEKLY-GOALS.md)):
a goal is a target with a **direction**, so "at most 5h of entertainment" is a
goal you succeed at by being under it rather than a bar you are failing to fill.
Reported by **pace** rather than as a scoreboard — where you should be right now,
and what the rest of the week has to look like. Fragmentation reported as its
components — longest unbroken stretch, planned versus unplanned switches, time in
fragments — and deliberately **not** synthesised into a score, because every other
number in this app can be checked by hand. A weekly review and report artifact,
read at a fixed moment, headline first. Next week's goals pre-filled from what
happened, at the same n ≥ 5 median discipline the estimate calibration uses.

**Observation categories are user-definable** (built, migration 0007). The fixed
`core`/`entertainment`/`other` split becomes a table a user can extend, spanning
**both applications and domains**, with a `counts_as` roll-up so adding a category
never moves an existing total. This is the one place the plan changes an existing
component rather than adding beside it, and it is there because the most valuable
thing in the source review was a category its user invented to answer a question
no shipped report could have anticipated.

An **off-plan nudge** — "you are on youtube.com during a block you plotted for the
auth refactor" — silenceable for the session and never fired on time nobody
plotted. It is a notice, not a block: Fruit will not close a tab or deny a
navigation, and the connector has no `host_permissions` with which to try.

Blocking, focus scores, per-URL rules, focus sounds, billing and team visibility
are out of scope; see the plan document for why each was rejected rather than
deferred.

### 3.6 Settings

General · Planner/month · Timer · Pomodoro · Activity privacy · Entertainment
rules · Data and backup · Excel · Shortcuts · About.

One command registry powers the palette, the keyboard handler and the shortcut
sheet, so an action that is not reachable both ways cannot exist.

---

## 4. What data does it handle?

### 4.1 Four record types, one timeline

This is the central data decision and everything else follows from it.

| Record | Meaning | Confirmed actual time? |
|---|---|---|
| `scheduled_block` | What the user intended to do, and when | **No** — it is the plan |
| `time_session` | Confirmed work on a project/task | **Yes** |
| `life_entry` | Confirmed non-work time: life area, sleep/rest, routine | **Yes** |
| `activity_span` | Automatic foreground app/domain observation | **Observed only** — confirmed only by reconciliation or an explicit rule |

**Precedence when sources overlap:**

1. confirmed `life_entry`
2. confirmed `time_session`
3. observed `activity_span`
4. empty / unaccounted

The planned block is a **separate overlay** and is never substituted for actual
time. An observation overlapping a confirmed session **enriches** it with
application evidence — it does not add a second duration.

### 4.2 The counting invariant

> For any local date, the confirmed, observed-only, idle, private and empty
> durations **sum to exactly the length of that day** — 24 hours, or 23 or 25
> across a DST transition — and no interval is counted twice.

This is enforced in the core as a property test over random overlapping
records, not asserted in the UI. It is the technical form of the product's
promise, and MVP acceptance criteria 2, 4 and 8 all reduce to it.

### 4.3 Required slot states

Every Day-view slot can visibly be:

planned and completed as intended · planned with overrun · planned with
underrun · planned but never started · unplanned confirmed activity ·
observed but unconfirmed · idle/away · sleep/rest · intentionally
private/untracked · **empty/unaccounted**.

Empty time stays visible until the user fills it, marks it private, or
deliberately accepts the gap.

### 4.4 Entity map

```
    ┌──────────┐                      ┌────────────┐
    │ project  │ target               │ life_area  │ target, kind, colour
    └────┬─────┘                      └─────┬──────┘
         │ 0..1                             │ 1
    ┌────▼─────┐   ┌─────┐                  │
 ┌──│   task   │──▶│ tag │            ┌─────▼──────┐
 └─▶│          │   └─────┘            │ life_entry │  CONFIRMED LIFE TIME
    └──┬────┬──┘                      │ is_private │
       │    └──▶ note (plain text)    └────────────┘
       │ 0..n
┌──────▼──────────┐  block_id  ┌────────────────┐
│ scheduled_block │◀───────────│  time_session  │  CONFIRMED WORK TIME
│  THE PLAN       │  nullable  │  contribution  │
└─────────────────┘            └───────┬────────┘
                                       │ running_session_id
  ┌───────────────┐  ┌──────────────┐  │  ┌────────────┐
  │  day_review   │  │ activity_span│  └─▶│ app_state  │
  │ 1 per date    │  │ OBSERVED     │     └────────────┘
  └───────────────┘  │ app, domain, │
                     │ category     │     ┌────────────┐
                     └──────────────┘     │  setting   │
                                          └────────────┘
```

### 4.5 Entities

| Entity | Answers | Key fields |
|---|---|---|
| `project` | what body of work | `name`, `colour`, `weekly_target_sec`, `monthly_target_sec`, `note`, `is_archived` |
| `task` | what am I trying to get done | `title`, `status`, `estimate_sec`, `is_rollover`, `due_date`/`due_at`, `priority`, `parent_id`, `note` |
| `tag` / `task_tag` | what kind of work | a real table — renamable and queryable |
| **`life_area`** | which part of life | `name`, `colour`, `kind` (core / entertainment / rest / other), `monthly_target_sec` |
| `scheduled_block` | **what I meant to do, and when** | `starts_at`, `duration_sec`, `local_date`, `tz`, `is_fixed`, `rrule`, `series_id`, `external_uid` |
| `time_session` | **confirmed work** | `started_at`, `ended_at`, `elapsed_sec`, `heartbeat_at`, `source`, `contribution`, `block_id` |
| **`life_entry`** | **confirmed non-work time** | `life_area_id`, `label`, `started_at`, `ended_at`, `local_date`, `tz`, `is_private`, `note` |
| `activity_span` | **observed PC use** | `started_at`, `ended_at`, `app_id`, `window_title`, `domain`, `category`, `is_idle` |
| `day_review` | this day is reconciled | one row per local date, plus the day's totals |
| `app_state` | is a timer running | singleton |
| `setting` | preferences, rules, undo tombstones | typed key/value |

**Default life areas**, from the workbook: Personal/Spiritual · Family ·
Wellbeing · Personal Development · Community Participation · Side Gig/Personal
Admin · Friendship · Team Time · Fun · Sleep/Rest. Users may add their own.

**Work contribution modes**, work records only: None · Attend · Support · Own ·
Assist. Converting a work record to a life entry clears its contribution after
confirmation. Life-area reports never group by contribution.

### 4.6 Data rules

1. Instants are `INTEGER` milliseconds, UTC.
2. Calendar dates are `TEXT 'YYYY-MM-DD'`, **local**.
3. Durations are `INTEGER` seconds.
4. Ids are UUIDv7.
5. Anything derivable is derived; caches are rebuildable from source records.
6. Deletes are soft, with one documented exception (sessions carry a tombstone).
7. **Plans and records never merge.**
8. **The four record types never merge**, and precedence resolves overlap at
   read time rather than by mutating anything.
9. A session covers one contiguous *awake* interval.
10. Activity exclusions are applied **before storage**, never on read.

### 4.7 Entertainment classification

`youtube.com`, `youtu.be` and `twitch.tv` are Entertainment by default, applied
to observed browser time when domain tracking is on. The user can override any
interval, domain, application, project, task or recurring pattern. A correction
may create a **prospective** local rule; it never rewrites prior records
without confirmation.

Full URLs, page contents, searches, messages and video titles are **not
required for the default rule and are not stored by default.**

### 4.8 Data in and out — Excel first

**Export** is a real `.xlsx` workbook: a month sheet visually close to the
client's workbook, the complete time matrix *including blank slots*, daily and
weekly totals, work by project/task and contribution, life-area and sleep/rest
target vs actual, core / planned-entertainment / unplanned-entertainment /
YouTube-Twitch totals, auditable formulas, and a note identifying which records
are confirmed, observed or imported.

**The export must never depend on cell fill colours for calculation.** Colour
communicates category; structured values produce totals. Correcting that is one
of the main reasons to replace the workbook.

**Import** starts with a mapping and variance preview, never alters the source
file, and requires the user to resolve duplicate or inconsistent periods before
commit. **JSON** remains the exact backup/restore format. **CSV** is secondary.
Automatic local snapshots daily, seven retained.

---

## 5. What are the constraints?

### 5.1 Platform

Windows 10+ only for MVP. macOS and Linux are explicitly out of scope. The
existing cross-platform code is retained but untested and unsupported — see
§5.8.

### 5.2 Stack

```
UI          React 19 · TypeScript · Vite · Zustand · Tailwind v4
Desktop     Tauri v2
Core        Rust, no UI dependency — fruit-core
Storage     Embedded SQLite (rusqlite, bundled), forward-only migrations
Browser     Minimal local Chrome/Edge connector          — MV3 extension + native-messaging host
Export      Offline XLSX generation + JSON backup        — rust_xlsxwriter
Infra       None. No backend, account, telemetry, CDN or database server.
```

### 5.3 Architectural rules

- The UI does not own elapsed time, execute SQL, or compute authoritative
  totals.
- Time, overlap, idle, recovery, recurrence, reconciliation and export
  invariants live in the testable core.
- Plans and records never merge; the four record types never merge.
- Activity exclusions are applied before storage.
- **No unexpected outbound network request, ever.**
- Every derived summary can be regenerated from source records.

The `fruit-core` / `src-tauri` split is load-bearing: because the core has no
Tauri dependency, the counting invariant in §4.2 is a property test rather than
something verified by clicking around an app on a machine with a webview.

### 5.4 Third-party integrations

| Integration | Direction | Notes |
|---|---|---|
| Windows foreground window + idle | Read | `GetForegroundWindow`, `QueryFullProcessImageNameW`, `GetLastInputInfo`. No permissions, no new crates. |
| **Browser domain connector** | Read, local only | A minimal Chrome/Edge MV3 extension talking to the local app over **native messaging** — no listening socket, no host permissions, so it cannot read a page. **Required** for the YouTube/Twitch goal: application-level observation cannot distinguish them from any other tab. Built and spiked; see [`SPIKE-BROWSER-CONNECTOR.md`](SPIKE-BROWSER-CONNECTOR.md). |
| `.ics` calendar files | **In only** | Local file, user-picked. No URL, no CalDAV, no account, no write-back. |
| Excel `.xlsx` | In and out | Offline generation and parsing. No Office installation required. |
| Everything else | — | None. |

Fonts are bundled, never fetched — a font request is a network request.

### 5.5 Security and privacy

- No SQL reaches the renderer; `src/lib/ipc.ts` is the only path to the
  backend and every command is typed and intent-based.
- The capability file lists exactly the commands in use — no `sql:*`, no broad
  `fs:*`. File pickers are Rust commands, so the webview receives a chosen path
  and nothing else.
- Explicit CSP, `default-src 'self'`, asset protocol disabled.
- The browser connector is local-only, minimum-permission, and transmits a
  **registrable domain and nothing else** — no URL, no query, no page content,
  no title. The reduction happens twice: in the extension before anything is
  sent, and again below the IPC boundary, because the extension is the part a
  user can swap. It has its own switch, off even when application tracking is
  on, and its own exclusion list matched after reduction so an entry covers
  every subdomain. Its transport is native messaging rather than a localhost
  port: an app badged OFFLINE cannot open a listening socket.

### 5.6 Performance and quality budgets

| Budget | Target |
|---|---|
| Cold start to interactive | < 1.5s |
| **Day view, populated 24-hour day** | **< 100ms** |
| **Month dashboard, populated 31-day month** | **< 250ms** |
| Week load, 500 blocks | < 100ms |
| Idle CPU, no timer and no sample due | ~0% |
| Data loss after forced close, sleep/wake or restart with a running timer | none |

Full keyboard operation, visible focus, reduced-motion support, no important
state by colour alone, usable from 960×640 and at 125% Windows text scaling.

### 5.7 Success measures

Baselined over the first seven days of production use:

1. ≥90% of active PC time observed automatically when Activity is enabled.
2. ≥80% of waking time classified or deliberately left unaccounted by daily
   reconciliation.
3. Daily reconciliation ≤5 minutes after the learning period; target 90 seconds.
4. Every 30-minute slot accounted for without double-counting.
5. ≥95% correct YouTube/Twitch classification after user exceptions.
6. A measurable four-week reduction in unplanned entertainment.
7. Excel exports reconcile to application totals with no unexplained variance.
8. The product works with no account, server or internet connection.

### 5.8 Open decisions

Carried from the plan's §17 and still unanswered. Defaults chosen for now are
in bold; each is reversible.

1. Windows-only, or must future macOS/Linux shape the architecture now?
   *Working assumption:* **Windows-only for MVP, cross-platform code retained
   but unsupported** — deleting it would cost more than leaving it.
2. Supported browsers: Chrome, Edge, or both? *Working assumption:* **both**,
   since a single Chromium extension covers them.
3. Final contribution list, and what "None" means versus non-work time.
   *Working assumption:* **None · Attend · Support · Own · Assist**, with
   "None" meaning *work with no contribution mode recorded*, distinct from
   non-work time, which has no contribution field at all.
4. Day view default resolution and how empty time reads. *Working assumption:*
   **30-minute rows; empty renders as a hatched, labelled "Unaccounted" row**,
   because blank is indistinguishable from "not loaded".
5. Entertainment intervention: notification-only, or a soft continue/cancel?
   *Working assumption:* **notification-only for MVP.**
6. Which historical month is the accepted Excel import/export reference?
   **Blocked on the client.**
7. Is the Fruit source in scope for reuse, or only its specification?
   **Answered by the repository: the source exists here and is being reused.**

---

## 6. Where the current build stands against this specification

| Area | State |
|---|---|
| Core, schema, migrations, timers, recovery, backups | **Built** — Phase 2 complete |
| Plan/record separation, drift, reconcile, calibration | **Built** |
| Planner (1/3/7-day) | **Built**; needs the month span, and the 1-day span retires |
| Projects, tasks, subtasks, estimates, backlog | **Built** |
| Recurring blocks, `.ics` import | **Built** |
| Activity: app observation, idle, exclusions, retention | **Built** |
| Life areas and life entries | **Built** |
| Work contribution modes | **Built** — on `time_session` only, so "never on personal time" is structural |
| Day view | **Built** — the primary screen, at 5/15/30/60-minute resolution |
| Month dashboard | **Built** — `get_month` is `get_day` summed, so the two cannot disagree |
| Browser domain connector and entertainment rules | **Built** — three field assumptions remain, listed in the spike report |
| Entertainment **budgets** | **Built** — a weekly goal with `direction = atMost`, migration 0008 |
| Planned entertainment **windows** | **Not built** — the other half of M11 |
| **Weekly goals and pacing** (W1/W2) | **Built** — migration 0008. Direction is first-class; the future is never a shortfall. |
| The rest of the week horizon — fragmentation, weekly review and report, focus sessions, notices, templates | **Built** — [`PLAN-WEEKLY-GOALS.md`](PLAN-WEEKLY-GOALS.md) W3–W6, W9, W10. The report is an `.xlsx` and a card that waits until it has been read; not a PDF and not an email, because an offline app has no mailer. |
| **User-defined observation categories** and the uncategorised surface | **Built** — migration 0007. Work · Study · Distraction · Life, extensible, per app **and** per site. `counts_as` keeps every existing total stable. |
| **Focus sessions with an intended length**, extendable in one key | **Built** — W3. The intended length is a plotted block, so extending shows as an overrun rather than as a larger plan. |
| **Off-plan nudge** (a notice, never a block) | **Built** — W5. Fires only during plotted time, and is silenceable for the session. |
| Excel **export** | **Built** — three sheets, real formulas, a preview that is the sheet |
| Excel **import** | **Built** (M13) — detection, then a mapping nobody can skip, then a signed per-day variance, then a commit that refuses while anything is unmapped or unresolved. Still unproven against the client's own workbook, which is open question 6 below. |
| Task notes | Built as **Markdown**; the plan requires compact plain text |

### A sequencing correction to the plan

The roadmap places the Day view in Phase 4 (weeks 6–7) and life entries in
Phase 5 (week 8). That order cannot work: §8.1 requires the Day view to show
"actual project/task **or life activity**", and acceptance criteria 1, 2 and 9
all depend on life entries existing. The Day view built without them would show
only work and empty hours, which is not the screen being specified.

**`life_area` / `life_entry`, contribution modes, and the precedence engine
must land before or with the Day view.** They are treated here as the first
half of Phase 4 rather than as Phase 5, which does not change the delivery date
— it moves work earlier that Phase 4 would otherwise have blocked on.

---

## Related documents

- [`ROADMAP.md`](ROADMAP.md) — the 12-week phases and what is done against them
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — why the layers and the time model are shaped this way
- [`ACCEPTANCE.md`](ACCEPTANCE.md) — the 16 MVP criteria and what covers each
- [`SPEC-DEVIATIONS.md`](SPEC-DEVIATIONS.md) — departures from the source specifications, and why
