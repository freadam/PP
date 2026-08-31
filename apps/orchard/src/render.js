/**
 * The drawing. Holds no arithmetic — every figure arrives from `engine.js`
 * already computed, for the same reason Fruit's React layer holds no SQL: a
 * percentage recomputed in the view is a percentage that can disagree with the
 * one in the report it sits inside.
 *
 * Everything goes into the DOM through `textContent`. This app renders window
 * *titles* — arbitrary text from other people's documents, tickets and web
 * pages — and building the screen out of `innerHTML` would turn a page title
 * into script execution.
 */

(function (root, factory) {
  const api = factory(root.Orchard || (typeof require === 'function' ? require('./engine.js') : null));
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.OrchardUI = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function (E) {
  'use strict';

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
      n.appendChild(typeof c === 'object' ? c : document.createTextNode(String(c)));
    }
    return n;
  }

  const panel = (title, children, wide) =>
    el('section', { class: wide ? 'panel panel-wide' : 'panel' }, [el('h3', { text: title })].concat(children || []));

  const caption = (t) => el('p', { class: 'caption', text: t });
  const empty = (t) => el('p', { class: 'empty', text: t });
  const pct = (share) => `${Math.round(share * 100)}%`;

  /**
   * How many rows a list shows before it asks.
   *
   * The tail of a ranked list answers nothing — a hundred one-second entries
   * are noise — but a list that silently stops at twelve is a list that lied
   * about being ranked. So the cut is always visible and always reversible.
   */
  const DEFAULT_ROWS = 12;

  /**
   * The expander. Returns nothing when there is nothing hidden, so a short list
   * does not carry a dead control.
   *
   * The count is in the label rather than in a caption beside it: "Show 34
   * more" tells you whether it is worth pressing, and "Show more" does not.
   */
  function moreRow(total, shown, expanded, onToggle) {
    if (total <= shown && !expanded) return null;
    const hidden = total - shown;
    return el('button', {
      class: 'more',
      type: 'button',
      'aria-expanded': String(!!expanded),
      text: expanded ? `Show fewer` : `Show ${hidden} more`,
      onClick: onToggle,
    });
  }

  /** The rows a list should draw, given whether it is expanded. */
  const visibleRows = (rows, expanded) => (expanded ? rows : rows.slice(0, DEFAULT_ROWS));

  /** A named share of a total, drawn as a bar under its own name. */
  function splitRows(rows, total) {
    return el(
      'div',
      {},
      rows.map((r) =>
        el('div', { class: 'split-row' }, [
          el('span', { class: 'dot', style: { background: r.colour } }),
          el('span', { class: 'grow' }, [
            el('span', { class: 'truncate', style: { display: 'block' }, text: r.name }),
            el('span', {
              class: 'split-bar',
              style: { width: `${(r.seconds / Math.max(total, 1)) * 100}%`, background: r.colour },
            }),
          ]),
          el('span', {
            class: 'data',
            style: { textAlign: 'right' },
            text: `${E.duration(r.seconds)} · ${pct(r.share)}`,
          }),
        ]),
      ),
    );
  }

  // --- the day ---------------------------------------------------------

  /**
   * The day on a real time axis.
   *
   * Drawn between the first and last thing observed rather than from midnight
   * to midnight: eight empty hours at the top of every screen is eight hours of
   * nothing, and the panel is about when things happened.
   */
  function timeline(day, categories, showTitles, onPick) {
    if (!day.spans.length) {
      return panel('The day', [
        empty(
          day.rawSec > 0
            ? 'Everything recorded on this day was filtered out — either you were away from the keyboard for it, or it came in stretches shorter than the floor. Settings can lower the floor.'
            : 'Nothing was recorded on this day.',
        ),
      ]);
    }

    // The axis has to fit the day it is drawing. A fixed 68px hour is right for
    // a working day and useless for a three-minute one, where the whole session
    // would be three pixels tall — which is how a real first run looks. So the
    // scale and the tick interval both come from the extent.
    const PANEL_MIN_PX = 260;
    const PANEL_MAX_PX = 1500;
    const extentMs = Math.max(60000, day.lastAt - day.firstAt);

    // The finest tick that gives a readable number of gridlines.
    const TICKS = [
      [60000, 'minute'],
      [300000, '5 minutes'],
      [900000, 'quarter hour'],
      [1800000, 'half hour'],
      [3600000, 'hour'],
      [10800000, '3 hours'],
    ];
    const [tickMs, TICK_WORD] = TICKS.find(([ms]) => extentMs / ms <= 14) || TICKS[TICKS.length - 1];

    // Start and end on a tick boundary, so the gridlines are round numbers.
    const originMs = Math.floor(day.firstAt / tickMs) * tickMs;
    const endMs = Math.ceil(day.lastAt / tickMs) * tickMs;
    const ticks = Math.max(1, Math.round((endMs - originMs) / tickMs));
    const height = Math.min(PANEL_MAX_PX, Math.max(PANEL_MIN_PX, (extentMs / 3600000) * 68));
    const y = (ms) => ((ms - originMs) / (endMs - originMs)) * height;

    const byId = new Map((categories || []).map((c) => [c.id, c]));
    const kids = [];
    for (let t = 0; t <= ticks; t++) {
      const top = (t / ticks) * height;
      kids.push(el('span', { class: 'hour-line', style: { top: `${top}px` } }));
      if (t < ticks) {
        kids.push(
          el('span', {
            class: 'hour-label',
            style: { top: `${top + 2}px` },
            text: E.clock(originMs + t * tickMs),
          }),
        );
      }
    }

    for (const sp of day.spans) {
      const cat = byId.get(sp.categoryId);
      const top = y(sp.start);
      const seconds = (sp.end - sp.start) / 1000;
      const label = sp.domain || E.appLabel(sp.data.app);
      // Suppressed here as well as in the panel, or the switch is cosmetic.
      const title = showTitles && sp.data.title ? ` — ${sp.data.title}` : '';
      // A one-second visit still has to be clickable, so every block gets a
      // floor of a few pixels even where that overstates its length. The
      // duration is written on the row, which is the honest carrier.
      const drawn = Math.max(4, y(sp.end) - top);
      kids.push(
        el('button', {
          class: 'span',
          type: 'button',
          // Unlabelled time is the sunken fill rather than a hue: it is an
          // absence, and giving it a colour would make it look decided. Its
          // text colour and outline live in the stylesheet rather than here,
          // so the contrast check can hold them to a number.
          'data-labelled': cat ? 'true' : 'false',
          style: {
            top: `${top}px`,
            height: `${drawn}px`,
            background: cat ? cat.colour : 'var(--sunken)',
          },
          title: `${E.clock(sp.start)}-${E.clock(sp.end)} · ${label}${title} · ${E.duration(seconds)}`,
          onClick: () => onPick && onPick(sp),
          text: drawn >= 14 ? `${E.clock(sp.start)}  ${label}${title}  ·  ${E.duration(seconds)}` : label,
        }),
      );
    }

    return panel(
      'The day',
      [
        el('div', { class: 'timeline', style: { height: `${height}px` } }, kids),
        caption(
          `${E.clock(day.firstAt)} to ${E.clock(day.lastAt)}, gridlines every ${TICK_WORD}. Each block is one unbroken stretch in one place; the colour is its label. Click one to label what it was.`,
        ),
      ],
      true,
    );
  }

  /**
   * What has no label yet, ranked by time, each row one click from having one.
   *
   * First on the tab, because it is the only panel that asks something of you.
   */
  function unlabelledPanel(rows, categories, onLabel, opts) {
    if (!rows.length) {
      return panel('No label yet', [empty('Everything observed in this range carries a label. Nothing to do here.')]);
    }
    const o = opts || {};
    const total = rows.reduce((a, r) => a + r.seconds, 0);
    const shown = visibleRows(rows, o.expanded);
    return panel(
      'No label yet',
      [
        caption(
          `${rows.length} ${rows.length === 1 ? 'thing' : 'things'} took ${E.duration(total)} between them and have no kind of time on them. Longest first.`,
        ),
        el(
          'div',
          {},
          shown.map((r) =>
            el('div', { class: 'unlabelled-row' }, [
              el('span', { class: 'truncate' }, [
                el('span', { class: 'kind', text: r.kind }),
                el('span', { text: r.display || r.value }),
              ]),
              el('span', {
                class: 'data caption',
                style: { textAlign: 'right' },
                // One four-hour stretch and forty six-minute ones are different
                // problems, so the count is shown beside the total.
                text: `${E.duration(r.seconds)} · ${r.occurrences}×`,
              }),
              el(
                'select',
                {
                  'aria-label': `Label ${r.value}`,
                  onChange: (ev) => onLabel && onLabel(r, ev.target.value || null),
                },
                [el('option', { value: '', text: 'Label…' })].concat(
                  categories.map((c) => el('option', { value: c.id, text: c.name })),
                ),
              ),
            ]),
          ),
        ),
        moreRow(rows.length, shown.length, o.expanded, o.onToggle),
      ],
      true,
    );
  }

  // --- charts ----------------------------------------------------------

  /**
   * Top window titles — the documents, tickets and videos that were on screen.
   *
   * The one panel that names *what* rather than *which program*, and the most
   * revealing thing this app draws, which is why it can be switched off
   * independently of everything else.
   */
  function titlesPanel(rows, opts) {
    if (!rows.length) return panel('Top window titles', [empty('Nothing observed in this range.')]);
    const o = opts || {};
    const total = rows.reduce((a, r) => a + r.seconds, 0);
    const shown = visibleRows(rows, o.expanded);
    return panel('Top window titles', [
      el(
        'div',
        {},
        shown.map((r) =>
          el('div', { class: 'split-row', title: `${r.title || 'No title reported'} — ${r.app}` }, [
            el('span', { class: 'dot', style: { background: r.colour } }),
            el('span', { class: 'grow' }, [
              el('span', {
                class: 'truncate',
                style: { display: 'block' },
                // A window that reports no title still reports real time; the
                // row says so rather than showing a blank line.
                text: r.title || 'No title reported',
              }),
              el('span', { class: 'micro truncate', style: { display: 'block' }, text: r.app }),
              el('span', {
                class: 'split-bar',
                style: { width: `${(r.seconds / Math.max(total, 1)) * 100}%`, background: r.colour },
              }),
            ]),
            el('span', {
              class: 'data',
              style: { textAlign: 'right' },
              text: `${E.duration(r.seconds)} · ${pct(r.share)}`,
            }),
          ]),
        ),
      ),
      moreRow(rows.length, shown.length, o.expanded, o.onToggle),
    ]);
  }

  function hourHistogram(byHour) {
    const max = Math.max(1, ...byHour);
    return panel('When it happened', [
      el(
        'div',
        { class: 'hours' },
        byHour.map((sec, h) =>
          el('span', {
            'data-zero': sec === 0 ? 'true' : 'false',
            style: { height: `${(sec / max) * 100}%` },
            title: `${String(h).padStart(2, '0')}:00 · ${E.duration(sec)}`,
          }),
        ),
      ),
      el(
        'div',
        { class: 'hour-axis' },
        byHour.map((_, h) => el('span', { text: h % 6 === 0 ? String(h) : '' })),
      ),
      caption('Hours of the local day, 0 to 23. A stretch crossing an hour counts in each, for the minutes it spent there.'),
    ]);
  }

  /** Days as stacked columns, so the mix within a day is visible, not just its height. */
  function dayColumns(range) {
    const max = Math.max(1, ...range.days.map((d) => d.observedSec));
    return panel('By day', [
      el(
        'div',
        { class: 'days' },
        range.days.map((d) =>
          el(
            'div',
            { class: 'col', title: `${d.date} · ${E.duration(d.observedSec)}` },
            d.byCategory
              .filter((c) => c.seconds > 0)
              .map((c) =>
                el('div', {
                  class: 'seg',
                  style: { height: `${(c.seconds / max) * 100}%`, background: c.colour },
                }),
              ),
          ),
        ),
      ),
      el(
        'div',
        { class: 'hour-axis' },
        range.days.map((d) =>
          el('span', { text: range.days.length > 10 ? d.date.slice(8) : E.weekdayName(d.date).slice(0, 2) }),
        ),
      ),
      caption('Each column is one day, stacked by kind of time.'),
    ]);
  }

  function findingsPanel(findings) {
    if (!findings.length) return null;
    return panel(
      'What the numbers say',
      findings.map((f) =>
        el('div', { class: 'finding', 'data-tone': f.tone }, [
          el('b', { text: f.headline }),
          el('span', { class: 'caption', text: f.detail }),
        ]),
      ),
      true,
    );
  }

  /**
   * The headline, and the one number it must never contradict.
   *
   * ActivityWatch's own dashboard shows a "Time active" for the same day. If
   * this panel shows a smaller figure with no explanation, the reader's first
   * conclusion is that one of the two is broken — and they are right to think
   * so. Whenever a filter has removed anything, the amount and the reason are
   * on the same line as the total, not buried in a finding further down.
   */
  function headline(range) {
    const dropped = Math.round(range.rawSec - range.observedSec);
    const period =
      range.days.length === 1
        ? 'Time at the machine, in front of something, on this day.'
        : `Across ${range.days.length} days, ${E.duration(range.dayLengthSec)} on an average day with anything on it.`;

    return panel(
      'Observed',
      [
        el('p', { class: 'display', text: E.duration(range.observedSec) }),
        caption(period),
        dropped > 0 &&
          caption(
            `ActivityWatch recorded ${E.duration(range.rawSec)} of foreground time; ${E.duration(dropped)} of it is not counted here — time you were away from the keyboard, and any stretch below the floor set in Settings.`,
          ),
      ],
      true,
    );
  }

  return {
    el,
    panel,
    caption,
    empty,
    splitRows,
    moreRow,
    visibleRows,
    DEFAULT_ROWS,
    timeline,
    unlabelledPanel,
    titlesPanel,
    hourHistogram,
    dayColumns,
    findingsPanel,
    headline,
  };
});
