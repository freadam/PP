# Acceptance criteria, and what covers each one

Three columns: the criterion, what verifies it, and whether that verification
actually runs. Criteria that need a human or a real desktop window say so
instead of being quietly marked green.

```bash
cargo test                                  # M, F, D, and the U criteria below the UI
node scripts/check-ui.mjs                   # I1, I3–I7, U10 (needs `npm run preview`)
```

Two sets, and they nest. **M1–M16** are the MVP acceptance criteria from
Project Plan Revision 3 — what the client signs off. **F/U/I/D** are the
engineering criteria inherited from Fruit v2; several M criteria are satisfied
by them, and the mapping is given below.

---

## MVP acceptance (M) — Project Plan Rev 3 §14

To be demonstrated on the client's Windows PC.

| # | Criterion | Covered by | Runs |
|---|---|---|---|
| M1 | The Day view shows all 24 hours for a date at 30-minute default resolution, **including empty/unaccounted time** | `a_day_accounts_for_every_second_exactly_once`; the Day view renders one row per slot from `get_day` | ✅ / view built |
| M2 | Plans, confirmed sessions/life entries and observed activity are distinguishable and never double-count | `resolve_day` assigns each segment exactly one owner by precedence; `overlapping_records_never_double_count` fuzzes 200 random overlapping records | ✅ |
| M3 | Planned-vs-tracked drift is accurate per block, task, project, week and calibration bucket | `f2_drift_is_per_block`, `f6_calibration_needs_five_samples_and_uses_the_median` | ✅ |
| M4 | Projects and tasks support estimates, timers, sessions, subtasks and **one compact plain-text note each**; no Markdown or Obsidian workflow | Estimates/timers/sessions/subtasks all covered by F1/F4. **The note is still a Markdown renderer — not yet reduced.** | ❌ partial |
| M5 | Contribution modes apply to **Work only** and clear when a record becomes life time | `contribution_is_work_only_and_clears_on_conversion` | ✅ |
| M6 | Foreground app and idle capture when enabled, with pause, exclusions, retention and delete enforced | `activity_respects_the_privacy_contract`, `activity_purges_at_the_retention_limit` | ✅ |
| M7 | YouTube and Twitch classify as Entertainment automatically; user exceptions work prospectively | `defaults_classify_the_domains_the_plan_names`; `a_rule_made_while_reconciling_classifies_forwards_and_never_backwards` proves the *prospectively* half — a rule made today cannot rewrite a month already closed | ✅ |
| M8 | A task timer overlapping PC activity **enriches** the interval instead of adding duplicate duration | `observation_enriches_a_confirmed_session_without_adding_time` | ✅ |
| M9 | Manual life entries fill gaps, repeat, replace an interval with confirmation, and are editable later | `life_entries_fill_gaps_and_replace_with_confirmation`; repeat is **not built** | ❌ partial |
| M10 | Daily reconciliation covers overruns, unstarted plans, unplanned work, **observed-only activity and empty hours** | First three by `f3_…` and the reconcile suite; the last two by `reconcile_covers_observed_only_and_empty_hours`, sourced from `resolve_day`'s own segments so the sheet and the Day view cannot disagree about what is left to decide | ✅ |
| M11 | Entertainment budgets and planned/unplanned totals reconcile to the underlying intervals | **Not built.** Planned as the `at_most` case of weekly goals — see [`PLAN-WEEKLY-GOALS.md`](PLAN-WEEKLY-GOALS.md) and W1/W2 below. Unplanned totals are already real; planned entertainment is flat zero because no window can be planned yet, and the dashboard says so rather than drawing an empty axis. | ❌ |
| M12 | Reports open to a month summary by default; a month exports to `.xlsx` in the approved format with accurate totals | Reports is month-first; `get_month` is `get_day` summed, so a figure here and on a day cannot be computed two ways. Export writes three sheets whose totals are `COUNTIF` formulas rather than pasted numbers, from the same matrix the preview renders. | ✅ / format needs client sign-off |
| M13 | A historical workbook month imports through a preview with no silent loss and a variance report | **Not built** | ❌ |
| M14 | Backup/restore succeeds on a clean profile; the product stays usable offline with no unexpected outbound connection | `d5_restore_from_snapshot`, `d6_export_import_round_trips_exactly`; I7 asserts zero external requests | ✅ |
| M15 | Timers and activity segmentation recover safely after restart, sleep/wake or forced close | `u7_recovery_trims_to_the_last_heartbeat`, `d10_sleep_is_not_counted_by_default`, `sleep_splits_the_session_instead_of_spanning_it` | ✅ |
| M16 | Primary actions reachable by keyboard; important states never colour-only | One `COMMANDS` registry (structural); `check-ui.mjs` U10 and I3 | ✅ |

**12 of 16 fully covered, 2 partial, 2 not started.** Both components that used
to gate the rest — the browser connector and the XLSX writer — now exist, so
nothing remaining is blocked on a missing capability.

| | |
|---|---|
| **M4**, **M9** — partial | The task note is still a Markdown renderer, and life entries do not repeat. |
| **M11** — not started | Entertainment budgets. Planned below as the `at_most` case of weekly goals. |
| **M13** — not started | Workbook import. |
| **M12** — built, unsigned | The export exists and its totals are formulas; the *format* needs the client's reference month before it can be signed off. |

---

## Weekly goals (W) — proposed

Not built. Planned in [`PLAN-WEEKLY-GOALS.md`](PLAN-WEEKLY-GOALS.md), and
recorded here so the measures are agreed before the code exists rather than
written to fit it afterwards. All ten are testable in `fruit-core` with a fake
clock and no webview.

| # | Criterion | Why this one |
|---|---|---|
| W1 | A goal in force during a week is the goal that week's review reports, **after that goal has since been edited** | A goal edited into a new number must not retroactively rewrite how a past week went, or reviews stop meaning anything. Same argument as `activity_span.category` being stamped at write time. |
| W2 | A goal at zero on Monday morning reports **on pace**, not behind; expected progress never counts a day that has not happened | The month dashboard's "6% accounted" bug, in a new place. An app that reports the future as a failure is one whose numbers you learn to discount. |
| W3 | Extending a focus session shows in drift as an overrun, not as a larger plan | Extending is a plan revision. Fruit separates plan from record, so an extension has to cost something — the reading Rize cannot offer, because it has no plan to diverge from. |
| W4 | A two-hour meeting (`contribution = 'attend'`) does not accrue toward the continuous-work notice; two hours of `own` work does | Sitting in a review is not two hours heads-down, and the schema already records the difference. |
| W5 | The off-plan nudge fires only during **plotted** time, and is silenceable for the session | Both rules come from the reviewer's own false-positive caveat. Time nobody planned is time Fruit has no standing to have an opinion about. |
| W6 | Planned and unplanned switches are counted **separately**; a day of one unbroken session reports one stretch and zero unplanned switches | A switch landing on a block boundary is you executing your intention. Counting it as an interruption throws away the plan — the thing Fruit knows that an app-watcher cannot. |
| W7 | A user-defined category collects **both apps and domains**, and adding one never changes an existing month total | Claude is a website to one person and a desktop app to another; a bucket catching one of them answers the question wrongly. `counts_as` is what keeps the dashboard's arithmetic stable. |
| W8 | The uncategorised surface ranks by time and is reachable **without opening Settings** | The governing constraint. The app names the three things worth categorising instead of presenting an empty taxonomy. |
| W9 | Goal calibration reports at **n ≥ 5 weeks** and uses the **median** | The same discipline `f6` already holds estimates to: five samples of noise must not move a recommendation. |
| W10 | A template with insufficient history **says so** instead of guessing | A template that opens with an invented round number is a goal you did not believe when you set it. |

**W2 is the one that matters most**, for the reason given in its own row.

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
| U10 | Focus visible on every interactive element | `check-ui.mjs` walks every visible focusable element on all five views and asserts a ≥2px outline or a box-shadow. It navigates by `G then <key>`, never a click — `:focus-visible` stops matching programmatic focus once the page has seen a mouse interaction | ✅ |
| U11 | Reconcile never blocks; `Esc` defers; deferred days auto-accept after 7 | `u11_deferred_days_auto_accept_after_a_week` | ✅ |

## UI (I)

| # | Criterion | Covered by | Runs |
|---|---|---|---|
| I1 | Every colour resolves to a §5.2 token | `check-ui.mjs` greps every component for a literal hex/rgb/hsl | ✅ |
| I2 | Contrast ≥4.5:1 body, ≥3:1 graphics, both themes, **including Focus text over all four gradients** | Not automated. Every gradient ships with a validated 28% scrim (`.focus::before`), which is the mechanism §4.7 asks for, but the ratios have not been measured. **Open.** | ❌ |
| I3 | No drift state distinguishable by colour alone | The §5.6 redundancy table is implemented as texture (dashed / solid / dotted / 45° hatch) + badge + `aria-label`; `check-ui.mjs` asserts every rail and every Activity bar either carries a text alternative or is explicitly `aria-hidden` because the same fact is already text beside it | ✅ |
| I4 | Tabular figures on every changing numeral | `check-ui.mjs` reads computed `font-variant-numeric` on `.data`, `.micro`, `.focus-clock` | ✅ |
| I5 | Holds at 960×640, at each §5.8 breakpoint, and at 125% text | `check-ui.mjs` asserts zero horizontal overflow at 960 / 1130 / 1280 / 1490 and at 125% root font size, on every view rather than only the one that opens first | ✅ |
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

## P2 (§2, phase-tagged features)

The spec tags recurrence, `.ics` import and Activity as P2. They are
implemented, and here is what holds them up.

| Feature | Covered by | Runs |
|---|---|---|
| A repeating block is a series of *real*, trackable blocks | `a_recurring_block_produces_trackable_instances` — starts a timer on an instance and asserts it carries drift like any other block | ✅ |
| "Make this repeat" keeps the block, its task and its tracked time | `repeating_an_existing_block_keeps_its_tracked_time`; it also asserts a second rule on a live series is refused rather than silently rewriting the future | ✅ |
| Removing an occurrence asks its scope, and undo restores the scope | `series_edits_have_an_explicit_scope` (instance / future / all, with a restore in the middle) | ✅ |
| The repeat picker cannot offer a rule the engine refuses | `every_repeat_preset_parses_and_describes_itself` — the list is generated from `rrule::PRESETS` and served over IPC, so the renderer holds no copy | ✅ |
| The RRULE subset is the documented one, and refuses what it can't do | 10 tests in `rrule.rs`: `BYSETPOS`, `FREQ=YEARLY` and `2MO` are errors, not silent misreadings | ✅ |
| `.ics` events import as fixed blocks, and re-importing updates in place | `ics_import_creates_fixed_blocks_and_is_idempotent` | ✅ |
| A repeating meeting imports as a series, using the same engine | `ics_recurring_events_expand` | ✅ |
| Folded lines, `TZID=`, floating times and `PT1H30M` durations parse | 7 tests in `ics.rs` | ✅ |
| Off by default; titles a separate switch; exclusions applied before the row is written | `activity_respects_the_privacy_contract` — every clause of §3.5's promise, asserted against the store rather than the UI | ✅ |
| A run of samples becomes one span, not a row every 20 seconds | `activity_samples_coalesce_into_spans` | ✅ |
| Correlation is deterministic when two apps tie | `activity_correlates_with_the_block_underneath_it` asserts the tie-break; without it `HashMap` seeding decided which app "you were mostly in" | ✅ |
| Retention purges at the limit | `activity_purges_at_the_retention_limit` | ✅ |
| The recording indicator only lights when a row was actually written | Driven by the `activity:sampled` event, emitted from `record_activity`'s `Ok(true)` arm | structural |
| A platform that cannot sample says why | `frontmost::Support::describe()`, printed in Settings next to the switch. The Windows FFI path itself needs a Windows desktop. **Open.** | ❌ |

---

## Not verified in this environment

Five criteria are honestly open: **I2** (measured contrast over the Focus
gradients), **I8** (tray legibility on a real menu bar), **D4** (a real
`SIGKILL` mid-write), **D13** (second-instance focus), and the Windows
frontmost-window FFI, which needs a Windows desktop to exercise. This container
has no system webview, so `src-tauri` cannot be compiled here — it has built on
Windows (see the README), but every change to that crate since is unverified
until it is built again. They are the first things to check on a machine with a
desktop session.
