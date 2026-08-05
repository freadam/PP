# Where this build departs from the spec

Nine deviations. Each one names the spec text it changes and the reason. Most
are cases where following the spec literally would produce a broken app, or
where the spec left a decision open (§9). Two — §5 and §6 — are changes the
product owner asked for after seeing the build.

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
| 6. Activity in 1.0 | **Yes, built** (revised — see §8) | Deferred at first as the item furthest from the loop; built once P0 and P1 were done, because the correlation panel is what makes CALIBRATE more than a self-report. |
| 7. Subtask depth | **Capped at 3**, as specified | Enforced in the command layer and tested. |
| 8. Subtask estimates | **Independent, rolled up for display only** | Roll-up is shown as `3/7 · 45m of 2h`; the parent's own estimate is never silently overwritten. |
| 9. Fonts | **Space Grotesk / Instrument Sans / Commit Mono**, referenced but not vendored | Third-party binaries are a release decision. `src/assets/fonts/README.md` explains. Never a CDN. |

---

## 5. The estimate field is a dropdown again

**Spec §1.5** lists this as a deliberate v1→v2 change: *"Estimate = dropdown of
5 fixed values → Free-text field with parser + presets"*, because a dropdown
*"contradicted the `~45m` capture token"*.

The product owner asked for the dropdown back, with a specific ladder — 30 min,
1, 1.5, 2, 2.5, 3, 3.5, 4 Hrs, Rollover. The spec's objection is still correct,
so the contradiction is handled rather than ignored: `estimateOptions` keeps any
off-ladder value the parser produced as an extra rung, labelled
*(from capture)*. Capturing `Fix login bug ~45m` and then opening the dropdown
shows 45 min in the list; it is never silently rounded to 30.

**Rollover** is the top of the ladder — work that does not fit one sitting and
carries across days. It is *not* the same as an unestimated task: "I haven't
thought about it" and "I have thought about it and it doesn't fit in a number"
are different states, and collapsing both into `estimate_sec IS NULL` would make
the backlog unreadable. So it is its own column (migration 0003), with the
pairing rule — rollover implies no estimate, an estimate clears rollover —
enforced in the command layer and covered by
`rollover_and_an_estimate_are_mutually_exclusive`.

Calibration ignores rollover tasks automatically: it already requires a
non-null estimate, and there is no ratio to compute without one.

---

## 6. Completed tasks stay on the project page

**Spec §3.2** lists the task groups as Overdue · Today · This week · No date ·
Someday — all open work. Completed tasks had nowhere to go.

They now form a sixth group, *Completed*, pinned last and rendered recessive:
greyed, struck through, with the drift rails dimmed. Full contrast returns on
hover and focus, so a completed row is never unreadable when you actually go to
it, and it stays keyboard-reachable.

The reason to show it at all: a finished project that renders empty is lying
about what it cost. The tail is capped at 100 rows, because it otherwise grows
without limit and this is a list view, not an archive.

---

## 7. Idle detection is OS-wide where the OS allows it

**Spec §4.5** requires idle detection but does not say how. "Idle" has to mean
*away from the machine* — a developer reading a stack trace in their editor is
working, and a tracker that discards that time is worse than no tracker.

`src-tauri/src/idle.rs` asks the OS directly: `GetLastInputInfo` on Windows,
`CGEventSourceSecondsSinceLastEventType` on macOS, both without permissions.
On Linux it returns `None` — X11 could answer through the XScreenSaver
extension and Wayland cannot answer at all — and the timer falls back to input
reported by Fruit's own window. That fallback is narrower, and Settings says so
rather than pretending, which is the same honesty §3.5 demands of Activity.

---

## 8. Activity was built, and §9.6 is now decided the other way

**Spec §9.6** asked whether Activity ships in 1.0 and recommended deferring it —
correctly, at the time: it is the feature furthest from the plan–track–reconcile
loop and it has the worst platform story of anything in the spec.

It is now built, because the argument for deferring it stops holding once the
loop itself is finished. CALIBRATE otherwise compares Fruit's record against
Fruit's record: an estimate against a timer that the same person started and
stopped. Activity is the only place an intention meets an *observation*, and
the sentence "you were in Slack for the hour you plotted for the refactor" is
not reachable any other way.

What the spec worried about is handled rather than argued away:

- **Platform.** Only Windows can do this today, through `GetForegroundWindow`
  and `QueryFullProcessImageNameW` — raw FFI, no new crates, no permissions.
  macOS needs an Accessibility grant and X11 needs `_NET_ACTIVE_WINDOW`; both
  are stubs that say they are stubs. Wayland cannot do it at all, by design, and
  says exactly that. `Support::describe()` is printed in Settings next to the
  switch, so a platform that cannot sample never shows a greyed-out control with
  no explanation.
- **Privacy.** Enforced in `store::activity`, below the IPC boundary, on the way
  in. See `docs/ARCHITECTURE.md`.

Nothing else in §9 changed.

---

## 9. `repeat_block` refuses to change a live rule

**Spec §2.3** lists recurring blocks without saying what happens when someone
changes the rule on a series that already exists.

This build refuses: *"This block already repeats. Remove the repeat first, then
set a new one."* Rewriting a live rule means deciding, silently, what happens to
occurrences the user may have already moved, tracked against or reconciled —
and there is no answer that is right in every case. Removing the series first
makes the loss explicit, asks for its scope, and is undoable in one step.

If this proves annoying in use, the fix is a rule editor that shows what it is
about to discard — not a quiet rewrite.

---

## 10. The browser connector reduces domains without the Public Suffix List

**Plan Rev 3 §5.4** requires the connector to record a domain rather than a URL.
It does not say how a hostname becomes a domain, and the correct answer is the
Public Suffix List — ~10,000 rules maintained by Mozilla.

This build ships a list of about thirty two-part suffixes instead, and falls back
to the last two labels for everything else.

The PSL changes monthly. An offline app that bundles a copy is an offline app
that carries a stale copy, misgroups domains, and never finds out. The failure is
silent and gets worse over time.

The small list is right for `youtube.com`, `twitch.tv`, `bbc.co.uk` and the
overwhelming majority of what this feature is pointed at. Where it is wrong, the
symptom is that two sites under an unusual suffix group together — visible, local
to one rule, and correctable by the user in the same breath as everything else,
because the classification is confirmed in the reconciler anyway.

If the client meets a suffix this gets wrong, the fix is one entry in
`MULTI_PART`, not a dependency.

---

## 11. The connector hands over a file, not a pipe

**Plan Rev 3 §5.4** says the extension talks to "the local app". It does not, and
the app is single-instance for a reason: the host process Chrome spawns cannot
open the database without recreating the two-writers corruption path.

The textbook hand-off is a named pipe. This build appends to a file in the app
data directory and drains it on the activity tick.

A pipe needs `CreateNamedPipeW`, a Windows-specific crate, `unsafe`, and a server
thread — none of which can be compiled or tested on the machine this was built
on, and all of it bought to save twenty seconds of latency on a signal whose own
sampling interval is twenty seconds. A file gets the user profile's ACL, the same
protection the database has, and survives Fruit being closed while Chrome runs —
which a pipe does not.

The bounds that make it safe are `MAX_SPOOL_BYTES`, `MAX_SAMPLE_AGE_MS` and the
enabled-sentinel, all covered by tests in `fruit_core::spool`.

If latency ever matters — it does not for a twenty-second signal — the pipe is a
drop-in replacement for `spool::append` and `spool::drain`, and nothing above
them changes.
