/**
 * The renderer, in a real browser, against a stubbed bridge.
 *
 * The engine tests prove the arithmetic. This proves the thing a user actually
 * opens: that the page boots under its own Content-Security-Policy, wires
 * itself to the bridge, puts the right figures on screen, says something useful
 * when ActivityWatch is not running, and does not execute a window title.
 *
 * The page is loaded over `file://` because that is how Electron's `loadFile`
 * serves it. A CSP that works over http and blocks the app's own scripts on
 * disk is a failure that only shows up in the packaged build, which is the
 * worst place to find it.
 *
 *   node test/check-ui.mjs
 */

import { chromium } from 'playwright';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const page_url = pathToFileURL(join(here, '..', 'src', 'index.html')).href;

let failures = 0;
const ok = (name, cond, detail) => {
  if (cond) console.log(`  ok   ${name}`);
  else {
    failures++;
    console.log(`  FAIL ${name}${detail ? ` — ${detail}` : ''}`);
  }
};

const HOUR = 3600;
const DAY = '2026-08-31'; // a Monday

const at = (date, h, m) => new Date(`${date}T00:00:00`).getTime() + (h * 60 + (m || 0)) * 60000;
const ev = (ms, dur, data) => ({ timestamp: new Date(ms).toISOString(), duration: dur, data });

/**
 * A day with one of everything: labelled work, a browser split across a
 * work site and a distraction, an unknown app that should land in the
 * "no label yet" panel, and a window title that is markup.
 */
function fixture() {
  return {
    buckets: { window: 'aw-watcher-window_test', afk: 'aw-watcher-afk_test', web: ['aw-watcher-web-firefox'], hosts: ['test', 'laptop'] },
    window: [
      ev(at(DAY, 9), 2 * HOUR, { app: 'code', title: '<img src=x onerror="window.__XSS=1">' }),
      ev(at(DAY, 11), 2 * HOUR, { app: 'firefox', title: 'Browsing' }),
      ev(at(DAY, 13), 1 * HOUR, { app: 'someunknownapp', title: 'Mystery' }),
      // Never labelled by the test, so there is always an unlabelled block on
      // screen for the contrast check to sample.
      ev(at(DAY, 14), 1 * HOUR, { app: 'slack', title: '#engineering' }),
      ev(at(DAY, 20), 1 * HOUR, { app: 'code', title: 'Evening' }),
    ],
    afk: [ev(at(DAY, 9), 12 * HOUR, { status: 'not-afk' })],
    web: [
      ev(at(DAY, 11), HOUR, { url: 'https://github.com/a/b', title: 'A pull request' }),
      ev(at(DAY, 12), HOUR, { url: 'https://www.youtube.com/watch?v=1', title: 'A video' }),
    ],
  };
}

/**
 * Twenty distinct windows in one application, so every ranked list on the page
 * is longer than it will draw. The expander cannot be checked against the day
 * above, where nothing is hidden.
 */
function manyFixture() {
  const window_ = [];
  for (let i = 0; i < 20; i++) {
    window_.push(ev(at(DAY, 9) + i * 5 * 60000, 5 * 60, { app: `app${String(i).padStart(2, '0')}`, title: `Window number ${i}` }));
  }
  return {
    buckets: { window: 'aw-watcher-window_test', afk: 'aw-watcher-afk_test', web: [], hosts: ['test'] },
    window: window_,
    afk: [ev(at(DAY, 9), 3 * HOUR, { status: 'not-afk' })],
    web: [],
  };
}

const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM_PATH ?? '/opt/pw-browsers/chromium',
});
const page = await browser.newPage({ viewport: { width: 1180, height: 900 } });

const errors = [];
page.on('pageerror', (e) => errors.push(String(e)));
page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));

await page.addInitScript((data) => {
  let settings = null;
  window.__written = [];
  window.__mode = 'ok';
  window.orchard = {
    readSettings: async () => ({ ok: true, settings: settings || { serverUrl: 'http://localhost:5600', hostname: null, minSpanSec: 0, excludedApps: [], categories: null, rules: null } }),
    writeSettings: async (s) => {
      window.__written.push(s);
      settings = s;
      return { ok: true, settings: s };
    },
    info: async () => ({ ok: true, info: { version: 'test' } }),
    range: async () => {
      if (window.__mode === 'offline') {
        return { ok: false, kind: 'offline', error: 'Nothing is listening at http://localhost:5600. ActivityWatch is probably not running.' };
      }
      return { ok: true, data: window.__mode === 'many' ? data.many : data.normal };
    },
  };
  // Freeze "today" so the report is deterministic.
  const R = Date;
  const F = R.parse('2026-08-31T15:00:00');
  class Fz extends R {
    constructor(...a) {
      super(...(a.length ? a : [F]));
    }
    static now() {
      return F;
    }
  }
  window.Date = Fz;
}, { normal: fixture(), many: manyFixture() });

await page.goto(page_url, { waitUntil: 'load' });

console.log('\nBoot');
// If the CSP blocked the page's own scripts this is where it shows, and the
// message will be a CSP violation rather than a missing element.
await page.waitForSelector('.timeline', { timeout: 10000 }).catch(() => {});
ok('the page boots under its own CSP over file://', (await page.locator('.panel').count()) > 0, errors.join(' | '));
// `innerText`, never `textContent`: textContent would include script source.
const body = () => page.innerText('body');

console.log('\nDay');
// 2h code + 1h github + 1h youtube + 1h unknown + 1h slack + 1h evening = 7h.
ok('the day total is the sum of what survived the filters', (await body()).includes('7h 0m') || (await body()).includes('7h'), await body());
ok('the timeline draws one block per unbroken stretch', (await page.locator('.timeline .span').count()) === 6);
ok('a browser hour becomes the site it was showing', (await body()).includes('github.com'));
ok('the two sites land in different categories', (await body()).includes('Distraction') && (await body()).includes('Work'));
ok('the unlabelled panel ranks what has no label', (await body()).includes('someunknownapp'));
ok('the hour histogram is drawn', (await page.locator('.hours span').count()) === 24);

// A window title that is markup must land as text. If it had ever gone through
// innerHTML, the onerror handler would have fired by now.
ok('a window title is escaped, not executed', (await page.evaluate(() => window.__XSS)) === undefined);
ok('a markup window title is shown literally', (await body()).includes('<img src=x onerror='));

console.log('\nWindow titles');
// Panel headings are uppercased by CSS and `innerText` reflects that, so the
// match is case-insensitive rather than asserting about the stylesheet.
const titlesText = () =>
  page.evaluate(() => {
    const p = [...document.querySelectorAll('.panel')].find((x) =>
      /^top window titles/i.test(x.innerText),
    );
    return p ? p.innerText : null;
  });

ok('the titles panel is drawn', (await titlesText()) !== null);
const tt = (await titlesText()) || '';
ok('it lists what was on screen', tt.includes('#engineering') && tt.includes('A pull request'));
ok('each row names the application it came from', tt.includes('slack') && tt.includes('firefox'));
ok(
  'the longest title is first',
  tt.indexOf('<img src=x onerror=') < tt.indexOf('#engineering'),
  'the two-hour editor window should outrank the one-hour Slack window',
);

console.log('\nLabelling');
// Targeted by name rather than by position: two rows can tie on time, and a
// test that labels "whichever came first" starts asserting about the sort.
const row = page.locator('.unlabelled-row').filter({ hasText: 'someunknownapp' });
await row.locator('select').selectOption({ label: 'Work' });
await page.waitForFunction(() => window.__written.length > 0, null, { timeout: 10000 });
const written = await page.evaluate(() => window.__written[0]);
const rule = written.rules.find((r) => r.value === 'someunknownapp');
ok('labelling writes a rule for the subject', rule && rule.categoryId === 'work' && rule.kind === 'app', JSON.stringify(rule));
// Scoped to the panel: the app is still drawn on the timeline, and should be —
// it now has a colour. What must change is that it is no longer being asked
// about.
await page
  .waitForFunction(
    () => ![...document.querySelectorAll('.unlabelled-row')].some((r) => r.innerText.includes('someunknownapp')),
    null,
    { timeout: 10000 },
  )
  .catch(() => {});
const stillAsking = await page.evaluate(() =>
  [...document.querySelectorAll('.unlabelled-row')].some((r) => r.innerText.includes('someunknownapp')),
);
ok('the labelled thing leaves the "no label yet" list', !stillAsking);
ok('and it keeps its place on the timeline, now coloured', (await body()).includes('someunknownapp'));

console.log('\nWeek');
await page.getByRole('tab', { name: 'Week' }).click();
await page.waitForSelector('.days', { timeout: 10000 });
ok('the week draws a column per day', (await page.locator('.days .col').count()) === 7);
ok('the week totals the same seven hours', (await body()).includes('7h'));

console.log('\nOffline');
await page.evaluate(() => {
  window.__mode = 'offline';
});
await page.getByRole('button', { name: 'Today' }).click();
await page.waitForSelector('.notice', { timeout: 10000 });
ok('an unreachable server says so instead of drawing an empty chart', (await body()).includes('ActivityWatch is not answering'));
ok('the failure repeats the sentence the main process produced', (await body()).includes('probably not running'));
ok('the connection badge says offline', (await page.locator('.status[data-state="offline"]').count()) === 1);
ok('no empty chart is drawn behind the failure', (await page.locator('.hours').count()) === 0);

console.log('\nSettings');
await page.getByRole('tab', { name: 'Settings' }).click();
await page.waitForSelector('.field', { timeout: 10000 });
ok('settings still open while the server is unreachable', (await body()).includes('Server address'));
ok('the rule table lists the shipped rules', (await page.locator('tbody tr').count()) > 10);

// Turning titles off has to remove them from the timeline as well as the panel,
// or the switch is decoration.
await page.evaluate(() => {
  window.__mode = 'ok';
});
await page.getByRole('checkbox', { name: 'Show window titles' }).uncheck();
await page.getByRole('button', { name: 'Save' }).click();
await page.getByRole('tab', { name: 'Day' }).click();
await page.waitForSelector('.timeline .span', { timeout: 10000 });
ok('turning titles off removes the panel', (await titlesText()) === null);
ok('and strips them from the timeline blocks', !(await body()).includes('#engineering'));
ok('while the application totals stay', (await body()).includes('slack'));

// Back on, so the checks below run against the full page.
await page.getByRole('tab', { name: 'Settings' }).click();
await page.waitForSelector('.field', { timeout: 10000 });
await page.getByRole('checkbox', { name: 'Show window titles' }).check();
await page.getByRole('button', { name: 'Save' }).click();
await page.getByRole('tab', { name: 'Day' }).click();
await page.waitForSelector('.timeline .span', { timeout: 10000 });
ok('and turning them back on restores them', (await body()).includes('#engineering'));

console.log('\nPresentation');
// Back to a working Day view first. The checks below are about what is drawn,
// and running them on the offline screen would pass by finding nothing — which
// is how a presentation check quietly stops checking anything.
await page.evaluate(() => {
  window.__mode = 'ok';
});
await page.getByRole('tab', { name: 'Day' }).click();
await page.waitForSelector('.timeline .span', { timeout: 10000 });
// Fruit's I1, carried over: no literal colour outside the token file.
const literals = await page.evaluate(() => {
  const out = [];
  for (const sheet of document.styleSheets) {
    const href = String(sheet.href || '');
    if (href.endsWith('tokens.css')) continue;
    let rules;
    try {
      rules = sheet.cssRules;
    } catch (_) {
      continue;
    }
    for (const rule of rules) {
      if (!rule.style) continue;
      if (/#[0-9a-f]{3,8}\b|\brgba?\(/i.test(rule.cssText.replace(/var\([^)]*\)/g, ''))) out.push(rule.selectorText);
    }
  }
  return out;
});
ok('no component outside tokens.css hard-codes a colour', literals.length === 0, literals.join(', '));

// Text on a filled block has to clear 4.5:1, in whichever theme is active.
// The timeline is the one place where arbitrary user text sits on a colour the
// app picked, so it is the one place this can silently go wrong.
const contrast = await page.evaluate(() => {
  const lum = (c) => {
    const [r, g, b] = c.match(/\d+(\.\d+)?/g).slice(0, 3).map((v) => {
      const x = +v / 255;
      return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
    });
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  };
  return [...document.querySelectorAll('.timeline .span')].map((s) => {
    const st = getComputedStyle(s);
    const [a, b] = [lum(st.color), lum(st.backgroundColor)].sort((x, y) => y - x);
    return { label: s.getAttribute('data-labelled'), ratio: (a + 0.05) / (b + 0.05) };
  });
});
const worst = contrast.reduce((a, c) => (c.ratio < a.ratio ? c : a), contrast[0]);
// The count is asserted too: `every` over an empty list is true, so without
// this the check would go green the moment the selector stopped matching.
ok(
  'text on every timeline block clears 4.5:1',
  contrast.length >= 3 && contrast.every((c) => c.ratio >= 4.5),
  contrast.length
    ? `${contrast.length} blocks, worst ${worst.ratio.toFixed(2)}:1 on data-labelled=${worst.label}`
    : 'no blocks were on screen to check',
);
ok(
  'and an unlabelled block was among them',
  contrast.some((c) => c.label === 'false'),
  'the low-contrast case is the unlabelled one, so it has to be in the sample',
);

const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
ok('the window does not scroll sideways at 1180px', overflow <= 0, `${overflow}px`);

await page.setViewportSize({ width: 720, height: 700 });
const narrow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
ok('the layout holds at the minimum window width', narrow <= 1, `${narrow}px`);

console.log('\nShow more');
await page.evaluate(() => {
  window.__mode = 'many';
});
await page.getByRole('button', { name: 'Today' }).click();
await page.waitForFunction(() => document.body.innerText.includes('Window number'), null, { timeout: 10000 });

const titlePanel = () =>
  page.locator('.panel').filter({ hasText: /^Top window titles/i });
const titleRows = () => titlePanel().locator('.split-row').count();

ok('a long list draws twelve rows, not all of them', (await titleRows()) === 12);
const moreBtn = titlePanel().getByRole('button');
ok('and offers the rest by name', (await moreBtn.textContent()) === 'Show 8 more', await moreBtn.textContent());
ok('the control reports its state', (await moreBtn.getAttribute('aria-expanded')) === 'false');

await moreBtn.click();
await page.waitForFunction(
  () => {
    const p = [...document.querySelectorAll('.panel')].find((x) => /^TOP WINDOW TITLES/i.test(x.innerText));
    return p && p.querySelectorAll('.split-row').length === 20;
  },
  null,
  { timeout: 10000 },
);
ok('expanding shows every row', (await titleRows()) === 20);
ok('and the control offers the way back', (await titlePanel().getByRole('button').textContent()) === 'Show fewer');
ok('while reporting its new state', (await titlePanel().getByRole('button').getAttribute('aria-expanded')) === 'true');

await titlePanel().getByRole('button').click();
await page.waitForFunction(
  () => {
    const p = [...document.querySelectorAll('.panel')].find((x) => /^TOP WINDOW TITLES/i.test(x.innerText));
    return p && p.querySelectorAll('.split-row').length === 12;
  },
  null,
  { timeout: 10000 },
);
ok('collapsing puts it back', (await titleRows()) === 12);

// The application list and the unlabelled list had the same silent cut.
const appPanel = page.locator('.panel').filter({ hasText: /^By application/i });
ok('the application list offers its rest too', (await appPanel.getByRole('button').textContent()) === 'Show 8 more');
const unlabelled = page.locator('.panel').filter({ hasText: /^No label yet/i });
ok(
  'and so does the unlabelled list',
  (await unlabelled.getByRole('button').filter({ hasText: 'Show' }).count()) === 1,
);

// One list expanding must not expand the others: they are separate questions.
await appPanel.getByRole('button').click();
await page.waitForFunction(
  () => {
    const p = [...document.querySelectorAll('.panel')].find((x) => /^BY APPLICATION/i.test(x.innerText));
    return p && p.querySelectorAll('.split-row').length === 20;
  },
  null,
  { timeout: 10000 },
);
ok('each list expands on its own', (await titleRows()) === 12 && (await appPanel.locator('.split-row').count()) === 20);

ok('no page errors', errors.length === 0, errors.join(' | '));

await browser.close();
console.log(failures === 0 ? '\nAll UI checks passed.' : `\n${failures} UI check(s) failed.`);
process.exit(failures === 0 ? 0 : 1);
