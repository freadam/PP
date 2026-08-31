/**
 * The controller: fetch a range once, build every panel from that one read.
 *
 * # One read, every panel
 *
 * A screen showing a day total and a per-app split that were fetched
 * separately is a screen whose two halves can disagree about which day it is.
 * Everything here comes out of one `orchard.range()` call and one
 * `engine.buildDay` per date, so the panels are views of a single snapshot.
 *
 * # Saying whether it is live
 *
 * This app reads another process's data. If `aw-server` is not running there is
 * nothing to show, and the one thing it must never do is show an empty chart
 * that implies a quiet week. Every failure gets the sentence that says what
 * happened and what to do about it.
 */

(function () {
  'use strict';

  const E = globalThis.Orchard;
  const UI = globalThis.OrchardUI;
  const el = UI.el;

  const TABS = [
    ['day', 'Day'],
    ['week', 'Week'],
    ['month', 'Month'],
    ['settings', 'Settings'],
  ];

  const state = {
    tab: 'day',
    date: E.todayLocal(),
    settings: E.defaultSettings(),
    range: null,
    days: [],
    buckets: null,
    connection: 'unknown',
    error: null,
    loading: true,
  };

  const api = () => globalThis.orchard;
  const root = () => document.getElementById('root');

  /** The dates a tab covers. Day is one; week is Monday to Sunday; month is calendar. */
  function boundsFor(tab, date) {
    if (tab === 'week') {
      const from = E.weekStart(date);
      return [from, E.shiftDate(from, 6)];
    }
    if (tab === 'month') {
      const [y, m] = date.split('-').map(Number);
      const from = `${y}-${String(m).padStart(2, '0')}-01`;
      return [from, E.localDate(new Date(y, m, 0, 12).getTime())];
    }
    return [date, date];
  }

  function shift(dir) {
    if (state.tab === 'week') state.date = E.shiftDate(state.date, 7 * dir);
    else if (state.tab === 'month') {
      const [y, m] = state.date.split('-').map(Number);
      state.date = E.localDate(new Date(y, m - 1 + dir, 1, 12).getTime());
    } else state.date = E.shiftDate(state.date, dir);
    void load();
  }

  // --- data ------------------------------------------------------------

  /**
   * Settings are normalised again on the way in, even though the main process
   * normalised them on the way out. The renderer is the consumer, and a panel
   * that assumes `categories` is an array is one absent field away from a blank
   * window — a failure with no message, which is the worst kind this app can
   * produce.
   */
  async function loadSettings() {
    const res = await api().readSettings();
    if (res.ok) state.settings = E.withDefaults(res.settings);
    else state.error = res.error;
  }

  async function load() {
    state.loading = true;
    state.error = null;
    draw();

    const tab = state.tab === 'settings' ? 'day' : state.tab;
    const [from, to] = boundsFor(tab, state.date);
    const res = await api().range(state.settings, from, to);

    if (!res.ok) {
      state.connection = res.kind === 'offline' || res.kind === 'timeout' ? 'offline' : 'error';
      state.error = res.error;
      state.range = null;
      state.days = [];
    } else {
      state.connection = 'live';
      state.buckets = res.data.buckets;
      state.days = E.dateSeries(from, to).map((date) => E.buildDay(date, res.data, state.settings));
      state.range = E.buildRange(state.days, state.settings.categories);
    }
    state.loading = false;
    draw();
  }

  async function saveSettings(next) {
    const res = await api().writeSettings(next);
    if (res.ok) {
      state.settings = E.withDefaults(res.settings);
      await load();
    } else {
      state.error = res.error;
      draw();
    }
  }

  /** Label a subject, and redraw the range it came from with the rule applied. */
  function label(subject, categoryId) {
    void saveSettings(
      Object.assign({}, state.settings, {
        rules: E.setRule(state.settings.rules, subject.kind, subject.value, categoryId),
      }),
    );
  }

  // --- chrome ----------------------------------------------------------

  function chrome() {
    const tabs = el(
      'div',
      { class: 'tabs', role: 'tablist' },
      TABS.map(([key, name]) =>
        el('button', {
          type: 'button',
          role: 'tab',
          'aria-selected': String(state.tab === key),
          text: name,
          onClick: () => {
            state.tab = key;
            if (key === 'settings') draw();
            else void load();
          },
        }),
      ),
    );

    const [from, to] = boundsFor(state.tab === 'settings' ? 'day' : state.tab, state.date);
    const label = from === to ? `${E.weekdayName(from)} ${from}` : `${from} → ${to}`;

    const stateWord =
      state.connection === 'live' ? 'Reading' : state.connection === 'offline' ? 'Offline' : 'Not connected';

    return el('header', { class: 'chrome' }, [
      el('span', { class: 'wordmark', text: 'Orchard' }),
      tabs,
      state.tab !== 'settings' &&
        el('div', { class: 'row' }, [
          el('button', { type: 'button', 'aria-label': 'Previous', text: '‹', onClick: () => shift(-1) }),
          el('button', {
            type: 'button',
            text: 'Today',
            onClick: () => {
              state.date = E.todayLocal();
              void load();
            },
          }),
          el('button', { type: 'button', 'aria-label': 'Next', text: '›', onClick: () => shift(1) }),
          el('span', { class: 'caption data', text: label }),
        ]),
      el('span', { class: 'grow' }),
      el('span', { class: 'status', 'data-state': state.connection, title: state.settings.serverUrl }, [
        el('span', { class: 'pip' }),
        el('span', { text: stateWord }),
      ]),
    ]);
  }

  /**
   * The failure screen. Never an empty chart: a blank week and an unreachable
   * server look identical, and only one of them is about how you spent your
   * time.
   */
  function problem() {
    return el('div', { class: 'notice' }, [
      el('h3', { text: state.connection === 'offline' ? 'ActivityWatch is not answering' : 'Could not read ActivityWatch' }),
      el('p', { class: 'caption', text: state.error || 'Unknown error.' }),
      el('div', { class: 'row', style: { marginTop: '10px' } }, [
        el('button', { type: 'button', text: 'Try again', onClick: () => void load() }),
        el('button', {
          type: 'button',
          text: 'Settings',
          onClick: () => {
            state.tab = 'settings';
            draw();
          },
        }),
      ]),
    ]);
  }

  // --- tabs ------------------------------------------------------------

  function dayTab() {
    const day = state.days[0];
    const range = state.range;
    const cats = state.settings.categories;
    if (!day) return [UI.empty('Nothing to show.')];

    return [
      UI.headline(range),
      UI.findingsPanel(range.findings),
      UI.unlabelledPanel(range.unlabelled, cats, label),
      UI.timeline(day, cats, (span) => {
        // Clicking a block labels the thing it was, which is the same action
        // the list above offers — reached from wherever you noticed it.
        const subject = { kind: span.subjectKind, value: span.subjectValue };
        const next = window.prompt(
          `Label "${subject.value}" as which kind of time?\n\n${cats.map((c) => c.name).join(', ')}`,
        );
        if (!next) return;
        const match = cats.find((c) => c.name.toLowerCase() === next.trim().toLowerCase());
        if (match) label(subject, match.id);
      }),
      el('div', { class: 'grid' }, [
        UI.panel('By kind of time', [
          range.observedSec > 0
            ? UI.splitRows(range.byCategory.filter((c) => c.seconds > 0), range.observedSec)
            : UI.empty('Nothing observed.'),
        ]),
        UI.panel('By application', [
          range.byApp.length
            ? UI.splitRows(range.byApp.slice(0, 12), range.observedSec)
            : UI.empty('Nothing observed.'),
        ]),
        UI.hourHistogram(range.byHour),
      ]),
    ];
  }

  function rangeTab() {
    const range = state.range;
    const cats = state.settings.categories;
    if (!range) return [UI.empty('Nothing to show.')];
    return [
      UI.headline(range),
      UI.findingsPanel(range.findings),
      el('div', { class: 'grid' }, [
        UI.dayColumns(range),
        UI.panel('By kind of time', [
          range.observedSec > 0
            ? UI.splitRows(range.byCategory.filter((c) => c.seconds > 0), range.observedSec)
            : UI.empty('Nothing observed.'),
        ]),
        UI.hourHistogram(range.byHour),
        UI.panel('By application', [
          range.byApp.length
            ? UI.splitRows(range.byApp.slice(0, 12), range.observedSec)
            : UI.empty('Nothing observed.'),
        ]),
      ]),
      UI.unlabelledPanel(range.unlabelled, cats, label),
    ];
  }

  function settingsTab() {
    const s = state.settings;
    const url = el('input', { type: 'text', value: s.serverUrl, spellcheck: 'false' });
    const floor = el('input', { type: 'number', min: '0', max: '3600', step: '30', value: String(s.minSpanSec) });
    const excluded = el('input', {
      type: 'text',
      value: (s.excludedApps || []).join(', '),
      placeholder: 'e.g. 1password, signal',
      spellcheck: 'false',
    });

    const hostOptions = [el('option', { value: '', text: 'Whichever is most recent' })].concat(
      ((state.buckets && state.buckets.hosts) || []).map((h) =>
        el('option', { value: h, text: h, selected: s.hostname === h }),
      ),
    );
    const host = el('select', {}, hostOptions);

    const save = () =>
      void saveSettings(
        Object.assign({}, s, {
          serverUrl: url.value.trim() || 'http://localhost:5600',
          minSpanSec: Math.max(0, parseInt(floor.value, 10) || 0),
          hostname: host.value || null,
          excludedApps: excluded.value
            .split(',')
            .map((x) => x.trim())
            .filter(Boolean),
        }),
      );

    const ruleRows = s.rules.slice(0, 200).map((r) =>
      el('tr', {}, [
        el('td', {}, [el('span', { class: 'kind', text: r.kind }), el('span', { text: r.value })]),
        el('td', {}, [
          el(
            'select',
            { 'aria-label': `Category for ${r.value}`, onChange: (ev) => label(r, ev.target.value || null) },
            [el('option', { value: '', text: 'Remove' })].concat(
              s.categories.map((c) => el('option', { value: c.id, text: c.name, selected: c.id === r.categoryId })),
            ),
          ),
        ]),
      ]),
    );

    return [
      el('div', { class: 'grid' }, [
        UI.panel('ActivityWatch', [
          el('label', { class: 'field' }, [el('span', { class: 'micro', text: 'Server address' }), url]),
          UI.caption(
            'Only an address on this machine is accepted. aw-server has no authentication, so a remote one would read another computer\'s history over an open connection.',
          ),
          el('label', { class: 'field', style: { marginTop: '10px' } }, [
            el('span', { class: 'micro', text: 'Hostname' }),
            host,
          ]),
          UI.caption('Which machine\'s buckets to read, when the server holds more than one.'),
          state.buckets &&
            UI.caption(
              `Reading ${state.buckets.window || 'no window bucket'}` +
                (state.buckets.afk ? `, ${state.buckets.afk}` : ', no afk bucket — away time is not filtered out') +
                (state.buckets.web.length ? `, and ${state.buckets.web.length} browser bucket(s)` : ''),
            ),
        ]),
        UI.panel('What counts', [
          el('label', { class: 'field' }, [
            el('span', { class: 'micro', text: 'Shortest stretch to report, in seconds' }),
            floor,
          ]),
          UI.caption(
            'Zero by default, so the totals here match what the ActivityWatch dashboard shows for the same day. Raise it — two minutes is a reasonable choice — to drop the alt-tab noise from a long day, at the cost of no longer agreeing with ActivityWatch on the total.',
          ),
          el('label', { class: 'field', style: { marginTop: '10px' } }, [
            el('span', { class: 'micro', text: 'Applications to leave out entirely' }),
            excluded,
          ]),
          UI.caption(
            'Comma separated. Excluded before anything is totalled, so an app named here cannot resurface through a chart or a report.',
          ),
        ]),
      ]),
      el('div', { class: 'panel panel-wide row' }, [
        el('button', { type: 'button', text: 'Save', onClick: save }),
        UI.caption(`Stored on this machine. Nothing here leaves it.`),
      ]),
      UI.panel(
        `Rules (${s.rules.length})`,
        [
          UI.caption(
            'A rule says what one application or one site counts as. Domain rules cover their subdomains, so one rule for github.com catches gist.github.com. First match wins, and a rule you set goes to the top.',
          ),
          el('table', {}, [
            el('thead', {}, [el('tr', {}, [el('th', { text: 'Matches' }), el('th', { text: 'Counts as' })])]),
            el('tbody', {}, ruleRows),
          ]),
        ],
        true,
      ),
    ];
  }

  // --- draw ------------------------------------------------------------

  function draw() {
    const mount = root();
    if (!mount) return;
    mount.textContent = '';
    mount.appendChild(chrome());

    const body = el('div', { class: 'body' }, []);
    const stack = el('div', { class: 'stack' }, []);

    if (state.tab === 'settings') {
      if (state.error) stack.appendChild(problem());
      for (const node of settingsTab()) if (node) stack.appendChild(node);
    } else if (state.error) {
      stack.appendChild(problem());
    } else if (state.loading && !state.range) {
      stack.appendChild(UI.panel('Orchard', [UI.caption('Reading ActivityWatch…')]));
    } else {
      const nodes = state.tab === 'day' ? dayTab() : rangeTab();
      for (const node of nodes) if (node) stack.appendChild(node);
    }

    body.appendChild(stack);
    mount.appendChild(body);
  }

  async function start() {
    if (!globalThis.orchard) {
      root().appendChild(
        UI.panel('No bridge', [
          UI.caption('This page is the Orchard renderer and only works inside the Orchard desktop app.'),
        ]),
      );
      return;
    }
    await loadSettings();
    await load();
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', () => void start());
  else void start();
})();
