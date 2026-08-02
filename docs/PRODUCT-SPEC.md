# Fruit — Product Specification

**What this document is.** A description of the application as it actually
exists in this repository, written to be read cold by someone who has never
seen it. It is not the original brief. Where the two differ, the brief is
wrong and [`SPEC-DEVIATIONS.md`](SPEC-DEVIATIONS.md) says why.

**Status at time of writing.** P0, P1 and P2 are implemented. 103 Rust tests
green; the renderer is verified in a headless browser at five viewport sizes on
every view. `src-tauri` builds and runs on Windows; macOS and Linux are
unbuilt.

---

## 1. What does the app do?

> **Fruit is a local-first desktop application that records the difference
> between the time you planned to spend on your work and the time you actually
> spent, and uses that difference to make your next plan more accurate.**

That sentence is doing more work than it looks. Three claims inside it are the
whole product:

**"the difference"** — Fruit stores two independent things and never merges
them. A `scheduled_block` is an intention: *I mean to spend 90 minutes on the
auth refactor, Tuesday at 09:00.* A `time_session` is a record: *between 09:04
and 10:47 the timer was running against that task.* The gap between them is
**drift**, and drift is the only number the interface is really about. Every
other feature exists to make drift honest or to make it useful.

**"local-first"** — the database is a SQLite file on the user's disk. There is
no account, no server, no sync, and no network call anywhere in the core loop.
The OFFLINE badge in the title bar is a statement of fact, not a connectivity
indicator.

**"more accurate"** — the loop closes. After thirty days of tracked work, Fruit
can say *"your 2-hour estimates run 1.6× over, from 11 samples"*, which is a
fact about you that you cannot get by trying harder to estimate well.

### The loop

```
PLAN ──▶ TRACK ──▶ RECONCILE ──▶ CALIBRATE ──▶ back to PLAN, better
 │         │           │              │
 │         │           │              └─ trailing 30-day tracked ÷ estimate,
 │         │           │                 bucketed, median, n ≥ 5
 │         │           └─ end of day: what overran, what was never started,
 │         │              what you did that wasn't on the plan
 │         └─ one timer, bound to the block it was started from
 └─ blocks on a 24-hour grid, at 1 / 3 / 7-day spans
```

### What it deliberately does not do

No sync. No accounts. No mobile app. No collaboration or shared workspaces. No
AI scheduling or auto-planning. No plugin API. No web version. No telemetry, no
crash reporting, no analytics of any kind. No writing back to your calendar.

These are not "not yet" items. Every one of them is a thing that would require
the data to leave the machine or would put a second author on the plan, and
both break the premise.

---

## 2. Who uses it?

### The user

A **solo knowledge worker who bills, reports, or budgets their own time** and
who has noticed that their sense of how long things take is unreliable.
Concretely: freelance developers and designers, consultants, researchers,
graduate students, indie founders, and salaried engineers who keep their own
sprint estimates.

Three things are assumed true of them and shape every decision below:

1. **They already have a task manager and are not looking for another one.**
   Fruit's task features exist to make blocks and sessions attachable to
   something meaningful — not to compete with a dedicated GTD app. This is why
   there are no filters-as-saved-views, no recurring *tasks*, no kanban.
2. **They work with a keyboard and dislike surprises.** They will learn
   shortcuts. They will notice a 1-second timer loop in Activity Monitor and
   they will complain about it publicly. They will read the schema.
3. **They are privacy-alert.** "Where does this data go" is a question they ask
   before installing, and "nowhere" has to be verifiable, not asserted.

### The primary task

> **At the start of the day, lay out the hours. During the day, run a timer
> against what you're doing. At the end of the day, spend ninety seconds
> reconciling the two.**

Everything else in the app is in service of that ninety seconds actually
happening. If the user stops reconciling, Fruit degrades into a mediocre timer,
and calibration — the only thing it offers that a calendar doesn't — never
arrives.

### Secondary tasks, in rough order of frequency

| Task | Where it happens |
|---|---|
| Capture a thought without leaving the keyboard | `C` from anywhere → the capture bar |
| Plot a captured task into a specific hour | Drag from the sidebar, or `S` then arrows |
| Correct a session you forgot to start | Task detail → Sessions → add manually |
| Answer "where did last week actually go" | Reports |
| Answer "was I actually doing that, or was I in Slack" | Activity |
| Set up a standing commitment | Select a block → `R` → pick a repeat rule |
| Bring in meetings you don't control | Settings → Data → Import a calendar |

### Explicit non-users

Teams (there is no shared anything), managers reporting on other people's time
(nothing is exportable *about* another person because nothing is shared),
anyone who wants automated time tracking with no timer (Activity observes but
never fills in your sessions for you — see §4).

---

## 3. What are the key screens?

Five primary views on a persistent shell, plus five overlays. The shell — nav
rail, title bar with the timer chip, and a sidebar — never goes away except in
Focus mode.

### Persistent shell

| Element | Content |
|---|---|
| **Nav rail** (52px, left) | Five icons: Planner, Tasks, Activity, Reports, Settings. A dot appears on Planner when a past day is unreconciled. |
| **Title bar** | Brand mark (which is the drift rail as a monogram), OFFLINE badge, timer chip, Pomodoro strip, the Recording indicator when Activity is sampling, a Reconcile button when a day is due, and Focus. |
| **Sidebar** (260px, collapses to 48px below 1130px) | Projects with weekly-target bars, tags, and the backlog. Rows here are drag sources for the Planner. |
| **Detail column** (≥1280px) | The task detail panel, which becomes an overlay sheet below that width. |

### 3.1 Planner — *the primary screen*

A 24-hour vertical grid at 1-, 3- or 7-day spans. Not 07:00–21:00: night
workers are real users and a clipped axis silently loses their data.

- **Blocks** are plates positioned by time and sized by duration. Each one
  carries a **drift rail** — a vertical two-track figure showing planned
  against tracked, with the overrun continuing past the plate's bottom edge
  into the gutter, so an overrun is legible without reading a number.
- **Collisions** lay out side by side in equal columns, up to three, then
  "+n more".
- **Drag** moves and resizes, with three collision policies: default overlaps
  with a warning tint, `Shift` pushes later non-fixed blocks down, `Alt`
  shrinks to fit. `Esc` cancels and writes nothing. 15-minute snap, 5-minute
  with `Alt`.
- **Every drag has a keyboard equivalent.** `S` plots the selected task into
  the next free slot; arrows nudge by 15 minutes; `Shift`+arrows resize.
- **Fixed blocks** (meetings, imported calendar events) are never pushed and
  never auto-shortened.
- **Repeating blocks** carry a `↻` and a dotted right edge. `R` opens the
  repeat picker; `⌫` on one asks which occurrences it means.
- A now-cursor hairline tracks the current minute. It updates on a
  minute-aligned timeout, not a one-second interval.
- Completed blocks recede to grey and keep their rail — what a finished block
  cost is the point of having drawn it.

### 3.2 Tasks

The backlog, in six groups, in this order: **Overdue · Today · This week · No
date · Someday · Completed**. Completed is pinned last, greyed and struck
through, capped at 100 rows, with full contrast restored on hover and focus.

- **Capture bar** at the top parses as you type and shows chips for what it
  found *before* you commit: `Fix login bug #dev ~45m !! ^tomorrow 9am` →
  a title, a tag, a 45-minute estimate, priority 2, and a due date. A chip for
  a project or tag that doesn't exist yet is marked as one that will be
  created.
- **Estimates** are a dropdown on a fixed ladder — 30 min, 1, 1.5, 2, 2.5, 3,
  3.5, 4 hours, then **Rollover** for work that doesn't fit one sitting. A
  value the parser produced that isn't on the ladder is kept as an extra rung
  labelled *(from capture)* rather than being rounded away.
- Each row carries a compact drift rail, so estimate accuracy is visible in the
  list, not only after opening something.
- **Subtasks are tasks**, capped at three levels deep. They schedule and track
  independently and roll up for display only (`3/7 · 45m of 2h`); a parent's
  own estimate is never silently overwritten.

### 3.3 Task detail

Three tabs: **Note** (markdown, autosaved on a 500ms debounce with a 3s max
wait, force-flushed on blur/close/window-hide), **Sessions**, **Subtasks**.

The Sessions tab is where the record can be corrected: add a session you forgot
to start, edit endpoints, re-attach one to a different block, delete one. Manual
and recovered sessions are visually distinguished from timer sessions, because
a record you can't tell apart from a measurement is worse than no record.

### 3.4 Activity *(opt-in, off by default)*

Three panels, and the order is the argument:

1. **Against the plan** — each block on the day with the applications actually
   in front of you while it ran, as a stacked bar plus a sentence: *"Mostly
   Slack · 42m of 1h plotted."* This is the only place in Fruit where an
   intention meets an **observation** rather than another self-report, and it
   is the reason the feature exists.
2. **Where the day went** — per-app totals, longest first, with shares.
3. **The day** — a timeline on the Planner's exact axis and hour height, so the
   two screens can be compared by looking rather than by reading numbers off
   both. It opens scrolled to the first thing that happened.

When Activity is off, this screen says so and offers the switch. When the
platform can't do it, it prints the platform's own reason.

### 3.5 Reports

Three panels, no more.

1. **Calibration** — trailing 30 days of `tracked ÷ estimate`, bucketed by
   estimate size, **median not mean**, reported only at n ≥ 5 per bucket, with
   a plain-language headline. Buckets below the threshold are shown greyed with
   their sample count rather than hidden, so the user can see how close they
   are to a readable number.
2. **Planned vs tracked per project per week** — the same plot/track encoding as
   the drift rail, rotated horizontal. Consistency of encoding across scales is
   what makes this a visual language rather than a set of charts.
3. **Weekly targets** with pace-to-date.

### 3.6 Settings

General (theme, 12/24-hour) · Planner (span, hour height, snap) · Timer (idle
threshold, sleep policy) · Pomodoro · **Activity** (the full privacy contract as
controls — see §4) · **Data** (export JSON/CSV/ICS, import a calendar, integrity
check, backups) · Shortcuts · About.

### Overlays

| Overlay | Trigger | Behaviour |
|---|---|---|
| **Command palette** | `⌘K` | Fuzzy over the single command registry. Every action in the app is here. |
| **Reconcile sheet** | `⌘R`, or the title-bar button | One item at a time, each with a default verb and one-key alternatives. Never blocks the app; `Esc` defers. |
| **Focus mode** | `F` | Full-screen, one task, a large clock, four gradient backgrounds, controls that fade. |
| **Shortcut sheet** | `?` | Generated from the same registry as the palette, so it cannot fall out of date. |
| **Recovery modal** | Automatic | The one `aria-live="assertive"` surface in the app. Shown when Fruit was killed with a timer running. |
| **Block dialogs** | `R`, `⌫` on a series | Repeat picker; series-scope prompt. |

### The rule that governs all of it

**Every action is reachable from the command palette and from a documented
key.** This is enforced structurally rather than by review: there is one
`COMMANDS` registry, and the palette, the keyboard handler and the shortcut
sheet all read it. A command that isn't reachable both ways cannot be written.

---

## 4. What data does it handle?

One SQLite database, WAL mode, `foreign_keys=ON` per connection,
`synchronous=NORMAL`, versioned by `PRAGMA user_version` with forward-only
migrations.

### Entity map

```
                     ┌──────────┐
                     │ project  │──weekly_target_sec
                     └────┬─────┘
                          │ 0..1
                     ┌────▼─────┐        ┌─────┐
              ┌──────│   task   │───────▶│ tag │  (many-to-many, task_tag)
     parent_id└─────▶│          │        └─────┘
     (≤3 deep)       └──┬────┬──┘
                        │    │ 1..1
                        │    └──────────▶┌──────┐
                        │                │ note │  (markdown)
                        │                └──────┘
        ┌───────────────┘
        │ 0..n                          0..n
┌───────▼──────────┐  block_id  ┌────────────────┐
│ scheduled_block  │◀───────────│  time_session  │
│  THE INTENTION   │  (nullable)│  THE RECORD    │
└───────┬──────────┘            └───────┬────────┘
        │ series_id                     │
        │ (self-grouping)               │ running_session_id
        │                       ┌───────▼────────┐
        │                       │   app_state    │ singleton, id = 1
        │                       └────────────────┘
        │
   ┌────▼──────────┐   ┌───────────────┐   ┌──────────────┐   ┌─────────┐
   │  day_review   │   │ activity_span │   │ *_tracked_   │   │ setting │
   │ 1 per date    │   │ (opt-in, P2)  │   │ cache        │   │ k/v     │
   └───────────────┘   └───────────────┘   └──────────────┘   └─────────┘
```

### The core entities

| Entity | It answers | Key fields |
|---|---|---|
| `project` | "what body of work is this part of" | `name`, `colour`, `kind`, `weekly_target_sec`, `is_archived` |
| `task` | "what am I trying to get done" | `title`, `status`, `estimate_sec`, `is_rollover`, `due_date` **or** `due_at`, `priority`, `energy`, `parent_id`, `completed_at` |
| `tag` / `task_tag` | "what kind of work is this" | a real table, so tags are renamable and queryable — not a JSON column |
| `note` | freeform markdown, one per task | `markdown` |
| **`scheduled_block`** | **"what I meant to do, and when"** | `starts_at`, `duration_sec`, `local_date`, `tz`, `is_fixed`, `rrule`, `series_id`, `external_uid` |
| **`time_session`** | **"what actually happened"** | `started_at`, `ended_at`, `elapsed_sec`, `heartbeat_at`, `source`, `is_confirmed`, `block_id` |
| `app_state` | "is a timer running" | singleton row, `running_session_id` |
| `day_review` | "this day has been reconciled" | one row per local date, plus the day's totals and its calibration ratio |
| `activity_span` | "which app was in front" | `started_at`, `ended_at`, `app_id`, `window_title` — opt-in |
| `setting` | typed key/value | preferences, plus undo tombstones |

### The relationship that matters

**`scheduled_block` and `time_session` are separate and never merge.**

A session may exist with **no block** — that is unplanned work, and Reconcile
offers to turn it into a retroactive block. A block may exist with **no
session** — that is something you meant to do and didn't, and it is a finding,
not a missing row. Both are meaningful states the interface renders, not edge
cases it hides.

This single decision is what makes drift computable per block, makes Reconcile
possible, and makes calibration meaningful. Collapsing them into one "time
entry" table — which most time trackers do — deletes the product.

### Data rules

1. **Instants** are `INTEGER` milliseconds, UTC. Never local, never seconds,
   never text.
2. **Calendar dates** are `TEXT 'YYYY-MM-DD'`, **local**. A due date with no
   time is a date; storing it as an instant means flying to another timezone
   silently moves your deadlines.
3. **Durations** are `INTEGER` seconds. One unit everywhere.
4. **Ids** are UUIDv7 — they sort by creation time, and two offline devices
   never collide even though there is no sync today.
5. **Anything derivable is derived.** Views `block_tracked` and `task_tracked`
   are the truth; the `*_cache` tables are written in the same transaction as
   every session mutation and can be regenerated from the views on demand. A
   cache that cannot be rebuilt is not a cache, it is a second truth.
6. **Deletes are soft**, with one documented exception: sessions are hard-
   deleted with a tombstone, because a soft-deleted session would keep counting
   toward drift while claiming to be gone.
7. **Intentions and records never merge.** See above.
8. **A session covers one contiguous *awake* interval.** See below.

### Two data decisions worth stating in full

**A session is a segment, not a sitting.** You start a timer, close the lid, and
reopen it three hours later. Counting on the monotonic clock keeps `elapsed_sec`
honest at twenty minutes — but a single row spanning `09:00 → 12:10` is still a
lie about *when* the work happened, and no amount of correct arithmetic fixes
it. So a run is made of one or more segments, and a segment closes at every
moment the record can no longer vouch for itself: the machine slept (close at
the last heartbeat), input stopped past the idle threshold (close at the last
input, after rolling the counter back), or you stopped. The Sessions tab shows
`09:00–09:20` and `11:30–12:10` with a visible gap, rather than one row that
quietly absorbs the meeting. Choosing "keep this time" reopens the segment and
folds the span back in, because the user's judgement outranks the heuristic.

**Recurring blocks are materialised rows, not a rule.** A repeating block is 90
days of real `scheduled_block` rows sharing a `series_id`, topped up
idempotently before each week load. The reason is rule 7 above: a session links
to a block **by id**, so a virtual occurrence could not be tracked against,
could not carry drift, and could not appear in Reconcile — a second-class block
that looks identical and does less. Instances are placed by the seed's *local
wall clock*, so a 09:00 series stays at 09:00 across a DST boundary.

### Activity's data contract

Activity is the only feature that records something the user did not type, so
its rules are enforced in the storage layer rather than described in the UI:

- Off by default. Application tracking and window-title tracking are
  **separate** switches; titles stay off when apps are turned on, and turning
  apps off turns titles off with them.
- A per-app exclusion list and a list of title fragments. **Both are applied on
  the way in** — an excluded app is never written, so it cannot resurface later
  through a query, an export, or a backup. Filtering on read would be a promise
  that only holds inside the UI.
- Pause is a stored setting, so it survives a restart.
- Retention is 30 days / 90 days / forever, purged automatically, with the next
  purge date on screen. "Delete everything recorded" is one button.
- Activity **never writes a `time_session`.** It observes; it does not fill in
  your record for you. An app that decides for you what you were working on is
  a different product with a different failure mode.

### Data in and out

- **Export**: JSON (round-trips exactly, ids included), CSV (tasks and
  sessions), ICS (export-only). Written to the user's Downloads folder, and the
  toast names the file — an export you can't find is an export you don't trust.
- **Import**: JSON in merge / replace / append modes; `.ics` calendars,
  read-only, as fixed blocks deduplicated on the VEVENT `UID`.
- **Backups**: a `VACUUM INTO` snapshot on launch when the newest is over 24
  hours old, 7 daily kept. Storing the live database in Dropbox, iCloud or
  OneDrive is a known corruption path and the UI says so.

---

## 5. What are the constraints?

### 5.1 Platform

| | |
|---|---|
| **Targets** | macOS 12+, Windows 10+, Linux (X11 and Wayland) |
| **Form** | Desktop application only. No web build, no mobile. |
| **Verified today** | Windows 10/11 x64, MSVC toolchain. macOS and Linux are unbuilt — the platform-specific code is confined to `src-tauri/src/idle.rs` and `src-tauri/src/frontmost.rs`. |

### 5.2 Stack

```
src/                React 19 · Vite 6 · Zustand 5 · Tailwind v4 · TypeScript 5.7
                    Formats DTOs. No SQL, no business logic, no derived values.
                    Never owns elapsed time.
src-tauri/          Tauri v2 · its own Cargo workspace
                    Windows, tray, the one-second loop, OS idle, frontmost
                    window, the IPC boundary. One thin wrapper per command.
crates/fruit-core/  Rust · rusqlite (bundled SQLite) · chrono · chrono-tz
                    · serde · uuid · thiserror
                    Schema, migrations, the command layer, the timer state
                    machine, the capture grammar, RRULE, ICS, calibration.
                    No UI. No Tauri dependency.
```

**The `fruit-core` / `src-tauri` split is the load-bearing constraint.** Because
the command layer has no Tauri dependency, invariants like *"at most one session
has `ended_at IS NULL`"* are a 10,000-operation fuzz test, and *"45 minutes of
sleep is not counted"* is a unit test with a fake clock — rather than something
verified by clicking around a running app on a machine with a system webview.
The cost is one crate boundary and a `Mutex<Store>` in the shell. The benefit is
that the parts of this app that would be catastrophic to get wrong — the ones
about *time* — are the parts under test.

**Dependency policy.** New crates need a reason. The Windows frontmost-window
implementation is raw `user32`/`kernel32` FFI rather than a binding crate,
because it is four calls against a stable documented ABI. There is no charting
library; the drift rail is CSS. There is no date-picker library.

### 5.3 Hosting and infrastructure

**None.** There is no backend, no API, no database server, no CDN, no auth
provider, no object store, no queue, no analytics endpoint, and no error
reporting service. The complete deployment story is a signed installer per
platform.

This is a constraint rather than a convenience. Every one of those would be a
place the user's record of their own working life could leak from, and the
product's central claim is that there is no such place.

### 5.4 Third-party integrations

| Integration | Direction | Notes |
|---|---|---|
| **`.ics` calendar files** | **In only** | Read from a local file the user picks. No URL subscription, no CalDAV, no account. Fruit never writes back to a calendar. |
| **OS idle detection** | Read | `GetLastInputInfo` (Windows), `CGEventSourceSecondsSinceLastEventType` (macOS), both permission-free. Linux returns nothing and falls back to input in Fruit's own window — narrower, and Settings says so. |
| **OS frontmost window** | Read, opt-in | `GetForegroundWindow` + `QueryFullProcessImageNameW` (Windows). macOS needs an Accessibility grant and X11 needs `_NET_ACTIVE_WINDOW`; both are stubs that say they are stubs. **Wayland cannot do this at all, by design**, and says so next to the switch it would enable. |
| **Everything else** | — | None. |

Fonts (Space Grotesk, Instrument Sans, Commit Mono) are bundled as `woff2`,
never loaded from a CDN — a font request is a network request, and the OFFLINE
badge has to be true.

### 5.5 Security

- **No SQL reaches the renderer.** `src/lib/ipc.ts` is the only file that talks
  to the backend; every command is typed and intent-based. This is a security
  decision, not a taste one: a webview that renders user-pasted markdown *and*
  holds `sql:allow-execute` is one `dangerouslySetInnerHTML` away from arbitrary
  SQL against the user's database.
- **The capability file lists exactly the commands in use** — no `sql:*`, no
  broad `fs:*`. The file picker is a Rust command, so the webview can receive a
  user-chosen path and nothing else.
- **Explicit CSP**, `default-src 'self'` with `object-src 'none'` and
  `frame-src 'none'`; the asset protocol is disabled. The markdown renderer
  builds React elements and never parses raw HTML, so `<img src=x onerror=…>` in
  a note renders inert.
- **Intent-based commands make invariants enforceable.** `start_timer` is not
  "insert a row"; it is one transaction that stops any running session, opens a
  new one and updates the singleton — three writes that must never land
  separately.

### 5.6 Performance budgets

| Budget | Target | How it's held |
|---|---|---|
| Cold start to interactive | < 1.5s | One composed DTO per view, not N+1 per block |
| Week load, 500 blocks | < 100ms | Indexed; guarded by query-plan tests that fail if an index stops being used |
| Idle CPU, no timer | ~0% | The one-second loop runs **only** while a timer is running. The now-cursor uses a minute-aligned timeout. |
| Installer | < 15MB per platform | `opt-level = "s"`, LTO, one codegen unit, `panic = "abort"` |

A one-second wake loop for a line that moves 0.6px per second is how an app
lands in "using significant energy", and this audience notices that publicly.

These are budgets with mechanisms behind them, not measurements. The query-plan
tests are real and run; the wall-clock numbers have not been profiled on a
packaged build, because that needs a machine `src-tauri` compiles on.

### 5.7 Accessibility and UI constraints

Full keyboard operation with no exceptions — drag is always an alternative,
never the only path. Visible focus on every interactive element. **No state
distinguishable by colour alone**: the drift encoding carries texture (dashed /
solid / dotted / hatched), a badge, and an accessible name. Tabular figures on
every changing numeral, so nothing jitters. `prefers-reduced-motion` disables
every transition. The layout holds from 960×640 up, and at 125% OS text scaling.

These are enforced by `scripts/check-ui.mjs` in a headless browser, across every
view, at five viewport sizes.

### 5.8 Known open items

Five things are honestly unverified and are the first to check on a machine with
a desktop session:

- Measured contrast of Focus-mode text over all four gradient backgrounds.
- Tray-icon legibility at 16px on a real menu bar.
- A real `SIGKILL` mid-write (WAL and the 500ms note debounce are the mechanism;
  it has not been fuzzed under an actual kill).
- Second-instance focus behaviour.
- The Windows frontmost-window FFI, and any change to `src-tauri` since the last
  Windows build — this container has no system webview, so that crate cannot be
  compiled here.

---

## Related documents

- [`ARCHITECTURE.md`](ARCHITECTURE.md) — why the layers sit where they do
- [`ACCEPTANCE.md`](ACCEPTANCE.md) — every acceptance criterion and what covers it
- [`SPEC-DEVIATIONS.md`](SPEC-DEVIATIONS.md) — where this build departs from the original brief, and why
