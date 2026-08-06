# Architecture

Why the code is shaped this way. For *what the app is* — the
screens, the entities, the constraints — see
[`PRODUCT-SPEC.md`](PRODUCT-SPEC.md).

Three layers, and the boundaries between them are the whole design.

```
┌─ src/                React renderer ────────────────────────────────┐
│  Formats DTOs. Holds no SQL, no business logic, no derived values.  │
│  Never owns elapsed time.                                            │
├─ src-tauri/          Shell ─────────────────────────────────────────┤
│  Windows, tray, the one-second loop, OS idle, the IPC boundary.      │
│  One thin wrapper per command. No rules.                             │
├─ crates/fruit-core/  Everything Fruit knows how to do ──────────────┤
│  Schema, migrations, intent-based commands, the timer state machine, │
│  the capture grammar, calibration. No UI. No Tauri dependency.       │
└─────────────────────────────────────────────────────────────────────┘

  crates/fruit-connector-host/   The browser connector's native-messaging
  connector/                     host, and the MV3 extension it talks to.
                                 Off to the side: a separate process, not a
                                 layer. See below.
```

## Why `fruit-core` has no Tauri dependency

Because otherwise the invariants in §6.5 could only be checked by clicking
around a running app, on a machine with a system webview, by hand.

With the split, "at most one session has `ended_at IS NULL`" is a 10,000-
operation fuzz test that runs in CI in 45 seconds. "Sleeping 45 minutes does not
count the sleep" is a unit test with a fake clock. "A v1 database migrates and
passes `quick_check`" is a test that builds a real v1 database and migrates it.

The cost is one extra crate boundary and a `Mutex<Store>` in the shell. The
benefit is that the parts of this app that would be catastrophic to get wrong —
the ones about *time* — are the parts under test.

## Why the browser connector is a fourth process

`crates/fruit-connector-host` has no Tauri dependency either, and for a reason
that is not the same as `fruit-core`'s.

Chrome launches the host itself — a second invocation of `fruit.exe`, detected by
the `chrome-extension://` origin Chrome passes as its first argument. That
process **must not open the database**: two processes on one SQLite file is the
corruption path the single-instance plugin exists to prevent. So the host writes
to an append-only spool in the app data directory, and the app drains it on the
activity tick it already runs.

That leaves the host as pure I/O plus framing — Chrome's 4-byte native-endian
length prefix, and the partial-read cases a naive loop gets wrong. It is the most
breakable code in the feature and it lives in `src-tauri`'s blast radius, which
cannot be compiled without a system webview. Pulling it into its own crate is
what lets `cargo test` cover it at all; the shell just re-exports it.

The hand-off deliberately avoids both a localhost socket (reachable by every
process on the machine, and an app badged OFFLINE cannot open one) and a named
pipe (correct, but `unsafe` Windows code bought on a spike to save twenty seconds
of latency on a twenty-second signal). See
[`SPIKE-BROWSER-CONNECTOR.md`](SPIKE-BROWSER-CONNECTOR.md).

## Why an observation's category is stored, not joined

`activity_span.category` is stamped at write time from the domain rules in force
then, rather than derived on read by joining to `domain_rule`.

The join would be less data and would look more obviously correct. It is wrong
here: a rule added in September would rewrite what August said you were doing.
A record of what you did must not move under someone who later changes their mind
about a domain — the rule decides new spans, and old ones keep their verdict.
There is an acceptance test for exactly this, because no screen can show it.

## Why SQL never reaches the renderer

§6.8, and it is a security decision rather than a taste one. A webview that
renders user-pasted markdown and holds `sql:allow-execute` is one
`dangerouslySetInnerHTML` away from arbitrary SQL against the user's database.

So: `src/lib/ipc.ts` is the only file that talks to the backend, every command
is typed and intent-based, and the capability file lists exactly those commands
— no `sql:*`, no broad `fs:*`. The markdown renderer builds React elements and
never parses raw HTML (D12).

Intent-based also makes invariants enforceable. `start_timer` is not "insert a
row"; it is one transaction that stops any running session, opens a new one and
updates the singleton — three writes that must never land separately.

## Why the renderer never owns elapsed time

Two clocks, deliberately separated (`crates/fruit-core/src/clock.rs`):

- **Wall time** is what you display. It jumps — the user fixes their timezone,
  NTP corrects a drift, DST arrives mid-session.
- **Monotonic time** is what you count with. It never runs backwards and does
  not advance while the machine is suspended.

Counting on the monotonic clock is what makes `elapsed_sec` immune to a clock
change (D9). The *gap* between the two deltas is precisely how suspend is
detected (D10): if wall time ran on and monotonic time did not, the machine
slept, and the accumulator already excluded it — so the honest default costs
nothing and Fruit only has to ask.

A `setInterval` in the renderer can do none of that, which is why Rust owns the
accumulator and the renderer only formats. The one interval in the renderer is
the now cursor, and it is minute-aligned:

```ts
const delay = 60_000 - (Date.now() % 60_000) + 50;
```

A one-second wake loop for a line that moves 0.6px a second is how an app lands
in "using significant energy", and this audience notices that publicly.

## Why a session is a segment, not a sitting

Counting correctly is not enough. A session row also carries a `started_at` and
an `ended_at`, and those are what a person reads back a week later — so they
have to be times that actually happened.

The failure case is a meeting. You start a timer, close the lid, and reopen it
three hours later. Counting on the monotonic clock keeps `elapsed_sec` honest at
twenty minutes, but a single row spanning `09:00 → 12:10` is still a lie about
*when* the work happened, and no amount of correct arithmetic fixes it.

So a `time_session` row is one **contiguous awake interval**, not one sitting.
A run — everything between pressing start and pressing stop — is made of one or
more segments, and the boundaries are the moments the record can no longer
vouch for:

| Boundary | Segment closes at | Then |
|---|---|---|
| Machine slept (wall ≫ monotonic) | the last heartbeat | idle challenge; discard opens a fresh segment |
| No input past the idle threshold | the last input | same, and the counter is rolled back first |
| Switched task, or stopped | now | the run ends |

Both endpoints of every segment are wall-clock instants, so the Sessions tab
reads `09:00–09:20` and `11:30–12:10` with a visible gap between them, rather
than one row that quietly absorbs the meeting. `sleep_splits_the_session_instead_of_spanning_it`
asserts exactly that: no segment's wall span may exceed what it counted by more
than a minute.

Two consequences worth stating:

- **The runtime tracks two figures.** `run_ms` is the whole run and is what the
  timer chip shows; `segment_ms` is the open segment and is what gets written to
  its row. A chip that reset to `00:00` every time you came back from a meeting
  would be reporting a bookkeeping detail as if it were news.
- **Choosing "keep" un-splits.** If the span *was* work, the closed segment is
  reopened and the span folded back in, so the record shows one interval rather
  than a suspicious pair. Splitting is reversible because the user's judgement
  outranks the heuristic.

An empty segment is deleted rather than kept. A zero-length interval in the
Sessions tab is noise, not a record.

## Why four record types resolve into one timeline at read time

The product shows a single 24-hour column, but it stores four different kinds of
claim about the same hour, and they are not interchangeable:

| Table | The claim | Who made it |
|---|---|---|
| `scheduled_block` | "I mean to do this" | the user, in advance |
| `time_session` | "I did this work" | the timer, or the user correcting it |
| `life_entry` | "I did this non-work thing" | the user |
| `activity_span` | "this app was in front" | the machine |

The tempting design is one `time_entry` table with a `kind` column. It is
wrong for a reason that shows up immediately: **an observation and a session
describe the same second without being the same fact.** If the timer says you
were on the auth refactor from 09:00 to 10:00 and the observer says Slack was
frontmost from 09:20 to 09:40, one table forces a choice — overwrite, or store
both and count 80 minutes for a 60-minute hour. Neither is acceptable, and the
second is the failure mode the product exists to avoid.

So the tables stay separate and **overlap is resolved on read**, never by
mutating anything:

1. a confirmed `life_entry` wins,
2. then a confirmed `time_session`,
3. then an `activity_span`,
4. then the hour is empty.

`store::day::resolve_day` builds the boundary set from every source, cuts the
day into segments at those boundaries, and gives each segment **exactly one**
owner. Totals sum segments, not rows — which is why a ten-minute session inside
a thirty-minute slot contributes ten minutes and not thirty. The slot grid is a
lens for the eye; the segments are the arithmetic.

Three things follow, and each is a requirement that would otherwise need its
own mechanism:

- **Observation enriches rather than adds.** The Slack span above is attached to
  the session's segment as evidence and contributes no duration of its own. The
  Day view can say "you were in Slack for twenty of those minutes" without the
  day summing to more than a day.
- **Empty is a real state, not an absent row.** Segments cover the day with no
  gaps, so an unaccounted hour is a segment whose owner is `Empty`. It is
  something the UI is handed, not something it has to notice is missing —
  which is what makes an empty hour reconcilable.
- **The invariant is one assertion.** For any local date, the segment durations
  sum to the length of that day: 24 hours, or 23 or 25 across a DST transition.
  `a_day_accounts_for_every_second_exactly_once` asserts it on a hand-built day;
  `overlapping_records_never_double_count` asserts it over 200 randomly
  overlapping records. Plan acceptance M2, M4 and M8 all reduce to that line.

The plan is deliberately *not* in this precedence list. A block is an intention,
and an intention that silently becomes actual time is how a planner starts
lying to you. It renders as a separate overlay, and the difference between the
two layers is the drift the whole product is about.

## Why derived data is computed in Rust, not the renderer

Drift, drift state, task groupings, the calibration headline, the reconcile
takeaway — all arrive already computed. Two consumers of the same rule will
diverge, and in an app *about* divergence that is a special kind of
embarrassing.

The most visible case is `DriftState`. It is computed once, in
`store::week::drift_state`, and the planner plate, the compact rail in a task
row and the report bar all render from it. They cannot disagree about whether
something overran, because none of them decides.

`TaskRow.first_session_at` / `last_session_at` follow the same rule. The
Completed group shows *when* a finished task was worked rather than what it was
estimated at, and those bounds are `MIN`/`MAX` over its sessions computed in the
projection — not something the renderer reconstructs by scanning a session list
it would have to fetch first.

Tracked time follows the same rule one level down: the views `block_tracked`
and `task_tracked` are the truth, the `*_cache` tables are written in the same
transaction as every session mutation, and `rebuild_tracked_caches` regenerates
them from the views on demand. A cache that cannot be rebuilt is not a cache,
it is a second truth — so the fuzz test asserts they still match after 10,000
operations.

## Why the drag state lives in the store

Most component state belongs to its component. Dragging is the exception, and
for a structural reason: §4.3 lists four drag sources — sidebar backlog item,
task row, existing block, block edge — and two of them start in a *different
component* from the one that handles the drop.

A drag that begins on a sidebar row and ends on the planner grid cannot live in
either component's `useState`; the Planner would never see it. So `taskDrag`
sits in the store, `useStartTaskDrag` writes it, and the Planner reads it and
owns everything geometric — the slot maths, the insertion plate, auto-scroll,
`Esc`-to-cancel, and the `schedule_block` call.

Dragging an *existing block* stays local to the Planner, because it starts and
ends there. Two mechanisms, but only where the boundary genuinely differs.

Both paths remain optional. `S` then arrows schedules and nudges without a
pointer, and the drag is never the only route to anything (§4.3, U1).

## Why recurring blocks are materialised rows

A repeating block could be a rule the planner draws on the fly. It is instead
90 days of real `scheduled_block` rows sharing a `series_id`, and the reason is
data rule 7 below: a `time_session` links to a block **by id**.

A virtual occurrence has no id. So it cannot be tracked against, cannot carry
drift, cannot appear in Reconcile, and cannot be moved without inventing an
exception table to remember that you moved it. Every one of those is a feature
that already works for ordinary blocks and would silently stop working for
repeating ones — a second-class block that looks identical and does less.

Real rows keep all of it, at the cost of a horizon:

- `schedule_recurring` (new block) and `repeat_block` (one you already have)
  both write a seed carrying the `rrule`, then materialise forward.
- `extend_series_to` runs before every week load. It is idempotent — an
  instance that already exists on a date is left alone — so scrolling past the
  edge tops the series up instead of finding nothing there.
- Instances are placed by the seed's **local wall clock**, not by adding
  intervals to its instant, so a 09:00 series stays at 09:00 across a DST
  boundary. A date whose local day cannot hold the block is skipped rather than
  written as a block that crosses midnight.
- Removing an occurrence always asks its scope — this one, this and later, all
  of them. Inferring it is not a convenience; it is data loss with a friendly
  name. The undo token restores the whole scope in one step, which is what makes
  asking safe rather than merely careful.

`.ics` import uses the same engine: a repeating meeting is a series in Fruit
too. Imported blocks are `is_fixed` because that is what an external meeting is
— the thing you plan *around* — and they carry the VEVENT `UID`, so
re-importing the same calendar updates in place instead of doubling every
meeting.

## Why Activity's privacy contract lives below the IPC boundary

Activity samples the frontmost application. Every promise it makes — off by
default, titles separate from apps, a per-app exclusion list, a retention
window — is enforced in `store::activity::record_activity`, which applies the
filtering **on the way in**.

That placement is the whole point. If the shell's sampler decided what to
record, a bug there could write something the user excluded, and the exclusion
would then be defeated permanently: the row exists, and it will surface in a
query, an export or a backup long after the bug is fixed. Filtering on read
would be worse — it would be a promise that only holds inside the UI.

So the shell does one thing: ask the OS what is in front and hand it over. It
holds no policy at all. `frontmost::Support::describe()` supplies the sentence
Settings prints next to the switch, so a platform that cannot do this says why
— Wayland does not let an application see the focused window, and that is
stated rather than shown as a greyed-out control with no explanation.

The recording indicator in the top bar is driven by the `activity:sampled`
event, which fires only when a row was actually written. An indicator wired to
the settings flag would claim to be recording while paused or while every
sample was being excluded.

## Why the browser preview reads recorded output

`npm run dev` in a browser has no backend. The tempting fix is a JavaScript
mock of the command layer — and that is a second implementation of every rule
in §6, drifting from the first from day one.

Instead `cargo run -p fruit-core --bin dump-fixtures` seeds a real store, runs
the real commands, and writes the real DTOs to `src/dev/fixtures.json`. Reads
in the preview are genuine output. Writes refuse with a sentence explaining
why, because a simulated write would be exactly the lie this avoids.

## Data rules worth restating

1. Instants are `INTEGER` milliseconds, UTC. Never local, never seconds, never text.
2. Calendar dates are `TEXT 'YYYY-MM-DD'`, **local** — a due date with no time is
   a date, and storing it as an instant means flying to another timezone
   silently moves your deadlines.
3. Durations are `INTEGER` seconds. One unit everywhere.
4. Ids are UUIDv7, so they sort by creation time and two offline devices never collide.
5. Anything derivable is derived; caches are named `*_cache` and rebuildable.
6. Deletes are soft (with one documented exception — see `SPEC-DEVIATIONS.md` §3).
7. Intentions and records never merge. `scheduled_block` is what you meant to
   do; `time_session` is what happened. A session may exist with no block
   (unplanned work) and a block with no session (never started), and both are
   meaningful states the UI renders rather than edge cases it hides.
8. A session covers one contiguous *awake* interval, so its endpoints are always
   real system-clock instants. See "Why a session is a segment" above.

## Why the browser and the sampler do not both bill the same hour

Two things write to `activity_span`: the foreground sampler (`chrome.exe`) and
the browser connector (`chrome.exe` on `youtube.com`). While Chrome is frontmost
both are correct and both produce rows for the same seconds.

`resolve_day` was never affected — it gives each segment exactly one owner by
precedence — which is why the double-count was invisible on every screen that
mattered most. But per-app totals, per-label totals and the uncategorised list
walk spans directly, and those were counting the hour twice.

`dedupe_browser_overlap` subtracts the domain-bearing intervals from the
app-only ones for the same application. Subtraction rather than deletion,
because the remainder is real: Chrome open on `chrome://settings` records no
domain, and that time genuinely is app-only.

The related trap is in the *write* path. `record_activity` used to coalesce
against the single most recently ended span. With two sources interleaving,
neither ever matched, so every twenty-second sample became its own row — and
once a minimum-duration floor existed, all of them were discarded. It now looks
for the most recent span *describing the same thing*, which is the only version
that survives more than one writer.

## Why a label is stored on the span, and a rule is separate

`activity_span.category_id` is stamped at write time from the rules in force
then — the same argument as `category` before it. A rule added in September
cannot rewrite what August said you were doing.

That leaves the case where the rule is right in general and wrong right here: a
YouTube lecture. Fruit sees a registrable domain and deliberately never the URL
or the page title, so it cannot tell. `set_span_category` changes one interval
and leaves the rule alone, which is the only honest arrangement available — the
person who watched it is the only one who knows.
