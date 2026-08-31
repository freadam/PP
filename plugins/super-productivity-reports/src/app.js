/**
 * The shell: read Super Productivity's state once, hand it to the engine, draw
 * what comes back.
 *
 * # One read, every panel
 *
 * Fruit's `get_work_report` is one command returning five panels, because five
 * commands would mean five range parses and five chances for panels on the same
 * screen to disagree about which week it is. The same rule holds here for a
 * different reason: every panel is computed from one `normalise()` of one
 * snapshot, so the calibration figure and the by-project split can never be
 * reading two different versions of the archive.
 */

(function () {
  'use strict';

  const E = globalThis.FruitReports;
  const UI = globalThis.FruitReportsUI;
  const el = UI.el;

  const SETTINGS_KEY = 'settings';
  const PERIODS = [
    { key: 'day', label: 'Day' },
    { key: 'week', label: 'Week' },
    { key: 'month', label: 'Month' },
  ];
  const DAY_NAMES = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

  const state = {
    tab: 'work',
    period: 'week',
    date: E.todayLocal(),
    model: null,
    settings: E.defaultSettings(),
    loading: true,
    error: null,
  };

  const api = () => globalThis.PluginAPI;
  const root = () => document.getElementById('root');

  // ─── data ───────────────────────────────────────────────────────────

  async function load() {
    state.loading = true;
    state.error = null;
    draw();
    try {
      const P = api();
      // The archive is not optional. Super Productivity moves finished tasks
      // out of the live set, so a 30-day calibration window read from live
      // tasks alone would be mostly empty and would report "come back after
      // five tasks" to somebody who has finished fifty.
      const [tasks, archived, projects, tags, saved] = await Promise.all([
        P.getTasks(),
        P.getArchivedTasks().catch(() => []),
        P.getAllProjects(),
        P.getAllTags(),
        P.loadSyncedData(SETTINGS_KEY).catch(() => null),
      ]);
      state.model = E.normalise({ tasks, archivedTasks: archived, projects, tags });
      state.settings = E.withDefaults(saved ? safeParse(saved) : null);
    } catch (err) {
      state.error = (err && err.message) || String(err);
    } finally {
      state.loading = false;
      draw();
    }
  }

  function safeParse(s) {
    try {
      return JSON.parse(s);
    } catch (_) {
      // Corrupt settings are not a reason to show no report. Fall back to the
      // defaults and carry on; the settings tab will save a clean copy.
      return null;
    }
  }

  async function saveSettings(next) {
    state.settings = E.withDefaults(next);
    draw();
    try {
      await api().persistDataSynced(JSON.stringify(state.settings), SETTINGS_KEY);
      api().showSnack({ msg: 'Report settings saved', type: 'SUCCESS' });
    } catch (err) {
      api().showSnack({ msg: 'Could not save report settings', type: 'ERROR' });
    }
  }

  // ─── period navigation ──────────────────────────────────────────────

  /** One step is one period — a button that moves a week while you are looking
   *  at a month is a button you stop trusting. */
  function shift(dir) {
    const d = E.parseDate(state.date);
    state.date = E.formatDate(
      state.period === 'month'
        ? E.shiftPeriod(d, 'month', dir)
        : E.addDays(d, state.period === 'week' ? 7 * dir : dir),
    );
    draw();
  }

  function rangeLabel(from, to) {
    if (from === to) return from;
    return `${from} → ${to}`;
  }

  // ─── chrome ─────────────────────────────────────────────────────────

  function tabs() {
    const mk = (key, label) =>
      el('button', {
        type: 'button',
        role: 'tab',
        'aria-selected': String(state.tab === key),
        text: label,
        onClick: () => {
          state.tab = key;
          draw();
        },
      });
    return el('div', { class: 'tabs', role: 'tablist' }, [
      mk('work', 'Work report'),
      mk('estimates', 'Estimates'),
      mk('targets', 'Targets'),
      mk('settings', 'Settings'),
    ]);
  }

  function periodBar() {
    const [from, to] = E.periodBounds(E.parseDate(state.date), state.period);
    return el('div', { class: 'row wrap' }, [
      el(
        'div',
        { class: 'segmented', role: 'group', 'aria-label': 'Period' },
        PERIODS.map((p) =>
          el('button', {
            type: 'button',
            'aria-pressed': String(state.period === p.key),
            text: p.label,
            onClick: () => {
              state.period = p.key;
              draw();
            },
          }),
        ),
      ),
      el('button', { type: 'button', 'aria-label': 'Previous period', text: '‹', onClick: () => shift(-1) }),
      el('button', {
        type: 'button',
        text: 'Today',
        onClick: () => {
          state.date = E.todayLocal();
          draw();
        },
      }),
      el('button', { type: 'button', 'aria-label': 'Next period', text: '›', onClick: () => shift(1) }),
      el('span', { class: 'caption data grow', text: rangeLabel(E.formatDate(from), E.formatDate(to)) }),
    ]);
  }

  // ─── settings ───────────────────────────────────────────────────────

  /**
   * Super Productivity has no daily work target and no per-project weekly
   * target, so the two numbers Fruit measures against are set here. Both are
   * entered in hours and minutes, because that is how people say them.
   */
  function settingsPanel() {
    const s = state.settings;
    const hours = s.dailyTargetSec ? Math.floor(s.dailyTargetSec / 3600) : '';
    const mins = s.dailyTargetSec ? Math.round((s.dailyTargetSec % 3600) / 60) : '';

    const hInput = el('input', { type: 'number', min: '0', max: '24', value: hours, placeholder: '0' });
    const mInput = el('input', { type: 'number', min: '0', max: '59', step: '5', value: mins, placeholder: '0' });

    const dayBoxes = DAY_NAMES.map((name, i) =>
      el('label', {}, [
        el('input', { type: 'checkbox', checked: (s.appliesDays & (1 << i)) !== 0, 'data-bit': String(1 << i) }),
        el('span', { text: name }),
      ]),
    );

    const projectInputs = [...state.model.projects.values()]
      .filter((p) => !p.isArchived)
      .map((p) => {
        const cur = s.weeklyTargets[p.id];
        const input = el('input', {
          type: 'number',
          min: '0',
          step: '0.5',
          value: cur ? String(Math.round((cur / 3600) * 100) / 100) : '',
          placeholder: '—',
          'data-project': p.id,
        });
        return el('tr', {}, [
          el('td', {}, [el('span', { class: 'dot', style: { background: p.colour, marginRight: '6px' } }), el('span', { text: p.name })]),
          el('td', { class: 'num', style: { width: '120px' } }, [input]),
        ]);
      });

    const save = () => {
      const h = parseInt(hInput.value, 10) || 0;
      const m = parseInt(mInput.value, 10) || 0;
      const total = h * 3600 + m * 60;
      let mask = 0;
      for (const box of dayBoxes) {
        const cb = box.querySelector('input');
        if (cb.checked) mask |= parseInt(cb.getAttribute('data-bit'), 10);
      }
      const weeklyTargets = {};
      for (const row of projectInputs) {
        const input = row.querySelector('input');
        const v = parseFloat(input.value);
        if (Number.isFinite(v) && v > 0) weeklyTargets[input.getAttribute('data-project')] = Math.round(v * 3600);
      }
      void saveSettings({ dailyTargetSec: total > 0 ? total : null, appliesDays: mask, weeklyTargets });
    };

    return [
      UI.panel('Daily work target', [
        UI.caption('What a working day should hold. Leave it empty and the report scores on how even the days were instead — with no target there is nothing to be behind.'),
        el('div', { class: 'row', style: { marginTop: 'var(--s, 8px)' } }, [
          el('label', { class: 'field' }, [el('span', { class: 'micro', text: 'Hours' }), hInput]),
          el('label', { class: 'field' }, [el('span', { class: 'micro', text: 'Minutes' }), mInput]),
        ]),
        el('p', { class: 'micro', style: { margin: 'var(--s, 8px) 0 4px' }, text: 'Applies on' }),
        el('div', { class: 'days-mask' }, dayBoxes),
        UI.caption('A Monday-to-Friday target does not report a shortfall on a Sunday, which is the whole reason this exists.'),
      ]),
      UI.panel('Weekly targets, per project', [
        UI.caption('Hours per week. A project with no target is simply not held to one.'),
        projectInputs.length
          ? el('table', {}, [
              el('thead', {}, [el('tr', {}, [el('th', { text: 'Project' }), el('th', { class: 'num', text: 'Hours/week' })])]),
              el('tbody', {}, projectInputs),
            ])
          : UI.empty('No active projects.'),
      ]),
      el('div', { class: 'panel panel-wide row' }, [
        el('button', { type: 'button', class: 'btn-primary', text: 'Save settings', onClick: save }),
        el('span', { class: 'caption grow', text: 'Saved with your Super Productivity data, so these travel with sync.' }),
      ]),
    ];
  }

  // ─── export ─────────────────────────────────────────────────────────

  function exportCsv(rows, from, to) {
    const csv = E.reportCsv(rows);
    try {
      api().downloadFile(`fruit-estimates-${from}-to-${to}.csv`, csv);
    } catch (err) {
      api().showSnack({ msg: 'Could not write the CSV', type: 'ERROR' });
    }
  }

  // ─── draw ───────────────────────────────────────────────────────────

  function draw() {
    const mount = root();
    if (!mount) return;
    mount.textContent = '';

    if (state.error) {
      mount.appendChild(
        UI.panel('Could not read your data', [
          el('p', { class: 'caption', text: state.error }),
          el('button', { type: 'button', text: 'Try again', onClick: () => void load() }),
        ]),
      );
      return;
    }
    if (state.loading || !state.model) {
      mount.appendChild(UI.panel('Fruit reports', [el('p', { class: 'caption', text: 'Reading…' })]));
      return;
    }

    const frag = document.createDocumentFragment();
    frag.appendChild(tabs());

    if (state.tab === 'work') {
      frag.appendChild(periodBar());
      const r = E.buildWorkReport(state.model, state.date, state.period, state.settings);
      for (const node of UI.renderWorkReport(r)) if (node) frag.appendChild(node);
    } else if (state.tab === 'estimates') {
      frag.appendChild(periodBar());
      const [from, to] = E.periodBounds(E.parseDate(state.date), state.period);
      const grid = el('div', { class: 'report-grid' }, [UI.renderCalibration(E.buildCalibration(state.model))]);
      frag.appendChild(grid);
      const rows = E.buildProjectWeeks(state.model, from, to, null);
      frag.appendChild(UI.renderProjectWeeks(rows, () => exportCsv(rows, E.formatDate(from), E.formatDate(to))));
    } else if (state.tab === 'targets') {
      frag.appendChild(
        el('div', { class: 'report-grid' }, [UI.renderWeeklyTargets(E.buildWeeklyTargets(state.model, state.settings))]),
      );
    } else {
      frag.appendChild(el('div', { class: 'report-grid' }, settingsPanel()));
    }

    const wrapper = el('div', { class: 'stack' }, []);
    wrapper.appendChild(frag);
    mount.appendChild(wrapper);
  }

  // ─── start ──────────────────────────────────────────────────────────

  function start() {
    if (!globalThis.PluginAPI) {
      root().appendChild(
        UI.panel('No plugin API', [
          el('p', {
            class: 'caption',
            text: 'This page is Super Productivity plugin UI and only works inside the app.',
          }),
        ]),
      );
      return;
    }
    void load();
    // The report is a read of state the user is actively changing. Redrawing on
    // task changes keeps it from quietly going stale behind an open panel.
    try {
      const P = api();
      if (P.registerHook && P.Hooks) {
        const refresh = () => void load();
        P.registerHook(P.Hooks.ANY_TASK_UPDATE, refresh);
        P.registerHook(P.Hooks.TASK_COMPLETE, refresh);
      }
    } catch (_) {
      // Hooks are a convenience; the Today button already reloads.
    }
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start);
  else start();
})();
