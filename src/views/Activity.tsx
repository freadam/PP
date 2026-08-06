/**
 * Activity (§3.5, P2) — "was I actually doing the thing the timer said?"
 *
 * Five panels, and the order is the argument:
 *
 *   1. **Where the day went, by label.** Work · Study · Distraction · Life, in
 *      the words the user chose. First because it is the only panel that answers
 *      the question in one line; everything below it is the working.
 *   2. **What has no label yet**, ranked by time, each row one click from having
 *      one. An empty taxonomy and a text field is a chore — "these three things
 *      took eleven hours between them" is a question people answer.
 *   3. **Against the plan.** Each block on the day, with the apps that were
 *      actually in front of you while it ran. This is the only place in Fruit
 *      where an intention meets an *observation* rather than another record of
 *      itself, and it is the reason the feature exists at all.
 *   4. **Per-app totals**, longest first.
 *   5. **The day itself**, on the Planner's exact time axis and hour height, so
 *      a block and the app usage beneath it read as one picture rather than as
 *      two charts that happen to be about the same hours. Every interval here is
 *      relabellable on the spot — the YouTube-lecture case.
 *
 * Off by default. When it is off this screen says so and links to the switch —
 * it never shows an empty chart that implies the data is merely missing.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useApp } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type {
  ActivityDay,
  ActivitySpanRow,
  AppTotal,
  ObservationCategory,
  UnlabelledRow,
} from "../lib/types";
import { Empty } from "../components/chrome";

/**
 * Which of the eight `--app-*` tokens an application gets (§5.2).
 *
 * A stable hash rather than order-of-appearance, so an app keeps its colour
 * from one day to the next — a legend you have to re-learn every morning is
 * worse than no colour at all.
 */
const APP_RAMP = 8;

function appColour(appId: string): string {
  let hash = 0;
  for (let i = 0; i < appId.length; i++) hash = (hash * 31 + appId.charCodeAt(i)) | 0;
  return `var(--app-${(Math.abs(hash) % APP_RAMP) + 1})`;
}

/** `code.exe` and `Code.app` are the same thing to a human reading a total. */
function appLabel(appId: string): string {
  return appId.replace(/\.(exe|app)$/i, "");
}

export function Activity() {
  const status = useApp((s) => s.activityStatus);
  const day = useApp((s) => s.activityDay);
  const date = useApp((s) => s.activityDate);
  const setDate = useApp((s) => s.setActivityDate);
  const load = useApp((s) => s.loadActivity);
  const go = useApp((s) => s.go);
  const hourHeight = useApp((s) => s.hourHeight);

  const [cats, setCats] = useState<ObservationCategory[]>([]);
  const [unlabelled, setUnlabelled] = useState<UnlabelledRow[]>([]);

  const reload = useCallback(async () => {
    await load();
    const [c, u] = await Promise.all([
      ipc.getCategories(date, date, fmt.tz()).catch(() => []),
      ipc.getUnlabelled(date, date, fmt.tz()).catch(() => []),
    ]);
    setCats(c);
    setUnlabelled(u);
  }, [load, date]);

  useEffect(() => {
    void reload();
  }, [reload]);

  if (!status) return <Empty>Loading Activity…</Empty>;

  // The platform's own sentence, never a bare "unavailable" (§3.10).
  if (status.support !== "full") {
    return <Empty>{status.supportNote}</Empty>;
  }

  if (!status.settings.enabled) {
    return (
      <Empty>
        Activity is off. It samples which application is in front of you every 20 seconds,
        stores it in the same local database as everything else, and never sends it anywhere.{" "}
        <button className="btn" onClick={() => go("settings")} style={{ marginLeft: 6 }}>
          Turn it on in Settings
        </button>
      </Empty>
    );
  }

  return (
    <div className="view-pad scroll-y">
      <div className="row" style={{ marginBottom: 12 }}>
        <button className="btn" aria-label="Previous day" onClick={() => setDate(fmt.addDays(date, -1))}>
          ‹
        </button>
        <button className="btn" aria-label="Next day" onClick={() => setDate(fmt.addDays(date, 1))}>
          ›
        </button>
        <strong className="display" style={{ fontSize: "1rem" }}>
          {fmt.longDate(date)}
        </strong>
        <span className="grow" />
        {status.settings.paused && (
          <span className="micro" style={{ color: "var(--track)" }}>
            Paused — nothing is being recorded
          </span>
        )}
        <span className="data caption">
          {day ? fmt.duration(day.trackedSec) : "0m"} observed
        </span>
        <button className="btn" onClick={() => setDate(fmt.today())}>
          Today
        </button>
      </div>

      {!day || day.spans.length === 0 ? (
        <Empty>
          Nothing recorded on this day. Sampling only runs while Fruit is open and the machine
          is awake, so a day you spent elsewhere is genuinely empty rather than lost.
        </Empty>
      ) : (
        <>
          <ByCategory cats={cats} />
          <Unlabelled
            rows={unlabelled}
            cats={cats}
            titlesOn={status.settings.titlesEnabled}
            onDone={reload}
          />
          <Correlations day={day} />
          <ByApp totals={day.byApp} trackedSec={day.trackedSec} />
          <DayTimeline day={day} hourHeight={hourHeight} cats={cats} onDone={reload} />
        </>
      )}
    </div>
  );
}

/**
 * §2.3 CALIBRATE — the plotted block against what was actually in front of you.
 *
 * The line "you were in Slack for the hour you plotted for the refactor" is the
 * whole payoff, so it is the first thing on the screen, not a footnote under a
 * chart.
 */
function Correlations({ day }: { day: ActivityDay }) {
  if (day.correlations.length === 0) {
    return (
      <section className="panel">
        <h2>Against the plan</h2>
        <p className="caption">
          Nothing was plotted on this day, so there is no intention to compare against. Plot a
          block in the Planner and this panel fills itself in.
        </p>
      </section>
    );
  }

  return (
    <section className="panel">
      <h2>Against the plan</h2>
      <div className="stack">
        {day.correlations.map((c) => {
          const total = c.topApps.reduce((sum, a) => sum + a.seconds, 0);
          const top = c.topApps[0];
          return (
            <div key={c.blockId} className="stack" style={{ gap: 4 }}>
              <div className="row">
                <span className="grow" style={{ minWidth: 0 }}>
                  {c.title}
                </span>
                <span className="data micro" style={{ color: "var(--muted)" }}>
                  {fmt.clockRange(c.startsAt, c.durationSec)}
                </span>
              </div>
              {/* One stacked bar per block: proportions, not a legend to decode. */}
              <span
                className="app-bar"
                role="img"
                aria-label={c.topApps
                  .map((a) => `${appLabel(a.appId)} ${fmt.duration(a.seconds)}`)
                  .join(", ")}
              >
                {c.topApps.map((a) => (
                  <i
                    key={a.appId}
                    style={{
                      width: `${(a.seconds / Math.max(1, total)) * 100}%`,
                      background: appColour(a.appId),
                    }}
                    title={`${appLabel(a.appId)} · ${fmt.duration(a.seconds)}`}
                  />
                ))}
              </span>
              {top && (
                <span className="caption">
                  Mostly <strong>{appLabel(top.appId)}</strong> ·{" "}
                  {fmt.duration(top.seconds)} of {fmt.duration(c.durationSec)} plotted
                </span>
              )}
            </div>
          );
        })}
      </div>
    </section>
  );
}

function ByApp({ totals, trackedSec }: { totals: AppTotal[]; trackedSec: number }) {
  const max = Math.max(1, ...totals.map((t) => t.seconds));
  return (
    <section className="panel">
      <h2>Where the day went</h2>
      <div className="stack" style={{ gap: 6 }}>
        {totals.slice(0, 12).map((t) => (
          <div key={t.appId} className="row" style={{ gap: 8 }}>
            <span style={{ width: 160, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis" }}>
              {appLabel(t.appId)}
            </span>
            {/* Decorative: the app name, the duration and the share are all
                already text on this row, so a label here would make a screen
                reader say each of them twice (I3). */}
            <span className="app-bar grow" aria-hidden="true">
              <i style={{ width: `${(t.seconds / max) * 100}%`, background: appColour(t.appId) }} />
            </span>
            <span className="data micro" style={{ width: 64, textAlign: "right" }}>
              {fmt.duration(t.seconds)}
            </span>
            <span className="micro" style={{ width: 40, textAlign: "right", color: "var(--muted)" }}>
              {Math.round((t.seconds / Math.max(1, trackedSec)) * 100)}%
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}

/**
 * The same 24-hour column as the Planner, at the same hour height, so the two
 * screens can be compared by looking rather than by reading numbers off both.
 */
function DayTimeline({
  day,
  hourHeight,
  cats,
  onDone,
}: {
  day: ActivityDay;
  hourHeight: number;
  cats: ObservationCategory[];
  onDone: () => void;
}) {
  const [picking, setPicking] = useState<ActivitySpanRow | null>(null);
  const dayStart = useMemo(() => new Date(`${day.localDate}T00:00:00`).getTime(), [day.localDate]);
  const scrollRef = useRef<HTMLDivElement>(null);

  const firstMin = useMemo(() => {
    const first = day.spans[0];
    return first ? (first.startedAt - dayStart) / 60_000 : 8 * 60;
  }, [day.spans, dayStart]);

  /* Open on the first thing that happened, not on midnight. A 24-hour column
     is right — night work is real — but scrolled to 00:00 it is eight screens
     of nothing before the day starts. */
  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = Math.max(0, (firstMin / 60) * hourHeight - hourHeight / 2);
  }, [firstMin, hourHeight]);

  return (
    <section className="panel">
      <h2>The day</h2>
      <p className="caption">
        Click any interval to label it. Labelling one interval never changes the rule — a
        YouTube lecture stays a lecture without making every future video one.
      </p>
      {picking && (
        <SpanLabeller
          span={picking}
          cats={cats}
          onClose={() => setPicking(null)}
          onDone={onDone}
        />
      )}
      <div className="activity-timeline" ref={scrollRef}>
        <div className="activity-canvas" style={{ height: 24 * hourHeight }}>
          {Array.from({ length: 24 }, (_, h) => (
            <div key={h} className="hour-row" style={{ height: hourHeight, top: h * hourHeight }}>
              <span className="hour-label micro" aria-hidden="true">
                {String(h).padStart(2, "0")}
              </span>
            </div>
          ))}
          <div className="activity-lane">
            {day.spans.map((s) => {
              const top = ((s.startedAt - dayStart) / 3_600_000) * hourHeight;
              const height = Math.max(2, ((s.endedAt - s.startedAt) / 3_600_000) * hourHeight);
              const cat = cats.find((c) => c.id === s.categoryId);
              // Named after the site where there is one. `chrome.exe` is not an
              // answer to "what was I doing", and that is the whole point of the
              // connector.
              const name = s.domain ?? appLabel(s.appId);
              return (
                <button
                  key={s.id}
                  className="activity-span"
                  style={{
                    top,
                    height,
                    background: cat ? cat.colour : appColour(s.appId),
                  }}
                  title={`${name}${s.windowTitle ? ` — ${s.windowTitle}` : ""}${
                    cat ? ` · ${cat.name}` : " · no label yet"
                  } · ${fmt.clock(s.startedAt)}–${fmt.clock(s.endedAt)} — click to label`}
                  onClick={() => setPicking(s)}
                >
                  {height >= 14 && (
                    <span className="micro">
                      {name}
                      {/* The window title before the label: which video it was
                          is the thing you need in order to decide. */}
                      {s.windowTitle && height >= 28 && <> — {s.windowTitle}</>}
                      {cat && height >= 42 && <> · {cat.name}</>}
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        </div>
      </div>
    </section>
  );
}

/**
 * Where the day went, in the user's own words.
 *
 * First panel on the screen because it is the only one that answers the
 * question in a line. Zero-second categories are dropped: a label you have not
 * used today is not news, and five empty rows push the two that matter off the
 * fold.
 */
function ByCategory({ cats }: { cats: ObservationCategory[] }) {
  const used = cats.filter((c) => c.seconds > 0);
  const total = used.reduce((n, c) => n + c.seconds, 0);
  if (used.length === 0) return null;

  return (
    <section className="panel">
      <h2>By label</h2>
      <div className="stack" style={{ gap: 6 }}>
        {used.map((c) => (
          <div key={c.id} className="row">
            <span className="swatch" style={{ background: c.colour }} aria-hidden="true" />
            <span style={{ width: 120 }}>{c.name}</span>
            <span className="bar grow" aria-hidden="true">
              <i style={{ width: `${(c.seconds / total) * 100}%`, background: c.colour }} />
            </span>
            <span className="data caption">{fmt.duration(c.seconds)}</span>
          </div>
        ))}
      </div>
      <p className="caption">
        Observed time only — what the machine saw, not what you confirmed. The Day view is
        where the two are reconciled.
      </p>
    </section>
  );
}

/**
 * What has no label yet, ranked by time.
 *
 * The surface this whole feature is usable from. Presenting an empty taxonomy
 * and asking someone to populate it is a chore nobody finishes; naming the four
 * things that actually ate the day is a question people answer in ten seconds.
 *
 * Labelling here writes a **rule**, so it applies from now on. Past intervals
 * keep what they were given — a rule that reached backwards would rewrite days
 * already reconciled.
 */
function Unlabelled({
  rows,
  cats,
  titlesOn,
  onDone,
}: {
  rows: UnlabelledRow[];
  cats: ObservationCategory[];
  titlesOn: boolean;
  onDone: () => void;
}) {
  const run = useApp((s) => s.run);
  const toast = useApp((s) => s.toast);
  const [open, setOpen] = useState<string | null>(null);
  if (rows.length === 0) return null;

  /** One interval, no rule — the YouTube-lecture case. */
  const labelOne = async (spanId: number, categoryId: string) => {
    const ok = await run(
      () => ipc.setSpanCategory(spanId, categoryId),
      "Couldn't label that interval.",
    );
    if (ok !== null) onDone();
  };

  const label = async (row: UnlabelledRow, categoryId: string) => {
    const ok = await run(
      () => ipc.setActivityRule(row.matchKind, row.matchValue, categoryId),
      "Couldn't save that label.",
    );
    if (ok) {
      const name = cats.find((c) => c.id === categoryId)?.name ?? "labelled";
      toast(`${row.matchValue} → ${name}. Applies from now on.`);
      onDone();
    }
  };

  return (
    <section className="panel">
      <h2>Not labelled yet</h2>
      <p className="caption">
        Ranked by how much of the day each took. Labelling one writes a rule, so it applies
        from now on — today&rsquo;s record keeps what it was given.
      </p>
      <div className="stack" style={{ gap: 8 }}>
        {rows.map((row) => {
          const key = `${row.matchKind}:${row.matchValue}`;
          const isOpen = open === key || row.occurrences === 1;
          return (
            <div key={key} className="stack" style={{ gap: 4 }}>
              <div className="row">
                <span className="tag micro">{row.matchKind === "domain" ? "site" : "app"}</span>
                <span className="grow">
                  {row.matchValue}{" "}
                  {row.occurrences > 1 && (
                    <button
                      className="btn micro"
                      aria-expanded={isOpen}
                      onClick={() => setOpen(isOpen ? null : key)}
                    >
                      {/* One 4-hour stretch and forty 6-minute ones are
                          different problems wearing the same total, so the
                          count is a control rather than a caption. */}
                      {isOpen ? "▾" : "▸"} {row.occurrences} stretches
                    </button>
                  )}
                </span>
                <span className="data caption" style={{ width: 64, textAlign: "right" }}>
                  {fmt.duration(row.seconds)}
                </span>
                <CategoryPicker cats={cats} onPick={(id) => void label(row, id)} />
              </div>

              {isOpen && (
                <div className="stack" style={{ gap: 4, paddingLeft: 24 }}>
                  {row.occurrences > 1 && (
                    <p className="caption">
                      The buttons above make a <b>rule</b> for every future visit. The ones
                      below label <b>that visit only</b>.
                    </p>
                  )}
                  {row.stretches.map((st) => (
                    <div key={st.id} className="row">
                      <span className="data micro" style={{ width: 92 }}>
                        {fmt.clock(st.startedAt)}–{fmt.clock(st.endedAt)}
                      </span>
                      <span className="grow micro">
                        {/* The window title is what tells a lecture from a music
                            video. It arrives only when titles are switched on,
                            so its absence gets an explanation rather than a
                            blank. */}
                        {st.windowTitle ?? (
                          <span className="caption">no window title recorded</span>
                        )}
                      </span>
                      <span className="data caption" style={{ width: 56, textAlign: "right" }}>
                        {fmt.duration(st.seconds)}
                      </span>
                      <CategoryPicker
                        cats={cats}
                        current={st.categoryId}
                        onPick={(id) => void labelOne(st.id, id)}
                      />
                    </div>
                  ))}
                  {row.stretches.length < row.occurrences && (
                    <p className="caption">
                      Showing the {row.stretches.length} longest of {row.occurrences}. Past
                      that, a rule is the better tool.
                    </p>
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
      {!titlesOn && (
        <p className="caption">
          Window titles are off, so a stretch cannot say <i>which</i> video or document it
          was — only which site. Settings → Activity → <b>Track window titles</b> turns them
          on. It is a separate switch because a title is the document name, the customer,
          the ticket.
        </p>
      )}
    </section>
  );
}

/**
 * The label buttons, as buttons rather than a `<select>`.
 *
 * Four or five labels is exactly the range where a row of buttons is one click
 * and a dropdown is three. It also puts the colours on screen, which is what
 * makes the timeline below readable without a legend.
 */
function CategoryPicker({
  cats,
  onPick,
  current,
}: {
  cats: ObservationCategory[];
  onPick: (id: string) => void;
  current?: string | null;
}) {
  return (
    <div className="row" style={{ gap: 4 }}>
      {cats.map((c) => (
        <button
          key={c.id}
          className="btn micro"
          aria-pressed={current === c.id || undefined}
          title={`Label as ${c.name}`}
          onClick={() => onPick(c.id)}
          style={{ borderColor: c.colour }}
        >
          {c.name}
        </button>
      ))}
    </div>
  );
}

/**
 * Relabelling one interval — the case the feature turns on.
 *
 * YouTube defaults to Distraction and *this* video was a lecture. Fruit sees a
 * registrable domain and deliberately never the URL or the page title, so it
 * cannot tell the difference and must not pretend to. The person who watched it
 * says so, for that interval alone, and the rule is left as it was.
 */
function SpanLabeller({
  span,
  cats,
  onClose,
  onDone,
}: {
  span: ActivitySpanRow;
  cats: ObservationCategory[];
  onClose: () => void;
  onDone: () => void;
}) {
  const run = useApp((s) => s.run);
  const name = span.domain ?? appLabel(span.appId);

  const apply = async (categoryId: string | null) => {
    const ok = await run(
      () => ipc.setSpanCategory(span.id, categoryId),
      "Couldn't relabel that interval.",
    );
    onClose();
    if (ok !== null) onDone();
  };

  return (
    <div className="row" style={{ gap: 8, flexWrap: "wrap", marginBottom: 8 }}>
      <strong>
        {name}{" "}
        <span className="data caption">
          {fmt.clock(span.startedAt)}–{fmt.clock(span.endedAt)}
        </span>
        {span.windowTitle && <span className="caption"> — {span.windowTitle}</span>}
      </strong>
      <CategoryPicker cats={cats} current={span.categoryId} onPick={(id) => void apply(id)} />
      <button className="btn micro" onClick={() => void apply(null)}>
        Clear
      </button>
      <button className="btn micro" onClick={onClose}>
        Cancel
      </button>
      <span className="caption" style={{ flexBasis: "100%" }}>
        This interval only. To change it for every future visit, use the rule under
        &ldquo;Not labelled yet&rdquo; or Settings → Activity.
      </span>
    </div>
  );
}
