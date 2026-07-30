# Where this build departs from the spec

Five deviations. Each one names the spec text it changes and the reason. Nothing
here is a preference; they are all cases where following the spec literally
would produce a broken app, or where the spec left a decision open (§9) and the
build had to make one.

---

## 1. The date `CHECK` constraints were unsatisfiable

**Spec §6.2** writes every calendar-date constraint as:

```sql
due_date TEXT CHECK (due_date IS NULL OR due_date GLOB '____-__-__')
```

In SQLite, `_` is a **`LIKE`** wildcard. `GLOB` uses `?` for a single character
and `*` for a run. So that pattern matches the ten-character literal string
`____-__-__` and rejects every real date — `2025-07-30` included. Taken
literally, no task could carry a due date, no block could be scheduled, and no
day could be reconciled.

Fixed in `0001_init.sql` with a digit character class, which is what the spec
meant and is stricter besides:

```sql
GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
```

Applies to `task.due_date`, `scheduled_block.local_date` and
`day_review.local_date`.

---

## 2. `elapsed_sec` may decrease — but only when the user says so

**Spec §6.5** lists the invariant *"A session's `elapsed_sec` never decreases —
monotonic accumulator; command clamps to previous value."*

Held literally, this contradicts §4.5: discarding a 20-minute idle span *must*
reduce the session's elapsed time, or "discard" means nothing. The same applies
to trimming a recovered session back to its last heartbeat.

The invariant as implemented: elapsed never decreases **while running**, which
is what protects it from a backwards clock (D9). A user-authorised trim — idle
discard, recovery — resets the floor explicitly, in one place
(`TimerRuntime::floor_sec`). Both behaviours are tested: `d9_a_backwards_clock_never_rewinds_the_timer`
and `u6_idle_defaults_to_discard_and_names_the_span`.

---

## 3. Sessions are hard-deleted, with a tombstone

**Spec §6.1 rule 6** makes deletes soft across the board. `time_session` is the
exception: `block_tracked` and `task_tracked` sum every session row, so a
soft-deleted session would keep counting toward drift while claiming to be
deleted — two truths, in the one place the app cannot afford them.

`delete_session` removes the row and writes the full record to a `setting` key
for the undo window; `restore` re-inserts it verbatim. Undo and the 8s toast
behave identically to every other delete. Sessions do not appear in *Recently
deleted*, which is a real (small) reduction in scope.

---

## 4. Open decisions from §9, as decided

The spec left nine decisions open. This build had to pick; each is reversible.

| §9 | Decision | Why |
|---|---|---|
| 1. Persistence boundary | **SQL in Rust now** | It is the recommendation, and doing it later means migrating real users' databases. It also made §8's D-criteria testable. |
| 2. Reconcile in 1.0 | **Yes** | It is the loop-closing feature; without it the app has no reason to be opened on day 30. |
| 3. Visual direction | **The instrument** — flat plates, hairlines, no shadows, no gradient mark | The recovery path if it reads as unfinished is documented in §5.1: warm the surface tokens, raise the radius to 4px. Not shadows. |
| 4. Now cursor | **`paper` hairline with a gutter indicator** | Red stays reserved for destructive actions. |
| 5. Pomodoro strip | **4 dots**, long break as a square | Shape carries the distinction, so it survives at 6px. |
| 6. Activity in 1.0 | **No, P2** | Furthest from the loop, worst platform story. The view says so plainly rather than hiding. |
| 7. Subtask depth | **Capped at 3**, as specified | Enforced in the command layer and tested. |
| 8. Subtask estimates | **Independent, rolled up for display only** | Roll-up is shown as `3/7 · 45m of 2h`; the parent's own estimate is never silently overwritten. |
| 9. Fonts | **Space Grotesk / Instrument Sans / Commit Mono**, referenced but not vendored | Third-party binaries are a release decision. `src/assets/fonts/README.md` explains. Never a CDN. |

---

## 5. Idle detection is OS-wide where the OS allows it

**Spec §4.5** requires idle detection but does not say how. "Idle" has to mean
*away from the machine* — a developer reading a stack trace in their editor is
working, and a tracker that discards that time is worse than no tracker.

`src-tauri/src/idle.rs` asks the OS directly: `GetLastInputInfo` on Windows,
`CGEventSourceSecondsSinceLastEventType` on macOS, both without permissions.
On Linux it returns `None` — X11 could answer through the XScreenSaver
extension and Wayland cannot answer at all — and the timer falls back to input
reported by Fruit's own window. That fallback is narrower, and Settings says so
rather than pretending, which is the same honesty §3.5 demands of Activity.
