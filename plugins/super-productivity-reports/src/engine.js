/**
 * The report engine — Fruit's reporting arithmetic, ported to Super
 * Productivity's data.
 *
 * # Why this file has no DOM in it
 *
 * The same reason `fruit-core` has no Tauri in it: the rules are the part worth
 * testing, and a rule you can only exercise by clicking is a rule nobody
 * exercises. Everything here is a pure function over a normalised model, so
 * `test/engine.test.js` can hold the same arithmetic to the same numbers the
 * Rust acceptance suite holds `fruit-core` to.
 *
 * # What was ported exactly, and what could not be
 *
 * Fruit reads a `time_session` table: rows with a start instant, an end
 * instant and a task. Super Productivity keeps `timeSpentOnDay` — a map of
 * local date to milliseconds, per task. Every figure that needs only *how much,
 * on which day, on what* survives the change intact. Every figure that needs
 * *when in the day* or *for how long unbroken* does not exist here at all, and
 * is reported as missing rather than approximated:
 *
 *   - the hour-of-day histogram        (needs session start times)
 *   - the working-day span, and the    (needs first/last session of a day)
 *     span-density finding
 *   - focus sessions, and the          (Super Productivity has no timed
 *     "Deliberate" score component      blocks plotted in advance)
 *   - the apps panel and the           (no screen observation)
 *     plan-vs-observation correlation
 *
 * Guessing at any of them from per-day totals would produce a number that
 * looks like the Fruit one and is not, which is the single worst outcome for a
 * report whose entire purpose is being trustworthy.
 *
 * # One rule that is easy to get wrong
 *
 * In Super Productivity a parent task's `timeSpentOnDay` is the *sum of its
 * subtasks'* — the reducer maintains it. Counting both parents and subtasks
 * doubles every total on the screen. `normalise` drops any task that has
 * subtasks and counts the leaves, which is why the totals here match the ones
 * the app itself shows.
 */

(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.FruitReports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
  'use strict';

  // ─── constants, mirrored from fruit-core ────────────────────────────

  /** Report a bucket only at n ≥ 5, so a single outlier doesn't shout (F6). */
  const MIN_SAMPLES = 5;
  const CALIBRATION_WINDOW_DAYS = 30;
  const BUCKET_ORDER = ['<=15m', '30m', '1h', '2h', '3h+'];

  /** The uncategorised bucket is an absence, not a category. Neutral grey. */
  const GREY = '#C7C7C7';
  /** Fruit's "Inbox" / no-project colour. */
  const NO_PROJECT = '#8B95A3';

  /**
   * Super Productivity attaches a synthetic `TODAY` tag to tasks planned for
   * today. It is a view, not a kind of work, and letting it into the split
   * would relabel half the week every morning.
   */
  const SYNTHETIC_TAGS = new Set(['TODAY']);

  // ─── local dates ────────────────────────────────────────────────────
  //
  // `timeSpentOnDay` is keyed by *local* date already, so there is no timezone
  // to resolve and nothing to convert — which removes the entire class of bug
  // Fruit needs `zone()` to defend against. Dates are handled as UTC-midnight
  // instants purely so that arithmetic on them cannot be bitten by DST.

  function parseDate(s) {
    const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(s || ''));
    if (!m) return null;
    const d = new Date(Date.UTC(+m[1], +m[2] - 1, +m[3]));
    return Number.isNaN(d.getTime()) ? null : d;
  }

  function formatDate(d) {
    return d.toISOString().slice(0, 10);
  }

  function addDays(d, n) {
    return new Date(d.getTime() + n * 86400000);
  }

  /** Monday, matching `fruit-core`'s `week_start`. */
  function weekStart(d) {
    return addDays(d, -((d.getUTCDay() + 6) % 7));
  }

  /** Today as a local date string — the same key space `timeSpentOnDay` uses. */
  function todayLocal(now) {
    const d = now ? new Date(now) : new Date();
    const p = (n) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
  }

  /** A millisecond instant to the local date it fell on. */
  function msToLocalDate(ms) {
    return typeof ms === 'number' && Number.isFinite(ms) ? todayLocal(ms) : null;
  }

  /** The calendar range a period covers, given any date inside it. */
  function periodBounds(day, period) {
    if (period === 'week') {
      const monday = weekStart(day);
      return [monday, addDays(monday, 6)];
    }
    if (period === 'month') {
      const first = new Date(Date.UTC(day.getUTCFullYear(), day.getUTCMonth(), 1));
      const next = new Date(Date.UTC(first.getUTCFullYear(), first.getUTCMonth() + 1, 1));
      return [first, addDays(next, -1)];
    }
    return [day, day];
  }

  /**
   * `n` periods before or after `from`. Months step by calendar month rather
   * than by 30 days, so "the last four months" is four months and not roughly
   * four — and the 31st does not quietly become the 1st.
   */
  function shiftPeriod(from, period, n) {
    if (period === 'week') return addDays(from, 7 * n);
    if (period === 'month') {
      return new Date(Date.UTC(from.getUTCFullYear(), from.getUTCMonth() + n, 1));
    }
    return addDays(from, n);
  }

  function dateSeries(from, to) {
    const out = [];
    for (let c = from; c.getTime() <= to.getTime(); c = addDays(c, 1)) out.push(formatDate(c));
    return out;
  }

  const WEEKDAYS = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];

  /** "2026-08-16" → "Sunday". Falls back to the date, never to a blank. */
  function weekdayName(date) {
    const d = parseDate(date);
    return d ? WEEKDAYS[(d.getUTCDay() + 6) % 7] : String(date);
  }

  /** Monday = 1 … Sunday = 64, matching Fruit's `goal.applies_days`. */
  const ALL_DAYS = 127;
  function weekdayApplies(date, mask) {
    const d = parseDate(date);
    if (!d) return false;
    return (mask & (1 << ((d.getUTCDay() + 6) % 7))) !== 0;
  }

  // ─── formatting ─────────────────────────────────────────────────────

  /** `fmt.duration` from the Fruit renderer: `6h 20m`, `45m`, `2h`. */
  function duration(sec) {
    const s = Math.abs(Math.round(sec));
    const h = Math.floor(s / 3600);
    const m = Math.round((s % 3600) / 60);
    if (h === 0) return `${m}m`;
    if (m === 0) return `${h}h`;
    return `${h}h ${m}m`;
  }

  /** The `{}h {:02}m` form the findings are written in. */
  function hm(sec) {
    const s = Math.abs(Math.round(sec));
    return `${Math.floor(s / 3600)}h ${String(Math.floor((s % 3600) / 60)).padStart(2, '0')}m`;
  }

  /** Signed, with a real minus sign: `+14m`, `−22m`. */
  function drift(sec) {
    if (Math.round(sec) === 0) return '0m';
    return `${sec > 0 ? '+' : '−'}${duration(sec)}`;
  }

  // ─── statistics ─────────────────────────────────────────────────────

  /**
   * Median, not mean — one abandoned task ruins a mean (§6.4). Even counts
   * average the middle pair, as `fruit-core` does.
   */
  function median(values) {
    if (!values.length) return null;
    const v = values.slice().sort((a, b) => a - b);
    const mid = v.length >> 1;
    return v.length % 2 === 0 ? (v[mid - 1] + v[mid]) / 2 : v[mid];
  }

  /** Rounded to 3dp, as Fruit's `share_of` does, so shares are stable. */
  function shareOf(part, total) {
    return total <= 0 ? 0 : Math.round((part / total) * 1000) / 1000;
  }

  /** Integer division that truncates toward zero, matching Rust's `i64 / i64`. */
  function idiv(a, b) {
    return b === 0 ? 0 : Math.trunc(a / b);
  }

  function pct01(x) {
    return Math.round(Math.min(1, Math.max(0, x)) * 100);
  }

  // ─── normalisation ──────────────────────────────────────────────────

  /**
   * Super Productivity's state, reduced to the handful of facts the reports
   * need.
   *
   * `tasks` may be the live set, the archive, or both; ids are deduplicated
   * with the live copy winning, because the archive holds a snapshot taken at
   * archive time and the live row is the one the user can still edit.
   */
  function normalise(input) {
    const projects = new Map();
    for (const p of input.projects || []) {
      projects.set(p.id, {
        id: p.id,
        name: p.title || 'Untitled project',
        colour: (p.theme && p.theme.primary) || NO_PROJECT,
        isArchived: !!p.isArchived,
      });
    }

    const tags = new Map();
    for (const t of input.tags || []) {
      if (SYNTHETIC_TAGS.has(t.id)) continue;
      tags.set(t.id, { id: t.id, name: t.title || 'Untitled tag', colour: t.color || GREY });
    }

    // Live first, then archived: `set` only fills a gap, so live wins.
    const raw = new Map();
    for (const t of input.tasks || []) if (t && t.id) raw.set(t.id, t);
    for (const t of input.archivedTasks || []) if (t && t.id && !raw.has(t.id)) raw.set(t.id, t);

    const tasks = [];
    for (const t of raw.values()) {
      // A parent's `timeSpentOnDay` is the sum of its subtasks'. Counting both
      // doubles every total on the screen.
      if (Array.isArray(t.subTaskIds) && t.subTaskIds.length > 0) continue;

      const parent = t.parentId ? raw.get(t.parentId) : null;

      const byDate = {};
      let tracked = 0;
      const spent = t.timeSpentOnDay || {};
      for (const date of Object.keys(spent)) {
        if (!parseDate(date)) continue;
        const sec = Math.round((+spent[date] || 0) / 1000);
        if (sec <= 0) continue;
        byDate[date] = (byDate[date] || 0) + sec;
        tracked += sec;
      }
      // A task whose per-day map is empty but which carries a total is a
      // legacy or imported row. Its total is still true; it just has no day to
      // put it on, so it counts for calibration and not for the day graph.
      const totalSec = tracked > 0 ? tracked : Math.round((+t.timeSpent || 0) / 1000);

      // A subtask inherits what it did not state for itself. In Super
      // Productivity subtasks routinely carry neither, and dropping them into
      // "No project / Untagged" would empty both splits.
      const projectId = t.projectId || (parent && parent.projectId) || null;
      let tagIds = (t.tagIds || []).filter((id) => tags.has(id));
      if (!tagIds.length && parent) tagIds = (parent.tagIds || []).filter((id) => tags.has(id));

      tasks.push({
        id: t.id,
        title: t.title || '',
        projectId: projects.has(projectId) ? projectId : null,
        // Fruit's category is single-valued and its splits must partition the
        // total, so the first tag is the task's kind of work. Attributing the
        // time to every tag would make the shares add to more than 100%.
        tagId: tagIds[0] || null,
        isDone: !!t.isDone,
        doneOn: msToLocalDate(t.doneOn),
        estimateSec: Math.round((+t.timeEstimate || 0) / 1000),
        trackedSec: totalSec,
        byDate,
      });
    }

    return { tasks, projects, tags };
  }

  // ─── settings ───────────────────────────────────────────────────────

  /**
   * Super Productivity has no daily work target and no per-project weekly
   * target, so the two figures Fruit measures against live in plugin settings.
   * Absent means *not set*, which is different from zero: with no target there
   * is nothing to be behind, and the report says so rather than reporting a
   * shortfall against a number nobody chose.
   */
  function defaultSettings() {
    return { dailyTargetSec: null, appliesDays: 31 /* Mon–Fri */, weeklyTargets: {} };
  }

  function withDefaults(settings) {
    const s = settings || {};
    const d = defaultSettings();
    return {
      dailyTargetSec:
        typeof s.dailyTargetSec === 'number' && s.dailyTargetSec > 0 ? s.dailyTargetSec : null,
      appliesDays: typeof s.appliesDays === 'number' ? s.appliesDays : d.appliesDays,
      weeklyTargets: s.weeklyTargets && typeof s.weeklyTargets === 'object' ? s.weeklyTargets : {},
    };
  }

  // ─── the work report ────────────────────────────────────────────────

  function workedSecOnDates(model, dates) {
    let total = 0;
    const set = new Set(dates);
    for (const t of model.tasks) {
      for (const date of Object.keys(t.byDate)) if (set.has(date)) total += t.byDate[date];
    }
    return total;
  }

  /**
   * Shared shaping for the two splits. `fallback` names the bucket for rows
   * with no project or no tag, which is always kept: the shares have to add up,
   * and hiding the remainder leaves the user unable to find the missing time.
   */
  function slices(totals, lookup, total, fallback) {
    const out = [];
    for (const [id, seconds] of totals) {
      if (seconds <= 0) continue;
      const meta = id ? lookup.get(id) : null;
      out.push({
        id: id || null,
        name: meta ? meta.name : fallback,
        colour: meta ? meta.colour : GREY,
        seconds,
        share: shareOf(seconds, total),
      });
    }
    out.sort((a, b) => b.seconds - a.seconds || a.name.localeCompare(b.name));
    return out;
  }

  function splitBy(model, dates, key, lookup, fallback, total) {
    const set = new Set(dates);
    const totals = new Map();
    for (const t of model.tasks) {
      let sec = 0;
      for (const date of Object.keys(t.byDate)) if (set.has(date)) sec += t.byDate[date];
      if (sec <= 0) continue;
      const k = t[key] || null;
      totals.set(k, (totals.get(k) || 0) + sec);
    }
    return slices(totals, lookup, total, fallback);
  }

  /**
   * One range, without the baseline. Split out so `buildWorkReport` can call it
   * once for the period being viewed and four more times for the periods behind
   * it — and so the recursion terminates, which it would not if the baseline
   * were computed inside the public entry point.
   */
  function reportForRange(model, from, to, period, settings, today) {
    const s = withDefaults(settings);
    const dates = dateSeries(from, to);

    const days = dates.map((date) => ({
      date,
      workSec: workedSecOnDates(model, [date]),
      targetApplies: s.dailyTargetSec !== null && weekdayApplies(date, s.appliesDays),
    }));
    const totalWorkSec = days.reduce((a, d) => a + d.workSec, 0);

    // Only days that have *happened* count towards "met". A Friday that has not
    // arrived is not a day you failed to hit six hours on.
    let met = null;
    let applicable = null;
    if (s.dailyTargetSec !== null) {
      met = 0;
      applicable = 0;
      for (const d of days) {
        if (d.targetApplies && d.date <= today) {
          applicable += 1;
          if (d.workSec >= s.dailyTargetSec) met += 1;
        }
      }
    }

    const byTag = splitBy(model, dates, 'tagId', model.tags, 'Untagged', totalWorkSec);
    const byProject = splitBy(model, dates, 'projectId', model.projects, 'No project', totalWorkSec);

    return {
      from: formatDate(from),
      to: formatDate(to),
      period,
      totalWorkSec,
      targetSec: s.dailyTargetSec,
      targetDaysMet: met,
      targetDaysApplicable: applicable,
      days,
      byTag,
      byProject,
      score: scoreFor(days, s.dailyTargetSec, byTag, totalWorkSec),
      baseline: null,
      findings: [],
    };
  }

  /**
   * The four comparable periods before this one, averaged.
   *
   * Periods with nothing recorded are skipped rather than averaged in as
   * zeroes. A fortnight's holiday is silence, not evidence, and letting it into
   * the mean would manufacture a rise out of coming back to work.
   */
  function baselineFor(model, from, period, settings, today) {
    const PERIODS = 4;
    const past = [];
    for (let back = 1; back <= PERIODS; back++) {
      const [a, b] = periodBounds(shiftPeriod(from, period, -back), period);
      const r = reportForRange(model, a, b, period, settings, today);
      if (r.totalWorkSec === 0) continue;
      past.push(r);
    }
    if (!past.length) return null;
    const n = past.length;
    const mean = (f) => idiv(past.reduce((acc, r) => acc + f(r), 0), n);
    return {
      periods: n,
      totalWorkSec: mean((r) => r.totalWorkSec),
      score: mean((r) => (r.score.scored ? r.score.value : 0)),
      dayLengthSec: mean((r) => {
        const worked = r.days.filter((d) => d.workSec > 0);
        return worked.length ? idiv(worked.reduce((a, d) => a + d.workSec, 0), worked.length) : 0;
      }),
    };
  }

  /**
   * The five-questions report, for `date`'s day, ISO week, or calendar month.
   *
   * `period` is `"day"`, `"week"` or `"month"`; anything else is a day, because
   * the narrowest answer is the safest one to give by mistake.
   */
  function buildWorkReport(model, date, period, settings, now) {
    const p = period === 'week' || period === 'month' ? period : 'day';
    const day = parseDate(date) || parseDate(todayLocal(now));
    const today = todayLocal(now);
    const [from, to] = periodBounds(day, p);
    const report = reportForRange(model, from, to, p, settings, today);
    report.baseline = baselineFor(model, from, p, settings, today);
    report.findings = findingsFor(report);
    return report;
  }

  // ─── the score ──────────────────────────────────────────────────────

  /**
   * One legible number for how the period went, and the arithmetic behind it.
   *
   * Fruit weighs four inputs: Against target (0.35), Deliberate (0.25), Depth
   * (0.25) and Accounted (0.15). **Deliberate cannot be computed here** —
   * Super Productivity has no focus sessions, so there is no "time inside a
   * block you plotted in advance" to measure. Dropping it and leaving the other
   * three at their Fruit weights would cap the score at 75 and quietly relabel
   * a good week as a mediocre one, so the surviving three are rescaled by
   * 1 / 0.75 to sum to one again. The panel prints the rescaled weights and
   * says which component is missing, because a score you cannot audit is a
   * score you do not believe.
   *
   * Nothing recorded means **not scored**, not zero. A 0/100 for a week you
   * were on holiday is a verdict this report has no business delivering.
   */
  const DROPPED_WEIGHT = 0.25; // "Deliberate", in Fruit
  const RESCALE = 1 / (1 - DROPPED_WEIGHT);

  function scoreFor(days, dailyTargetSec, byTag, totalWorkSec) {
    if (totalWorkSec === 0) {
      return { value: 0, rating: 'Not scored', scored: false, components: [] };
    }
    const worked = days.filter((d) => d.workSec > 0);

    // 1. Did the work happen at all, against the target you set? With no target
    //    there is nothing to be behind, so this scores on consistency instead —
    //    a steady week beats one heroic Thursday.
    let targetValue;
    let targetDetail;
    if (dailyTargetSec !== null) {
      const applicable = worked.filter((d) => d.targetApplies);
      if (!applicable.length) {
        targetValue = 100;
        targetDetail = 'No day in this range had a target';
      } else {
        const met = applicable.filter((d) => d.workSec >= dailyTargetSec).length;
        targetValue = pct01(met / applicable.length);
        targetDetail = `${met} of ${applicable.length} days hit the target`;
      }
    } else {
      const mean = totalWorkSec / worked.length;
      const spread =
        worked.reduce((a, d) => a + Math.abs(d.workSec - mean), 0) / worked.length;
      targetValue = pct01(1 - spread / Math.max(mean, 1));
      targetDetail = 'No target set — scored on how even the days were';
    }

    // 2. How fragmented the work was, as the length of an average working day.
    //    Short scattered days score lower than long continuous ones, up to a
    //    ceiling: past about six hours the extra is not concentration.
    const dayLength = idiv(totalWorkSec, Math.max(worked.length, 1));
    const depthValue = pct01(dayLength / (6 * 3600));

    // 3. How much of it you can actually account for by kind. This is the only
    //    component you fix by *labelling* rather than by working differently.
    const named = byTag.filter((s) => s.id !== null).reduce((a, s) => a + s.seconds, 0);
    const namedValue = pct01(named / totalWorkSec);

    const components = [
      {
        label: 'Against target',
        value: targetValue,
        weight: 0.35 * RESCALE,
        detail: targetDetail,
      },
      {
        label: 'Depth',
        value: depthValue,
        weight: 0.25 * RESCALE,
        detail: `${idiv(dayLength, 3600)}h ${idiv(dayLength % 3600, 60)}m of work on an average working day`,
      },
      {
        label: 'Accounted',
        value: namedValue,
        weight: 0.15 * RESCALE,
        detail: `${namedValue}% of it carries a tag`,
      },
    ];

    const value = Math.round(components.reduce((a, c) => a + c.value * c.weight, 0));
    const rating =
      value <= 39 ? 'Low' : value <= 59 ? 'Fair' : value <= 79 ? 'Good' : 'Very good';
    return { value, rating, scored: true, components };
  }

  // ─── findings ───────────────────────────────────────────────────────

  /**
   * The report, read back to you in sentences.
   *
   * Each finding names the figure it came from, so it can be checked against
   * the panel beside it rather than taken on trust. They state facts and stop:
   * this report does not tell you what to do about your own week, because it
   * does not know what your week was for.
   *
   * Nothing is emitted from a quantity that was not measured. A period with no
   * baseline gets no comparison findings at all rather than a comparison
   * against zero, which is the failure mode that makes generated prose
   * worthless — and the same rule is why Fruit's `late-work`, `early-work`,
   * `span-density` and `no-focus` findings are absent here rather than
   * approximated. The last finding says so out loud.
   */
  function findingsFor(r) {
    const out = [];
    const worked = r.days.filter((d) => d.workSec > 0);
    if (!worked.length) return out;
    const periodWord = r.period === 'day' ? 'day' : r.period === 'month' ? 'month' : 'week';

    // ── against the baseline ──────────────────────────────────────────
    if (r.baseline && r.baseline.totalWorkSec > 0) {
      const diff = r.totalWorkSec - r.baseline.totalWorkSec;
      const pct = Math.round((diff / r.baseline.totalWorkSec) * 100);
      // Under a tenth either way is noise, and calling it a trend would teach
      // you to ignore the ones that are not.
      if (Math.abs(pct) >= 10) {
        out.push({
          id: 'total-vs-baseline',
          headline: `You worked ${hm(Math.abs(diff))} ${diff > 0 ? 'more' : 'less'} than your typical ${periodWord}`,
          detail: `${hm(r.totalWorkSec)} this ${periodWord} against an average of ${hm(r.baseline.totalWorkSec)} over the last ${r.baseline.periods === 1 ? 'period' : `${r.baseline.periods} periods`}. That is ${diff > 0 ? '+' : '−'}${pct}%.`,
          tone: 'neutral',
        });
      }
    }

    // ── against the target ────────────────────────────────────────────
    if (r.targetSec !== null && r.targetDaysApplicable > 0) {
      const missed = r.targetDaysApplicable - r.targetDaysMet;
      out.push({
        id: 'target',
        headline:
          missed === 0
            ? `Every day hit the ${hm(r.targetSec)} target`
            : `${missed} of ${r.targetDaysApplicable} days fell short of the ${hm(r.targetSec)} target`,
        detail: `Counted over the ${r.targetDaysApplicable} ${r.targetDaysApplicable === 1 ? 'day' : 'days'} the target applied to and that have already happened. Days still to come are not counted as missed.`,
        tone: missed === 0 ? 'good' : 'neutral',
      });
    }

    // ── the longest day ───────────────────────────────────────────────
    const longest = worked.reduce((a, d) => (d.workSec > a.workSec ? d : a), worked[0]);
    const mean = idiv(r.totalWorkSec, worked.length);
    // Only worth saying when one day genuinely dominates. Half again as long as
    // the average, and more than an hour clear of it.
    if (worked.length > 1 && longest.workSec > mean * 1.5 && longest.workSec - mean > 3600) {
      out.push({
        id: 'longest-day',
        // The weekday, not the ISO date. "Sunday carried 46%" is a sentence;
        // "2026-08-16 carried 46%" is a log line.
        headline: `${weekdayName(longest.date)} carried ${Math.round((longest.workSec / r.totalWorkSec) * 100)}% of the ${periodWord}'s work`,
        detail: `${hm(longest.workSec)} on that day against an average of ${hm(mean)} across the ${worked.length} days worked.`,
        tone: 'watch',
      });
    }

    // ── what it cannot tell you, and why ──────────────────────────────
    //
    // The most useful finding a report can offer is often the one about its own
    // blind spot. An untagged week means every split on this screen is one grey
    // bar, and saying so is more use than drawing it.
    const named = r.byTag.filter((s) => s.id !== null).reduce((a, s) => a + s.seconds, 0);
    if (r.totalWorkSec > 0 && named * 2 < r.totalWorkSec) {
      out.push({
        id: 'untagged',
        headline: `${Math.round(((r.totalWorkSec - named) / r.totalWorkSec) * 100)}% of the work has no kind on it`,
        detail:
          'The split by kind of work cannot say anything useful until tasks carry a tag. This report reads a task\'s first tag as its kind of work.',
        tone: 'watch',
      });
    }

    out.push({
      id: 'not-measured',
      headline: 'Four of Fruit\'s readings are not on this screen',
      detail:
        'Super Productivity records how long you spent on a task on a given day, not when in the day it happened. The hour histogram, the working-day span, focus sessions and the apps panel all need session start and end times, so they are absent here rather than estimated from daily totals.',
      tone: 'neutral',
    });

    return out;
  }

  // ─── calibration ────────────────────────────────────────────────────

  function bucketFor(estimateSec) {
    if (estimateSec <= 900) return '<=15m';
    if (estimateSec <= 1800) return '30m';
    if (estimateSec <= 3600) return '1h';
    if (estimateSec <= 7200) return '2h';
    return '3h+';
  }

  /**
   * Stated in plain language: *"You run 1.24× your estimates. Your 3h estimates
   * are the least accurate."*
   */
  function calibrationHeadline(med, n, worst) {
    if (med === null) {
      return 'Come back after five tracked tasks — calibration needs a few estimates to compare.';
    }
    if (n < MIN_SAMPLES) return `${n} of 5 estimates tracked so far. Calibration starts at five.`;
    let s;
    if (med >= 1.05) s = `You run ${med.toFixed(2)}× your estimates.`;
    else if (med <= 0.95) s = `You come in at ${med.toFixed(2)}× your estimates — you over-estimate.`;
    else s = `You land within 5% of your estimates (${med.toFixed(2)}×).`;
    if (worst) s += ` Your ${worst} estimates are the least accurate.`;
    return s;
  }

  /**
   * Trailing 30-day `tracked ÷ estimate`, bucketed by estimate size.
   *
   * A direct port: Fruit windows on `completed_at` and divides the task's whole
   * tracked total by its estimate, which is exactly `doneOn` and `timeSpent`
   * here. This is the one panel that comes across with nothing lost.
   */
  function buildCalibration(model, projectId, now) {
    const today = parseDate(todayLocal(now));
    const since = formatDate(addDays(today, -CALIBRATION_WINDOW_DAYS));

    const byBucket = new Map(BUCKET_ORDER.map((b) => [b, []]));
    const all = [];
    for (const t of model.tasks) {
      if (!t.isDone || !t.doneOn || t.doneOn < since) continue;
      if (t.estimateSec <= 0 || t.trackedSec <= 0) continue;
      if (projectId && t.projectId !== projectId) continue;
      const ratio = t.trackedSec / t.estimateSec;
      byBucket.get(bucketFor(t.estimateSec)).push(ratio);
      all.push(ratio);
    }

    const buckets = BUCKET_ORDER.map((name) => {
      const ratios = byBucket.get(name);
      return {
        bucket: name,
        n: ratios.length,
        medianRatio: median(ratios) || 0,
        isReportable: ratios.length >= MIN_SAMPLES,
      };
    });

    const reportable = buckets.filter((b) => b.isReportable);
    let worst = null;
    for (const b of reportable) {
      if (worst === null || Math.abs(b.medianRatio - 1) > Math.abs(worst.medianRatio - 1)) worst = b;
    }
    const overall = median(all);

    return {
      headline: calibrationHeadline(overall, all.length, worst && worst.bucket),
      worstBucket: worst && worst.bucket,
      overallMedian: overall,
      sampleCount: all.length,
      buckets,
    };
  }

  // ─── estimated vs actual, by project and week ───────────────────────

  /**
   * Fruit's "planned vs tracked" panel compares a week's *plotted* block time
   * against the time tracked in it. Super Productivity has no scheduled blocks,
   * so there is no plot to compare against — and inventing one from due dates
   * would be a made-up number sitting in the panel whose whole job is honesty.
   *
   * What survives the change is the like-for-like: over the tasks **completed**
   * in a week, what you said they would take against what they took. Both sides
   * describe the same set of tasks, so the drift is a real quantity rather than
   * two different populations subtracted from each other — and it is the same
   * `drift_sec = tracked − planned` column Fruit's CSV carries.
   */
  function buildProjectWeeks(model, from, to, projectId) {
    const fromS = formatDate(from);
    const toS = formatDate(to);
    const rows = new Map();

    for (const t of model.tasks) {
      if (!t.isDone || !t.doneOn) continue;
      if (t.doneOn < fromS || t.doneOn > toS) continue;
      if (t.estimateSec <= 0 && t.trackedSec <= 0) continue;
      if (projectId && t.projectId !== projectId) continue;

      const ws = formatDate(weekStart(parseDate(t.doneOn)));
      const key = `${t.projectId || ''}|${ws}`;
      let row = rows.get(key);
      if (!row) {
        const p = t.projectId ? model.projects.get(t.projectId) : null;
        row = {
          projectId: t.projectId,
          projectName: p ? p.name : 'No project',
          projectColour: p ? p.colour : NO_PROJECT,
          weekStart: ws,
          plannedSec: 0,
          trackedSec: 0,
          tasks: 0,
        };
        rows.set(key, row);
      }
      row.plannedSec += t.estimateSec;
      row.trackedSec += t.trackedSec;
      row.tasks += 1;
    }

    const out = [...rows.values()];
    out.sort((a, b) => a.weekStart.localeCompare(b.weekStart) || a.projectName.localeCompare(b.projectName));
    return out;
  }

  // ─── weekly targets ─────────────────────────────────────────────────

  /**
   * Target vs actual per project, with pace-to-date rather than a bare total.
   * A direct port — the only difference is where the target is stored, since
   * Super Productivity's projects have no such field.
   */
  function buildWeeklyTargets(model, settings, now) {
    const s = withDefaults(settings);
    const today = parseDate(todayLocal(now));
    const ws = weekStart(today);
    const daysElapsed = Math.round((today.getTime() - ws.getTime()) / 86400000) + 1;
    const dates = new Set(dateSeries(ws, addDays(ws, 6)));

    const out = [];
    for (const p of model.projects.values()) {
      const target = s.weeklyTargets[p.id];
      if (!target || target <= 0 || p.isArchived) continue;
      let tracked = 0;
      for (const t of model.tasks) {
        if (t.projectId !== p.id) continue;
        for (const date of Object.keys(t.byDate)) if (dates.has(date)) tracked += t.byDate[date];
      }
      out.push({
        projectId: p.id,
        projectName: p.name,
        projectColour: p.colour,
        targetSec: target,
        trackedSec: tracked,
        paceSec: idiv(target * daysElapsed, 7),
      });
    }
    out.sort((a, b) => a.projectName.localeCompare(b.projectName));
    return out;
  }

  // ─── export ─────────────────────────────────────────────────────────

  function csvEscape(s) {
    return /[",\n]/.test(s) ? `"${String(s).replace(/"/g, '""')}"` : String(s);
  }

  /** The rows behind the report, in Fruit's `report_csv` column order. */
  function reportCsv(rows) {
    let out = 'week_start,project,planned_sec,tracked_sec,drift_sec\n';
    for (const r of rows) {
      out += `${r.weekStart},${csvEscape(r.projectName)},${r.plannedSec},${r.trackedSec},${r.trackedSec - r.plannedSec}\n`;
    }
    return out;
  }

  return {
    MIN_SAMPLES,
    CALIBRATION_WINDOW_DAYS,
    BUCKET_ORDER,
    ALL_DAYS,
    parseDate,
    formatDate,
    addDays,
    weekStart,
    todayLocal,
    periodBounds,
    shiftPeriod,
    dateSeries,
    weekdayName,
    weekdayApplies,
    duration,
    hm,
    drift,
    median,
    shareOf,
    normalise,
    defaultSettings,
    withDefaults,
    buildWorkReport,
    buildCalibration,
    buildProjectWeeks,
    buildWeeklyTargets,
    reportCsv,
  };
});
