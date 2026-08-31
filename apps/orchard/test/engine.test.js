/**
 * The engine, held to ActivityWatch's own semantics.
 *
 * Where a test asserts a literal, that literal comes from the behaviour
 * `aw_transform/flood.py` documents in its own comments, or from Fruit's Rust
 * — not from a run of this code. A port that agrees with itself proves nothing.
 */

const test = require('node:test');
const assert = require('node:assert');
const E = require('../src/engine.js');

const MIN = 60;
const HOUR = 3600;

/** An ActivityWatch event, as the REST API returns one: ISO stamp, seconds. */
function ev(startIso, durationSec, data) {
  return { timestamp: startIso, duration: durationSec, data };
}

/** Local-time helper, so tests read as clock times rather than as instants. */
function at(date, h, m) {
  return new Date(`${date}T00:00:00`).getTime() + (h * 60 + (m || 0)) * 60000;
}

function iso(ms) {
  return new Date(ms).toISOString();
}

const DAY = '2026-08-31'; // a Monday

// --- flood ------------------------------------------------------------

test('flood closes a gap shorter than the pulsetime', () => {
  // Two events 3s apart, same data: the pair becomes one, and no time is
  // invented beyond the gap that was already there.
  const out = E.flood(
    E.toIntervals([
      ev('2026-08-31T09:00:00Z', 60, { app: 'code' }),
      ev('2026-08-31T09:01:03Z', 60, { app: 'code' }),
    ]),
  );
  assert.strictEqual(out.length, 1);
  assert.strictEqual((out[0].end - out[0].start) / 1000, 123);
});

test('flood leaves a gap longer than the pulsetime alone', () => {
  const out = E.flood(
    E.toIntervals([
      ev('2026-08-31T09:00:00Z', 60, { app: 'code' }),
      ev('2026-08-31T09:05:00Z', 60, { app: 'code' }),
    ]),
  );
  assert.strictEqual(out.length, 2);
});

test('a gap between differing neighbours is split at the midpoint', () => {
  // flood.py: "the gap is an interval of uncertainty: without evidence that
  // either neighbour owns more of it, split it at the midpoint."
  const out = E.flood(
    E.toIntervals([
      ev('2026-08-31T09:00:00Z', 60, { app: 'code' }),
      ev('2026-08-31T09:01:04Z', 60, { app: 'firefox' }),
    ]),
  );
  assert.strictEqual(out.length, 2);
  assert.strictEqual((out[0].end - out[0].start) / 1000, 62);
  assert.strictEqual((out[1].end - out[1].start) / 1000, 62);
  // The pair is contiguous: the uncertainty is allocated, not duplicated.
  assert.strictEqual(out[0].end, out[1].start);
});

test('overlapping equal-data events merge rather than double-count', () => {
  const out = E.flood(
    E.toIntervals([
      ev('2026-08-31T09:00:00Z', 120, { app: 'code' }),
      ev('2026-08-31T09:01:00Z', 120, { app: 'code' }),
    ]),
  );
  assert.strictEqual(out.length, 1);
  assert.strictEqual((out[0].end - out[0].start) / 1000, 180);
});

test('overlapping differing-data events give the later one precedence', () => {
  // The normalisation pass: the result never claims the same second twice.
  const out = E.flood(
    E.toIntervals([
      ev('2026-08-31T09:00:00Z', 600, { app: 'code' }),
      ev('2026-08-31T09:05:00Z', 300, { app: 'firefox' }),
    ]),
  );
  const total = out.reduce((a, e) => a + (e.end - e.start), 0) / 1000;
  assert.strictEqual(total, 600);
  for (let i = 1; i < out.length; i++) assert.ok(out[i].start >= out[i - 1].end, 'no overlap survives');
});

test('flood drops zero-length events', () => {
  const out = E.flood(E.toIntervals([ev('2026-08-31T09:00:00Z', 0, { app: 'code' })]));
  assert.deepStrictEqual(out, []);
});

// --- afk --------------------------------------------------------------

test('a window left open while you were away does not count', () => {
  const spans = E.intersect(
    E.flood(E.toIntervals([ev('2026-08-31T09:00:00Z', 3600, { app: 'code' })])),
    E.notAfkPeriods([
      ev('2026-08-31T09:00:00Z', 900, { status: 'not-afk' }),
      ev('2026-08-31T09:15:00Z', 1800, { status: 'afk' }),
      ev('2026-08-31T09:45:00Z', 900, { status: 'not-afk' }),
    ]),
  );
  // One hour in front of the editor, half of it away from the keyboard.
  assert.strictEqual(spans.reduce((a, s) => a + (s.end - s.start), 0) / 1000, 1800);
  // And it is two stretches, not one: the break is a real boundary.
  assert.strictEqual(spans.length, 2);
});

test('no afk bucket at all is different from being afk all day', () => {
  const day = E.buildDay(
    DAY,
    { window: [ev(iso(at(DAY, 9)), 3600, { app: 'code' })], afk: [], web: [] },
    { minSpanSec: 0 },
  );
  assert.strictEqual(day.hasAfk, false);
  assert.strictEqual(day.observedSec, 3600);
});

// --- domains ----------------------------------------------------------

test('a browser span becomes the sites it was showing, split between them', () => {
  const day = E.buildDay(
    DAY,
    {
      window: [ev(iso(at(DAY, 10)), 3600, { app: 'firefox', title: 'Browsing' })],
      afk: [ev(iso(at(DAY, 10)), 3600, { status: 'not-afk' })],
      web: [
        ev(iso(at(DAY, 10)), 1800, { url: 'https://github.com/x/y', title: 'A pull request' }),
        ev(iso(at(DAY, 10, 30)), 1800, { url: 'https://www.youtube.com/watch?v=1', title: 'A video' }),
      ],
    },
    { minSpanSec: 0 },
  );
  const subjects = day.bySubject.map((s) => `${s.kind}:${s.value}`);
  assert.deepStrictEqual(subjects.sort(), ['domain:github.com', 'domain:youtube.com']);
  // And they land in different categories, which is the entire point.
  const named = Object.fromEntries(day.byCategory.map((c) => [c.name, c.seconds]));
  assert.strictEqual(named.Work, 1800);
  assert.strictEqual(named.Distraction, 1800);
});

test('a browser event overlapping a non-browser window does not rename it', () => {
  // Without the guard, a stale browser event would turn an editor into a site.
  const day = E.buildDay(
    DAY,
    {
      window: [ev(iso(at(DAY, 10)), 3600, { app: 'code' })],
      afk: [ev(iso(at(DAY, 10)), 3600, { status: 'not-afk' })],
      web: [ev(iso(at(DAY, 10)), 3600, { url: 'https://youtube.com/watch', title: 'A video' })],
    },
    { minSpanSec: 0 },
  );
  assert.deepStrictEqual(day.bySubject.map((s) => s.value), ['code']);
});

test('browser time no site covers stays the browser rather than the nearest site', () => {
  const day = E.buildDay(
    DAY,
    {
      window: [ev(iso(at(DAY, 10)), 3600, { app: 'firefox' })],
      afk: [ev(iso(at(DAY, 10)), 3600, { status: 'not-afk' })],
      web: [ev(iso(at(DAY, 10)), 1800, { url: 'https://github.com/x' })],
    },
    { minSpanSec: 0 },
  );
  const byValue = Object.fromEntries(day.bySubject.map((s) => [s.value, s.seconds]));
  assert.strictEqual(byValue['github.com'], 1800);
  assert.strictEqual(byValue.firefox, 1800);
});

test('www is dropped from a domain but nothing else is', () => {
  assert.strictEqual(E.domainOf('https://www.github.com/a/b'), 'github.com');
  assert.strictEqual(E.domainOf('https://gist.github.com/a'), 'gist.github.com');
  assert.strictEqual(E.domainOf('not a url'), null);
  assert.strictEqual(E.domainOf(''), null);
});

// --- the floor --------------------------------------------------------

test('a stretch shorter than the floor is not a context switch', () => {
  const spans = [
    { start: 0, end: 10 * MIN * 1000, data: { app: 'code' }, domain: null },
    { start: 10 * MIN * 1000, end: 10 * MIN * 1000 + 30000, data: { app: 'slack' }, domain: null },
    { start: 10 * MIN * 1000 + 30000, end: 20 * MIN * 1000, data: { app: 'code' }, domain: null },
  ];
  const out = E.applyMinSpan(spans, 120);
  // The 30-second glance is dropped, and the two halves of the editor session
  // it interrupted become one span again rather than staying split.
  assert.strictEqual(out.length, 1);
  assert.strictEqual((out[0].end - out[0].start) / 1000, 20 * MIN);
});

test('a floor of zero keeps everything', () => {
  const spans = [{ start: 0, end: 1000, data: { app: 'code' }, domain: null }];
  assert.strictEqual(E.applyMinSpan(spans, 0).length, 1);
});

test('the day reports what the filters removed rather than only the total', () => {
  const day = E.buildDay(
    DAY,
    {
      window: [
        ev(iso(at(DAY, 9)), 3600, { app: 'code' }),
        ev(iso(at(DAY, 10)), 30, { app: 'slack' }),
      ],
      afk: [ev(iso(at(DAY, 9)), 3600, { status: 'not-afk' })],
      web: [],
    },
    { minSpanSec: 120 },
  );
  assert.strictEqual(day.rawSec, 3630);
  assert.strictEqual(day.observedSec, 3600);
});

// --- categorisation ---------------------------------------------------

test('a domain rule covers its subdomains', () => {
  const rules = [{ kind: 'domain', value: 'github.com', categoryId: 'work' }];
  assert.strictEqual(E.matchRule({ kind: 'domain', value: 'gist.github.com' }, rules).categoryId, 'work');
  assert.strictEqual(E.matchRule({ kind: 'domain', value: 'github.com' }, rules).categoryId, 'work');
  // But not a different registrable name that merely ends in the same letters.
  assert.strictEqual(E.matchRule({ kind: 'domain', value: 'notgithub.com' }, rules), null);
});

test('an app rule does not match a domain of the same name, or the reverse', () => {
  const rules = [{ kind: 'app', value: 'slack', categoryId: 'work' }];
  assert.strictEqual(E.matchRule({ kind: 'domain', value: 'slack' }, rules), null);
});

test('a rule you set beats a shipped one and replaces it', () => {
  const before = E.SEED_RULES.filter((r) => r.kind === 'domain' && r.value === 'youtube.com');
  assert.strictEqual(before[0].categoryId, 'distraction');
  const after = E.setRule(E.SEED_RULES, 'domain', 'YouTube.com', 'study');
  const hits = after.filter((r) => r.kind === 'domain' && r.value === 'youtube.com');
  assert.strictEqual(hits.length, 1, 'the old rule is replaced, not shadowed');
  assert.strictEqual(E.matchRule({ kind: 'domain', value: 'youtube.com' }, after).categoryId, 'study');
});

test('labelling with no category removes the rule', () => {
  const after = E.setRule(E.SEED_RULES, 'domain', 'youtube.com', null);
  assert.strictEqual(E.matchRule({ kind: 'domain', value: 'youtube.com' }, after), null);
});

test('the splits partition the total and always keep the unlabelled remainder', () => {
  const day = E.buildDay(
    DAY,
    {
      window: [
        ev(iso(at(DAY, 9)), 3600, { app: 'code' }),
        ev(iso(at(DAY, 10)), 3600, { app: 'someunknownapp' }),
      ],
      afk: [ev(iso(at(DAY, 9)), 7200, { status: 'not-afk' })],
      web: [],
    },
    {},
  );
  assert.strictEqual(day.byCategory.reduce((a, c) => a + c.seconds, 0), day.observedSec);
  const unlabelled = day.byCategory.find((c) => c.id === null);
  assert.strictEqual(unlabelled.seconds, 3600);
  assert.strictEqual(unlabelled.share, 0.5);
});

test('an excluded app is gone before anything is totalled', () => {
  const day = E.buildDay(
    DAY,
    {
      window: [
        ev(iso(at(DAY, 9)), 3600, { app: 'code' }),
        ev(iso(at(DAY, 10)), 3600, { app: '1password' }),
      ],
      afk: [ev(iso(at(DAY, 9)), 7200, { status: 'not-afk' })],
      web: [],
    },
    { excludedApps: ['1Password'] },
  );
  assert.strictEqual(day.observedSec, 3600);
  assert.ok(!day.byApp.some((a) => a.name === '1password'));
  assert.ok(!day.spans.some((s) => s.data.app === '1password'));
});

// --- days and ranges --------------------------------------------------

test('a day is bounded by local midnight, and an event outside it does not count', () => {
  const day = E.buildDay(
    DAY,
    {
      window: [
        ev(iso(at(DAY, 23, 30)), 3600, { app: 'code' }), // crosses midnight
        ev(iso(at(DAY, -2)), 3600, { app: 'code' }), // yesterday evening
      ],
      afk: [],
      web: [],
    },
    { minSpanSec: 0 },
  );
  // Half of the midnight-crossing hour belongs to this day; yesterday's does not.
  assert.strictEqual(day.observedSec, 1800);
});

test('the hour histogram splits a span across the hours it spent time in', () => {
  const day = E.buildDay(
    DAY,
    { window: [ev(iso(at(DAY, 9, 40)), 100 * MIN, { app: 'code' })], afk: [], web: [] },
    { minSpanSec: 0 },
  );
  assert.strictEqual(day.byHour[9], 20 * MIN);
  assert.strictEqual(day.byHour[10], 60 * MIN);
  assert.strictEqual(day.byHour[11], 20 * MIN);
  assert.strictEqual(day.byHour.reduce((a, b) => a + b, 0), day.observedSec);
});

test('a range sums its days, and the days are the ones asked for', () => {
  const sources = {
    window: [ev(iso(at(DAY, 9)), 2 * HOUR, { app: 'code' }), ev(iso(at('2026-09-02', 9)), HOUR, { app: 'code' })],
    afk: [],
    web: [],
  };
  const days = E.dateSeries(DAY, '2026-09-06').map((d) => E.buildDay(d, sources, { minSpanSec: 0 }));
  const range = E.buildRange(days, E.BUILTIN_CATEGORIES);
  assert.strictEqual(days.length, 7);
  assert.strictEqual(range.observedSec, 3 * HOUR);
  assert.strictEqual(range.byHour.reduce((a, b) => a + b, 0), 3 * HOUR);
  assert.strictEqual(range.dayLengthSec, Math.trunc((3 * HOUR) / 2));
});

test('an empty range produces no findings rather than findings about zero', () => {
  const days = E.dateSeries(DAY, '2026-09-06').map((d) => E.buildDay(d, { window: [], afk: [], web: [] }, {}));
  const range = E.buildRange(days, E.BUILTIN_CATEGORIES);
  assert.strictEqual(range.observedSec, 0);
  assert.deepStrictEqual(range.findings, []);
});

test('the roll-up moves when a category does, and shares still add up', () => {
  const sources = {
    window: [ev(iso(at(DAY, 9)), 2 * HOUR, { app: 'code' }), ev(iso(at(DAY, 11)), HOUR, { app: 'steam' })],
    afk: [],
    web: [],
  };
  const day = E.buildDay(DAY, sources, { minSpanSec: 0 });
  const range = E.buildRange([day], E.BUILTIN_CATEGORIES);
  assert.strictEqual(range.rollUp.core, 2 * HOUR);
  assert.strictEqual(range.rollUp.entertainment, HOUR);
  assert.strictEqual(range.rollUp.core + range.rollUp.entertainment + range.rollUp.other, range.observedSec);
});

// --- findings ---------------------------------------------------------

test('one dominating day is named by its weekday', () => {
  const sources = {
    window: [
      ev(iso(at(DAY, 9)), HOUR, { app: 'code' }),
      ev(iso(at('2026-09-01', 9)), HOUR, { app: 'code' }),
      ev(iso(at('2026-09-05', 9)), 8 * HOUR, { app: 'code' }),
    ],
    afk: [],
    web: [],
  };
  const days = E.dateSeries(DAY, '2026-09-06').map((d) => E.buildDay(d, sources, { minSpanSec: 0 }));
  const f = E.buildRange(days, E.BUILTIN_CATEGORIES).findings.find((x) => x.id === 'longest-day');
  assert.strictEqual(f.headline, 'Saturday carried 80% of the week');
});

test('a mostly unlabelled range says so rather than drawing one grey bar', () => {
  const sources = {
    window: [ev(iso(at(DAY, 9)), 9 * HOUR, { app: 'someunknownapp' }), ev(iso(at(DAY, 20)), HOUR, { app: 'code' })],
    afk: [],
    web: [],
  };
  const range = E.buildRange([E.buildDay(DAY, sources, { minSpanSec: 0 })], E.BUILTIN_CATEGORIES);
  const f = range.findings.find((x) => x.id === 'unlabelled');
  assert.strictEqual(f.headline, '90% of it has no label yet');
});

test('the filtered-out time is reported when there is enough of it to matter', () => {
  const sources = {
    window: [ev(iso(at(DAY, 9)), 4 * HOUR, { app: 'code' })],
    afk: [
      ev(iso(at(DAY, 9)), HOUR, { status: 'not-afk' }),
      ev(iso(at(DAY, 10)), 2 * HOUR, { status: 'afk' }),
      ev(iso(at(DAY, 12)), HOUR, { status: 'not-afk' }),
    ],
    web: [],
  };
  const range = E.buildRange([E.buildDay(DAY, sources, { minSpanSec: 0 })], E.BUILTIN_CATEGORIES);
  const f = range.findings.find((x) => x.id === 'filtered');
  assert.strictEqual(f.headline, '2h 00m of recorded time is not in these totals');
});

test('late work is reported, and early work only when late work is not', () => {
  const late = E.buildRange(
    [E.buildDay(DAY, { window: [ev(iso(at(DAY, 20)), 3 * HOUR, { app: 'code' })], afk: [], web: [] }, { minSpanSec: 0 })],
    E.BUILTIN_CATEGORIES,
  );
  assert.strictEqual(late.findings.find((f) => f.id === 'late-work').headline, '100% of it happened after 18:00');
  assert.strictEqual(late.findings.find((f) => f.id === 'early-work'), undefined);

  const early = E.buildRange(
    [E.buildDay(DAY, { window: [ev(iso(at(DAY, 5)), 90 * MIN, { app: 'code' })], afk: [], web: [] }, { minSpanSec: 0 })],
    E.BUILTIN_CATEGORIES,
  );
  assert.strictEqual(early.findings.find((f) => f.id === 'early-work').headline, '100% of it happened before 07:00');
});

// --- buckets ----------------------------------------------------------

test('the bucket for this host is preferred over another machine\'s', () => {
  const chosen = E.chooseBuckets(
    {
      a: { id: 'aw-watcher-window_other', type: 'currentwindow', hostname: 'other', last_updated: '2026-09-01' },
      b: { id: 'aw-watcher-window_mine', type: 'currentwindow', hostname: 'mine', last_updated: '2026-08-01' },
      c: { id: 'aw-watcher-afk_mine', type: 'afkstatus', hostname: 'mine', last_updated: '2026-08-01' },
      d: { id: 'aw-watcher-web-firefox', type: 'web.tab.current', hostname: 'mine', last_updated: '2026-08-01' },
      e: { id: 'aw-watcher-web-chrome', type: 'web.tab.current', hostname: 'mine', last_updated: '2026-08-01' },
    },
    'mine',
  );
  assert.strictEqual(chosen.window.id, 'aw-watcher-window_mine');
  assert.strictEqual(chosen.afk.id, 'aw-watcher-afk_mine');
  // Every browser bucket is read: two browsers must not become one.
  assert.strictEqual(chosen.web.length, 2);
  assert.deepStrictEqual(chosen.hosts, ['mine', 'other']);
});

test('with no hostname chosen, the most recently updated bucket wins', () => {
  const chosen = E.chooseBuckets(
    {
      a: { id: 'old', type: 'currentwindow', hostname: 'x', last_updated: '2026-01-01' },
      b: { id: 'new', type: 'currentwindow', hostname: 'y', last_updated: '2026-09-01' },
    },
    null,
  );
  assert.strictEqual(chosen.window.id, 'new');
});

test('no window bucket at all is null rather than a guess', () => {
  const chosen = E.chooseBuckets({ a: { id: 'x', type: 'afkstatus', hostname: 'h' } }, null);
  assert.strictEqual(chosen.window, null);
});

// --- formatting -------------------------------------------------------

test('durations read the way Fruit writes them, and keep seconds where they matter', () => {
  assert.strictEqual(E.duration(45 * MIN), '45m');
  assert.strictEqual(E.duration(2 * HOUR), '2h');
  assert.strictEqual(E.duration(2 * HOUR + 20 * MIN), '2h 20m');
  assert.strictEqual(E.hm(2 * HOUR + 5 * MIN), '2h 05m');
  // A finding about two minutes should not be written as though it were about
  // hours: "0h 02m of it, across the day" is not a sentence anyone says.
  assert.strictEqual(E.hm(142), '2m 22s');
  // ActivityWatch reports a two-second visit as "2s". Rendering that as "0m"
  // would be this app failing to read its own source.
  assert.strictEqual(E.duration(2), '2s');
  assert.strictEqual(E.duration(1), '1s');
  assert.strictEqual(E.duration(75), '1m 15s');
  assert.strictEqual(E.duration(142), '2m 22s');
  // Seconds stop once they are noise rather than information.
  assert.strictEqual(E.duration(600), '10m');
});

// --- parity with the ActivityWatch dashboard --------------------------

/**
 * A real short session, transcribed from an ActivityWatch dashboard reading
 * `Time active: 2m 22s` with chrome 1m 15s, electron 1m 4s, explorer 2s and
 * ClickUp 1s.
 *
 * Every span in it is under two minutes. With a two-minute floor on by default
 * this whole day rounded to nothing, and the app reported 0m for a day its own
 * source reported as 2m 22s — which is the first thing a new user sees, since a
 * first run is a short session by definition.
 */
test('a short session matches what the ActivityWatch dashboard reports', () => {
  const t = (h, m, sec) => new Date(`${DAY}T00:00:00`).getTime() + ((h * 60 + m) * 60 + (sec || 0)) * 1000;
  const day = E.buildDay(
    DAY,
    {
      window: [
        ev(t(23, 10, 0), 75, { app: 'chrome.exe', title: 'Grubby - Twitch - Google Chrome' }),
        ev(t(23, 11, 15), 64, { app: 'electron.exe', title: 'Orchard' }),
        ev(t(23, 12, 19), 2, { app: 'explorer.exe', title: '' }),
        ev(t(23, 12, 21), 1, { app: 'ClickUp.exe', title: '' }),
      ],
      afk: [ev(t(23, 10, 0), 142, { status: 'not-afk' })],
      web: [],
    },
    E.defaultSettings(),
  );

  assert.strictEqual(day.observedSec, 142, 'the total is the dashboard total');
  assert.strictEqual(E.duration(day.observedSec), '2m 22s');
  assert.deepStrictEqual(
    day.byApp.map((a) => `${a.name} ${E.duration(a.seconds)}`),
    ['chrome 1m 15s', 'electron 1m 4s', 'explorer 2s', 'ClickUp 1s'],
  );
  // And the parts add to the whole, which is the property the rounding broke.
  assert.strictEqual(day.byApp.reduce((a, x) => a + x.seconds, 0), day.observedSec);

  // One screen, one spelling: the rule key is lower-cased but the name shown
  // is the one the watcher reported.
  const clickup = day.bySubject.find((x) => x.value === 'clickup');
  assert.strictEqual(clickup.display, 'ClickUp');
});

test('the decluttering floor is off by default and still works when set', () => {
  assert.strictEqual(E.defaultSettings().minSpanSec, 0);
  const spans = [
    { start: 0, end: 30000, data: { app: 'slack' }, domain: null },
    { start: 30000, end: 30000 + 10 * MIN * 1000, data: { app: 'code' }, domain: null },
  ];
  assert.strictEqual(E.applyMinSpan(spans, 0).length, 2, 'off: nothing is dropped');
  assert.strictEqual(E.applyMinSpan(spans, E.DECLUTTER_MIN_SPAN_SEC).length, 1, 'on: the glance goes');
});

test('an application keeps its colour between runs', () => {
  assert.strictEqual(E.appColour('code'), E.appColour('code'));
  assert.match(E.appColour('code'), /^var\(--app-[1-8]\)$/);
  assert.strictEqual(E.appLabel('Code.exe'), 'Code');
  assert.strictEqual(E.appLabel('Safari.app'), 'Safari');
});

test('the week starts on Monday', () => {
  assert.strictEqual(E.weekStart('2026-08-31'), '2026-08-31');
  assert.strictEqual(E.weekStart('2026-09-06'), '2026-08-31');
  assert.strictEqual(E.weekStart('2026-08-30'), '2026-08-24');
});

test('settings survive nonsense without taking it', () => {
  const s = E.withDefaults({ serverUrl: '', minSpanSec: -5, rules: 'not an array' });
  assert.strictEqual(s.serverUrl, 'http://localhost:5600');
  assert.strictEqual(s.minSpanSec, 0);
  assert.ok(Array.isArray(s.rules));
  // A floor the user did choose survives untouched.
  assert.strictEqual(E.withDefaults({ minSpanSec: 120 }).minSpanSec, 120);
});
