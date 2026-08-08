# Fruit — Front-End PRD, Wireframes and User Flows

**Purpose of this document.** Everything a designer or front-end engineer needs
to produce a new interface for Fruit without reading the Rust. It states what
each screen is *for*, what data it can actually have, what the user does on it,
and which decisions are load-bearing rather than aesthetic.

**Status.** The backend described here is built and tested (288 tests in
`fruit-core`, all green). Every data contract below was read out of the shipped
code, not designed on paper. Where a screen is missing something the
specification asks for, this document says so in that screen's section rather
than quietly drawing it.

**How to read it.**

- §1–§4 are constraints. Read them once; they apply everywhere.
- §5 is the shell every screen sits inside.
- §6 is one section per screen: purpose · data contract · wireframe · states ·
  interactions · what must not change.
- §7 is user flows, end to end.
- §8–§12 are cross-cutting: components, accessibility, performance, copy.

**A note on the wireframes.** They are ASCII and deliberately so: they fix
*information hierarchy, grouping and order*, which is what a redesign must
preserve, and they fix nothing about visual treatment, which is what a redesign
is for. Column widths in the diagrams reflect the real layout arithmetic in
§5.2 — those numbers are in the code as CSS custom properties.

---

## 1. The product in one page

> **A local-first Windows desktop application that shows how you planned to
> spend the month, what you actually did across work and life, where PC
> entertainment displaced intention, and how to make the next plan more
> realistic.**

**The primary outcome is behavioural, not informational:** reduce *unplanned*
PC entertainment. The reports exist to make the behaviour visible. The Day view
exists to make keeping the record cheap enough that it actually gets kept.

**The user.** One privacy-conscious solo knowledge worker on Windows. They plan
and report their own time. They want automatic evidence of PC use but the final
say on every classification. They use the keyboard heavily and will learn
shortcuts. Every byte stays on the machine — there is no server, no account, no
sync, and no network request of any kind.

**The primary task, and the number that governs every design decision:**

> Plan the month. During each day, let the timer and the observer do most of the
> recording. At the end of the day, spend **ninety seconds** confirming what the
> app got right and filling in what it could not see.

Ninety seconds is the design target; five minutes is the accepted ceiling after
a learning period. **If reconciliation stops happening, the monthly account
stops being trustworthy and the product's entire claim collapses.** So every
feature — and every design change — is judged partly on whether it makes those
ninety seconds shorter.

**The loop the whole app is shaped around:**

```
PLAN ──▶ TRACK ──▶ RECONCILE ──▶ CALIBRATE ──▶ PLAN BETTER
  │        │           │             │
  │        │           │             └─ estimate accuracy, recurring patterns
  │        │           └─ confirm gaps, overruns, unplanned and observed-only time
  │        └─ timers, manual life entries, automatic PC observation
  └─ projects, tasks, life targets, scheduled blocks
```

---

## 2. Five ideas the interface must not break

These are not style preferences. Each one is enforced in the backend and
visible in the data; a design that contradicts any of them will be *wrong*,
not merely different.

### 2.1 There are four kinds of record, and they never merge

| Record | Plain meaning | Confirmed actual time? |
|---|---|---|
| `scheduled_block` | **A plan.** "I intend to work on auth 9–11." | **No.** An intention. |
| `time_session` | **Confirmed work.** "I worked on auth 9:05–10:40." | **Yes** |
| `life_entry` | **Confirmed non-work.** Sleep, lunch, family. | **Yes** |
| `activity_span` | **What the machine observed.** Chrome, youtube.com. | **No — observed only.** |

**A plan is never drawn as though it were real time.** If you planned two hours
and did none, the interface must show two hours planned and zero done. Most time
trackers blur this; Fruit refuses to. The Day view expresses it as two separate
columns — *Planned* and *Actual* — and they are never combined.

When records overlap, one fixed precedence decides who owns the minute:

```
life_entry  ▶  time_session  ▶  activity_span  ▶  empty/unaccounted
```

An observation overlapping a confirmed session **enriches** it — it becomes
*evidence attached to* that session, shown in the PC-evidence column. It never
adds a second duration.

### 2.2 The counting invariant

> For any local date, confirmed + observed-only + idle + private + empty
> durations **sum to exactly the length of that day** — 24 hours, or 23 or 25
> across a DST transition — and no interval is counted twice.

**Design consequences, all mandatory:**

- Every hour of every day is present on the Day view, including empty ones.
  Empty time is a *state with its own visual treatment*, never a blank row and
  never absent. Blank is indistinguishable from "not loaded yet", and empty
  time is the state the user is there to act on.
- A filter may hide rows but must **say so** and must never change the totals.
  The current implementation prints "Showing 14 of 48 rows. The totals above
  are always the whole day." Keep that promise however it is worded.
- A total that cannot be checked by hand is a bug. The user must be able to add
  the categories up and get the day.

### 2.3 Drift is the signature reading

Every block carries two traces: a **dashed cool** one for the plot (what you
planned) and a **solid warm** one for the track (what you did). The gap between
them is the product's whole argument.

- **Cool = planned. Warm = actual.** This axis is used nowhere else.
- **There is deliberately no red in the drift system.** Overrun is a
  continuation, not an alarm. Red (`--danger`) is reserved exclusively for
  destructive actions.
- Drift states: `notStarted · inProgress · onEstimate · overrun · underrunPast`.

### 2.4 Observation is never a record

The machine's opinion never becomes confirmed truth on its own. An observed
interval stays `observedOnly` until a human confirms it in the reconciler. The
interface must always let the two be told apart at a glance, and must never
present an observation with the same confidence as a record.

### 2.5 Provenance is shown, not implied

Where a figure comes from is part of the figure. The Excel export carries a
literal "Source" column saying `sheet formula` or `from the record`. The Day
view distinguishes evidence from duration in words: *"What the machine saw
during this interval. Evidence, not duration — it is already counted once,
above."*

**A figure you cannot trace is a figure you cannot check.** Any redesign that
removes these attributions removes the reason to trust the numbers.

---

## 3. Design system

Everything in this section exists in `src/styles/tokens.css` as CSS custom
properties. **No component may introduce a literal colour** — there is an
automated check (`check-ui.mjs`, criterion I1) that fails the build if one does.
A redesign may change the *values*; it must keep them in tokens.

### 3.1 The governing visual idea

> Fruit should not look like a productivity app. It should look like an
> **instrument**: ruled lines, graduated scales, and the visible offset between
> two traces.

Reference points are the oscilloscope, the tide table and the engineering
drawing — not the dashboard, the kanban board, or the wellness app. Tight radii
(2–6px), hairline rules, dense but never cramped, and numerals that line up.

### 3.2 Colour — dark is primary

```
Dark (default)                     Light
--ink        #0e1116  page         #f5f6f8
--surface    #151a21  panels       #ffffff
--raised     #1d242d  plates       #edeff3
--line       #262e39  borders      #dadee5
--rule       #1f2630  hairlines    #e4e7ec
--paper      #e3e7ec  body text    #12161c
--muted      #8b95a3  secondary    #5b6572
--faint      #5a6472  tertiary     #96a0ae

The drift axis — ordinal, meaning is fixed:
--plot       #56c2d6  PLANNED      #0e7c90
--track      #e9a63c  ACTUAL       #b4741a
--over       #e2603c  overrun      #c2451f
--done       #6ebe8c  complete     #2f8b57
--danger     #d9455f  DESTRUCTIVE  #c22943   ← never used for overrun
```

Notes that matter:

- `--ink` is **blue-black, not neutral black** — neutral black reads dead
  under a cyan accent.
- `--paper` is **not pure white** — `#fff` halates at 14px on a dark ground.
- Light theme hues are *darker*, not the same hues on a light ground, so they
  hold 4.5:1 contrast.

**Activity has its own separate categorical ramp** (`--app-1` … `--app-8`),
eight hues at one lightness with no ranking implied. This is deliberate: the
drift axis is *ordinal* (cool means planned), and reusing it for "which
application" would mean a hue signified overrun in one panel and Slack in the
next. The app→hue mapping is a hash; the hues live in tokens.

### 3.3 Type

| Face | Role | Token |
|---|---|---|
| **Space Grotesk** 500/700 | Focus clock, view titles, large numerals | `--font-display` |
| **Instrument Sans** 400/500/600 | All interface text | `--font-ui` |
| **Commit Mono** 400/500 | Durations, clock times, parser tokens, OFFLINE badge | `--font-data` |

```
--t-display-xl  4.5rem     the Focus clock, and nothing else
--t-display     1.5rem     view titles
--t-title       1.125rem   panel headings
--t-body        0.875rem   body
--t-label       0.8125rem
--t-caption     0.75rem
--t-data        0.8125rem  monospace figures
--t-micro       0.6875rem
```

All sizes are **rem**, so Windows text scaling works. The layout is verified at
125% scaling as part of the automated checks.

**Every changing numeral uses tabular figures** (`font-variant-numeric:
tabular-nums`). This is checked automatically (criterion I4). A timer whose
digits shift width as it counts is unreadable at a glance, and this app is full
of counting numbers.

Fonts are **bundled as woff2 and self-hosted**. Referencing a font CDN is
forbidden — an offline-first app that links one silently falls back to system
faces on exactly the machine it was built for.

### 3.4 Space and radius

```
--s-2 --s-4 --s-6 --s-8 --s-12 --s-16 --s-20 --s-24 --s-32 --s-40 --s-48
```

A 4px base. **Nothing between these values.** If a layout needs 14px, the
layout is wrong.

```
--r-plate  2px    blocks, chips        (plates are not cards)
--r-panel  4px    panels
--r-sheet  6px    modals, overlays
```

### 3.5 Motion

```
--m-fast    120ms   hovers, toggles
--m-settle  240ms   the drift rail settling in
--m-sheet   180ms   overlays
--e-out            ease-out
--e-settle         cubic-bezier(0.2, 0.8, 0.2, 1)
```

`prefers-reduced-motion: reduce` **removes the settle animation and every
transition**, not merely shortens them. This is verified automatically
(criterion I6).

### 3.6 Layout arithmetic

```
--rail-w              76px   icon-over-label navigation
--sidebar-w          260px   projects / backlog
--sidebar-collapsed-w 48px
--detail-w           360px   right inspector
--topbar-h            48px
--gutter-w            48px   the time gutter on Day and Planner
--hour-h              56px   scales 32–120px via ⌘+ / ⌘−, persisted
```

Minimum supported window: **960 × 640**. Below 1280px the right detail panel
drops out entirely rather than squeezing the grid past legibility — selection
then opens nothing rather than opening something unreadable.

---

## 4. Voice and copy rules

The interface talks like a careful colleague, never like a coach and never like
a marketing page.

**The error pattern is: what failed · why · the action.**

> *"There isn't 5 minutes free there. Drop it somewhere with more room."*
> *"This block already repeats. Remove the repeat first, then set a new one."*
> *"A note holds 2000 characters and this one is 2431. It's a note, not a
> document — cut it down, or make the detail a subtask."*

**Rules a redesign must keep:**

1. **Never fabricate.** If a window title was not recorded, say "no window
   title recorded" rather than showing a blank. If a template has no history to
   base a number on, it says so and asks rather than inventing a round figure.
2. **State the limitation in place.** When the browser extension is not
   connected, the Activity screen says plainly that it can only see
   `chrome.exe`, rather than showing an empty panel.
3. **Numbers carry direction.** "12h of variance" is unusable; "+12h" or "−12h"
   is actionable. Signed figures are signed in *text*, never by colour alone.
4. **Say what a destructive thing will destroy, before it happens.** Replacing
   a record is never the default, and the checkbox that enables it explains
   what goes.
5. **An empty state is an invitation to act,** centred, `caption` weight,
   `muted`, one sentence, with the keyboard hint inline. Never a shrug.
6. **No exclamation marks, no encouragement, no streak-shaming.** The app
   reports; the user decides. Fragmentation is deliberately reported as its
   *components* rather than synthesised into a "focus score", because every
   other number in this app can be checked by hand and a score cannot.

---

## 5. The global shell

Every screen sits inside the same frame.

### 5.1 Shell anatomy

```
┌────────────────────────────────────────────────────────────────────────────┐
│ ⦙ Fruit  OFFLINE      ⟨timer chip⟩ ⟨pomodoro⟩   ●REC  Commands ⌘K          │ 48px
│                                                 ● Reconcile ⌘R   Focus F   │
├──────┬─────────────────────────────────────────────────────┬───────────────┤
│      │                                                     │               │
│ ▦    │                                                     │               │
│ DAY  │                                                     │               │
│      │                                                     │   detail /    │
│ ▤    │              main view                              │   inspector   │
│PLANNR│                                                     │   (360px,     │
│      │                                                     │   drops below │
│ ☰    │                                                     │   1280px)     │
│PROJCT│                                                     │               │
│      │                                                     │               │
│ ◷    │                                                     │               │
│ACTVTY│                                                     │               │
│      │                                                     │               │
│ ◔    │                                                     │               │
│REPORT│                                                     │               │
│      │                                                     │               │
│ ⚙    │                                                     │               │
│SETTNG│                                                     │               │
│ 76px │                                                     │               │
└──────┴─────────────────────────────────────────────────────┴───────────────┘
                                            ┌──────────────────────────┐
                                            │ Monday card / toasts     │  fixed
                                            │ bottom-right             │  bottom
                                            └──────────────────────────┘
```

### 5.2 The navigation rail

**Order is priority order: Day · Planner · Projects · Activity · Reports ·
Settings.** Day is first and is the default screen — this is the single biggest
structural decision in the product, and it is deliberate: the Planner is *not*
the primary screen.

The rail shows **icon over label** at 76px. Six destinations is too many to
learn from icons alone, and an unlabelled icon rail costs more in hesitation
than it saves in pixels.

Two screens are **not** on the rail because they act on something you are
already looking at, and are reached from Reports:

- **Export** — acts on the month you have open
- **Import** — a historical month arrives here once

### 5.3 The topbar

| Element | Behaviour |
|---|---|
| Brand mark | The drift rail as a monogram. Animates only while a timer runs. |
| `OFFLINE` badge | A statement of fact, in monospace. There are no network calls in the core loop. Never a status that can turn "online". |
| Timer chip | Task, elapsed, drift against its block. Centre. |
| Pomodoro strip | Four dots plus a break marker, when Pomodoro is active. |
| `● RECORDING` | Activity observation is on. Reports `paused` as its own state. |
| **`Commands ⌘K`** | The command palette's visible affordance. |
| `● Reconcile ⌘R` | **Only when there are unreconciled days**, with the count. |
| `Focus F` | Always. |

Keyboard hints are spelled **for the platform** — a single helper rewrites `⌘`
to `Ctrl ` on Windows, in one place, so the palette, the shortcut sheet and the
topbar can never disagree. The MVP is Windows-only, where `⌘` is a key the user
does not have.

### 5.4 Overlays — mutually exclusive, `Esc` dismisses

| Overlay | Opened by | Shape |
|---|---|---|
| Command palette | `⌘K` / `⌘F` / topbar button | Centred, top-third |
| Shortcut sheet | `?` | Centred modal, grouped |
| Reconcile | `⌘R` / topbar | Full-width three-column sheet |
| Focus | `F` | Full-screen, one number |
| Task detail | `Enter` on a task | Column ≥1280px, sheet below |
| Block dialogs | `R` / `⌫` on a block | Small modal |
| Fill dialog | Day-view gap | Small modal |
| Recovery modal | On launch, unresolved session | Blocking |
| Idle banner | Idle detected | Bottom toast-stack |
| **Monday card** | A finished week, unread | Bottom-right panel, self-appearing |
| Toasts | Any completed action | Bottom-right stack |

Only one overlay at a time. `Esc` closes the topmost, and every overlay
registers its own capture-phase key handler so a nested one cannot leak.

### 5.5 The complete keyboard map

Every action lives in one registry that feeds the palette, the key handler and
the shortcut sheet — so an action that is not reachable both ways cannot be
written. This is criterion **U1** and it is structural.

```
GLOBAL                              PLANNER
⌘K   command palette                1 / 3 / 7 / M   span
⌘F   search                         T    jump to today
C    quick capture                  ← →  previous / next period
F    focus mode                     ⌘+ ⌘−  taller / shorter hours
⌘R   reconcile a day                ↑ ↓  move block ±15m
?    shortcut sheet                 ⇧↑ ⇧↓  shrink / grow block ±15m
⌘Z   undo                           D    duplicate block
⌘,   settings                       R    repeat block…
G D  go to Day                      ⌫    unschedule block
G P  go to Planner                  S    schedule the selected task
G T  go to Projects
G A  go to Activity                 TASKS
G R  go to Reports                  X      complete
                                    Enter  open detail
TIMER                               ⌫      delete
Space  start / stop
⌘.     stop                         RECONCILE
                                    1–4    pick a choice
                                    Enter  take the recommendation
```

---

## 6. Screens

Each screen below gives: **purpose · entry · data contract · wireframe ·
states · interactions · gaps**. The data contract lists the *actual* IPC calls
available, so a designer can tell what is possible without asking.

---

### 6.1 DAY — the primary operational screen

**Purpose.** A complete 24-hour table for one date, modelled on the client's
spreadsheet. This is where the ninety seconds are spent and the screen the
whole product is organised around.

**Entry.** Default screen on launch · `G D` · "Review source intervals" from a
Reports finding · a divergence link in the weekly report.

**Data contract.**

```ts
getDay(date, tz, slotMinutes?) → DayView {
  localDate, tz, slotMinutes, startsAt, endsAt, now, isToday, isReconciled,
  slots: DaySlot[] {
    index, startsAt, endsAt, state, segments[], plans[]
  },
  segments: DaySegment[] {
    from, to, owner: SlotOwner, evidence: AppTotal[], hasDistraction
  },
  totals: DayTotals {
    daySec, plannedSec, plannedEntertainmentSec,
    confirmedWorkSec, confirmedLifeSec, sleepSec, privateSec,
    observedOnlySec, idleSec, emptySec,
    entertainmentSec, entertainmentInWindowSec, pcSec,
    byArea[], byProject[], byApp[], byContribution[], byDomain[]
  },
  fragmentation
}

SlotOwner =
  | { kind:"life",  entryId, areaId, areaName, areaColour, areaKind,
                    label, isPrivate }
  | { kind:"work",  sessionId, taskId, taskTitle,
                    projectId, projectName, projectColour, contribution }
  | { kind:"observed", appId, domain, category }
  | { kind:"idle" }
  | { kind:"empty" }

SlotState = empty | plannedNotStarted | confirmedWork
          | confirmedLife | private | observedOnly | idle
```

Writes available from this screen: `addLifeEntry` · `addSession` ·
`splitLifeEntry` · `splitSession` · `mergeLifeEntries` · `mergeSessions` ·
`repeatLifeEntry` · `deleteLifeSeries` · `setSessionContribution` ·
`convertSessionToLife`.

**Wireframe.**

```
┌────────────────────────────────────────────────────────────────────────────┐
│ ‹  Thursday, August 6  ›   Today   │5m│15m│30m│60m│      FILTER ▾   + Add  │
├────────────────────────────────────────────────────────────────────────────┤
│ ■WORK      ■LIFE      ■SLEEP     ■ENTERTAIN   ■OBSERVED    ⌸UNACCOUNTED    │
│ 2h 19m     1h         6h 30m     25m          2h 45m       11h 30m         │
│ ▓▓▓▓▓▓░░░░░░░░▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▓▓▓▓▓▓░░░░░░/////////////////////////////    │
│ PLOTTED 1H 30M   AT THE PC 4H 35M   PRIVATE 1H                             │
├────────────────────────────────────────────────────────────────────────────┤
│ 09:00–12:30 selected · 7 rows · 3h 30m   [Merge 2 · Firefight] [Record all]│ ← only when a range is selected
├──────┬──────────────┬──────────────┬───────────┬────────────┬──────────────┤
│ TIME │ PLANNED      │ ACTUAL       │PC EVIDENCE│CLASSIFICATN│  ┌─────────┐ │
├──────┼──────────────┼──────────────┼───────────┼────────────┤  │ INTERVAL│ │
│05:00 │              │▌Sleep        │           │ ■ LIFE     │  │ DETAIL  │ │
│      │//////////////│Unaccounted—fil//////////│ · UNACCOUNTD│  │         │ │
│06:00 │//////////////│Unaccounted—fil//////////│ · UNACCOUNTD│  │ 08:00–  │ │
│      │              │              │           │            │  │ 09:15   │ │
│08:00 │▌Refactor auth│▌Refactor auth│CODE,CHROME│■WORK+DISTRA│  │         │ │
│      │▌Daily standup│              │           │            │  │ TASK    │ │
│      │▌Refactor auth│▌Refactor auth│CODE,CHROME│■WORK+DISTRA│  │ Refactor│ │
│09:00 │              │▌Refactor auth│CODE,CHROME│■WORK+DISTRA│  │         │ │
│ NOW ─┼──────────────┼─▌Production f┼───────────┼────────────┤  │ CONTRIB │ │
│      │              │ + fill the gap│          │            │  │ [Own ▾] │ │
│      │//////////////│Unaccounted—fil//////////│ · UNACCOUNTD│  │         │ │
│10:00 │▌Answer mail  │▌code ▌Answer  │CODE      │ ▢ OBSERVED │  │ SPLIT   │ │
│      │              │▌code +fill gap│          │ ▢ OBSERVED │  │ [−15m]  │ │
│11:00 │              │▌slack +fill   │          │ ▢ OBSERVED │  │  08:37  │ │
│      │              │▌Lunch and walk│CODE,SLACK │ ■ LIFE     │  │ [+15m]  │ │
│12:00 │              │▌Lunch and walk│CODE,SLACK │ ■ LIFE     │  │[Split at│ │
│      │              │▌code          │           │ ▢ OBSERVED │  │  08:37] │ │
│13:00 │              │▌youtube ▌code │           │ ▢ OBSERVED │  │         │ │
│      │              │▌chrome ▌youtub│           │ ▢ OBSERVED │  │ PC EVID │ │
│      │//////////////│Unaccounted—fil//////////│ · UNACCOUNTD│  │ code 47m│ │
└──────┴──────────────┴──────────────┴───────────┴────────────┘  └─────────┘ │
```

**The five columns are the screen's argument** and their order is fixed:
Time · Planned · Actual · PC evidence · Classification. Reading left to right
is reading *intention → record → evidence → verdict*.

**Row states — ten specified, and each needs a non-colour indicator** so the
whole thing survives a greyscale screenshot (criterion I3). The current glyph
set:

```
·  Unaccounted          ○  Planned, not started    ■  Work
▣  Life                 ▨  Private                 ▢  Observed only
–  Idle
```

**Unaccounted rows are hatched and labelled**, never blank. This is the state
the user is here to act on; blank would be indistinguishable from "not loaded".

**Interactions.**

| Gesture | Result |
|---|---|
| Click a chip in *Actual* | Opens the interval detail panel |
| Click **Unaccounted — fill** | Opens the fill dialog for that slot |
| Click **+ fill the gap** | Same, for a partial gap beside a record |
| Click a **time cell** | Anchors a range selection |
| **Shift-click** a second time cell | Extends the range; the selection bar appears |
| `+ Add interval` | Fill dialog, defaulting to 09:00–10:00 |
| Zoom `5m/15m/30m/60m` | Changes the lens **only** — never the stored precision |

**Multi-select lives on the time column, not the fill button.** This is a real
constraint discovered in build: the fill button opens a modal, and a modal
covers the table you would be shift-clicking into. On the time cell it is the
spreadsheet gesture on the column that looks like a spreadsheet.

**Selection acts on nothing until asked.** The bar states what is selected and
offers *Merge* (only when the range holds two or more records of a single
subject) and *Record all of it*. A selection that acted on its own would be a
four-hour edit made by a stray shift-click.

**Filters.** Everything · Unaccounted · Needs a decision · Work · Life, plus
per-project and per-area groups **built from the day's own segments**. A filter
offering thirty projects when two had time today is a menu of dead ends.

**Known gaps on this screen** (see `BACKLOG.md`):

- **A5** — no drag and no keyboard editing. The largest outstanding item, and
  §2 describes a keyboard-heavy user, so this is closer to a requirement than a
  nicety.
- **A11** — seven of ten specified slot states render. Planned-and-completed,
  planned-with-overrun and planned-with-underrun all collapse into
  `confirmedWork`; sleep/rest collapses into `confirmedLife`. The drift data
  exists but lives on the block and is drawn on the Planner, so the *primary*
  screen does not show the product's signature reading. **A redesign should
  solve this.**
- **A6** — the summary cards do not break out each life area, though
  `totals.byArea` already carries it.
- **A7** — work-contribution and entertainment filters are missing.

---

### 6.2 THE FILL DIALOG — where a gap becomes a record

**Purpose.** Turn an unaccounted interval into confirmed time, in one
interaction, without leaving the Day view. This dialog is where a large share
of the ninety seconds is spent.

**Wireframe.**

```
        ┌──────────────────────────────────────────────────┐
        │  14:20–16:05 · 1h 45m                            │
        ├──────────────────────────────────────────────────┤
        │  Start  [14:20]                    [−30m] [+30m] │
        │  End    [16:05]                    [−30m] [+30m] │
        │                                                  │
        │  ┌──────┬──────┐                                 │
        │  │ Life │ Work │   ← mode                        │
        │  └──────┴──────┘                                 │
        │                                                  │
        │  ▸ LIFE MODE                                     │
        │  ┌────────────┬────────────┬────────────┐        │
        │  │● Sleep/Rest│● Family    │● Wellbeing │        │
        │  │● Personal  │● Community │● Friendship│        │
        │  │● Team Time │● Fun       │● Side Gig  │        │
        │  └────────────┴────────────┴────────────┘        │
        │     ↑ one click commits                          │
        │                                                  │
        │  ▸ WORK MODE                                     │
        │  [ Find a task…                              ]   │
        │  ┌────────────┬────────────┬────────────┐        │
        │  │Refactor    │Fix the DST │Rewrite sync│        │
        │  │auth module │off-by-one  │layer       │        │
        │  └────────────┴────────────┴────────────┘        │
        │  Contribution  [ Attend — you were present   ▾]  │
        │  Work away from this PC — a second machine, an   │
        │  offline meeting, a task done on paper — is      │
        │  never observed, so this is the only way it is   │
        │  ever recorded.                                  │
        │                                                  │
        │  ☐ Replace anything already recorded here        │
        │                                                  │
        │  [Private]              Cancel Esc  [Record 1h45]│
        └──────────────────────────────────────────────────┘
```

**Design decisions that are load-bearing:**

1. **Both kinds of confirmed time are reachable from the gap.** The observer
   sees one machine — work on a second computer, an offline meeting, a task
   done on paper produce *no observation at all* and can only ever be entered
   by hand. Sending the user elsewhere to do it puts the friction exactly where
   the app is blindest.
2. **Times are typed *and* nudged.** The stepper is right for trimming what the
   app guessed; it is wrong for entering a meeting that ran 14:20–16:05, which
   no number of half-hour steps can reach.
3. **Typing a start later than the end moves the interval and keeps its
   length** — it does not clamp. Clamping produced 05:59 from a typed 14:20,
   which reads as the app ignoring the keyboard.
4. **Life commits in one click** (pick an area). **Work needs a button**,
   because a task must be chosen first. That asymmetry is correct: the
   ninety-second budget belongs to the common case.
5. **Private is life-only.** A work session names a task by definition, so
   "accounted for, nothing recorded about it" has nowhere to attach.
6. **Replace is never the default** and the label says what goes.

---

### 6.3 PLANNER — secondary

**Purpose.** The canvas. Where a day or a week is laid out by hand: drag a
block on, resize it, push it later.

**The Day view is a *ledger*; the Planner is a *canvas*.** They are different
jobs — this distinction was written into the spec after the app was used, when
an earlier plan to delete the 1-day span turned out to be wrong.

**Entry.** `G P` · dragging a task from the backlog · `S` on a selected task.

**Data contract.**

```ts
getWeek(from, to, tz) → WeekView {
  days: [{ localDate, isToday, isPast, isReconciled, plannedSec,
           blocks: BlockView[] }],
  unplanned: …
}
BlockView { block: BlockRow, title, projectId, projectColour, taskStatus,
            plannedSec, trackedSec, driftSec, driftState, isRunning,
            lane, lanes }
BlockRow  { id, taskId, label, startsAt, durationSec, localDate, tz,
            isFixed, seriesId, rrule, externalUid, intent }
BlockIntent = "work" | "entertainment" | "life"
```

**Wireframe.**

```
┌──────────┬─────────────────────────────────────────────────────────────────┐
│ BACKLOG  │  ‹  4–10 August  ›  Today   │1 day│3 days│7 days│Month│   ⌘+ ⌘− │
│          ├─────────────────────────────────────────────────────────────────┤
│ OVERDUE  │      MON 4      TUE 5      WED 6      THU 7      FRI 8          │
│ ▸ Fix…   │      2h 30m     4h         6h 15m     1h 30m     3h             │
│          ├──────┬──────────┬──────────┬──────────┬──────────┬──────────────┤
│ TODAY    │08:00 │          │          │┌────────┐│          │              │
│ ▸ Refac… │      │          │┌────────┐││↻Standup││          │              │
│ ▸ Answe… │09:00 │┌────────┐││Refactor│|└────────┘│┌────────┐│              │
│          │      ││Refactor│││auth    ││┌────────┐││Write   ││              │
│ THIS WEEK│10:00 ││auth ▐▐ │││   ▐▐▐  │││Refactor│││migratn ││              │
│ ▸ Revie… │      │└────────┘│└────────┘││auth ▐  │││guide ▐ ││              │
│          │11:00 │          │          │└────────┘│└────────┘│              │
│ NO DATE  │      │          │          │          │          │              │
│ ▸ Rewri… │12:00 │          │          │          │          │              │
│          │      │          │          │          │          │              │
│ SOMEDAY  │…     │          │          │          │          │              │
│          │20:00 │          │          │┌ ─ ─ ─ ─┐│          │              │
│ COMPLETED│      │          │          ││Window ·││          │              │
│ ▸ Fix f… │21:00 │          │          ││Film    ││          │              │
│          │      │          │          │└ ─ ─ ─ ─┘│          │              │
│  260px   │ 48px │          │          │  ↑dashed = entertainment intent    │
└──────────┴──────┴──────────┴──────────┴──────────┴──────────┴──────────────┘
```

**The drift rail is the block's left edge**: a dashed cool trace for the plot,
a solid warm one for the track, continuing *below* the plate into the gutter
when it overruns. Blocks are therefore deliberately **not** `overflow: hidden`.

**Block intent** (migration 0009) tints and marks a block: an evening plotted
for a film is a plan, and must not read as two hours of missing work. Dashed
border plus a `Window ·` or `Life ·` prefix — never colour alone.

**Collision policies on drop:** default *overlap* (the UI tints it), `Shift`
*push* (subsequent non-fixed blocks move down), `Alt` *shrink* (the dropped
block shortens to fit). **Fixed blocks are never pushed and never
auto-shortened** — the app stops and says so rather than quietly overlapping.

**Repeating blocks look like every other block.** The `↻` marker earns its
pixels by changing what `⌫` does: a series member asks *this one / this and
later / all of them* rather than guessing. Deleting "just this one" and
deleting six months of stand-ups are different enough that inferring is a
data-loss bug with a friendly name.

---

### 6.4 PROJECTS & TASKS

**Purpose.** The backlog that feeds the Planner. Tasks exist only to support
planning and tracking — **not GTD, not kanban.**

**Data contract.**

```ts
getBacklog(filter, tz) → BacklogView { tasks: TaskRow[], groups }
getProjects() → ProjectRow[] { id, name, colour, icon, kind, sortRank,
                               isArchived, weeklyTargetSec,
                               openTaskCount, weekTrackedSec }
getTaskDetail(id) → { task, sessions[], blocks[], note, noteUpdatedAt }
parseCapture(text) → chips     ← quick capture grammar
```

**Six backlog groups, in this order:** Overdue · Today · This week · No date ·
Someday · Completed. Completed sits at the bottom and recedes.

**Wireframe.**

```
┌──────────┬──────────────────────────────────────────┬──────────────────────┐
│ PROJECTS │  All tasks                        12     │ TASK DETAIL          │
│          │  [+ Capture: fix login #auth ~45m !! ]   │                      │
│ ● Welcome│   ╰─ chips appear before commit ─╯       │ Refactor auth module │
│   PERSNL3│                                          │ ● Deep work          │
│ ● Deep   ├──────────────────────────────────────────┤                      │
│   WORK  3│ OVERDUE                                  │ ┌────┬────┬────┬────┐│
│          │  ▸ Fix the DST off-by-one    !!  15m ▐▐  │ │Note│Sess│Sub │Blks││
│ + New    │                                          │ └────┴────┴────┴────┘│
│          │ TODAY                                    │                      │
│ RECENTLY │  ▸ Refactor auth module      !   1h  ▐▐▐ │ ESTIMATE  [1h    ▾]  │
│ DELETED  │  ▸ Answer support mail            30m    │ PRIORITY  [!!    ▾]  │
│ Nothing  │                                          │ DUE       [2026-08-08│
│ in 30 d. │ THIS WEEK                                │ TAGS      #dev       │
│          │  ▸ Review the drift rail spec     30m    │                      │
│          │                                          │ ── Note ─────────────│
│          │ NO DATE                                  │ 1240 / 2000 chars    │
│          │  ▸ Rewrite the sync layer    ROLLOVER    │ ┌──────────────────┐ │
│          │                                          │ │Plain text only.  │ │
│          │ SOMEDAY                                  │ │Cmd+Enter turns   │ │
│          │  ▸ …                                     │ │the current line  │ │
│          │                                          │ │into a subtask.   │ │
│          │ COMPLETED                            ▾   │ └──────────────────┘ │
│          │  ▸ Fix the flaky test        ✓ 25m       │                      │
│  260px   │                                          │       360px          │
└──────────┴──────────────────────────────────────────┴──────────────────────┘
```

**Quick capture grammar** — parsed into visible chips *before* commit, and
`⌘Z` restores the raw text:

```
#project    @tag    ~45m    !!    ^tomorrow 9am
```

**The note is plain text and capped at 2000 characters.** There is no Markdown
renderer — this was removed deliberately (§1 puts "no personal-notes system,
wiki, Markdown editor" out of scope). A bare `https://…` still becomes a link,
which is not markup: there is no syntax to learn, nothing to escape, and no way
to write a link that displays as different text — which is precisely the
feature that turns a note field into a document format.

The character count appears at 80% of the cap. **A limit you only meet at the
moment you lose work is a trap, not a limit.**

**Estimates sit on a fixed ladder** rather than being free text, with
`ROLLOVER` at the top meaning *"doesn't fit one sitting"* — a distinct state
from "not estimated yet".

**Known gap: A4** — projects have neither a note nor a monthly target, though
the spec lists both. Projects are currently the only targetable thing in the
app that cannot be measured against a month.

---

### 6.5 ACTIVITY

**Purpose.** What the machine observed, and the surface where observations
become *categories*. Opt-in, off by default.

**Data contract.**

```ts
getActivityDay(date, tz) → ActivityDay {
  spans[], byApp[], byDomain[], correlations[], settings, status
}
getUnlabelled(from, to, tz, limit) → UnlabelledRow[] {
  matchKind, matchValue, seconds, occurrences, stretches[]
}
getCategories · createCategory · updateCategory · deleteCategory
getActivityRules · setActivityRule · deleteActivityRule
setSpanCategory · getDomainTotals · getConnectorStatus · installConnector
clearActivity
```

**Wireframe.**

```
┌────────────────────────────────────────────────────────────────────────────┐
│  Activity · Thursday, August 6        ● RECORDING     [Pause]  [Settings]  │
├────────────────────────────────────────────────────────────────────────────┤
│  ⚠ The browser extension isn't connected, so Fruit can only see            │
│    "chrome.exe" — not which site. [Set it up]                              │
├────────────────────────────────────────────────────────────────────────────┤
│  BY CATEGORY                                                               │
│  ■ Work        3h 20m  ████████████████████                                │
│  ■ Distraction 1h 05m  ██████                                              │
│  ■ Study         40m   ████                                                │
│  ■ Life          15m   █                                                   │
├────────────────────────────────────────────────────────────────────────────┤
│  NOT CATEGORISED YET                          ranked by time               │
│  ▸ youtube.com          1h 12m   8 stretches            [Category ▾]       │
│    └ 09:35–09:57  "Rust lifetimes explained — YouTube"  [Category ▾]       │
│    └ 14:00–14:35  "Kurzgesagt — black holes"            [Category ▾]       │
│    └ 16:10–16:22  no window title recorded              [Category ▾]       │
│  ▸ docs.google.com        45m   3 stretches             [Category ▾]       │
│  ▸ slack.exe              22m   5 stretches             [Category ▾]       │
├────────────────────────────────────────────────────────────────────────────┤
│  AGAINST THE PLAN                                                          │
│  09:00 Refactor auth module (plotted 1h)                                   │
│        └ code.exe 47m · chrome/youtube.com 22m · slack 18m                 │
├────────────────────────────────────────────────────────────────────────────┤
│  TIMELINE                                                                  │
│  06 ─────────────────────────────────────────────────────────────── 22     │
│     ▓▓▓▓░░░░▓▓▓▓▓▓▒▒▒▒░░░░░░▓▓▓▓▓▓▓▓░░░▒▒▒▒▒▒░░░░░░░░▓▓▓░░░░░░              │
└────────────────────────────────────────────────────────────────────────────┘
```

**Design requirements specific to this screen:**

1. **Each stretch is individually labellable**, not just the domain total.
   Showing "8 stretches of chrome" and making the user label all eight at once
   was a real complaint: a YouTube video that was research and a YouTube video
   that was not are the same domain and different categories.
2. **The window title names *which* video**, and when it was not recorded the
   row says "no window title recorded" rather than showing a blank.
3. **Labelling removes the row from *Not categorised yet* immediately.**
   Rules apply forward *and* backfill spans that have no label at all — but
   they never touch a span that already has one. "Filling a blank is not
   rewriting an answer."
4. **A verdict is stamped at write time, never joined on read.** A rule made
   today cannot rewrite a month already signed off. This is invisible in the
   UI and must stay true.
5. **When the connector is not installed, say so plainly** with a one-click
   install — do not show an empty panel.

---

### 6.6 REPORTS — month-first

**Purpose.** Patterns over time. **Opens to the month**, because that is the
plan's default reporting horizon.

**Data contract.**

```ts
getMonth(month, tz) → MonthView {
  month, label, from, to, tz,
  days: MonthDay[] { dayOfMonth, localDate, daySec, confirmedWorkSec,
                     confirmedLifeSec, sleepSec, privateSec, observedOnlySec,
                     idleSec, emptySec, entertainmentSec,
                     entertainmentInWindowSec, plannedEntertainmentSec,
                     isReconciled, hasHappened, accountedRatio },
  totals: DayTotals, accountedRatio, elapsedSec, elapsedEmptySec,
  unreconciledDays, findings: MonthFinding[]
}
getWeekReview(date, tz) · getWeekReport(date, tz) · getReports() · getGoals()
```

**Wireframe — month horizon.**

```
┌────────────────────────────────────────────────────────────────────────────┐
│  August 2026    ‹  This month  ›   │Day│Week│Month│   [Import] [Export ▸]  │
├────────────────────────────────────────────────────────────────────────────┤
│ ACCOUNTED  WORK      LIFE     SLEEP    ENTERTAIN  UNACCOUNTED              │
│    68%     42h 10m   18h      62h 30m  9h 45m     31h 20m                  │
│                                                                            │
│  148h of August 2026 has happened. Every figure above is measured against  │
│  that, not against the whole month.                                        │
├──────────────────────────────────┬─────────────────────────────────────────┤
│ ENTERTAINMENT · PLANNED VS       │ DATA QUALITY · August                   │
│ UNPLANNED                        │  1  2  3  4  5  6  7                    │
│  ╭╮                              │ ▓8 ▓9 ▓7 ▓9 ▓6 ▓8 ░─                    │
│  ││ ╭╮      ╭─╮                  │  8  9 10 11 12 13 14                    │
│  ││ ││  ╭╮  │ │   solid=unplanned│ ▓7 ▓9 ▓8 ▓4·▓9 ▓8 ░─   ·=unreconciled   │
│ ─┴┴─┴┴──┴┴──┴─┴──                │  …                                      │
│  ┄┄┄┄╌╌╌╌┄┄┄┄╌╌╌  dashed=planned │                                         │
│                                  │ 3 unreconciled days · 12h observed-only │
│ 9h 45m of entertainment this     │                                         │
│ month: 3h inside a window you    ├─────────────────────────────────────────┤
│ plotted, 6h 45m outside one.     │ LIFE AREAS · TARGET VS ACTUAL           │
│                                  │ Family        12h ████████░░  20h       │
├──────────────────────────────────┤ Wellbeing      6h █████░░░░░  15h       │
│ FINDINGS                         │ Friendship     0h ░░░░░░░░░░  10h  ⚠    │
│ ▸ 6h 45m unplanned entertainment │                                         │
│   Worst day: Tue 12 (2h 10m)     │                                         │
│   [Review source intervals →]    │                                         │
└──────────────────────────────────┴─────────────────────────────────────────┘
```

**Two rules this screen must keep:**

1. **Measure against elapsed time, not the whole month.** A fresh August on the
   4th is otherwise "6% accounted", which is arithmetically true and a useless
   headline — the missing 27 days are the future, not a gap. `accountedRatio`
   and `elapsedSec` are the pair that agree.
2. **A future day is outlined, never flagged.** A day that has not arrived is
   not "unreviewed"; marking it as a problem is how a dashboard trains someone
   to ignore its warnings.

**Wireframe — week horizon** (goals, fragmentation, the weekly report):

```
┌────────────────────────────────────────────────────────────────────────────┐
│ LAST WEEK                       2026-07-27 – 2026-08-02   [Save as .xlsx]  │
│                                                                            │
│ 24h 10m of work, 6h 45m of entertainment.        ← headline first, always  │
│                                                                            │
│ Category              Time      Share                                      │
│ Work                  24h 10m    14%                                       │
│ Life                  18h 00m    11%                                       │
│ Sleep                 52h 30m    31%                                       │
│ Entertainment          6h 45m     4%                                       │
│ Private                2h 00m     1%                                       │
│ Observed, unconfirmed 12h 15m     7%                                       │
│ Unaccounted           52h 20m    31%                                       │
│ Every hour of the week appears in exactly one row above.                   │
│                                                                            │
│ BIGGEST DIVERGENCE                                                         │
│ [2026-07-29 · entertainment nobody plotted    2h 10m, no window plotted →] │
│                                                                            │
│ NOT CATEGORISED YET                                                        │
│ youtube.com 1h 12m · docs.google.com 45m     [Name them in Activity →]     │
├────────────────────────────────────────────────────────────────────────────┤
│ THIS WEEK · GOALS                                    [Add a goal]          │
│ ▸ At least 20h of deep work                                                │
│   ████████████░░░░░░  12h 30m of 20h · on pace · 2h 30m a day for 3 days   │
│ ▸ At most 5h of entertainment                                              │
│   ██████████████████  4h 20m of 5h · 40m left                              │
│                                                                            │
│ FRAGMENTATION            this week    last week                            │
│ Longest unbroken stretch   1h 45m      2h 10m  ↓                           │
│ Planned switches               12          14                              │
│ Unplanned switches             31          22  ↑                           │
│ Time in fragments < 15m     3h 20m      2h 05m ↑                           │
│                                                                            │
│ CALIBRATION                                                                │
│ Deep work: 20h target, 12h median over 6 weeks. A goal you miss every week │
│ stops being a goal. Try 13h?                        [Use 13h]  [Dismiss]   │
└────────────────────────────────────────────────────────────────────────────┘
```

**Goals are reported by *pace*, never as a scoreboard.** A goal at zero on
Monday morning is **on pace** and must say so — the future is never a
shortfall. Direction is first-class: *at most 5h of entertainment* is a goal you
succeed at by being under it, and a bar that turns red when you do the right
thing teaches people to ignore bars.

**Fragmentation is reported as components and deliberately never scored.**

**Known gap: A3** — no YouTube/Twitch trend panel, though `byDomain` now exists.

---

### 6.7 RECONCILE — the ninety seconds

**Purpose.** The end-of-day pass. Bounded, keyboard-driven, deferrable.
**This overlay is the product's beating heart** — if it stops happening, the
monthly account stops being trustworthy.

**Wireframe.**

```
┌────────────────────────────────────────────────────────────────────────────┐
│  Reconcile · Thursday, August 6              4 of 11        Esc to defer   │
├─────────────────┬──────────────────────────────┬───────────────────────────┤
│ QUEUE           │  THE ITEM                    │  EVIDENCE                 │
│                 │                              │                           │
│ ✓ 09:00 overrun │  14:00–14:35                 │  The machine saw:         │
│ ✓ 10:00 unplan  │  Observed only · 35m         │                           │
│ ▸ 14:00 observd │                              │  chrome.exe               │
│   15:00 empty   │  youtube.com                 │  youtube.com              │
│   16:30 empty   │  "Kurzgesagt — black holes"  │  35m continuous           │
│   17:00 notstrt │                              │                           │
│   …             │  WHAT WAS THIS?              │  Nobody has said what     │
│                 │                              │  this time was. This is   │
│ 5 done, 6 left  │  ① Record as life…    ◀ rec  │  an observation, not a    │
│                 │  ② Accept as observed        │  fact.                    │
│                 │  ③ Mark private              │                           │
│                 │  ④ Ignore                    │  ☐ Apply my choice to     │
│                 │                              │    future youtube.com     │
│                 │  ↳ Life area  [Fun      ▾]   │    Forward only — a rule  │
│                 │                              │    made today cannot      │
│                 │  Enter takes the recommend.  │    rewrite a signed-off   │
│                 │                              │    month.                 │
└─────────────────┴──────────────────────────────┴───────────────────────────┘
```

**Three columns, and the split is the argument:** what is left · the one
decision in front of you · why the machine thinks so.

**Item kinds:** overrun · planned-but-never-started · unplanned session ·
observed-only · empty hour. The last two come from `resolve_day`'s own
segments, not a separate query — so the reconciler asks about *exactly* the
intervals the Day view shows.

**The verbs:** accept · reschedule remainder · split · drop · mark done ·
revise estimate · move to tomorrow · leave unscheduled · create retro block ·
assign to task · log as break · ignore · record as life · mark private.

**Interaction rules:**

- `1`–`4` pick, `Enter` takes the recommendation, `Esc` defers the whole day.
- **The recommendation is a heavier border, not a fill.** The other choices are
  equally legitimate and must not read as discouraged.
- Each choice carries **its own consequence line** where the verb's name does
  not carry it.
- The "apply to future" checkbox appears **only for a claim with a domain
  behind it** — an application name is not durable enough to key a rule on, and
  a control that is inert half the time teaches people to ignore it.

---

### 6.8 FOCUS

**Purpose.** One number, full screen, nothing else. Started with `F`.

```
┌────────────────────────────────────────────────────────────────────────────┐
│                                                                    Esc     │
│                                                                            │
│                                                                            │
│                            Refactor auth module                            │
│                                                                            │
│                             1 4 : 3 2                                      │
│                          ╰──── 4.5rem ────╯                                │
│                                                                            │
│                          of 45m intended  ·  [+15m]                        │
│                                                                            │
│                    ▐▐▐▐▐▐▐▐▐▐▐▐▐▐▐░░░░░░░░░░░░░                            │
│                                                                            │
│                        Space to stop   ·   F to exit                       │
└────────────────────────────────────────────────────────────────────────────┘
```

**Focus sessions commit to a length**, and that intended length is written as a
*plotted block*. So **extending shows up later as an overrun, not as a larger
plan** — which is the reading no app without a plan/record split can offer.

The clock counts down and then **keeps counting up**, because the session did
not stop when the intention ran out.

Four gradient backgrounds, each with a validated 28% scrim so the text holds
contrast over all of them.

---

### 6.9 EXCEL EXPORT

**Purpose.** The client's primary exchange format. This is the artefact the
product will be judged by.

```
┌────────────────────────────────────────────────────────────────────────────┐
│  Export August 2026 to Excel                     [Cancel]  [Export .xlsx]  │
├──────────────────────────────────────┬─────────────────────────────────────┤
│  OPTIONS                             │  PREVIEW · this IS the sheet        │
│  ☑ Include unaccounted slots         │  ┌────┬────┬────┬────┬────┐         │
│  ☑ Include observed-only slots       │  │Time│1 Sa│2 Su│3 Mo│4 Tu│         │
│  ☐ Name private areas                │  │0000│Slee│Slee│Slee│Slee│         │
│    (durations always exported;       │  │0030│Slee│Slee│Slee│Slee│         │
│     only the area name is withheld)  │  │…   │    │    │    │    │         │
│                                      │  │0900│Gap │Gap │Refa│Refa│         │
│  OUTPUT                              │  │0930│Gap │Gap │Refa│Answ│         │
│  [ C:\Users\you\Downloads\Aug.xlsx ] │  └────┴────┴────┴────┴────┘         │
│                                      │                                     │
│  RECONCILIATION                      │  THREE SHEETS                       │
│  Measure      App      Sheet   Var   │  1 · August 2026 — the matrix       │
│  Accounted   60h10m   60h30m  +20m   │  2 · Summary — every figure, with   │
│  Observed    12h15m   12h00m  −15m   │      a Source column saying         │
│  Unaccounted 31h20m   31h30m  +10m   │      "sheet formula" or "from the   │
│                                      │      record"                        │
│  Variance is expected and explained: │  3 · Source mapping — what each     │
│  the sheet is a half-hour grid and   │      label means                    │
│  the record is to the second.        │                                     │
└──────────────────────────────────────┴─────────────────────────────────────┘
```

**The preview *is* the sheet** — both render from the same matrix, so the
screen cannot promise a layout the file does not have.

**The reconciliation table is the reason to trust either**: the app's figures
beside the same figures recounted from the sheet's own cells. Variance is
expected and non-zero, and *explained* rather than hidden — "no *unexplained*
variance" is the actual measure.

---

### 6.10 IMPORT

**Purpose.** A historical workbook, once. Mapping → variance → commit.

```
┌────────────────────────────────────────────────────────────────────────────┐
│  Import a workbook                                     [Back to Reports]   │
├────────────────────────────────────────────────────────────────────────────┤
│  THE FILE                                                                  │
│  Fruit reads the file and never writes to it. Nothing is imported until    │
│  you have said what every label means and looked at the variance.         │
│  WORKBOOK [ C:\Users\you\Documents\2025.xlsx        ]        [Read it]     │
├────────────────────────────────────────────────────────────────────────────┤
│  WHAT FRUIT FOUND                                                          │
│  SHEET [August 2026 — 31 day columns ▾]   MONTH [August 2026]              │
│  Time in column 1, days across row 1, 30 minutes a row. The sheet doesn't  │
│  say which month it covers, so that one is yours to pick.                  │
├────────────────────────────────────────────────────────────────────────────┤
│  WHAT EACH LABEL MEANS                                                     │
│  Every label has to be given a meaning, and Ignore is a meaning. An        │
│  importer that quietly drops what it doesn't recognise is how a month      │
│  arrives eighty per cent complete and nobody notices.                      │
│                                                                            │
│  Label                Time      Import as                                  │
│  Sleep/Rest          32h 30m   [Life · Sleep/Rest        ▾]                │
│  Deep work            8h 00m   [Work                     ▾]                │
│  Lunch                1h 00m   [— not decided —          ▾]  ⚠             │
│  Admin                  30m    [Ignore                   ▾]                │
├────────────────────────────────────────────────────────────────────────────┤
│  VARIANCE                                                                  │
│  ⚠ 1 label is still unmapped. Say what it is — including "ignore".         │
│  ⚠ 5 days already have records in Fruit. Choose keep or replace.           │
│  ☐ Replace what Fruit already holds on those 5 days                        │
│                                                                            │
│  Day          In the sheet   In Fruit   Difference                         │
│  2026-08-03      8h 00m       0m         +8h 00m                           │
│  2026-08-04      6h 30m       3h         +3h 30m                           │
│  …                                                                         │
│                                              [Import]  ← disabled while ⚠  │
├────────────────────────────────────────────────────────────────────────────┤
│  IMPORTS SO FAR                                                            │
│  2026-08 · August 2026 · C:\…\2025.xlsx   3 sessions · 12 life   [Undo]    │
└────────────────────────────────────────────────────────────────────────────┘
```

**Three rules, enforced in the core rather than trusted to the UI:** every
label must be given a meaning · the variance must be seen · conflicts must be
resolved. The Import button is disabled while any blocker stands, *and* the
backend refuses anyway.

---

### 6.11 SETTINGS

Ten groups: **General · Planner · Timer · Pomodoro · Activity · Notices ·
Labels (entertainment rules) · Data · Shortcuts · About.**

```
┌────────────────────────────────────────────────────────────────────────────┐
│  Settings                                                                  │
├────────────────────────────────────────────────────────────────────────────┤
│  GENERAL      Timezone · week start · theme · date order                   │
│  PLANNER      Default span · hour height · snap                            │
│  TIMER        Idle threshold · what idle does by default                   │
│  POMODORO     Work / break lengths · long-break cadence                    │
│  ACTIVITY     ☐ Observe applications        ← OFF by default               │
│               ☐ Record window titles        ← separate control, OFF        │
│               ☐ Observe browser domains     ← separate control, OFF        │
│               Retention  ○ 30 days ● 90 days ○ Forever                     │
│               Next purge: 2026-11-04                                       │
│               Exclusions: [1password.exe] [banking.com]        [+ Add]     │
│               [Pause observation]  [Delete everything observed]            │
│               BROWSER CONNECTOR      ● Not connected                       │
│               [Register the native host]  Extension ID [        ]          │
│  NOTICES      ☐ Continuous work    ☐ Daily ceiling    ☐ Off-plan nudge     │
│               All off by default. A notice, never a block.                 │
│  LABELS       youtube.com  → Distraction   [Edit] [Delete]                 │
│               coursera.org → Study         [Edit] [Delete]                 │
│               [+ Add a rule]                                               │
│  DATA         [Export JSON] [Import JSON] [Backups…] [Integrity check]     │
│  SHORTCUTS    The full registry, grouped                                   │
│  ABOUT        Version · schema · "no account, no server, no internet"      │
└────────────────────────────────────────────────────────────────────────────┘
```

**Privacy controls are three separate switches, all off by default:**
application observation, window titles, browser domains. Turning one on never
turns another on. That separation is the promise the product is sold on.

**Known gap: A9** — there is no Excel settings group; those options live on the
export screen.

---

## 7. User flows

### 7.1 First run

```
Launch ─▶ migrations run ─▶ seed writes a demo project
                                   │
                                   ▼
                    Day view, today, with:
                    · one plotted block (09:00, 1h)
                    · one session against it (74m) ── an overrun, on screen
                    · ten life areas ready to use    before you track a minute
                    · a task whose note is the 60-second guide
```

The seeded overrun is deliberate: **the signature drift rail is visible before
the user has tracked anything.** The first ninety seconds decide whether the
product is understood.

### 7.2 The daily ninety seconds — the flow that matters most

```
Evening ─▶ topbar shows "● Reconcile ⌘R  (11)"
              │
              ▼
         ⌘R opens the sheet
              │
              ├─ item 1: overrun    ─ Enter (accept)          ~3s
              ├─ item 2: unplanned  ─ Enter (create retro)    ~3s
              ├─ item 3: observed   ─ 1, pick "Fun", Enter    ~6s
              │                       ☑ apply to future youtube.com
              ├─ item 4: empty      ─ 1, pick "Family", Enter ~5s
              │  …
              ▼
         last item ─▶ day closes ─▶ takeaway line + streak
                                     "6 of 11 hours were plotted.
                                      Your third day running."
```

Every decision is one key. `Esc` defers the whole day without losing progress;
a deferred day auto-accepts after a week rather than accumulating forever.

### 7.3 Recording work done away from the PC

```
Day view ─▶ spot an unaccounted afternoon
              │
              ▼
         click "Unaccounted — fill"
              │
              ▼
         type Start 14:20 · End 16:05
              │
              ▼
         switch to [Work]
              │
              ▼
         filter "design" ─▶ pick "Design review"
              │
              ▼
         Contribution [Attend — you were present]
              │
              ▼
         [Record 1h 45m of work] ─▶ toast ─▶ Day view reloads
```

This flow exists because **the observer sees one machine.** Work on a second
computer, an offline meeting, a task done on paper produce no observation at
all — the reconciler will never offer them, so hand entry is the only path.

### 7.4 Labelling a site

```
Activity ─▶ "Not categorised yet" ─▶ youtube.com, 1h 12m, 8 stretches
                                          │
                        ┌─────────────────┴──────────────────┐
                        ▼                                    ▼
              label the whole domain            expand ▸ and label one stretch
              [Distraction ▾]                   "Rust lifetimes" → Study
                        │                                    │
                        ▼                                    ▼
              a prospective rule is made        only that interval changes
              (forward only; backfills          
               unlabelled spans, never
               overwrites a labelled one)
                        │
                        ▼
              the row disappears from "Not categorised yet"
```

### 7.5 Monday morning

```
Launch, first time after a week ends
              │
              ▼
    ┌──────────────────────────────────────┐
    │ Last week is ready   27 Jul – 2 Aug  │   self-appearing, bottom-right
    │ 24h 10m of work, 6h 45m entertainment│
    │ [Read it] [Save as .xlsx] [Dismiss]  │
    └──────────────────────────────────────┘
              │                    │
              ▼                    ▼
      Reports · week        writes an .xlsx
      headline first        + [Show it] toast
```

It **waits until read** and never fires for a week still in progress. An empty
week produces no card at all — a notification about nothing is the fastest way
to teach someone to dismiss notifications.

### 7.6 Planning the month

```
Projects ─▶ capture tasks (#project ~45m ^tomorrow)
              │
              ▼
Planner ─▶ 7-day span ─▶ drag a task onto a slot
              │              │
              │              ├─ default: overlap, tinted
              │              ├─ Shift:   push later blocks down
              │              └─ Alt:     shrink to fit the gap
              ▼
         R to repeat ─▶ pick a preset ─▶ 90 days materialise
              │
              ▼
         plot an evening as an entertainment window
         (palette ▸ "Mark block as an entertainment window")
              │
              ▼
         the month dashboard's dashed line now has something to draw
```

### 7.7 Month end

```
Reports (month) ─▶ read findings ─▶ "Review source intervals" ─▶ Day view
        │
        ▼
   [Export ▸] ─▶ options ─▶ preview (the sheet itself)
        │
        ▼
   reconciliation table: app figures vs sheet figures, variance explained
        │
        ▼
   [Export .xlsx] ─▶ toast with the path ─▶ [Show it]
```

---

## 8. Component inventory

| Component | Where | Notes for a redesign |
|---|---|---|
| `NavRail` | shell | Icon over label, six items, order is priority order |
| `TopBar` | shell | Brand · OFFLINE · timer · recording · Commands · Reconcile · Focus |
| `TimerChip` | topbar | Task, elapsed, drift; tabular figures |
| `PomodoroStrip` | topbar | Four dots + break marker |
| `RecordingIndicator` | topbar | Three states: recording, paused, off |
| `DriftRail` | blocks, task rows, report bars | **The signature element.** Two traces; state computed once in Rust so all three renderings agree |
| `DriftBar` | reports | Horizontal variant |
| `Sidebar` | Projects, Planner | 260px; collapses to 48px |
| `PlannerBacklog` | Planner | Drag source |
| `Palette` | overlay | Fuzzy match over commands, tasks, projects, tags; matched substring in `--plot` |
| `ShortcutSheet` | overlay | Reads the same registry |
| `FillDialog` | Day | Life/Work modes; typed + nudged times |
| `SplitControl` | Day detail | Midpoint default, quarter-hour steps |
| `RepeatLife` | Day detail | Presets from Rust; scope on delete |
| `BlockDialogs` | Planner | Repeat picker · scope picker |
| `MondayCard` | shell | Self-appearing; the only card not caused by a user action |
| `WeekReportPanel` | Reports | Headline first |
| `Toasts` | shell | 8s, pauses on hover **and** focus, carries Undo |
| `RecoveryModal` | shell | Blocking; unresolved session from last run |
| `IdleBanner` | shell | Keep / discard / break |
| `Empty` | everywhere | One sentence + inline key hint |
| `Note` | Task detail | Plain text, `pre-wrap`, bare URLs linkified |

---

## 9. Accessibility — non-negotiable

These are acceptance criteria with automated checks. A redesign must keep them.

| # | Requirement | How it is checked |
|---|---|---|
| **I1** | No literal colours in components — tokens only | Automated, fails the build |
| **I3** | Every drift/state encoding has a text alternative | Automated: counts unlabelled encodings |
| **I4** | Changing numerals use tabular figures | Automated |
| **I5** | No horizontal overflow at 960×640, 1130×720, 1280×800, 1490×900, and 1440×900 at 125% text | Automated, five viewports, every view |
| **I6** | `prefers-reduced-motion` removes settle **and** every transition | Automated |
| **I7** | No network request of any kind, fonts included | Automated |
| **U10** | Focus visible on every interactive element | Automated |
| **I2** | Contrast ≥4.5:1 body, ≥3:1 graphics, both themes, including Focus text over all four gradients | **Not yet automated — open** |

**Colour is never the only carrier.** Every state has a glyph or a word:
selected rows get a solid left edge as well as a tint; signed variances carry
`+`/`−` in text; block intent carries a `Window ·` prefix.

---

## 10. Performance budgets

| Budget | Target | Measured? |
|---|---|---|
| Month dashboard, populated 31 days | < 250ms | ✅ tested |
| **Day view, populated 24 hours** | **< 100ms** | ❌ |
| Week load, 500 blocks | < 100ms | ❌ |
| Cold start to interactive | < 1.5s | ❌ |
| Idle CPU, no timer | ~0% | ❌ |
| Data loss on forced close | none | ❌ |

The Day view is the screen the ninety-second target runs on and the one that
has changed most. **A budget nobody measures degrades in increments nobody
notices.**

---

## 11. Explicitly out of scope

A redesign must not add these, however natural they seem:

No sync · accounts · cloud · web · mobile · macOS · Linux · collaboration ·
team workspaces · manager reporting · notes system · wiki · Markdown editor ·
attachments · role/KPI/value scoring · expense or income tracking · AI
scheduling · telemetry or crash uploads · calendar write-back · plugin API ·
**website blocking** · tamper prevention · focus scores · per-URL rules ·
focus sounds · billing.

**Website blocking deserves its own note:** the off-plan nudge is a *notice*,
never a block. Fruit will not close a tab or deny a navigation, and the browser
connector has no `host_permissions` with which to try. That is a promise made
in the architecture, not just the copy.

---

## 12. Open questions for whoever designs this

Genuine uncertainties, not rhetorical ones.

1. **How should the Day view show drift?** (gap A11) The information exists but
   is currently only on the Planner. Does the Classification column gain
   states, does the Planned column gain a drift mark, or does the Actual chip
   carry it? All three are defensible; the data reaches the renderer either
   way. **This is the most consequential open design question**, because drift
   is the signature reading and the Day view is the primary screen.
2. **What is the right drag gesture on a table?** (gap A5) "Drag on the Day
   view" could mean resizing a record's edge, moving a whole record, or
   painting a range. Building the wrong one wastes the effort.
3. **Should sleep/rest be a distinct row state**, or is a distinct summary card
   enough? A third of every month lands in that bucket.
4. **Where do the two missing filters live** — work-contribution and
   entertainment — without turning the filter control into a menu?
5. **Does the month dashboard need a YouTube/Twitch panel of its own** (gap
   A3), or should it be a breakdown inside the entertainment chart?

---

## 13. Source documents

| Document | What it holds |
|---|---|
| `PRODUCT-SPEC.md` | The specification of record. Where it disagrees with anything else, it wins. |
| `ACCEPTANCE.md` | What is signed off, criterion by criterion, with the test that proves it |
| `BACKLOG.md` | What is known to be missing, and why it matters |
| `WIREFRAME-GAP.md` | The original wireframes, screen by screen, against the build |
| `PLAN-WEEKLY-GOALS.md` | The week horizon: goals, pace, fragmentation, notices, the report |
| `ARCHITECTURE.md` | The three layers and why the split is load-bearing |
| `SPIKE-BROWSER-CONNECTOR.md` | The connector's protocol and its three open field assumptions |
