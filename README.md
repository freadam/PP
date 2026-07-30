# Fruit

**Local-first planner, tracker, and reconciler.** You plot a course, you record
your actual track, and the difference is drift. A navigator doesn't admire the
drift — they correct for it. Fruit's job is the same three moves: show the
drift, make correcting it one keystroke, and let the accumulated record make
tomorrow's plot better.

Built to the *Fruit — Technical Product Specification v2*. Section references
throughout the code (`§4.5`, `§6.8`) point at that document.

```
PLAN ──▶ TRACK ──▶ RECONCILE ──▶ CALIBRATE ──▶ back to PLAN, better
```

---

## Layout

```
crates/fruit-core/     Everything Fruit knows how to do. No UI, no Tauri.
  migrations/          Forward-only SQL (§6.6)
  src/store/           The intent-based command layer (§6.8)
  src/parser.rs        The capture grammar (§4.4)
  src/clock.rs         Wall vs monotonic time — why sleep is never counted
  tests/               The §8 acceptance criteria, plus query-plan guards
src-tauri/             The shell: windows, tray, clock loop, IPC. Its own workspace.
src/                   React renderer. Holds no SQL and no business logic.
  styles/tokens.css    The §5 design system, one file
  components/DriftRail.tsx   The signature, at all three scales (§5.6)
scripts/check-ui.mjs   The UI half of §8 that a headless browser can check
```

The split between `fruit-core` and `src-tauri` is the load-bearing decision.
Because the command layer has no Tauri dependency, the invariants in §6.5 — one
running timer, no midnight-crossing blocks, caches that match their views — are
covered by `cargo test` rather than by clicking around a running app.

## Running it

```bash
npm install
npm run app          # the real thing: Tauri window + SQLite on disk
```

```bash
npm run dev          # browser preview, reading recorded DTOs (see below)
cargo test           # 68 tests, incl. the §8 acceptance criteria
node scripts/check-ui.mjs      # I1, I3–I7, U10 against `npm run preview`
```

### Browser preview

`npm run dev` outside a Tauri window has no backend attached. Rather than mock
the command layer in JavaScript — a second implementation of the rules, in an
app about divergence — it reads DTOs **recorded from the real Rust code**:

```bash
cargo run -p fruit-core --bin dump-fixtures    # regenerates src/dev/fixtures.json
```

Reads work and look exactly like the app. Writes refuse, with a sentence saying
why. Use `npm run app` for anything real.

## What it does

**Plan.** Projects, tasks, subtasks that are real tasks, a 24-hour planner at
1/3/7-day spans, drag-to-schedule with three collision policies, and a capture
grammar (`Fix login bug #work ~45m !! ^tomorrow 9am`) that shows you what it
parsed *before* you commit.

**Track.** One timer, enforced by the schema. Bound to a block when started
from one, which is what makes drift per-block computable. Manual session entry,
because people forget to press start and a record you can't correct is a record
you can't trust. Crash recovery that trims to the last heartbeat, idle detection
that discards by default, and sleep that is never counted silently.

**Reconcile.** The day review: overran blocks, never-started blocks, untracked
gaps, unplanned sessions — each with a default action and a one-key alternative.
Closing it writes one `day_review` row and one plain-language takeaway. It never
blocks the app; `Esc` defers, and a deferred day auto-accepts after seven.

**Calibrate.** Trailing 30-day `tracked ÷ estimate`, bucketed by estimate size,
median not mean, reported only at n ≥ 5. Planned-vs-tracked per project per
week. Weekly targets with pace-to-date.

## What it doesn't do

No sync, no accounts, no mobile, no collaboration, no AI scheduling, no plugin
API, no web version, no telemetry, and no network calls in the core loop — the
OFFLINE badge in the top bar is a statement of fact, not a status indicator.

## State of the build

| Part | Status |
|---|---|
| `fruit-core` | Complete and tested — 68 tests green, incl. F1–F7, U4/U6/U7/U8/U11, D1–D3, D5–D12 |
| Renderer | Complete for P0 + P1; verified in a headless browser (I1, I3–I7, U10) |
| `src-tauri` | **Written but not compiled here** — linking needs a system webview, and this container has none. See below. |
| Activity (§3.5) | P2, deliberately not built. The view says so and explains Wayland. |
| Recurring blocks, `.ics` import | P2, not built. `rrule`/`series_id` exist in the schema. |

`src-tauri` is the one part of this repo that has never been through a
compiler. It is thin by design — windows, tray, a one-second loop, and a
one-line wrapper per command — but "thin" is not "verified". On a machine with
a webview, `npm run app` is the first thing to try, and `cargo build` inside
`src-tauri/` will surface anything that needs fixing.

## Documents

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — why the layers are where they are
- [`docs/ACCEPTANCE.md`](docs/ACCEPTANCE.md) — every §8 criterion, and what covers it
- [`docs/SPEC-DEVIATIONS.md`](docs/SPEC-DEVIATIONS.md) — where this build departs from the spec, and why
