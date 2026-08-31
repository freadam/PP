/**
 * The renderer. Holds no arithmetic — every figure on screen comes out of
 * `engine.js` already computed, for the same reason Fruit's React layer holds
 * no SQL: a percentage recomputed in the view is a percentage that can disagree
 * with the one in the report it sits inside.
 *
 * The one thing this file does decide is *how* a number is drawn, and it draws
 * every comparison the same way — a quiet plot line for the intention, a solid
 * track for what happened, on one baseline. That is Fruit's drift rail, and it
 * is the whole reason the reports are legible at a glance.
 */

(function (root, factory) {
  const api = factory(root.FruitReports || (typeof require === 'function' ? require('./engine.js') : null));
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.FruitReportsUI = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function (E) {
  'use strict';

  // ─── DOM helpers ────────────────────────────────────────────────────
  //
  // Everything goes in through `textContent` or a typed attribute setter.
  // Task titles, project names and tag names are user text; building this
  // screen out of innerHTML would make a project called `<img onerror=…>` a
  // scripting bug in a reporting plugin.

  function el(tag, attrs, children) {
    const n = document.createElement(tag);
    for (const k of Object.keys(attrs || {})) {
      const v = attrs[k];
      if (v === null || v === undefined || v === false) continue;
      if (k === 'style') Object.assign(n.style, v);
      else if (k === 'class') n.className = v;
      else if (k === 'text') n.textContent = String(v);
      else if (k.startsWith('on')) n.addEventListener(k.slice(2).toLowerCase(), v);
      else n.setAttribute(k, v === true ? '' : String(v));
    }
    for (const c of [].concat(children || [])) {
      if (c === null || c === undefined || c === false) continue;
      n.appendChild(typeof c === 'string' || typeof c === 'number' ? document.createTextNode(String(c)) : c);
    }
    return n;
  }

  const panel = (title, children, wide) =>
    el('section', { class: wide ? 'panel panel-wide' : 'panel' }, [el('h3', { text: title })].concat(children || []));

  const caption = (t) => el('p', { class: 'caption', text: t });
  const empty = (t) => el('p', { class: 'empty', text: t });
  const pct = (share) => `${Math.round(share * 100)}%`;

  /**
   * The drift rail. `plot` and `track` are both fractions of `max`, so two
   * rails in the same panel share a scale and their lengths are comparable —
   * which is the only reason a bar chart beats a table of numbers.
   */
  function rail(plotSec, trackSec, max, label) {
    const w = (s) => `${Math.max(0, Math.min(100, (s / Math.max(max, 1)) * 100))}%`;
    return el('span', { class: 'rail-bar', role: 'img', 'aria-label': label }, [
      el('span', { class: 'plot', style: { left: 0, width: w(plotSec) } }),
      el('span', { class: 'track', style: { width: w(trackSec) } }),
    ]);
  }

  // ─── the work report ────────────────────────────────────────────────

  function headline(r) {
    const b = r.baseline;
    const bits = [];
    if (b && b.totalWorkSec > 0) {
      const diff = r.totalWorkSec - b.totalWorkSec;
      const p = Math.round((diff / b.totalWorkSec) * 100);
      bits.push(
        `${diff >= 0 ? '+' : '−'}${Math.abs(p)}% against an average of ${E.duration(b.totalWorkSec)} ` +
          `over the last ${b.periods === 1 ? 'comparable period' : `${b.periods} comparable periods`}`,
      );
    } else {
      // No history to compare against, so no arrow. Drawing one anyway is how
      // a report starts asserting things it cannot support.
      bits.push('No earlier period with anything recorded in it, so there is nothing to compare against yet.');
    }
    return panel('Work recorded', [
      el('p', { class: 'display', text: E.duration(r.totalWorkSec) }),
      caption(bits.join('')),
    ], true);
  }

  function findingsPanel(r) {
    if (!r.findings.length) return null;
    return panel(
      'What the numbers say',
      r.findings.map((f) =>
        el('div', { class: 'finding', 'data-tone': f.tone }, [
          el('b', { text: f.headline }),
          el('span', { class: 'caption', text: f.detail }),
        ]),
      ),
      true,
    );
  }

  function scorePanel(r) {
    const s = r.score;
    if (!s.scored) {
      return panel('Score', [
        el('p', { class: 'display', text: 'Not scored' }),
        caption('Nothing was recorded in this period. A zero for a week you were away is a verdict this report has no business delivering.'),
      ]);
    }
    const rows = s.components.map((c) =>
      el('div', { class: 'score-row' }, [
        el('span', {}, [el('span', { text: c.label }), el('br'), el('span', { class: 'micro', text: c.detail })]),
        el('span', { class: 'data', text: String(c.value) }),
        el('span', { class: 'weight data', text: `× ${(c.weight * 100).toFixed(1)}%` }),
      ]),
    );
    const delta =
      r.baseline && r.baseline.score > 0
        ? `${s.value - r.baseline.score >= 0 ? '+' : '−'}${Math.abs(s.value - r.baseline.score)} against a typical score of ${r.baseline.score}`
        : null;
    return panel('Score', [
      el('p', { class: 'display', text: `${s.value} · ${s.rating}` }),
      delta ? caption(delta) : null,
      el('div', { style: { marginTop: 'var(--s, 8px)' } }, rows),
      caption(
        'The weights are a judgement and are stated as one. Fruit weighs a fourth input — how much of the work sat inside a focus session — which Super Productivity cannot supply, so the remaining three are rescaled to sum to 100.',
      ),
    ]);
  }

  function dayGraph(r) {
    const max = Math.max(1, ...r.days.map((d) => d.workSec), r.targetSec || 0);
    const cols = r.days.map((d) =>
      el('div', { class: 'day-col', title: `${d.date} · ${E.duration(d.workSec)}` }, [
        el('div', {
          class: 'day-fill',
          'data-zero': d.workSec === 0 ? 'true' : 'false',
          style: { height: `${(Math.max(d.workSec, 0) / max) * 100}%` },
        }),
      ]),
    );
    // The target line is drawn only where the target applies, so it never
    // implies a shortfall on a day it was never set for.
    const applies = r.days.map((d) => d.targetApplies);
    const first = applies.indexOf(true);
    const last = applies.lastIndexOf(true);
    const line =
      r.targetSec && first !== -1
        ? el('span', {
            class: 'target-line',
            style: {
              bottom: `${(r.targetSec / max) * 100}%`,
              left: `${(first / r.days.length) * 100}%`,
              right: `${((r.days.length - 1 - last) / r.days.length) * 100}%`,
            },
          })
        : null;

    const labels =
      r.days.length <= 31
        ? el(
            'div',
            { class: 'day-labels' },
            r.days.map((d) => el('span', { text: r.days.length > 14 ? d.date.slice(8) : E.weekdayName(d.date).slice(0, 2) })),
          )
        : null;

    return panel('Work by day', [
      el('div', { class: 'day-graph' }, cols.concat(line ? [line] : [])),
      labels,
      caption(
        r.targetSec
          ? `Dashed line: the ${E.duration(r.targetSec)} daily target, drawn across the days it applies to. ${r.targetDaysMet} of ${r.targetDaysApplicable} elapsed days met it.`
          : 'No daily target set. Settings, below, is where one goes.',
      ),
    ]);
  }

  function splitPanel(title, slices, emptyText, note) {
    if (!slices.length) return panel(title, [empty(emptyText)]);
    const total = slices.reduce((a, s) => a + s.seconds, 0);
    return panel(title, [
      el(
        'div',
        {},
        slices.map((s) =>
          el('div', { class: 'split-row' }, [
            el('span', { class: 'dot', style: { background: s.colour } }),
            el('span', { class: 'grow' }, [
              el('span', { class: 'truncate', style: { display: 'block' }, text: s.name }),
              el('span', {
                class: 'split-bar',
                style: { display: 'block', width: `${(s.seconds / Math.max(total, 1)) * 100}%`, background: s.colour },
              }),
            ]),
            el('span', { class: 'data', style: { textAlign: 'right' }, text: `${E.duration(s.seconds)} · ${pct(s.share)}` }),
          ]),
        ),
      ),
      note ? caption(note) : null,
    ]);
  }

  function renderWorkReport(r) {
    return [
      headline(r),
      findingsPanel(r),
      el('div', { class: 'report-grid' }, [
        scorePanel(r),
        dayGraph(r),
        splitPanel(
          'By kind of work',
          r.byTag,
          'No tagged work in this range.',
          "A task's first tag is read as its kind of work, so the shares partition the total instead of double-counting a task with several tags.",
        ),
        splitPanel('By project', r.byProject, 'No work recorded against a project in this range.'),
      ]),
    ];
  }

  // ─── calibration ────────────────────────────────────────────────────

  function renderCalibration(cal) {
    if (cal.sampleCount < E.MIN_SAMPLES) {
      return panel('Calibration', [
        caption(
          `Come back after five tracked tasks — calibration needs a few estimates to compare.${cal.sampleCount > 0 ? ` You have ${cal.sampleCount}.` : ''}`,
        ),
        caption('A task counts once it is done, carries an estimate, and has time tracked against it, within the last 30 days.'),
      ]);
    }
    return panel('Calibration', [
      el('p', { class: 'display', style: { fontSize: '1.125rem' }, text: cal.headline }),
      el(
        'div',
        {},
        cal.buckets.map((b) =>
          el('div', { class: 'bucket-row', 'data-reportable': String(b.isReportable) }, [
            el('span', { class: 'data', text: b.bucket }),
            b.isReportable
              ? el(
                  'span',
                  {
                    class: 'rail-bar',
                    role: 'img',
                    'aria-label': `${b.bucket} estimates run ${b.medianRatio.toFixed(2)} times over, from ${b.n} samples`,
                  },
                  [
                    // Parity sits at the midpoint, so over and under read as a
                    // deflection from the line rather than a bare length.
                    el('span', { class: 'track', style: { width: `${Math.min(100, b.medianRatio * 50)}%` } }),
                    el('span', { class: 'parity' }),
                  ],
                )
              : el('span', { class: 'caption', text: `needs ${E.MIN_SAMPLES - b.n} more` }),
            el('span', {
              class: 'data',
              style: { textAlign: 'right' },
              text: b.isReportable ? `${b.medianRatio.toFixed(2)}× · n=${b.n}` : `n=${b.n}`,
            }),
          ]),
        ),
      ),
      caption(
        'Trailing 30 days, tracked ÷ estimate, bucketed by estimate size. The median, not the mean — one abandoned task ruins a mean. A bucket stays quiet below five samples.',
      ),
    ]);
  }

  // ─── estimated vs actual ────────────────────────────────────────────

  function renderProjectWeeks(rows, onExport) {
    if (!rows.length) {
      return panel(
        'Estimated vs actual, by project and week',
        [empty('No task with an estimate has been completed in this range yet.')],
        true,
      );
    }
    const max = Math.max(1, ...rows.map((r) => Math.max(r.plannedSec, r.trackedSec)));
    const totalPlanned = rows.reduce((a, r) => a + r.plannedSec, 0);
    const totalTracked = rows.reduce((a, r) => a + r.trackedSec, 0);

    return panel(
      'Estimated vs actual, by project and week',
      [
        el(
          'div',
          { class: 'stack', style: { gap: '2px' } },
          rows.map((r) =>
            el('div', { class: 'row' }, [
              el('span', { class: 'dot', style: { background: r.projectColour } }),
              el('span', { class: 'truncate', style: { width: '150px' }, text: r.projectName }),
              el('span', { class: 'data caption', style: { width: '84px' }, text: r.weekStart }),
              el('span', { class: 'grow' }, [
                rail(
                  r.plannedSec,
                  r.trackedSec,
                  max,
                  `${r.projectName}, week of ${r.weekStart}: estimated ${E.duration(r.plannedSec)}, took ${E.duration(r.trackedSec)}`,
                ),
              ]),
              el('span', {
                class: 'data',
                style: { width: '170px', textAlign: 'right' },
                text: `${E.duration(r.plannedSec)} → ${E.duration(r.trackedSec)} (${E.drift(r.trackedSec - r.plannedSec)})`,
              }),
            ]),
          ),
        ),
        el('div', { class: 'row wrap', style: { marginTop: 'var(--s, 8px)' } }, [
          el('span', { class: 'caption grow', text: `Totals: estimated ${E.duration(totalPlanned)}, took ${E.duration(totalTracked)} (${E.drift(totalTracked - totalPlanned)}).` }),
          el('button', { type: 'button', onClick: onExport, text: 'Export CSV' }),
        ]),
        caption(
          'Super Productivity has no plotted blocks, so this compares the same set of tasks to itself: over the tasks completed in a week, what you said they would take against what they took. Both bars describe one population, which is what makes the difference a real quantity.',
        ),
      ],
      true,
    );
  }

  // ─── weekly targets ─────────────────────────────────────────────────

  function renderWeeklyTargets(rows) {
    if (!rows.length) {
      return panel('Weekly targets', [
        empty('No project has a weekly target yet. Settings, below, is where one goes.'),
      ]);
    }
    return panel(
      'Weekly targets',
      [
        el(
          'div',
          { class: 'stack', style: { gap: '2px' } },
          rows.map((t) => {
            const behind = t.trackedSec < t.paceSec;
            return el('div', { class: 'row' }, [
              el('span', { class: 'dot', style: { background: t.projectColour } }),
              el('span', { class: 'truncate', style: { width: '130px' }, text: t.projectName }),
              el('span', { class: 'grow' }, [
                // Decorative: the tracked total, the target and "behind pace"
                // are all read out as text on the same row.
                el('span', { class: 'rail-bar', 'aria-hidden': 'true' }, [
                  el('span', { class: 'plot', style: { left: 0, width: `${Math.min(100, (t.paceSec / t.targetSec) * 100)}%` } }),
                  el('span', { class: 'track', style: { width: `${Math.min(100, (t.trackedSec / t.targetSec) * 100)}%` } }),
                ]),
              ]),
              el('span', { class: 'data', style: { width: '190px', textAlign: 'right' } }, [
                el('span', { text: `${E.duration(t.trackedSec)} / ${E.duration(t.targetSec)} ` }),
                el('span', { class: 'micro', style: { color: behind ? 'var(--over)' : 'var(--under)' }, text: behind ? 'behind pace' : 'on pace' }),
              ]),
            ]);
          }),
        ),
        caption('The quiet line is pace: where the project would be by now if the week were spent evenly.'),
      ],
    );
  }

  return {
    el,
    panel,
    caption,
    empty,
    rail,
    renderWorkReport,
    renderCalibration,
    renderProjectWeeks,
    renderWeeklyTargets,
  };
});
