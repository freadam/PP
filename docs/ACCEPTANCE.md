# Acceptance criteria (§8), and what covers each one

Three columns: the criterion, what verifies it, and whether that verification
actually runs. Criteria that need a human or a real desktop window say so
instead of being quietly marked green.

```bash
cargo test                                  # F, D, and the U criteria below the UI
node scripts/check-ui.mjs                   # I1, I3–I7, U10 (needs `npm run preview`)
```

---

## Feature (F)

| # | Criterion | Covered by | Runs |
|---|---|---|---|
| F1 | Create → estimate → tag → schedule → track → reconcile → complete, keyboard only | `f1_full_loop_through_the_command_layer` covers every step existing as a command; the keyboard half is the single registry in `src/lib/commands.ts`, which the palette and `useKeyboard` both read | ✅ / structural |
| F2 | Sessions link to the block they were started from; drift is per block | `f2_drift_is_per_block` | ✅ |
| F3 | An unplanned session appears in Reconcile and converts to a retroactive block | `f3_unplanned_session_becomes_a_retroactive_block` | ✅ |
| F4 | A subtask schedules and tracks independently, and rolls up | `f4_subtasks_are_real_tasks` | ✅ |
| F5 | Reconciling writes exactly one `day_review` row and a takeaway | `f5_reconciling_writes_one_row_and_a_takeaway` (reconciles three times, asserts one row) | ✅ |
| F6 | Calibration reports at n ≥ 5 and uses median | `f6_calibration_needs_five_samples_and_uses_the_median` (the fifth sample is a 10× outlier the median shrugs off) | ✅ |
| F7 | Manual sessions exist and are visually distinguished | `f7_manual_sessions_are_flagged` for the record; the `source` badge and the unconfirmed left-border in `TaskDetail.tsx` for the display | ✅ / visual |

## UX (U)

| # | Criterion | Covered by | Runs |
|---|---|---|---|
| U1 | Every action reachable from the palette and a documented key | One registry (`COMMANDS`) feeds the palette, the keyboard handler and the `?` sheet — a command that is not reachable both ways cannot be written | structural |
| U2 | Parse chips before commit; `Cmd+Z` restores the raw text | Parser: 16 tests in `parser.rs`. Undo: the `pushUndo` inverse in `CaptureBar` restores `raw` into the input | ✅ / manual |
| U3 | `Esc` cancels a drag and restores the original position | `Planner.tsx` — the drag is local state and nothing is committed until `pointerup`, so cancelling is a state discard | manual |
| U4 | Collision policy is followed; fixed blocks never move | `u4_collision_policies` (overlap, push, push-into-fixed, shrink) | ✅ |
| U5 | Delete/complete/reschedule/parse reversible via `Cmd+Z` and an 8s toast that pauses on hover **and** focus | `deletes_are_soft_and_reversible` for the data; `.toast-progress` pauses on `:hover` and `:focus-within` | ✅ / visual |
| U6 | Idle defaults to discard; the exact span is shown | `u6_idle_defaults_to_discard_and_names_the_span` | ✅ |
| U7 | `kill -9` recovery trims to the last heartbeat, ±30s | `u7_recovery_trims_to_the_last_heartbeat` | ✅ |
| U8 | First run seeds a project with one already-drifted block | `u8_first_run_seeds_a_visible_drift_rail` | ✅ |
| U9 | Purposeful empty states; failed writes say what/why/next | Every view has one (`Empty`); `store.run()` is the single funnel that turns a `WireError` into copy | visual |
| U10 | Focus visible on every interactive element | `check-ui.mjs` walks 40 focusable elements and asserts a ≥2px outline or a box-shadow | ✅ |
| U11 | Reconcile never blocks; `Esc` defers; deferred days auto-accept after 7 | `u11_deferred_days_auto_accept_after_a_week` | ✅ |

## UI (I)

| # | Criterion | Covered by | Runs |
|---|---|---|---|
| I1 | Every colour resolves to a §5.2 token | `check-ui.mjs` greps every component for a literal hex/rgb/hsl | ✅ |
| I2 | Contrast ≥4.5:1 body, ≥3:1 graphics, both themes, **including Focus text over all four gradients** | Not automated. Every gradient ships with a validated 28% scrim (`.focus::before`), which is the mechanism §4.7 asks for, but the ratios have not been measured. **Open.** | ❌ |
| I3 | No drift state distinguishable by colour alone | The §5.6 redundancy table is implemented as texture (dashed / solid / dotted / 45° hatch) + badge + `aria-label`; `check-ui.mjs` asserts every rail carries a text alternative | ✅ |
| I4 | Tabular figures on every changing numeral | `check-ui.mjs` reads computed `font-variant-numeric` on `.data`, `.micro`, `.focus-clock` | ✅ |
| I5 | Holds at 960×640, at each §5.8 breakpoint, and at 125% text | `check-ui.mjs` asserts zero horizontal overflow at 960 / 1130 / 1280 / 1490 and at 125% root font size | ✅ |
| I6 | `prefers-reduced-motion` disables settle, drift and transitions | `check-ui.mjs` loads with `reducedMotion: "reduce"` and asserts no element has a duration > 50ms | ✅ |
| I7 | Fonts load from the bundle, no network request | `check-ui.mjs` fails on any request outside the origin. Note the woff2 files are **not vendored** yet (see `src/assets/fonts/README.md`) — the check proves no CDN is reached, not that the bundled faces render | ✅ / partial |
| I8 | Tray icon legible at 16px, communicates state without a badge | The mark is the drift rail as a monogram (`src-tauri/icons/`, `BrandMark`). Legibility at 16px on a real menu bar is unverified. **Open.** | ❌ |

## Data (D)

| # | Criterion | Covered by | Runs |
|---|---|---|---|
| D1 | `foreign_keys=ON` on every connection; no orphans after a 10k-op fuzz | `foreign_keys_are_on_for_every_connection` + `d1_d7_d11_fuzz_leaves_the_database_consistent` | ✅ |
| D2 | A v(n−1) fixture migrates in <3s, `quick_check` passes, snapshot exists | `d2_migration_from_the_previous_schema` (real v1 database, timed) | ✅ / small fixture |
| D3 | A higher `user_version` refuses cleanly | `d3_refuses_a_database_from_the_future`, `refuses_a_newer_schema` | ✅ |
| D4 | Force-kill during a write stays consistent, loses ≤500ms of typing | WAL + `synchronous=NORMAL` + a 500ms note debounce with a 3s maxWait. Not fuzzed under a real `SIGKILL`. **Open.** | ❌ |
| D5 | Restoring the newest snapshot yields a working app | `d5_restore_from_snapshot` | ✅ |
| D6 | Export → wipe → import reproduces every entity, ids included | `d6_export_import_round_trips_exactly` (compares all eight tables, then re-imports for idempotence) | ✅ |
| D7 | View totals equal the rebuilt caches after a fuzz | `d1_d7_d11_fuzz_leaves_the_database_consistent` | ✅ |
| D8 | 09:00 renders at 09:00 across a DST transition and a zone change; 02:30 on a spring-forward date resolves | `d8_dst_and_timezone_correctness`, plus five tests in `time.rs` | ✅ |
| D9 | A backwards clock never rewinds `elapsed_sec` | `d9_a_backwards_clock_never_rewinds_the_timer` | ✅ |
| D10 | 45 minutes of sleep is not counted, and the choice is offered | `d10_sleep_is_not_counted_by_default` | ✅ |
| D11 | At most one open session under concurrent fuzzing | `d1_d7_d11_fuzz_leaves_the_database_consistent` asserts it on every start | ✅ / single-threaded |
| D12 | `<img src=x onerror=…>` renders inert | `d12_notes_are_stored_verbatim_not_executed` for storage; `Markdown.tsx` never calls `dangerouslySetInnerHTML` and never parses raw HTML, so React escapes it | ✅ / structural |
| D13 | A second launch focuses the existing window | `tauri-plugin-single-instance` wired in `lib.rs`. Needs a desktop session. **Open.** | ❌ |

---

## Not verified in this environment

Five criteria are honestly open: **I2** (measured contrast over the Focus
gradients), **I8** (tray legibility on a real menu bar), **D4** (a real
`SIGKILL` mid-write), **D13** (second-instance focus), and everything that
depends on `src-tauri` compiling — this container has no system webview, so
that crate has never been through a compiler. They are the first things to
check on a machine with a desktop session.
