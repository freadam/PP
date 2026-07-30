# Architecture

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

## Why derived data is computed in Rust, not the renderer

Drift, drift state, task groupings, the calibration headline, the reconcile
takeaway — all arrive already computed. Two consumers of the same rule will
diverge, and in an app *about* divergence that is a special kind of
embarrassing.

The most visible case is `DriftState`. It is computed once, in
`store::week::drift_state`, and the planner plate, the compact rail in a task
row and the report bar all render from it. They cannot disagree about whether
something overran, because none of them decides.

Tracked time follows the same rule one level down: the views `block_tracked`
and `task_tracked` are the truth, the `*_cache` tables are written in the same
transaction as every session mutation, and `rebuild_tracked_caches` regenerates
them from the views on demand. A cache that cannot be rebuilt is not a cache,
it is a second truth — so the fuzz test asserts they still match after 10,000
operations.

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
