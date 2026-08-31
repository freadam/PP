/**
 * The engine — ActivityWatch's raw event stream, turned into a day you can read.
 *
 * # Why the transforms are ported rather than delegated
 *
 * ActivityWatch ships a query language, and asking `aw-server` to do this work
 * would be less code. It would also mean the numbers on screen are produced by
 * a string this app cannot test, against a server version it cannot pin, in a
 * language whose semantics change between releases. `flood` in particular is
 * the function that decides what a total *is* — it is ported here from
 * `aw_transform/flood.py`, and `test/engine.test.js` holds it to the cases that
 * file's own comments call out.
 *
 * # The pipeline, and why it is in this order
 *
 *   raw window events
 *     -> flood            close sampler gaps, merge equal neighbours
 *     -> intersect(afk)   drop the time you were not at the machine
 *     -> attach domains   a browser window becomes the site it was showing
 *     -> min-span floor   alt-tabbing to read a message is not a context switch
 *     -> categorise       a rule you wrote turns an app into a kind of time
 *
 * Every step narrows. Nothing after the first can invent time the watcher did
 * not record, which is the property that makes the total on screen defensible.
 *
 * # No DOM, no fetch
 *
 * Pure functions over plain data. The HTTP lives in the main process, the
 * drawing lives in `render.js`, and this file is the part that can be wrong in
 * a way a test can catch.
 */

(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.Orchard = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
  'use strict';

  // --- constants ------------------------------------------------------

  /**
   * ActivityWatch's own default. Gaps up to this are sampler uncertainty, not
   * time you spent doing nothing.
   */
  const PULSETIME_SEC = 5;

  /**
   * A decluttering floor, **off by default**.
   *
   * Fruit uses two minutes: alt-tabbing to check a message is not a context
   * switch worth a row in a day's account, and over a full working day that is
   * the right call. It is the wrong *default* here, because Orchard's promise
   * is to read what ActivityWatch recorded — and a first run is a short
   * session, where a two-minute floor can discard every span there is and
   * leave the app reporting nothing for a day the ActivityWatch dashboard
   * reports as three minutes. An app that disagrees with its own source on the
   * first screen has spent the trust it needs.
   *
   * So the floor ships off and is offered in Settings, which is where a
   * judgement about what counts as noise belongs.
   */
  const DECLUTTER_MIN_SPAN_SEC = 120;

  /** Negative gaps smaller than this are rounding, and are pruned to zero. */
  const NEG_GAP_TRIM_MS = 100;

  /** ActivityWatch bucket types this app knows how to read. */
  const TYPE_WINDOW = 'currentwindow';
  const TYPE_AFK = 'afkstatus';
  const TYPE_WEB = 'web.tab.current';

  /**
   * Applications whose foreground time is really *a website's* time. Used to
   * decide which window spans a browser bucket may relabel — without the guard
   * a browser event overlapping an editor would rename the editor.
   */
  const BROWSER_APPS = [
    'chrome', 'google-chrome', 'chromium', 'firefox', 'firefox-esr', 'librewolf',
    'safari', 'msedge', 'microsoft edge', 'edge', 'brave', 'brave-browser',
    'opera', 'vivaldi', 'zen', 'arc', 'waterfox', 'thorium',
  ];

  /**
   * The five kinds of time, carried over from Fruit unchanged so a number here
   * means what the same number means there.
   *
   * `countsAs` is the roll-up: adding a category can never move a figure that
   * was already on screen, because every report reads the roll-up rather than
   * the category list.
   */
  const BUILTIN_CATEGORIES = [
    { id: 'work', name: 'Work', colour: 'var(--app-4)', countsAs: 'core', sortRank: 1 },
    { id: 'study', name: 'Study', colour: 'var(--app-1)', countsAs: 'core', sortRank: 2 },
    { id: 'distraction', name: 'Distraction', colour: 'var(--app-7)', countsAs: 'entertainment', sortRank: 3 },
    { id: 'life', name: 'Life', colour: 'var(--app-5)', countsAs: 'other', sortRank: 4 },
    { id: 'other', name: 'Other', colour: 'var(--app-2)', countsAs: 'other', sortRank: 5 },
  ];

  /**
   * A starting taxonomy, so the first run is not a blank rule table.
   *
   * Deliberately short. A hundred shipped rules look thorough and quietly
   * mislabel a third of somebody's week; these are the cases where the guess is
   * safe, and everything else arrives in the "no label yet" panel where the
   * person who was actually there decides.
   */
  const SEED_RULES = [
    ['app', 'code', 'work'], ['app', 'code - insiders', 'work'], ['app', 'cursor', 'work'],
    ['app', 'zed', 'work'], ['app', 'sublime_text', 'work'], ['app', 'intellij idea', 'work'],
    ['app', 'pycharm', 'work'], ['app', 'webstorm', 'work'], ['app', 'xcode', 'work'],
    ['app', 'terminal', 'work'], ['app', 'alacritty', 'work'], ['app', 'kitty', 'work'],
    ['app', 'wezterm', 'work'], ['app', 'gnome-terminal', 'work'], ['app', 'iterm2', 'work'],
    ['app', 'windowsterminal', 'work'],
    ['domain', 'github.com', 'work'], ['domain', 'gitlab.com', 'work'],
    ['domain', 'stackoverflow.com', 'work'], ['domain', 'localhost', 'work'],
    ['domain', 'docs.google.com', 'work'], ['domain', 'linear.app', 'work'],
    ['domain', 'wikipedia.org', 'study'], ['domain', 'arxiv.org', 'study'],
    ['domain', 'coursera.org', 'study'],
    ['domain', 'youtube.com', 'distraction'], ['domain', 'reddit.com', 'distraction'],
    ['domain', 'x.com', 'distraction'], ['domain', 'twitter.com', 'distraction'],
    ['domain', 'instagram.com', 'distraction'], ['domain', 'tiktok.com', 'distraction'],
    ['domain', 'netflix.com', 'distraction'], ['domain', 'twitch.tv', 'distraction'],
    ['app', 'spotify', 'life'], ['app', 'steam', 'distraction'],
  ].map(([kind, value, categoryId]) => ({ kind, value, categoryId, isBuiltin: true }));

  // --- time -----------------------------------------------------------

  const parseTs = (s) => (typeof s === 'number' ? s : Date.parse(s));

  function pad(n) {
    return String(n).padStart(2, '0');
  }

  /** A millisecond instant to its local calendar date. */
  function localDate(ms) {
    const d = new Date(ms);
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  }

  function todayLocal(now) {
    return localDate(now === undefined ? Date.now() : now);
  }

  /** Local midnight opening `date`, as an instant. */
  function dayStart(date) {
    const [y, m, d] = String(date).split('-').map(Number);
    return new Date(y, m - 1, d, 0, 0, 0, 0).getTime();
  }

  /**
   * Local midnight closing `date`. Built by adding a day to the *calendar*
   * date rather than 24 hours to the instant, so a day containing a clock
   * change is 23 or 25 hours long — which is what it actually was.
   */
  function dayEnd(date) {
    const [y, m, d] = String(date).split('-').map(Number);
    return new Date(y, m - 1, d + 1, 0, 0, 0, 0).getTime();
  }

  function shiftDate(date, days) {
    const [y, m, d] = String(date).split('-').map(Number);
    return localDate(new Date(y, m - 1, d + days, 12).getTime());
  }

  /** Monday, as everywhere else in Fruit. */
  function weekStart(date) {
    const [y, m, d] = String(date).split('-').map(Number);
    const dt = new Date(y, m - 1, d, 12);
    return localDate(dt.getTime() - ((dt.getDay() + 6) % 7) * 86400000);
  }

  function dateSeries(from, to) {
    const out = [];
    for (let c = from; c <= to; c = shiftDate(c, 1)) out.push(c);
    return out;
  }

  const WEEKDAYS = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];

  function weekdayName(date) {
    const [y, m, d] = String(date).split('-').map(Number);
    return WEEKDAYS[(new Date(y, m - 1, d, 12).getDay() + 6) % 7];
  }

  /**
   * Below this, a duration is written with its seconds. Above it, seconds are
   * noise and Fruit's `6h 20m` form is the legible one.
   */
  const SECONDS_BELOW = 600;

  /**
   * `6h 20m`, `45m`, `1m 15s`, `2s`.
   *
   * Fruit's `fmt.duration` rounds to the minute, which is right for a planner
   * and wrong here: ActivityWatch reports a two-second visit to Explorer as
   * `2s`, and a viewer that renders the same fact as `0m` is not reading the
   * same data. Seconds appear only under ten minutes, so a week total is still
   * `36h 6m` rather than a wall of digits.
   */
  function duration(sec) {
    const s = Math.abs(Math.round(sec));
    if (s < SECONDS_BELOW) {
      const m = Math.floor(s / 60);
      const r = s % 60;
      if (m === 0) return `${r}s`;
      return r === 0 ? `${m}m` : `${m}m ${r}s`;
    }
    const h = Math.floor(s / 3600);
    const m = Math.round((s % 3600) / 60);
    if (h === 0) return `${m}m`;
    if (m === 0) return `${h}h`;
    return `${h}h ${m}m`;
  }

  /**
   * The `{}h {:02}m` form the findings are written in — falling back to
   * `duration` below ten minutes, because a finding that reads "0h 02m of it,
   * across the day" is a sentence about two minutes written as though it were
   * about hours.
   */
  function hm(sec) {
    const s = Math.abs(Math.round(sec));
    if (s < SECONDS_BELOW) return duration(s);
    return `${Math.floor(s / 3600)}h ${pad(Math.floor((s % 3600) / 60))}m`;
  }

  /** `14:05`, for a timeline axis. */
  function clock(ms) {
    const d = new Date(ms);
    return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }

  // --- events to intervals --------------------------------------------

  /**
   * ActivityWatch events carry an ISO timestamp and a duration in *seconds*
   * (a float). Everything downstream works in milliseconds against a start and
   * an end, because a half-open interval is the only shape that clips and
   * merges without special cases.
   */
  function toIntervals(events) {
    const out = [];
    for (const e of events || []) {
      const start = parseTs(e.timestamp);
      if (!Number.isFinite(start)) continue;
      const dur = Math.max(0, (+e.duration || 0) * 1000);
      out.push({ start, end: start + dur, data: e.data || {} });
    }
    out.sort((a, b) => a.start - b.start || a.end - b.end);
    return out;
  }

  const sameData = (a, b) => JSON.stringify(a) === JSON.stringify(b);

  /**
   * Fill short gaps between events and merge nearby equal-data neighbours.
   *
   * A port of `aw_transform/flood.py`. The details are load-bearing and are not
   * obvious, so they are kept in the original's order: equal-data neighbours
   * merge across a gap and the longer one keeps its extent; differing-data
   * neighbours each take half, because the gap is an interval of uncertainty
   * and nothing says who owns more of it; and a final normalisation pass gives
   * the later event precedence, so pairwise flooding can never leave two events
   * claiming the same second.
   */
  function flood(intervals, pulsetimeSec) {
    const pulse = (pulsetimeSec === undefined ? PULSETIME_SEC : pulsetimeSec) * 1000;
    const ev = intervals.map((e) => ({ start: e.start, end: e.end, data: e.data }));
    ev.sort((a, b) => a.start - b.start || a.end - b.end);

    for (let i = 0; i + 1 < ev.length; i++) {
      const e1 = ev[i];
      const e2 = ev[i + 1];
      const gap = e2.start - e1.end;
      if (gap === 0) continue;

      if (gap < 0 && sameData(e1.data, e2.data)) {
        // A negative gap with the same data is a double-report; merging is safe.
        const start = Math.min(e1.start, e2.start);
        const end = Math.max(e1.end, e2.end);
        e1.start = start;
        e1.end = end;
        e2.start = end;
        e2.end = end;
      } else if (gap < -NEG_GAP_TRIM_MS) {
        // Overlapping events with differing data cannot be merged safely. Left
        // alone here; the normalisation pass below resolves them.
        continue;
      } else if (gap > -NEG_GAP_TRIM_MS && gap <= pulse) {
        const e2End = e2.end;
        if (sameData(e1.data, e2.data)) {
          if (e1.end - e1.start >= e2.end - e2.start) {
            e1.end = e2End;
            e2.start = e2End;
            e2.end = e2End;
          } else {
            e2.start = e1.start;
            e2.end = e2End;
            e1.end = e1.start;
          }
        } else {
          // The gap is an interval of uncertainty: without evidence that either
          // neighbour owns more of it, split it at the midpoint.
          const midpoint = e1.end + gap / 2;
          e1.end = midpoint;
          e2.start = midpoint;
          e2.end = e2End;
        }
      }
    }

    const out = [];
    for (const event of ev) {
      if (event.end <= event.start) continue;
      let placed = false;
      while (out.length && out[out.length - 1].end > event.start) {
        const prev = out[out.length - 1];
        if (sameData(prev.data, event.data)) {
          prev.end = Math.max(prev.end, event.end);
          placed = true;
          break;
        }
        prev.end = event.start;
        if (prev.end <= prev.start) {
          out.pop();
          continue;
        }
        out.push(event);
        placed = true;
        break;
      }
      if (!placed) out.push(event);
    }
    return out;
  }

  /** Merge touching intervals into the fewest periods covering the same time. */
  function union(intervals) {
    const sorted = intervals.slice().sort((a, b) => a.start - b.start);
    const out = [];
    for (const i of sorted) {
      const last = out[out.length - 1];
      if (last && i.start <= last.end) last.end = Math.max(last.end, i.end);
      else out.push({ start: i.start, end: i.end });
    }
    return out;
  }

  /**
   * The periods you were actually at the machine, from an afk bucket.
   *
   * ActivityWatch's own canonical query does exactly this — flood the afk
   * bucket, keep `status == "not-afk"` — and everything else is measured
   * against the result.
   */
  function notAfkPeriods(afkEvents) {
    return union(flood(toIntervals(afkEvents)).filter((e) => e.data && e.data.status === 'not-afk'));
  }

  /**
   * Clip `intervals` to `periods`, the way `filter_period_intersect` does. One
   * event spanning a break becomes two, which is the point: a window left open
   * while you made coffee is not two hours of work.
   */
  function intersect(intervals, periods) {
    if (!periods.length) return [];
    const out = [];
    for (const e of intervals) {
      for (const p of periods) {
        if (p.end <= e.start) continue;
        if (p.start >= e.end) break;
        const start = Math.max(e.start, p.start);
        const end = Math.min(e.end, p.end);
        if (end > start) out.push({ start, end, data: e.data });
      }
    }
    out.sort((a, b) => a.start - b.start);
    return out;
  }

  /** Clip to a single window, for a day or a report range. */
  function clip(intervals, from, to) {
    return intersect(intervals, [{ start: from, end: to }]);
  }

  // --- naming ---------------------------------------------------------

  /** `code.exe` and `Code.app` are the same thing to a human reading a total. */
  function appLabel(appId) {
    return String(appId || '').replace(/\.(exe|app)$/i, '');
  }

  const isBrowser = (appId) => BROWSER_APPS.includes(appLabel(appId).toLowerCase());

  /**
   * The host of a URL, without `www.` — nobody thinks of `www.github.com` and
   * `github.com` as two places. Nothing else is stripped: going further would
   * merge two genuinely different sites on the country-code TLDs.
   */
  function domainOf(url) {
    if (!url) return null;
    try {
      const host = new URL(url).hostname.toLowerCase();
      return host.replace(/^www\./, '') || null;
    } catch (_) {
      return null;
    }
  }

  /**
   * The eight `--app-*` tokens, by stable hash rather than order of appearance,
   * so an app keeps its colour from one day to the next. A legend you have to
   * re-learn every morning is worse than no colour at all.
   */
  function appColour(appId) {
    let hash = 0;
    const s = String(appId || '');
    for (let i = 0; i < s.length; i++) hash = (hash * 31 + s.charCodeAt(i)) | 0;
    return `var(--app-${(Math.abs(hash) % 8) + 1})`;
  }

  // --- the pipeline ---------------------------------------------------

  /**
   * Give browser spans the site they were showing.
   *
   * Only spans whose app is a known browser are eligible. Without that guard a
   * browser event overlapping an editor would rename the editor, which is the
   * bug that makes app totals stop being believable.
   *
   * A browser span covering three sites becomes three spans, so the time lands
   * on the site that had it rather than on whichever one happened to be first.
   */
  function attachDomains(spans, webEvents) {
    const web = flood(toIntervals(webEvents));
    if (!web.length) return spans.map((s) => Object.assign({ domain: null }, s));

    const out = [];
    for (const span of spans) {
      if (!isBrowser(span.data.app)) {
        out.push(Object.assign({ domain: null }, span));
        continue;
      }
      const overlaps = [];
      for (const w of web) {
        const start = Math.max(span.start, w.start);
        const end = Math.min(span.end, w.end);
        if (end > start) overlaps.push({ start, end, domain: domainOf(w.data.url), title: w.data.title });
      }
      if (!overlaps.length) {
        out.push(Object.assign({ domain: null }, span));
        continue;
      }
      // Any part of the span no browser event covers stays the browser itself,
      // rather than being attributed to the nearest site.
      let cursor = span.start;
      for (const o of overlaps) {
        if (o.start > cursor) out.push(Object.assign({}, span, { start: cursor, end: o.start, domain: null }));
        out.push(
          Object.assign({}, span, {
            start: Math.max(o.start, cursor),
            end: o.end,
            domain: o.domain,
            data: Object.assign({}, span.data, o.title ? { title: o.title } : {}),
          }),
        );
        cursor = Math.max(cursor, o.end);
      }
      if (cursor < span.end) out.push(Object.assign({}, span, { start: cursor, end: span.end, domain: null }));
    }
    return out.filter((s) => s.end > s.start).sort((a, b) => a.start - b.start);
  }

  /** The window title a span was showing, or the empty string. */
  const titleOf = (span) => String((span.data && span.data.title) || '');

  /**
   * What makes two neighbouring spans "the same thing".
   *
   * The title is part of it, and that is a correction rather than a detail.
   * Merging on app and domain alone collapses two adjacent stretches in the
   * same application into one block that is then *labelled with the first
   * title* — so a block reading "Grubby - Twitch" silently covers the minutes
   * spent on two other tabs, and the second title is gone from every total that
   * reads these spans. A block claims to be one unbroken stretch in one place;
   * a different document is a different place.
   */
  const subjectKey = (span) => `${span.data.app}\u0000${span.domain || ''}\u0000${titleOf(span)}`;

  /** Merge neighbouring spans that describe the same thing. */
  function mergeAdjacent(spans) {
    const out = [];
    for (const s of spans) {
      const prev = out[out.length - 1];
      if (prev && subjectKey(prev) === subjectKey(s) && s.start <= prev.end) {
        prev.end = Math.max(prev.end, s.end);
      } else {
        out.push(Object.assign({}, s));
      }
    }
    return out;
  }

  /**
   * Drop observation shorter than the floor, then absorb: a span separated from
   * an identical neighbour only by something that was itself dropped is one
   * span, not two.
   *
   * A port of Fruit's `apply_min_span`. This removes time, so the day view
   * reports what it dropped rather than quietly shrinking the total.
   */
  function applyMinSpan(spans, minSec) {
    if (!minSec || minSec <= 0) return spans.map((s) => Object.assign({}, s));
    const minMs = minSec * 1000;
    const kept = spans.filter((s) => s.end - s.start >= minMs);
    const out = [];
    for (const span of kept) {
      const prev = out[out.length - 1];
      if (
        prev &&
        subjectKey(prev) === subjectKey(span) &&
        span.start >= prev.end &&
        span.start - prev.end <= minMs
      ) {
        prev.end = span.end;
      } else {
        out.push(Object.assign({}, span));
      }
    }
    return out;
  }

  // --- categories -----------------------------------------------------

  /**
   * What a span should be matched on. A browser showing a site is the site —
   * otherwise every tab in the day collapses into one row called "firefox",
   * which is the least useful label available.
   */
  /**
   * `value` is the rule key and is lower-cased, because rules match without
   * regard to case. `display` is the name as the watcher reported it, so the
   * unlabelled list and the per-application split do not print `clickup` and
   * `ClickUp` for the same thing on the same screen.
   */
  function subjectOf(span) {
    return span.domain
      ? { kind: 'domain', value: span.domain, display: span.domain }
      : { kind: 'app', value: appLabel(span.data.app).toLowerCase(), display: appLabel(span.data.app) };
  }

  /**
   * First matching rule wins, and rules match exactly rather than by regex.
   *
   * Exact matching is a deliberate limit. A regex table is more powerful and is
   * also the thing nobody maintains: the panel this app is built around is
   * "these three things took eleven hours between them", answered with one
   * click, and that only works if a rule is a fact about one app rather than a
   * pattern you have to debug.
   *
   * A domain rule also covers its subdomains, so one rule for `github.com`
   * catches `gist.github.com` without a second.
   */
  function matchRule(subject, rules) {
    const value = subject.value;
    for (const r of rules || []) {
      if (r.kind !== subject.kind) continue;
      const rv = String(r.value).toLowerCase();
      if (rv === value) return r;
      if (subject.kind === 'domain' && value.endsWith(`.${rv}`)) return r;
    }
    return null;
  }

  function categorise(spans, rules) {
    return spans.map((s) => {
      const subject = subjectOf(s);
      const rule = matchRule(subject, rules);
      return Object.assign({}, s, {
        subjectKind: subject.kind,
        subjectValue: subject.value,
        subjectDisplay: subject.display,
        categoryId: rule ? rule.categoryId : null,
      });
    });
  }

  // --- buckets --------------------------------------------------------

  /**
   * Pick one bucket per role.
   *
   * A machine that has run more than one watcher version, or that holds another
   * host's synced data, has several candidates. Preferring the chosen hostname
   * and then the most recently updated is what lets the app open on *this*
   * computer's day without asking.
   */
  function chooseBuckets(buckets, hostname) {
    const all = Object.values(buckets || {});
    const pick = (type) => {
      const candidates = all.filter((b) => b.type === type);
      if (!candidates.length) return null;
      const mine = hostname ? candidates.filter((b) => b.hostname === hostname) : [];
      const pool = mine.length ? mine : candidates;
      return pool
        .slice()
        .sort((a, b) => String(b.last_updated || '').localeCompare(String(a.last_updated || '')))[0];
    };
    return {
      window: pick(TYPE_WINDOW),
      afk: pick(TYPE_AFK),
      // Every browser keeps its own bucket and all of them are read, so someone
      // who uses two browsers does not silently lose one.
      web: all.filter((b) => b.type === TYPE_WEB),
      hosts: [...new Set(all.map((b) => b.hostname).filter(Boolean))].sort(),
    };
  }

  // --- a day ----------------------------------------------------------

  const secOf = (s) => (s.end - s.start) / 1000;
  const totalSec = (spans) => spans.reduce((a, s) => a + secOf(s), 0);

  function shareOf(part, total) {
    return total <= 0 ? 0 : Math.round((part / total) * 1000) / 1000;
  }

  /**
   * One local day, end to end.
   *
   * `rawSec` is what the watcher recorded inside the day; `observedSec` is what
   * survived the afk filter and the floor. Both are carried because the
   * difference is itself a fact about the day — "forty minutes of it was in
   * stretches too short to count" is worth saying, and one total cannot say it.
   */
  function buildDay(date, sources, settings) {
    const s = withDefaults(settings);
    const from = dayStart(date);
    const to = dayEnd(date);

    const windowed = clip(flood(toIntervals(sources.window || [])), from, to);
    const rawSec = totalSec(windowed);

    const hasAfk = (sources.afk || []).length > 0;
    // No afk bucket at all is a different fact from being afk all day. With no
    // watcher running there is nothing to filter against, so the window events
    // stand — and the day records that the filter was not applied.
    const present = hasAfk ? intersect(windowed, notAfkPeriods(sources.afk)) : windowed;

    const withDomains = attachDomains(present, sources.web || []);
    const excluded = new Set((s.excludedApps || []).map((a) => String(a).toLowerCase()));
    const allowed = withDomains.filter((sp) => !excluded.has(appLabel(sp.data.app).toLowerCase()));

    const floored = applyMinSpan(mergeAdjacent(allowed), s.minSpanSec);
    const spans = categorise(floored, s.rules);

    return {
      date,
      from,
      to,
      spans,
      rawSec,
      presentSec: totalSec(present),
      observedSec: totalSec(spans),
      hasAfk,
      firstAt: spans.length ? spans[0].start : null,
      lastAt: spans.length ? spans[spans.length - 1].end : null,
      byApp: totalsBy(spans, (sp) => appLabel(sp.data.app)),
      byTitle: totalsByTitle(spans),
      bySubject: totalsBySubject(spans),
      byCategory: totalsByCategory(spans, s.categories),
      byHour: byHour(spans),
    };
  }

  function totalsBy(spans, key) {
    const totals = new Map();
    for (const s of spans) {
      const k = key(s);
      totals.set(k, (totals.get(k) || 0) + secOf(s));
    }
    const total = totalSec(spans);
    return [...totals.entries()]
      .map(([name, seconds]) => ({ name, seconds, share: shareOf(seconds, total), colour: appColour(name) }))
      .sort((a, b) => b.seconds - a.seconds || a.name.localeCompare(b.name));
  }

  /**
   * What was actually on screen, by window title, longest first.
   *
   * Keyed on **application and title**, which is what ActivityWatch's own
   * dashboard does — its desktop query is
   * `merge_events_by_keys(events, ["app", "title"])`. Keying on the title alone
   * would fold every window that reports no title into one row, and the two
   * lists would disagree on the first day that contains two such windows.
   *
   * An empty title is kept rather than dropped: a watcher that reports no title
   * still reports real time, and hiding it would stop the rows adding up.
   */
  function totalsByTitle(spans) {
    const totals = new Map();
    for (const s of spans) {
      const title = titleOf(s);
      const app = appLabel(s.data.app);
      const key = `${app}\u0000${title}`;
      const cur = totals.get(key) || { title, app, seconds: 0, occurrences: 0 };
      cur.seconds += secOf(s);
      cur.occurrences += 1;
      totals.set(key, cur);
    }
    const total = totalSec(spans);
    return [...totals.values()]
      .map((r) => ({
        title: r.title,
        app: r.app,
        colour: appColour(r.app),
        seconds: r.seconds,
        occurrences: r.occurrences,
        share: shareOf(r.seconds, total),
      }))
      .sort((a, b) => b.seconds - a.seconds || a.title.localeCompare(b.title));
  }

  function totalsBySubject(spans) {
    const totals = new Map();
    for (const s of spans) {
      const k = `${s.subjectKind} ${s.subjectValue}`;
      const cur =
        totals.get(k) ||
        {
          kind: s.subjectKind,
          value: s.subjectValue,
          display: s.subjectDisplay || s.subjectValue,
          seconds: 0,
          occurrences: 0,
          categoryId: s.categoryId,
        };
      cur.seconds += secOf(s);
      cur.occurrences += 1;
      totals.set(k, cur);
    }
    return [...totals.values()].sort((a, b) => b.seconds - a.seconds || a.value.localeCompare(b.value));
  }

  /**
   * The split by kind of time. Every category is returned, including empty
   * ones, and the unlabelled remainder is always present when it has time in
   * it — the shares have to add up, or a reader cannot find where the missing
   * hours went.
   */
  function totalsByCategory(spans, categories) {
    const cats = categories && categories.length ? categories : BUILTIN_CATEGORIES;
    const totals = new Map();
    for (const s of spans) totals.set(s.categoryId, (totals.get(s.categoryId) || 0) + secOf(s));
    const total = totalSec(spans);

    const rows = cats.map((c) => ({
      id: c.id,
      name: c.name,
      colour: c.colour,
      countsAs: c.countsAs,
      seconds: totals.get(c.id) || 0,
      share: shareOf(totals.get(c.id) || 0, total),
    }));
    const unlabelled = totals.get(null) || 0;
    if (unlabelled > 0) {
      rows.push({
        id: null,
        name: 'No label yet',
        // An absence, not a category. Giving it a hue would put it in the same
        // visual class as a real one.
        colour: 'var(--sunken)',
        countsAs: 'other',
        seconds: unlabelled,
        share: shareOf(unlabelled, total),
      });
    }
    return rows;
  }

  /**
   * Observed seconds by hour of local day, 0-23.
   *
   * Split at hour boundaries rather than filed under the hour a span started
   * in: a span running 09:40-11:20 belongs to three hours, for the minutes it
   * spent in each, or the histogram stops summing to the total beside it.
   */
  function byHour(spans) {
    const hours = new Array(24).fill(0);
    for (const s of spans) {
      let cursor = s.start;
      while (cursor < s.end) {
        const d = new Date(cursor);
        const intoHour = d.getMinutes() * 60000 + d.getSeconds() * 1000 + d.getMilliseconds();
        const sliceEnd = Math.min(cursor + (3600000 - intoHour), s.end);
        hours[d.getHours()] += (sliceEnd - cursor) / 1000;
        cursor = sliceEnd;
      }
    }
    return hours;
  }

  // --- a range --------------------------------------------------------

  /**
   * Several days, summed — and the findings only a range can support.
   *
   * Built by combining whole days rather than by re-running the pipeline over a
   * wider window, so a week total and the day totals it is made of can never be
   * arrived at two different ways.
   */
  function buildRange(days, categories) {
    const cats = categories && categories.length ? categories : BUILTIN_CATEGORIES;
    const observedSec = days.reduce((a, d) => a + d.observedSec, 0);
    const worked = days.filter((d) => d.observedSec > 0);

    const appTotals = new Map();
    const titleTotals = new Map();
    const subjectTotals = new Map();
    const catTotals = new Map();
    const hours = new Array(24).fill(0);

    for (const d of days) {
      for (const row of d.byApp) appTotals.set(row.name, (appTotals.get(row.name) || 0) + row.seconds);
      for (const row of d.byTitle) {
        const key = `${row.app}\u0000${row.title}`;
        const cur = titleTotals.get(key) || { title: row.title, app: row.app, seconds: 0, occurrences: 0 };
        cur.seconds += row.seconds;
        cur.occurrences += row.occurrences;
        titleTotals.set(key, cur);
      }
      for (const row of d.bySubject) {
        const k = `${row.kind} ${row.value}`;
        const cur =
          subjectTotals.get(k) ||
          {
            kind: row.kind,
            value: row.value,
            display: row.display || row.value,
            seconds: 0,
            occurrences: 0,
            categoryId: row.categoryId,
          };
        cur.seconds += row.seconds;
        cur.occurrences += row.occurrences;
        subjectTotals.set(k, cur);
      }
      for (const row of d.byCategory) catTotals.set(row.id, (catTotals.get(row.id) || 0) + row.seconds);
      for (let h = 0; h < 24; h++) hours[h] += d.byHour[h];
    }

    const byCategory = cats
      .map((c) => ({
        id: c.id,
        name: c.name,
        colour: c.colour,
        countsAs: c.countsAs,
        seconds: catTotals.get(c.id) || 0,
        share: shareOf(catTotals.get(c.id) || 0, observedSec),
      }))
      .concat(
        (catTotals.get(null) || 0) > 0
          ? [{
              id: null,
              name: 'No label yet',
              colour: 'var(--sunken)',
              countsAs: 'other',
              seconds: catTotals.get(null),
              share: shareOf(catTotals.get(null), observedSec),
            }]
          : [],
      );

    const rollUp = { core: 0, entertainment: 0, other: 0 };
    for (const row of byCategory) rollUp[row.countsAs] = (rollUp[row.countsAs] || 0) + row.seconds;

    const range = {
      from: days.length ? days[0].date : null,
      to: days.length ? days[days.length - 1].date : null,
      days,
      observedSec,
      rawSec: days.reduce((a, d) => a + d.rawSec, 0),
      byApp: [...appTotals.entries()]
        .map(([name, seconds]) => ({ name, seconds, share: shareOf(seconds, observedSec), colour: appColour(name) }))
        .sort((a, b) => b.seconds - a.seconds || a.name.localeCompare(b.name)),
      byTitle: [...titleTotals.values()]
        .map((r) => ({
          title: r.title,
          app: r.app,
          colour: appColour(r.app),
          seconds: r.seconds,
          occurrences: r.occurrences,
          share: shareOf(r.seconds, observedSec),
        }))
        .sort((a, b) => b.seconds - a.seconds || a.title.localeCompare(b.title)),
      unlabelled: [...subjectTotals.values()]
        .filter((r) => r.categoryId === null)
        .sort((a, b) => b.seconds - a.seconds),
      byCategory,
      rollUp,
      byHour: hours,
      dayLengthSec: worked.length ? Math.trunc(observedSec / worked.length) : 0,
    };
    range.findings = findingsFor(range);
    return range;
  }

  /**
   * The range, read back in sentences.
   *
   * Each finding names the figure it came from, so it can be checked against
   * the panel beside it rather than taken on trust. They state facts and stop:
   * this app does not tell you what to do about your own week, because it does
   * not know what your week was for.
   *
   * Nothing is emitted from a quantity that was not measured.
   */
  function findingsFor(r) {
    const out = [];
    const worked = r.days.filter((d) => d.observedSec > 0);
    if (!worked.length) return out;
    const word = r.days.length === 1 ? 'day' : r.days.length > 8 ? 'month' : 'week';

    // -- what the afk filter and the floor removed ---------------------
    //
    // The most useful thing a report can say is often about its own inputs. A
    // total that is 60% of what the watcher recorded is a different claim from
    // one that is 98%, and only this says which.
    const dropped = r.rawSec - r.observedSec;
    if (r.rawSec > 0 && dropped > 600) {
      out.push({
        id: 'filtered',
        headline: `${hm(dropped)} of recorded time is not in these totals`,
        detail:
          `The watcher recorded ${hm(r.rawSec)} of foreground time; ${hm(r.observedSec)} of it survived. ` +
          'The difference is time you were away from the keyboard, plus stretches too short to count as a context switch. Both numbers are real — this is the one you were at the machine for.',
        tone: 'neutral',
      });
    }

    // -- the shape of the day ------------------------------------------
    const spans = worked.map((d) => (d.firstAt && d.lastAt ? (d.lastAt - d.firstAt) / 1000 : 0)).filter(Boolean);
    if (spans.length) {
      const spanSec = spans.reduce((a, b) => a + b, 0);
      const gap = spanSec - r.observedSec;
      if (spanSec > 0 && gap > 1800) {
        out.push({
          id: 'span-density',
          headline: `${Math.round((r.observedSec / spanSec) * 100)}% of the time between first and last activity was observed`,
          detail: `${hm(r.observedSec)} inside a ${hm(spanSec)} span, so ${hm(gap)} went to something this app cannot see — meetings away from the machine, breaks, or a watcher that was not running.`,
          tone: 'neutral',
        });
      }
    }

    // -- when it happened ----------------------------------------------
    const totalHours = r.byHour.reduce((a, b) => a + b, 0);
    if (totalHours > 0) {
      const late = r.byHour.slice(18).reduce((a, b) => a + b, 0);
      const early = r.byHour.slice(0, 7).reduce((a, b) => a + b, 0);
      const p = (x) => Math.round((x / totalHours) * 100);
      if (p(late) >= 20) {
        out.push({
          id: 'late-work',
          headline: `${p(late)}% of it happened after 18:00`,
          detail: `${hm(late)} of it, across the ${word}.`,
          tone: 'watch',
        });
      } else if (p(early) >= 20) {
        out.push({
          id: 'early-work',
          headline: `${p(early)}% of it happened before 07:00`,
          detail: `${hm(early)} of it, across the ${word}.`,
          tone: 'neutral',
        });
      }
    }

    // -- one dominating day --------------------------------------------
    if (worked.length > 1) {
      const longest = worked.reduce((a, d) => (d.observedSec > a.observedSec ? d : a), worked[0]);
      const mean = Math.trunc(r.observedSec / worked.length);
      if (longest.observedSec > mean * 1.5 && longest.observedSec - mean > 3600) {
        out.push({
          id: 'longest-day',
          // The weekday, not the ISO date. "Sunday carried 46%" is a sentence;
          // "2026-08-16 carried 46%" is a log line.
          headline: `${weekdayName(longest.date)} carried ${Math.round((longest.observedSec / r.observedSec) * 100)}% of the ${word}`,
          detail: `${hm(longest.observedSec)} on that day against an average of ${hm(mean)} across the ${worked.length} days with anything on them.`,
          tone: 'watch',
        });
      }
    }

    // -- the taxonomy's own blind spot ---------------------------------
    const labelled = r.byCategory.filter((c) => c.id !== null).reduce((a, c) => a + c.seconds, 0);
    if (r.observedSec > 0 && labelled * 2 < r.observedSec) {
      out.push({
        id: 'unlabelled',
        headline: `${Math.round(((r.observedSec - labelled) / r.observedSec) * 100)}% of it has no label yet`,
        detail:
          'The split by kind of time cannot say anything useful until more of it carries a label. The unlabelled list on the Day tab is ranked by how much time each one took, so the three that matter are at the top.',
        tone: 'watch',
      });
    }

    // -- the one comparison this data actually supports -----------------
    if (r.rollUp.entertainment > 0 && r.rollUp.core > 0) {
      const ratio = r.rollUp.entertainment / r.rollUp.core;
      if (ratio >= 0.25) {
        out.push({
          id: 'entertainment',
          headline: `${hm(r.rollUp.entertainment)} of entertainment against ${hm(r.rollUp.core)} of work and study`,
          detail: `That is ${Math.round(ratio * 100)} minutes of the first for every 100 of the second. An observation about what was on screen, not a verdict about the ${word}.`,
          tone: 'neutral',
        });
      }
    }

    return out;
  }

  // --- settings -------------------------------------------------------

  function defaultSettings() {
    return {
      serverUrl: 'http://localhost:5600',
      hostname: null,
      // Off: see DECLUTTER_MIN_SPAN_SEC.
      minSpanSec: 0,
      excludedApps: [],
      // Applications and window titles are separate switches, as they are in
      // Fruit. A title is the document, the ticket, the video — far more
      // revealing than the fact that an editor was open — and someone who wants
      // the app totals on a shared screen should not have to accept the titles
      // with them.
      showTitles: true,
      categories: BUILTIN_CATEGORIES.slice(),
      rules: SEED_RULES.slice(),
    };
  }

  function withDefaults(settings) {
    const d = defaultSettings();
    const s = settings || {};
    return {
      serverUrl: typeof s.serverUrl === 'string' && s.serverUrl ? s.serverUrl : d.serverUrl,
      hostname: s.hostname || null,
      minSpanSec: Number.isFinite(s.minSpanSec) && s.minSpanSec >= 0 ? s.minSpanSec : d.minSpanSec,
      excludedApps: Array.isArray(s.excludedApps) ? s.excludedApps : [],
      showTitles: s.showTitles !== false,
      categories: Array.isArray(s.categories) && s.categories.length ? s.categories : d.categories,
      rules: Array.isArray(s.rules) ? s.rules : d.rules,
    };
  }

  /**
   * Set a rule, replacing any existing one for the same subject and putting it
   * first — so a rule you just made always beats a shipped one, which is what
   * "editing a built-in makes it yours" has to mean.
   */
  function setRule(rules, kind, value, categoryId) {
    const v = String(value).toLowerCase();
    const rest = (rules || []).filter((r) => !(r.kind === kind && String(r.value).toLowerCase() === v));
    if (!categoryId) return rest;
    return [{ kind, value: v, categoryId, isBuiltin: false }].concat(rest);
  }

  return {
    PULSETIME_SEC,
    DECLUTTER_MIN_SPAN_SEC,
    SECONDS_BELOW,
    TYPE_WINDOW,
    TYPE_AFK,
    TYPE_WEB,
    BUILTIN_CATEGORIES,
    SEED_RULES,
    localDate,
    todayLocal,
    dayStart,
    dayEnd,
    shiftDate,
    weekStart,
    dateSeries,
    weekdayName,
    duration,
    hm,
    clock,
    toIntervals,
    flood,
    union,
    notAfkPeriods,
    intersect,
    clip,
    appLabel,
    isBrowser,
    domainOf,
    appColour,
    attachDomains,
    mergeAdjacent,
    applyMinSpan,
    subjectOf,
    titleOf,
    matchRule,
    categorise,
    chooseBuckets,
    buildDay,
    buildRange,
    defaultSettings,
    withDefaults,
    setRule,
  };
});
