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
  function timeline(day, categories, onPick) {
    if (!day.spans.length) {
      return panel('The day', [
        empty(
          day.rawSec > 0
            ? 'Everything recorded on this day was filtered out — either you were away from the keyboard for it, or it came in stretches shorter than the floor. Settings can lower the floor.'
            : 'Nothing was recorded on this day.',
        ),
      ]);
    }

    const HOUR_PX = 68;
    const firstHour = Math.floor((day.firstAt - day.from) / 3600000);
    const lastHour = Math.min(24, Math.ceil((day.lastAt - day.from) / 3600000));
    const hours = Math.max(1, lastHour - firstHour);
    const originMs = day.from + firstHour * 3600000;
    const y = (ms) => ((ms - originMs) / 3600000) * HOUR_PX;

    const byId = new Map((categories || []).map((c) => [c.id, c]));
    const kids = [];
    for (let h = 0; h <= hours; h++) {
      kids.push(el('span', { class: 'hour-line', style: { top: `${h * HOUR_PX}px` } }));
      if (h < hours) {
        kids.push(
          el('span', {
            class: 'hour-label',
            style: { top: `${h * HOUR_PX + 2}px` },
            text: `${String(firstHour + h).padStart(2, '0')}:00`,
          }),
        );
      }
    }

    for (const s of day.spans) {
      const cat = byId.get(s.categoryId);
      const top = y(s.start);
      const height = Math.max(3, y(s.end) - top);
      const label = s.domain || E.appLabel(s.data.app);
      const title = s.data.title ? ` — ${s.data.title}` : '';
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
            height: `${height}px`,
            background: cat ? cat.colour : 'var(--sunken)',
          },
          title: `${E.clock(s.start)}–${E.clock(s.end)} · ${label}${title} · ${E.duration((s.end - s.start) / 1000)}`,
          onClick: () => onPick && onPick(s),
          text: height >= 14 ? `${E.clock(s.start)}  ${label}${title}` : label,
        }),
      );
    }

    return panel(
      'The day',
      [
        el('div', { class: 'timeline', style: { height: `${hours * HOUR_PX}px` } }, kids),
        caption(
          `${E.clock(day.firstAt)} to ${E.clock(day.lastAt)}. Each block is one unbroken stretch in one place; the colour is its label. Click one to label what it was.`,
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
  function unlabelledPanel(rows, categories, onLabel) {
    if (!rows.length) {
      return panel('No label yet', [empty('Everything observed in this range carries a label. Nothing to do here.')]);
    }
    const total = rows.reduce((a, r) => a + r.seconds, 0);
    return panel(
      'No label yet',
      [
        caption(
          `${rows.length} ${rows.length === 1 ? 'thing' : 'things'} took ${E.duration(total)} between them and have no kind of time on them. Longest first.`,
        ),
        el(
          'div',
          {},
          rows.slice(0, 12).map((r) =>
            el('div', { class: 'unlabelled-row' }, [
              el('span', { class: 'truncate' }, [
                el('span', { class: 'kind', text: r.kind }),
                el('span', { text: r.value }),
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
      ],
      true,
    );
  }

  // --- charts ----------------------------------------------------------

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

  function headline(range) {
    return panel(
      'Observed',
      [
        el('p', { class: 'display', text: E.duration(range.observedSec) }),
        caption(
          range.days.length === 1
            ? 'Time at the machine, in front of something, on this day.'
            : `Across ${range.days.length} days, ${E.duration(range.dayLengthSec)} on an average day with anything on it.`,
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
    timeline,
    unlabelledPanel,
    hourHistogram,
    dayColumns,
    findingsPanel,
    headline,
  };
});
