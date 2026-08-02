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
  src/rrule.rs         The RFC 5545 subset behind repeating blocks
  src/ics.rs           Read-only calendar import
  tests/               The §8 acceptance criteria, plus query-plan guards
src-tauri/             The shell: windows, tray, clock loop, IPC. Its own workspace.
  src/frontmost.rs     Which app is in front, per platform — and why, where it can't
src/                   React renderer. Holds no SQL and no business logic.
  styles/tokens.css    The §5 design system, one file
  components/DriftRail.tsx   The signature, at all three scales (§5.6)
scripts/check-ui.mjs   The UI half of §8 that a headless browser can check
scripts/gen-icons.py   Regenerates every bundle icon from the brand mark
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
cargo test           # 103 tests, incl. the §8 acceptance criteria
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
parsed *before* you commit. Estimates run on a fixed ladder — 30 min to 4 Hrs,
then Rollover for work that doesn't fit one sitting — and completed tasks sit
greyed at the bottom of the project rather than vanishing.

**Track.** One timer, enforced by the schema. Bound to a block when started
from one, which is what makes drift per-block computable. Every session covers
one contiguous *awake* interval, so a laptop that sleeps through a meeting
splits the record rather than producing a row that claims three hours of work.
Manual session entry, because people forget to press start and a record you
can't correct is a record you can't trust. Crash recovery that trims to the
last heartbeat, and idle that discards by default.

**Reconcile.** The day review: overran blocks, never-started blocks, untracked
gaps, unplanned sessions — each with a default action and a one-key alternative.
Closing it writes one `day_review` row and one plain-language takeaway. It never
blocks the app; `Esc` defers, and a deferred day auto-accepts after seven.

**Calibrate.** Trailing 30-day `tracked ÷ estimate`, bucketed by estimate size,
median not mean, reported only at n ≥ 5. Planned-vs-tracked per project per
week. Weekly targets with pace-to-date. And, if you turn it on, Activity: which
applications were actually in front of you during the block you plotted — the
one report that compares an intention against an *observation* rather than
against Fruit's own record.

**Repeat.** A repeating block is a series of real, individually trackable
blocks, materialised 90 days ahead and topped up as you scroll — not a rule
drawn over an empty calendar. Removing one always asks whether you mean this
occurrence, this and later, or all of them. Local `.ics` files import
read-only, as fixed blocks; re-importing the same file updates in place rather
than doubling every meeting.

### Activity, and what it promises

Off until you switch it on, and every promise is a control in Settings rather
than a sentence in a privacy policy:

- Applications and window titles are **separate** switches, and titles stay off
  when you enable applications.
- A per-app exclusion list, plus title fragments that suppress the title while
  still recording the app. Both are applied **before** the row is written, so an
  excluded app cannot resurface later through a query, an export or a backup.
- Pause survives a restart. Retention is 30 days, 90 days or forever, with the
  next purge date on screen. "Delete everything recorded" is one button.
- While it is sampling, the top bar says **Recording** — driven by the sampler
  actually writing a row, not by the setting being on.

Nothing leaves the machine, because nothing in Fruit ever does.

## What it doesn't do

No sync, no accounts, no mobile, no collaboration, no AI scheduling, no plugin
API, no web version, no telemetry, and no network calls in the core loop — the
OFFLINE badge in the top bar is a statement of fact, not a status indicator.

## State of the build

| Part | Status |
|---|---|
| `fruit-core` | Complete and tested — 103 tests green, incl. F1–F7, U4/U6/U7/U8/U11, D1–D3, D5–D12 |
| Renderer | Complete for P0 + P1 + P2; verified in a headless browser (I1, I3–I7, U10) across every view |
| `src-tauri` | Compiles and runs on **Windows** (x64, MSVC). Not built on macOS or Linux yet. See below. |
| Activity (§3.5) | Built. Off by default. Sampling implemented on Windows; macOS and X11 are stubs that say so, Wayland says why it can't. |
| Recurring blocks, `.ics` import | Built. An RFC 5545 subset — `DAILY`/`WEEKLY`/`MONTHLY` with `INTERVAL`, `BYDAY`, `BYMONTHDAY`, `COUNT`, `UNTIL`. |

`src-tauri` cannot be compiled in the container this repo was developed in —
linking needs a system webview and there is none — so it is the one part not
covered by the automated checks above. It has since been built and run on
Windows 10/11 with the MSVC toolchain. macOS and Linux are still unbuilt; the
platform-specific code is confined to `src-tauri/src/idle.rs` and
`src-tauri/src/frontmost.rs`, which are the first places to look if either
fails.

### Windows: "Access is denied (os error 5)" when building

Cargo cannot overwrite `fruit.exe` while a copy of it is running — Windows
locks the file, unlike Linux and macOS. Close the app (check the tray as well
as the taskbar; `tauri-plugin-single-instance` means a stray instance quietly
takes focus rather than starting a second one) or:

```powershell
taskkill /IM fruit.exe /F
```

then build again. Nothing is wrong with the code when this happens — the
message appears at the *link* step, which means everything before it compiled.

## Documents

- [`docs/PRODUCT-SPEC.md`](docs/PRODUCT-SPEC.md) — the whole app in one document: what it does, who for, every screen, every entity, every constraint
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — why the layers are where they are
- [`docs/ACCEPTANCE.md`](docs/ACCEPTANCE.md) — every §8 criterion, and what covers it
- [`docs/SPEC-DEVIATIONS.md`](docs/SPEC-DEVIATIONS.md) — where this build departs from the spec, and why
