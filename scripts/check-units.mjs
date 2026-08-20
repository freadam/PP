/**
 * The pure frontend derivations, checked the way the core checks its own.
 *
 *     node --experimental-strip-types --test scripts/check-units.mjs
 *     npm run test:units
 *
 * Almost nothing in `src/` belongs here: the rule this codebase runs on is that
 * anything which *decides* something lives in Rust and is covered by
 * `cargo test`, and the renderer only formats. What does belong here is the
 * short list of functions that group or fold DTOs the core already produced —
 * where a bug would put a wrong number on screen without any Rust test going
 * red.
 *
 * `splitByCapture` is the first of them, and it is C1's acceptance gate: the
 * ratio the frame is built on now has a number, and the number is tested.
 *
 * No test framework, deliberately. `node:test` and `node:assert` ship with the
 * runtime, and adding Vitest to check three folds would mean a bundler config,
 * a second module resolver and forty megabytes of node_modules for an app whose
 * whole argument is that the logic is somewhere else.
 */

import test from "node:test";
import assert from "node:assert/strict";

import {
  LIVE_TARGET,
  captureSlices,
  captureSplit,
  splitByCapture,
} from "../src/lib/honesty.ts";

const H = 3_600_000;

/** A resolved day segment, the shape `get_day` hands the renderer. */
const seg = (fromH, toH, owner) => ({
  from: fromH * H,
  to: toH * H,
  owner,
  evidence: [],
  hasDistraction: false,
});

const work = (fromH, toH, source) =>
  seg(fromH, toH, {
    kind: "work",
    sessionId: `s${fromH}`,
    taskId: "t",
    taskTitle: "Refactor the scheduler",
    projectId: null,
    projectName: null,
    projectColour: null,
    contribution: null,
    source,
  });

const life = (fromH, toH) =>
  seg(fromH, toH, {
    kind: "life",
    entryId: `e${fromH}`,
    areaId: "a",
    areaName: "Wellbeing",
    areaColour: "x",
    areaKind: "core",
    label: "Lunch",
    isPrivate: false,
  });

const gap = (fromH, toH) => seg(fromH, toH, { kind: "empty" });

test("the three buckets partition the confirmed seconds exactly", () => {
  const split = splitByCapture([
    { source: "timer", seconds: 3600 },
    { source: "pomodoro", seconds: 1500 },
    { source: "manual", seconds: 900 },
    { source: "recovered", seconds: 600 },
  ]);

  assert.equal(split.liveSec, 5100, "timer and pomodoro are both live capture");
  assert.equal(split.reconstructedSec, 900);
  assert.equal(split.recoveredSec, 600);
  // The counting invariant, in the small: nothing is dropped and nothing is
  // counted twice.
  assert.equal(
    split.confirmedSec,
    3600 + 1500 + 900 + 600,
    "the buckets sum to every second handed in",
  );
  assert.equal(split.liveSec + split.reconstructedSec + split.recoveredSec, split.confirmedSec);
});

test("recovered time is excluded from the ratio but not from the total", () => {
  const split = splitByCapture([
    { source: "timer", seconds: 3600 },
    { source: "recovered", seconds: 3600 },
  ]);

  // A crash restored the second hour. That is neither a clean live capture nor
  // a memory fill, so counting it either way would make the headline number a
  // statement about the recovery machine rather than about the user.
  assert.equal(split.livePct, 1, "the ratio is live / (live + reconstructed)");
  assert.equal(split.confirmedSec, 7200, "but it is still real, confirmed work");
});

test("an empty day has no ratio rather than a zero one", () => {
  const split = splitByCapture([]);
  // 0% reads as failure. "Nothing confirmed yet" is not a failure, and the
  // difference has to survive as far as the component or it cannot render it.
  assert.equal(split.livePct, null, "no divide-by-zero, and no hollow 0%");
  assert.equal(split.meetsTarget, false);
  assert.equal(split.confirmedSec, 0);
});

test("a day of nothing but recovered time still has no ratio", () => {
  const split = splitByCapture([{ source: "recovered", seconds: 3600 }]);
  assert.equal(split.livePct, null, "the denominator is empty even though the day is not");
  assert.equal(split.recoveredSec, 3600);
});

test("the gate is met at exactly the target, not only above it", () => {
  const at = splitByCapture([
    { source: "timer", seconds: 90 },
    { source: "manual", seconds: 10 },
  ]);
  assert.equal(at.livePct, LIVE_TARGET);
  assert.equal(at.meetsTarget, true, "≥90%, so 90% counts");

  const under = splitByCapture([
    { source: "timer", seconds: 89 },
    { source: "manual", seconds: 11 },
  ]);
  assert.equal(under.meetsTarget, false);
});

test("only confirmed work is read; life, observation and gaps are not", () => {
  const slices = captureSlices([
    work(9, 10, "timer"),
    life(12.5, 13.5),
    seg(14, 15, { kind: "observed", appId: "chrome.exe", domain: null, category: null }),
    seg(15, 16, { kind: "idle" }),
    gap(16, 17),
  ]);

  assert.equal(slices.length, 1, "four of the five segments are not claims about work");
  assert.deepEqual(slices[0], { source: "timer", seconds: 3600 });
});

test("the split is over segments, so it agrees with the day's work total", () => {
  // The case that makes segment-level the only honest input: a timer left
  // running through lunch. Rust resolves the overlap in favour of the confirmed
  // life entry, so the day holds 09:00–12:30 and 13:30–14:00 of work — not the
  // five unbroken hours the session row would claim.
  const segments = [work(9, 12.5, "timer"), life(12.5, 13.5), work(13.5, 14, "timer")];
  const confirmedWorkSec = segments
    .filter((s) => s.owner.kind === "work")
    .reduce((acc, s) => acc + (s.to - s.from) / 1000, 0);

  const split = captureSplit(segments);
  assert.equal(
    split.confirmedSec,
    confirmedWorkSec,
    "the headline percentage is a percentage of the number shown beside it",
  );
  assert.equal(split.liveSec, 4 * 3600, "lunch is not work, however the timer was left");
});

test("a mixed day reads the way the card will report it", () => {
  const split = captureSplit([
    work(9, 10, "timer"),
    work(10, 10.5, "pomodoro"),
    work(11, 11.5, "manual"),
    work(15, 15.25, "recovered"),
  ]);

  assert.equal(split.liveSec, 5400);
  assert.equal(split.reconstructedSec, 1800);
  assert.equal(split.recoveredSec, 900);
  assert.equal(Math.round(split.livePct * 100), 75);
  assert.equal(split.meetsTarget, false, "75% is under the 90% gate");
});
