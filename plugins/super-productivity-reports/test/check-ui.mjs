/**
 * The built plugin, in a real browser, against a stubbed Plugin API.
 *
 * The engine tests prove the arithmetic. This proves the thing a user actually
 * opens: that `dist/index.html` boots with no host, wires itself to the API,
 * puts the right figures on screen, and survives a project called
 * `<img onerror=alert(1)>` — which is a real title someone can type.
 *
 *   node test/check-ui.mjs
 */

import { chromium } from 'playwright';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const built = join(here, '..', 'dist', 'index.html');

let failures = 0;
const ok = (name, cond, detail) => {
  if (cond) console.log(`  ok   ${name}`);
  else {
    failures++;
    console.log(`  FAIL ${name}${detail ? ` — ${detail}` : ''}`);
  }
};

const HOUR = 3600_000;

/**
 * A week of work: 4h Monday, 6h Tuesday, 2h Wednesday, on two projects, one of
 * them tagged. Plus five completed 1h-estimate tasks that each took 90 minutes,
 * which is exactly enough to make calibration reportable at 1.50×.
 */
function fixture(today) {
  const done = Array.from({ length: 5 }, (_, i) => ({
    id: `done-${i}`,
    title: `Finished thing ${i}`,
    timeEstimate: HOUR,
    timeSpent: 1.5 * HOUR,
    timeSpentOnDay: { '2026-08-26': 1.5 * HOUR },
    isDone: true,
    doneOn: Date.parse('2026-08-26T15:00:00Z'),
    projectId: 'p1',
    tagIds: ['deep'],
    subTaskIds: [],
    created: 0,
  }));
  return {
    tasks: [
      {
        id: 'a',
        title: 'Refactor the parser',
        timeEstimate: 2 * HOUR,
        timeSpent: 10 * HOUR,
        timeSpentOnDay: { '2026-08-31': 4 * HOUR, '2026-09-01': 6 * HOUR },
        isDone: false,
        projectId: 'p1',
        tagIds: ['deep', 'TODAY'],
        subTaskIds: [],
        created: 0,
      },
      {
        id: 'b',
        title: 'Email',
        timeEstimate: 0,
        timeSpent: 2 * HOUR,
        timeSpentOnDay: { '2026-09-02': 2 * HOUR },
        isDone: false,
        projectId: 'p2',
        tagIds: [],
        subTaskIds: [],
        created: 0,
      },
      // A parent and its subtask: the parent carries the roll-up, and counting
      // both would put 4h of phantom work on the screen.
      {
        id: 'par',
        title: 'Parent',
        timeEstimate: 0,
        timeSpent: 2 * HOUR,
        timeSpentOnDay: { '2026-09-02': 2 * HOUR },
        isDone: false,
        projectId: 'p2',
        tagIds: [],
        subTaskIds: ['b'],
        created: 0,
      },
    ],
    archivedTasks: done,
    projects: [
      { id: 'p1', title: 'Fruit', theme: { primary: '#7c4dff' }, taskIds: [], backlogTaskIds: [], noteIds: [], advancedCfg: {} },
      { id: 'p2', title: '<img src=x onerror="window.__XSS=1">', theme: { primary: '#00bcd4' }, taskIds: [], backlogTaskIds: [], noteIds: [], advancedCfg: {} },
    ],
    tags: [{ id: 'deep', title: 'Deep work', color: '#ff9800', created: 0, taskIds: [], theme: {}, advancedCfg: {} }],
  };
}

const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM_PATH ?? '/opt/pw-browsers/chromium',
});
const page = await browser.newPage({ viewport: { width: 1100, height: 900 } });

const errors = [];
page.on('pageerror', (e) => errors.push(String(e)));
page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));

// The stub goes in before the document runs, exactly as the host injects the
// real bridge into the srcdoc iframe.
await page.addInitScript((data) => {
  const store = {};
  window.__snacks = [];
  window.__downloads = [];
  window.PluginAPI = {
    Hooks: { ANY_TASK_UPDATE: 'anyTaskUpdate', TASK_COMPLETE: 'taskComplete' },
    registerHook: () => {},
    getTasks: async () => data.tasks,
    getArchivedTasks: async () => data.archivedTasks,
    getAllProjects: async () => data.projects,
    getAllTags: async () => data.tags,
    loadSyncedData: async (k) => store[k] ?? null,
    persistDataSynced: async (v, k) => {
      store[k] = v;
    },
    showSnack: (c) => window.__snacks.push(c),
    downloadFile: (name, body) => window.__downloads.push({ name, body }),
    showIndexHtmlAsView: () => {},
  };
  // Freeze "today" so the report is deterministic: Wednesday 2026-09-02.
  const Real = Date;
  const FROZEN = Real.parse('2026-09-02T12:00:00');
  class Frozen extends Real {
    constructor(...args) {
      super(...(args.length ? args : [FROZEN]));
    }
    static now() {
      return FROZEN;
    }
  }
  window.Date = Frozen;
}, fixture());

// A real navigation, not `setContent`: init scripts run on document creation,
// and `setContent` writes into a document that already exists — so the stub
// would land after the plugin had already looked for it.
await page.goto(pathToFileURL(built).href, { waitUntil: 'load' });
await page.waitForSelector('.panel', { timeout: 10000 });
await page.waitForFunction(() => !document.body.innerText.includes('Reading…'), null, { timeout: 10000 });

console.log('\nWork report');
// `innerText`, never `textContent`: textContent on <body> includes the source
// of every inlined <script>, so an assertion against it would happily match a
// number that appears in a code comment and never reaches the screen.
const body = () => page.innerText('body');

// 4h + 6h + 2h = 12h across the week; the parent roll-up must not double it.
ok('the week total is the sum of its days, with no parent double-count', (await body()).includes('12h'));
ok('the day graph draws one column per day of the week', (await page.locator('.day-col').count()) === 7);
ok('the by-project split names both projects', (await body()).includes('Fruit'));
ok('the untagged remainder is shown, not hidden', (await body()).includes('Untagged'));
ok(
  'the score prints its rating alongside the number',
  /score/i.test(await body()) && /(Low|Fair|Good|Very good|Not scored)/.test(await body()),
);
ok(
  'the report says which of Fruit\'s readings it cannot give',
  (await body()).includes('not on this screen'),
);

// A project title that is markup must land as text. If it were ever written
// through innerHTML, the onerror handler would have fired by now.
ok('a project title is escaped, not executed', (await page.evaluate(() => window.__XSS)) === undefined);
ok(
  'a markup project title is displayed literally',
  (await body()).includes('<img src=x onerror='),
);

console.log('\nEstimates');
await page.getByRole('tab', { name: 'Estimates' }).click();
await page.waitForSelector('.bucket-row', { timeout: 5000 });
ok('calibration reports at five samples', (await body()).includes('You run 1.50× your estimates'));
ok('the 1h bucket is the one named as least accurate', (await body()).includes('Your 1h estimates are the least accurate'));
ok('buckets below five samples are marked unreportable', (await page.locator('.bucket-row[data-reportable="false"]').count()) === 4);

// Calibration is a trailing 30-day window and does not move with the period;
// the estimate-versus-actual rows do. The five completed tasks belong to the
// week before the one on screen, so stepping back is what should reveal them.
ok(
  'a period with nothing completed in it says so rather than drawing an empty chart',
  (await body()).includes('No task with an estimate has been completed'),
);
await page.getByRole('button', { name: 'Previous period' }).click();
await page.waitForFunction(
  () => !document.body.innerText.includes('No task with an estimate has been completed'),
  null,
  { timeout: 10000 },
);
ok(
  'the previous week shows the five completed tasks as estimated against actual',
  (await body()).includes('5h → 7h 30m'),
);

await page.getByRole('button', { name: 'Export CSV' }).click();
const dl = await page.evaluate(() => window.__downloads);
ok(
  "the CSV export carries Fruit's header row and the row behind the bar",
  dl.length === 1 &&
    dl[0].body.startsWith('week_start,project,planned_sec,tracked_sec,drift_sec') &&
    dl[0].body.includes('2026-08-24,Fruit,18000,27000,9000'),
  dl.length ? JSON.stringify(dl[0].body) : 'nothing downloaded',
);

console.log('\nTargets and settings');
await page.getByRole('tab', { name: 'Targets' }).click();
ok('no target set says so rather than showing an empty chart', (await body()).includes('No project has a weekly target yet'));

await page.getByRole('tab', { name: 'Settings' }).click();
await page.waitForSelector('.days-mask', { timeout: 5000 });
const hours = page.locator('input[type="number"]').first();
await hours.fill('6');
await page.getByRole('button', { name: 'Save settings' }).click();
await page.waitForFunction(() => window.__snacks.length > 0, { timeout: 5000 });
ok('settings save through persistDataSynced', (await page.evaluate(() => window.__snacks))[0].type === 'SUCCESS');

await page.getByRole('tab', { name: 'Work report' }).click();
await page.waitForSelector('.day-graph', { timeout: 5000 });
ok('the saved daily target reaches the report', (await body()).includes('6h daily target'));
ok('the target line is drawn only across the days it applies to', (await page.locator('.target-line').count()) === 1);

console.log('\nPresentation');
// Fruit's I1: no literal colour in a component. Every colour on screen must
// come from a theme variable or from user data (a project or tag colour).
const literals = await page.evaluate(() => {
  const out = [];
  for (const sheet of document.styleSheets) {
    let rules;
    try {
      rules = sheet.cssRules;
    } catch (_) {
      continue;
    }
    for (const rule of rules) {
      if (!rule.style) continue;
      const text = rule.cssText;
      // Fallbacks inside var() are the documented way to survive a host that
      // has not injected a variable, so they are allowed.
      const stripped = text.replace(/var\([^)]*\)/g, '');
      if (/#[0-9a-f]{3,8}\b/i.test(stripped)) out.push(rule.selectorText);
    }
  }
  return out;
});
ok('no component hard-codes a colour outside a var() fallback', literals.length === 0, literals.join(', '));

const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
ok('the page does not scroll sideways at 1100px', overflow <= 0, `${overflow}px of overflow`);

await page.setViewportSize({ width: 420, height: 900 });
const narrow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
ok('the layout holds at 420px', narrow <= 1, `${narrow}px of overflow`);

ok('no page errors', errors.length === 0, errors.join(' | '));

await browser.close();
console.log(failures === 0 ? '\nAll UI checks passed.' : `\n${failures} UI check(s) failed.`);
process.exit(failures === 0 ? 0 : 1);
