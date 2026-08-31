/**
 * The engine, held to the numbers `fruit-core` produces.
 *
 * Where a test asserts a literal — 1.1 for the median, 2.5 for an even count,
 * the bucket edges, the score weights — that literal is transcribed from the
 * Rust it was ported from, not from a run of this code. A port that agrees with
 * itself proves nothing.
 */

const test = require('node:test');
const assert = require('node:assert');
const E = require('../src/engine.js');

const HOUR = 3600;
const ms = (sec) => sec * 1000;

/** A minimal Super Productivity state. */
function state(over) {
  return Object.assign(
    { tasks: [], archivedTasks: [], projects: [], tags: [] },
    over,
  );
}

function task(over) {
  return Object.assign(
    {
      id: 't' + Math.random().toString(36).slice(2),
      title: 'Task',
      timeEstimate: 0,
      timeSpent: 0,
      timeSpentOnDay: {},
      isDone: false,
      projectId: null,
      tagIds: [],
      subTaskIds: [],
      created: 0,
    },
    over,
  );
}

// ─── the statistic ────────────────────────────────────────────────────

test('median is not mean', () => {
  // Transcribed from `median_is_not_mean` in fruit-core: one abandoned task
  // gives mean 3.1 and median 1.1. The median is the honest one.
  const ratios = [1.0, 1.1, 1.2, 1.05, 11.2];
  assert.strictEqual(E.median(ratios), 1.1);
  assert.ok(ratios.reduce((a, b) => a + b, 0) / ratios.length > 3.0);
});

test('median of even counts averages the middle', () => {
  assert.strictEqual(E.median([1.0, 2.0, 3.0, 4.0]), 2.5);
});

test('median of nothing is null, never zero', () => {
  assert.strictEqual(E.median([]), null);
});

// ─── dates ────────────────────────────────────────────────────────────

test('the week starts on Monday', () => {
  // 2026-08-31 is a Monday; the Sunday before it belongs to the week before.
  assert.strictEqual(E.formatDate(E.weekStart(E.parseDate('2026-08-31'))), '2026-08-31');
  assert.strictEqual(E.formatDate(E.weekStart(E.parseDate('2026-09-06'))), '2026-08-31');
  assert.strictEqual(E.formatDate(E.weekStart(E.parseDate('2026-08-30'))), '2026-08-24');
});

test('a month period covers the calendar month, not thirty days', () => {
  const [a, b] = E.periodBounds(E.parseDate('2026-02-14'), 'month');
  assert.strictEqual(E.formatDate(a), '2026-02-01');
  assert.strictEqual(E.formatDate(b), '2026-02-28');
});

test('stepping back a month from the 31st does not become the 1st of the wrong month', () => {
  // Rust's `shift_period` lands on the first of the target month by
  // construction; the bounds then expand it.
  const back = E.shiftPeriod(E.parseDate('2026-03-31'), 'month', -1);
  const [a, b] = E.periodBounds(back, 'month');
  assert.strictEqual(E.formatDate(a), '2026-02-01');
  assert.strictEqual(E.formatDate(b), '2026-02-28');
});

test('an unknown period is a day, because the narrowest answer is safest', () => {
  const [a, b] = E.periodBounds(E.parseDate('2026-08-19'), 'fortnight');
  assert.strictEqual(E.formatDate(a), '2026-08-19');
  assert.strictEqual(E.formatDate(b), '2026-08-19');
});

test('the weekday mask is Monday = 1 … Sunday = 64', () => {
  const MON_FRI = 31;
  assert.ok(E.weekdayApplies('2026-08-31', MON_FRI)); // Monday
  assert.ok(E.weekdayApplies('2026-09-04', MON_FRI)); // Friday
  assert.ok(!E.weekdayApplies('2026-09-05', MON_FRI)); // Saturday
  assert.ok(!E.weekdayApplies('2026-09-06', MON_FRI)); // Sunday
  assert.ok(E.weekdayApplies('2026-09-06', E.ALL_DAYS));
});

test('a weekday is named, not printed as an ISO date', () => {
  assert.strictEqual(E.weekdayName('2026-08-16'), 'Sunday');
  assert.strictEqual(E.weekdayName('nonsense'), 'nonsense');
});

// ─── formatting ───────────────────────────────────────────────────────

test('durations read the way Fruit writes them', () => {
  assert.strictEqual(E.duration(45 * 60), '45m');
  assert.strictEqual(E.duration(2 * HOUR), '2h');
  assert.strictEqual(E.duration(2 * HOUR + 20 * 60), '2h 20m');
  assert.strictEqual(E.hm(2 * HOUR + 5 * 60), '2h 05m');
  assert.strictEqual(E.drift(-22 * 60), '−22m');
  assert.strictEqual(E.drift(0), '0m');
});

// ─── normalisation ────────────────────────────────────────────────────

test('a parent task is not counted on top of its subtasks', () => {
  // Super Productivity maintains the parent's timeSpentOnDay as the sum of its
  // subtasks'. Counting both doubles every total on the screen.
  const m = E.normalise(
    state({
      tasks: [
        task({ id: 'p', subTaskIds: ['a', 'b'], timeSpentOnDay: { '2026-08-31': ms(3 * HOUR) } }),
        task({ id: 'a', parentId: 'p', timeSpentOnDay: { '2026-08-31': ms(HOUR) } }),
        task({ id: 'b', parentId: 'p', timeSpentOnDay: { '2026-08-31': ms(2 * HOUR) } }),
      ],
    }),
  );
  assert.strictEqual(m.tasks.length, 2);
  const r = E.buildWorkReport(m, '2026-08-31', 'day', null, Date.parse('2026-08-31T12:00:00'));
  assert.strictEqual(r.totalWorkSec, 3 * HOUR);
});

test('a subtask inherits the project and tags it does not state', () => {
  const m = E.normalise(
    state({
      projects: [{ id: 'proj', title: 'Work', theme: { primary: '#123456' } }],
      tags: [{ id: 'deep', title: 'Deep work', color: '#abcdef' }],
      tasks: [
        task({ id: 'p', subTaskIds: ['a'], projectId: 'proj', tagIds: ['deep'] }),
        task({ id: 'a', parentId: 'p', timeSpentOnDay: { '2026-08-31': ms(HOUR) } }),
      ],
    }),
  );
  assert.strictEqual(m.tasks[0].projectId, 'proj');
  assert.strictEqual(m.tasks[0].tagId, 'deep');
});

test("the synthetic TODAY tag is not a kind of work", () => {
  const m = E.normalise(
    state({
      tags: [{ id: 'deep', title: 'Deep work' }],
      tasks: [task({ id: 'a', tagIds: ['TODAY', 'deep'], timeSpentOnDay: { '2026-08-31': ms(HOUR) } })],
    }),
  );
  assert.strictEqual(m.tasks[0].tagId, 'deep');
});

test('the live copy of a task wins over the archived snapshot', () => {
  const m = E.normalise(
    state({
      tasks: [task({ id: 'x', timeSpentOnDay: { '2026-08-31': ms(2 * HOUR) } })],
      archivedTasks: [task({ id: 'x', timeSpentOnDay: { '2026-08-31': ms(HOUR) } })],
    }),
  );
  assert.strictEqual(m.tasks.length, 1);
  assert.strictEqual(m.tasks[0].trackedSec, 2 * HOUR);
});

test('archived tasks are still counted — the 30-day window is mostly archive', () => {
  const m = E.normalise(
    state({ archivedTasks: [task({ id: 'x', timeSpentOnDay: { '2026-08-31': ms(HOUR) } })] }),
  );
  assert.strictEqual(m.tasks.length, 1);
});

// ─── calibration ──────────────────────────────────────────────────────

/** `n` done tasks of `estimate` seconds that each took `ratio` × that. */
function doneTasks(n, estimate, ratio, doneOn) {
  return Array.from({ length: n }, (_, i) =>
    task({
      id: `${estimate}-${i}`,
      isDone: true,
      doneOn: Date.parse(`${doneOn}T12:00:00`),
      timeEstimate: ms(estimate),
      timeSpentOnDay: { [doneOn]: ms(Math.round(estimate * ratio)) },
    }),
  );
}

const NOW = Date.parse('2026-08-31T12:00:00');

test('a bucket is silent below five samples and speaks at five', () => {
  const four = E.buildCalibration(
    E.normalise(state({ tasks: doneTasks(4, HOUR, 1.5, '2026-08-25') })),
    null,
    NOW,
  );
  assert.strictEqual(four.buckets.find((b) => b.bucket === '1h').isReportable, false);
  assert.match(four.headline, /4 of 5 estimates tracked so far/);

  const five = E.buildCalibration(
    E.normalise(state({ tasks: doneTasks(5, HOUR, 1.5, '2026-08-25') })),
    null,
    NOW,
  );
  const b = five.buckets.find((x) => x.bucket === '1h');
  assert.strictEqual(b.isReportable, true);
  assert.strictEqual(b.n, 5);
  assert.strictEqual(b.medianRatio, 1.5);
  assert.strictEqual(five.headline, 'You run 1.50× your estimates. Your 1h estimates are the least accurate.');
});

test('the estimate buckets sit on the boundaries fruit-core uses', () => {
  const edges = [
    [900, '<=15m'],
    [901, '30m'],
    [1800, '30m'],
    [1801, '1h'],
    [3600, '1h'],
    [3601, '2h'],
    [7200, '2h'],
    [7201, '3h+'],
  ];
  for (const [estimate, bucket] of edges) {
    const c = E.buildCalibration(
      E.normalise(state({ tasks: doneTasks(5, estimate, 1.0, '2026-08-25') })),
      null,
      NOW,
    );
    const hit = c.buckets.filter((b) => b.n > 0).map((b) => b.bucket);
    assert.deepStrictEqual(hit, [bucket], `estimate ${estimate}s belongs in ${bucket}`);
  }
});

test('calibration only looks back thirty days', () => {
  const inside = E.buildCalibration(
    E.normalise(state({ tasks: doneTasks(5, HOUR, 2.0, '2026-08-02') })),
    null,
    NOW,
  );
  assert.strictEqual(inside.sampleCount, 5);
  const outside = E.buildCalibration(
    E.normalise(state({ tasks: doneTasks(5, HOUR, 2.0, '2026-07-01') })),
    null,
    NOW,
  );
  assert.strictEqual(outside.sampleCount, 0);
  assert.match(outside.headline, /Come back after five tracked tasks/);
});

test('the worst bucket is the one furthest from 1.0 in either direction', () => {
  const c = E.buildCalibration(
    E.normalise(
      state({
        tasks: [
          ...doneTasks(5, HOUR, 1.2, '2026-08-25'),
          ...doneTasks(5, 4 * HOUR, 0.5, '2026-08-25'),
        ],
      }),
    ),
    null,
    NOW,
  );
  // 0.5 is 0.5 from parity; 1.2 is 0.2. Under-running counts as badly as over.
  assert.strictEqual(c.worstBucket, '3h+');
});

test('a task with no estimate or no tracked time is not a sample', () => {
  const c = E.buildCalibration(
    E.normalise(
      state({
        tasks: [
          task({ id: 'a', isDone: true, doneOn: NOW, timeEstimate: 0, timeSpentOnDay: { '2026-08-31': ms(HOUR) } }),
          task({ id: 'b', isDone: true, doneOn: NOW, timeEstimate: ms(HOUR), timeSpentOnDay: {} }),
          task({ id: 'c', isDone: false, doneOn: NOW, timeEstimate: ms(HOUR), timeSpentOnDay: { '2026-08-31': ms(HOUR) } }),
        ],
      }),
    ),
    null,
    NOW,
  );
  assert.strictEqual(c.sampleCount, 0);
});

test('the headline names over-estimating separately from under', () => {
  const under = E.buildCalibration(
    E.normalise(state({ tasks: doneTasks(5, HOUR, 0.5, '2026-08-25') })),
    null,
    NOW,
  );
  assert.match(under.headline, /^You come in at 0\.50× your estimates — you over-estimate\./);

  const level = E.buildCalibration(
    E.normalise(state({ tasks: doneTasks(5, HOUR, 1.0, '2026-08-25') })),
    null,
    NOW,
  );
  assert.match(level.headline, /^You land within 5% of your estimates \(1\.00×\)\./);
});

// ─── the work report ──────────────────────────────────────────────────

test('a day with no work is a zero, never a missing point', () => {
  const m = E.normalise(
    state({ tasks: [task({ timeSpentOnDay: { '2026-08-31': ms(HOUR) } })] }),
  );
  const r = E.buildWorkReport(m, '2026-08-31', 'week', null, NOW);
  assert.strictEqual(r.days.length, 7);
  assert.strictEqual(r.days[0].date, '2026-08-31');
  assert.strictEqual(r.days[6].date, '2026-09-06');
  assert.strictEqual(r.days[3].workSec, 0);
});

test('the days of a range sum to the range', () => {
  const m = E.normalise(
    state({
      tasks: [
        task({ timeSpentOnDay: { '2026-08-31': ms(HOUR), '2026-09-01': ms(2 * HOUR) } }),
        task({ timeSpentOnDay: { '2026-09-01': ms(1800) } }),
      ],
    }),
  );
  const r = E.buildWorkReport(m, '2026-09-02', 'week', null, NOW);
  assert.strictEqual(r.days.reduce((a, d) => a + d.workSec, 0), r.totalWorkSec);
  assert.strictEqual(r.totalWorkSec, 3 * HOUR + 1800);
});

test('the splits partition the total and the shares add to one', () => {
  const m = E.normalise(
    state({
      projects: [
        { id: 'p1', title: 'Alpha', theme: { primary: '#111111' } },
        { id: 'p2', title: 'Beta', theme: { primary: '#222222' } },
      ],
      tags: [{ id: 'deep', title: 'Deep work' }],
      tasks: [
        task({ projectId: 'p1', tagIds: ['deep'], timeSpentOnDay: { '2026-08-31': ms(3 * HOUR) } }),
        task({ projectId: 'p2', timeSpentOnDay: { '2026-08-31': ms(HOUR) } }),
      ],
    }),
  );
  const r = E.buildWorkReport(m, '2026-08-31', 'day', null, NOW);
  assert.strictEqual(r.byProject.reduce((a, s) => a + s.seconds, 0), r.totalWorkSec);
  assert.strictEqual(r.byTag.reduce((a, s) => a + s.seconds, 0), r.totalWorkSec);
  // Longest first.
  assert.deepStrictEqual(r.byProject.map((s) => s.name), ['Alpha', 'Beta']);
  assert.strictEqual(r.byProject[0].share, 0.75);
});

test('the untagged and no-project buckets are kept, so the time can be found', () => {
  const m = E.normalise(
    state({ tasks: [task({ timeSpentOnDay: { '2026-08-31': ms(HOUR) } })] }),
  );
  const r = E.buildWorkReport(m, '2026-08-31', 'day', null, NOW);
  assert.deepStrictEqual(r.byProject.map((s) => s.name), ['No project']);
  assert.deepStrictEqual(r.byTag.map((s) => s.name), ['Untagged']);
  assert.strictEqual(r.byTag[0].id, null);
});

test('an empty period is not scored, rather than scored zero', () => {
  const r = E.buildWorkReport(E.normalise(state({})), '2026-08-31', 'week', null, NOW);
  assert.strictEqual(r.score.scored, false);
  assert.strictEqual(r.score.rating, 'Not scored');
  assert.strictEqual(r.findings.length, 0);
});

test('the score weights sum to one after Deliberate is dropped', () => {
  const m = E.normalise(
    state({ tasks: [task({ timeSpentOnDay: { '2026-08-31': ms(6 * HOUR) } })] }),
  );
  const r = E.buildWorkReport(m, '2026-08-31', 'day', null, NOW);
  const sum = r.score.components.reduce((a, c) => a + c.weight, 0);
  assert.ok(Math.abs(sum - 1) < 1e-9, `weights sum to ${sum}`);
  assert.deepStrictEqual(
    r.score.components.map((c) => c.label),
    ['Against target', 'Depth', 'Accounted'],
  );
  // Fruit's 0.35 / 0.25 / 0.15 rescaled by 1/0.75.
  assert.ok(Math.abs(r.score.components[0].weight - 0.35 / 0.75) < 1e-9);
});

test('the score is the printed sum of its parts', () => {
  const m = E.normalise(
    state({
      tags: [{ id: 'deep', title: 'Deep work' }],
      tasks: [task({ tagIds: ['deep'], timeSpentOnDay: { '2026-08-31': ms(6 * HOUR) } })],
    }),
  );
  const r = E.buildWorkReport(m, '2026-08-31', 'day', null, NOW);
  const expected = Math.round(
    r.score.components.reduce((a, c) => a + c.value * c.weight, 0),
  );
  assert.strictEqual(r.score.value, expected);
  // Six hours on the one working day is a full mark for Depth; fully tagged is
  // a full mark for Accounted; a single day is perfectly even.
  assert.strictEqual(r.score.value, 100);
  assert.strictEqual(r.score.rating, 'Very good');
});

test('a day still to come is not a day you missed the target on', () => {
  const m = E.normalise(
    state({ tasks: [task({ timeSpentOnDay: { '2026-08-31': ms(6 * HOUR) } })] }),
  );
  const settings = { dailyTargetSec: 6 * HOUR, appliesDays: 31 };
  // Viewing the week from its Monday: only Monday has happened.
  const r = E.buildWorkReport(m, '2026-08-31', 'week', settings, NOW);
  assert.strictEqual(r.targetDaysApplicable, 1);
  assert.strictEqual(r.targetDaysMet, 1);
  const target = r.findings.find((f) => f.id === 'target');
  assert.strictEqual(target.headline, 'Every day hit the 6h 00m target');
  assert.strictEqual(target.tone, 'good');
});

test('a target that does not apply on a weekend reports no shortfall there', () => {
  const m = E.normalise(
    state({ tasks: [task({ timeSpentOnDay: { '2026-09-05': ms(HOUR) } })] }),
  );
  const r = E.buildWorkReport(
    m,
    '2026-08-31',
    'week',
    { dailyTargetSec: 6 * HOUR, appliesDays: 31 },
    Date.parse('2026-09-07T12:00:00'),
  );
  // Mon–Fri all elapsed and all missed; the Saturday that carried the work is
  // not counted at all.
  assert.strictEqual(r.targetDaysApplicable, 5);
  assert.strictEqual(r.targetDaysMet, 0);
});

// ─── the baseline ─────────────────────────────────────────────────────

test('a period with nothing recorded is skipped, not averaged in as zero', () => {
  const m = E.normalise(
    state({
      tasks: [
        task({ timeSpentOnDay: { '2026-08-31': ms(4 * HOUR) } }), // this week
        task({ timeSpentOnDay: { '2026-08-24': ms(2 * HOUR) } }), // one week back
        // two and three weeks back: silence (a holiday)
        task({ timeSpentOnDay: { '2026-08-03': ms(6 * HOUR) } }), // four weeks back
      ],
    }),
  );
  const r = E.buildWorkReport(m, '2026-08-31', 'week', null, NOW);
  assert.strictEqual(r.baseline.periods, 2);
  assert.strictEqual(r.baseline.totalWorkSec, 4 * HOUR); // (2h + 6h) / 2
});

test('no history at all means no baseline and no comparison finding', () => {
  const m = E.normalise(
    state({ tasks: [task({ timeSpentOnDay: { '2026-08-31': ms(4 * HOUR) } })] }),
  );
  const r = E.buildWorkReport(m, '2026-08-31', 'week', null, NOW);
  assert.strictEqual(r.baseline, null);
  assert.strictEqual(r.findings.find((f) => f.id === 'total-vs-baseline'), undefined);
});

test('a change under a tenth is noise and is not called a trend', () => {
  const m = E.normalise(
    state({
      tasks: [
        task({ timeSpentOnDay: { '2026-08-31': ms(10 * HOUR) } }),
        task({ timeSpentOnDay: { '2026-08-24': ms(10 * HOUR + 1800) } }),
      ],
    }),
  );
  const r = E.buildWorkReport(m, '2026-08-31', 'week', null, NOW);
  assert.ok(r.baseline);
  assert.strictEqual(r.findings.find((f) => f.id === 'total-vs-baseline'), undefined);
});

test('a real change is stated with its direction and percentage', () => {
  const m = E.normalise(
    state({
      tasks: [
        task({ timeSpentOnDay: { '2026-08-31': ms(10 * HOUR) } }),
        task({ timeSpentOnDay: { '2026-08-24': ms(5 * HOUR) } }),
      ],
    }),
  );
  const r = E.buildWorkReport(m, '2026-08-31', 'week', null, NOW);
  const f = r.findings.find((x) => x.id === 'total-vs-baseline');
  assert.strictEqual(f.headline, 'You worked 5h 00m more than your typical week');
  assert.match(f.detail, /That is \+100%\./);
});

// ─── findings ─────────────────────────────────────────────────────────

test('one dominating day is named by its weekday', () => {
  const m = E.normalise(
    state({
      tasks: [
        task({ timeSpentOnDay: { '2026-08-31': ms(HOUR) } }),
        task({ timeSpentOnDay: { '2026-09-01': ms(HOUR) } }),
        task({ timeSpentOnDay: { '2026-09-05': ms(8 * HOUR) } }),
      ],
    }),
  );
  const r = E.buildWorkReport(m, '2026-08-31', 'week', null, NOW);
  const f = r.findings.find((x) => x.id === 'longest-day');
  assert.strictEqual(f.headline, 'Saturday carried 80% of the week\'s work');
  assert.strictEqual(f.tone, 'watch');
});

test('an evenly spread week gets no dominating-day finding', () => {
  const m = E.normalise(
    state({
      tasks: [
        task({ timeSpentOnDay: { '2026-08-31': ms(3 * HOUR) } }),
        task({ timeSpentOnDay: { '2026-09-01': ms(3 * HOUR) } }),
        task({ timeSpentOnDay: { '2026-09-02': ms(3 * HOUR) } }),
      ],
    }),
  );
  const r = E.buildWorkReport(m, '2026-08-31', 'week', null, NOW);
  assert.strictEqual(r.findings.find((x) => x.id === 'longest-day'), undefined);
});

test('a mostly untagged period says so instead of drawing one grey bar', () => {
  const m = E.normalise(
    state({
      tags: [{ id: 'deep', title: 'Deep work' }],
      tasks: [
        task({ tagIds: ['deep'], timeSpentOnDay: { '2026-08-31': ms(HOUR) } }),
        task({ timeSpentOnDay: { '2026-08-31': ms(9 * HOUR) } }),
      ],
    }),
  );
  const r = E.buildWorkReport(m, '2026-08-31', 'day', null, NOW);
  const f = r.findings.find((x) => x.id === 'untagged');
  assert.strictEqual(f.headline, '90% of the work has no kind on it');
});

test('the report always says which of Fruit\'s readings it cannot give', () => {
  const m = E.normalise(
    state({ tasks: [task({ timeSpentOnDay: { '2026-08-31': ms(HOUR) } })] }),
  );
  const r = E.buildWorkReport(m, '2026-08-31', 'day', null, NOW);
  assert.ok(r.findings.find((x) => x.id === 'not-measured'));
});

// ─── estimated vs actual, by project and week ─────────────────────────

test('estimated against actual compares the same set of tasks', () => {
  const m = E.normalise(
    state({
      projects: [{ id: 'p1', title: 'Alpha', theme: { primary: '#111111' } }],
      tasks: [
        task({
          projectId: 'p1',
          isDone: true,
          doneOn: Date.parse('2026-09-02T12:00:00'),
          timeEstimate: ms(2 * HOUR),
          timeSpentOnDay: { '2026-09-01': ms(HOUR), '2026-09-02': ms(2 * HOUR) },
        }),
        // Not done: it has no estimate-versus-actual to report yet.
        task({ projectId: 'p1', timeEstimate: ms(HOUR), timeSpentOnDay: { '2026-09-02': ms(HOUR) } }),
      ],
    }),
  );
  const rows = E.buildProjectWeeks(m, E.parseDate('2026-08-31'), E.parseDate('2026-09-06'), null);
  assert.strictEqual(rows.length, 1);
  assert.strictEqual(rows[0].weekStart, '2026-08-31');
  assert.strictEqual(rows[0].plannedSec, 2 * HOUR);
  assert.strictEqual(rows[0].trackedSec, 3 * HOUR);
  assert.strictEqual(rows[0].tasks, 1);
});

test('rows sort by week and then by project name', () => {
  const mk = (proj, doneOn) =>
    task({
      projectId: proj,
      isDone: true,
      doneOn: Date.parse(`${doneOn}T12:00:00`),
      timeEstimate: ms(HOUR),
      timeSpentOnDay: { [doneOn]: ms(HOUR) },
    });
  const m = E.normalise(
    state({
      projects: [
        { id: 'b', title: 'Beta', theme: {} },
        { id: 'a', title: 'Alpha', theme: {} },
      ],
      tasks: [mk('b', '2026-09-01'), mk('a', '2026-09-01'), mk('b', '2026-08-25')],
    }),
  );
  const rows = E.buildProjectWeeks(m, E.parseDate('2026-08-24'), E.parseDate('2026-09-06'), null);
  assert.deepStrictEqual(
    rows.map((r) => `${r.weekStart} ${r.projectName}`),
    ['2026-08-24 Beta', '2026-08-31 Alpha', '2026-08-31 Beta'],
  );
});

test('the CSV carries Fruit\'s columns, and escapes a comma in a project name', () => {
  const rows = [
    { weekStart: '2026-08-31', projectName: 'Alpha, Ltd', plannedSec: 7200, trackedSec: 10800 },
  ];
  assert.strictEqual(
    E.reportCsv(rows),
    'week_start,project,planned_sec,tracked_sec,drift_sec\n2026-08-31,"Alpha, Ltd",7200,10800,3600\n',
  );
});

// ─── weekly targets ───────────────────────────────────────────────────

test('pace-to-date is the target prorated over the days elapsed', () => {
  const m = E.normalise(
    state({
      projects: [{ id: 'p1', title: 'Alpha', theme: {} }],
      tasks: [task({ projectId: 'p1', timeSpentOnDay: { '2026-08-31': ms(2 * HOUR) } })],
    }),
  );
  // Wednesday 2026-09-02 is the third day of the week.
  const rows = E.buildWeeklyTargets(
    m,
    { weeklyTargets: { p1: 14 * HOUR } },
    Date.parse('2026-09-02T12:00:00'),
  );
  assert.strictEqual(rows.length, 1);
  assert.strictEqual(rows[0].paceSec, Math.trunc((14 * HOUR * 3) / 7));
  assert.strictEqual(rows[0].trackedSec, 2 * HOUR);
});

test('a project with no target set is not a project behind pace', () => {
  const m = E.normalise(
    state({
      projects: [{ id: 'p1', title: 'Alpha', theme: {} }],
      tasks: [task({ projectId: 'p1', timeSpentOnDay: { '2026-08-31': ms(HOUR) } })],
    }),
  );
  assert.deepStrictEqual(E.buildWeeklyTargets(m, {}, NOW), []);
});

test('an archived project is not held to a target', () => {
  const m = E.normalise(
    state({ projects: [{ id: 'p1', title: 'Alpha', theme: {}, isArchived: true }] }),
  );
  assert.deepStrictEqual(E.buildWeeklyTargets(m, { weeklyTargets: { p1: HOUR } }, NOW), []);
});

test('only this week counts towards this week\'s target', () => {
  const m = E.normalise(
    state({
      projects: [{ id: 'p1', title: 'Alpha', theme: {} }],
      tasks: [
        task({ projectId: 'p1', timeSpentOnDay: { '2026-08-30': ms(9 * HOUR) } }), // last week
        task({ projectId: 'p1', timeSpentOnDay: { '2026-08-31': ms(HOUR) } }), // this week
      ],
    }),
  );
  const rows = E.buildWeeklyTargets(m, { weeklyTargets: { p1: 10 * HOUR } }, NOW);
  assert.strictEqual(rows[0].trackedSec, HOUR);
});
