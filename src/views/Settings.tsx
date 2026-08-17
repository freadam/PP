/**
 * Settings (§3.8).
 *
 * Every control here does something, and nothing points at a screen that has no
 * switch on it. Where a platform can't do a thing, the reason is printed next
 * to the control rather than left as "unavailable".
 */

import { useEffect, useState } from "react";
import { useApp } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type {
  ActivityRule,
  ConnectorStatus,
  IntegrityReport,
  MatchKind,
  GoalRow,
  ObservationCategory,
  ResetSummary,
  TaskCategoryRow,
} from "../lib/types";

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="panel">
      <h2>{title}</h2>
      <div className="stack">{children}</div>
    </section>
  );
}

/**
 * A switch that reads as on or off without colour alone (§5.9, U11): the state
 * is in `aria-checked`, in the knob's position, and in the word next to it.
 */
function Switch({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (next: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      className="switch"
      onClick={() => onChange(!checked)}
    >
      <i />
      <span className="micro">{checked ? "On" : "Off"}</span>
    </button>
  );
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="row" style={{ alignItems: "flex-start" }}>
      <span className="label" style={{ width: 200, color: "var(--muted)", paddingTop: 4 }}>
        {label}
      </span>
      <div className="stack grow">
        {children}
        {hint && <span className="caption">{hint}</span>}
      </div>
    </div>
  );
}

/**
 * Launch at login.
 *
 * Coverage, not convenience. Observation only records while Fruit is running,
 * so a morning that starts with "I forgot to open it" leaves a hole no amount
 * of reconciling can honestly fill — and 90%-of-waking-hours, the number the
 * whole product is measured by, is unreachable without this.
 *
 * A login launch starts to the tray. A tracker that puts a window on top of
 * whatever you opened your machine to do is a tracker whose autostart gets
 * turned off within a week.
 */
function StartAtLogin() {
  const run = useApp((s) => s.run);
  const [enabled, setEnabled] = useState<boolean | null>(null);

  useEffect(() => {
    void ipc
      .getAutostart()
      .then(setEnabled)
      .catch(() => setEnabled(null));
  }, []);

  return (
    <Field
      label="Start with Windows"
      hint="Fruit opens to the tray when you log in and begins observing straight away — no window, no focus stolen. This is what makes a day's record complete: nothing is recorded while the app is closed, and a morning you forgot to open it is a gap that cannot be reconciled honestly afterwards."
    >
      {enabled === null ? (
        <span className="caption">
          Not available in browser preview — this writes a real startup entry.
        </span>
      ) : (
        <Switch
          checked={enabled}
          label="Start Fruit when you log in"
          onChange={async (next) => {
            const now = await run(() => ipc.setAutostart(next), "Couldn't change that.");
            if (now !== null) setEnabled(now);
          }}
        />
      )}
    </Field>
  );
}

/** Monday = 1 … Sunday = 64, the bitmask `goal.applies_days` uses. */
const WEEKDAYS = [
  { bit: 1, label: "M" },
  { bit: 2, label: "T" },
  { bit: 4, label: "W" },
  { bit: 8, label: "T" },
  { bit: 16, label: "F" },
  { bit: 32, label: "S" },
  { bit: 64, label: "S" },
];

/**
 * A daily work target — "at least six hours, on a weekday".
 *
 * A goal with `period: "day"`, so it reuses the whole goal machinery rather
 * than becoming a second kind of target with its own progress rules.
 *
 * The day mask is the part that stops this being annoying. A flat seven-day
 * target reports you two days behind every Monday morning because you did not
 * work at the weekend, which teaches you to ignore it. And unlike the weekly
 * goals, a daily one is **not** pro-rated through the day: there is no honest
 * way to spread six hours across the hours of a day, so it expects its whole
 * target by the end and nothing before it.
 */
function DailyWorkTarget() {
  const run = useApp((s) => s.run);
  const toast = useApp((s) => s.toast);
  const [target, setTarget] = useState<GoalRow | null>(null);
  const [hours, setHours] = useState("6");
  const [days, setDays] = useState(0b0011111);
  const [loaded, setLoaded] = useState(false);

  const load = async () => {
    const goals = await ipc.getGoals(false).catch(() => null);
    setLoaded(true);
    if (!goals) return;
    const live = goals.find((g) => g.period === "day" && g.subjectId === "allWork") ?? null;
    setTarget(live);
    if (live) {
      setHours(String(Math.round((live.targetSec / 3600) * 100) / 100));
      setDays(live.appliesDays);
    }
  };
  useEffect(() => {
    void load();
  }, []);

  const save = async () => {
    const value = Number(hours);
    if (!Number.isFinite(value) || value <= 0 || value > 24) {
      toast("A daily target has to be between 0 and 24 hours.", { tone: "danger" });
      return;
    }
    const saved = await run(
      () =>
        ipc.setGoal(
          {
            subjectKind: "metric",
            subjectId: "allWork",
            period: "day",
            direction: "atLeast",
            targetSec: Math.round(value * 3600),
            appliesDays: days,
          },
          fmt.today(),
        ),
      "Couldn't save that target.",
    );
    if (saved) {
      setTarget(saved);
      toast(`Daily work target set to ${fmt.duration(saved.targetSec)}.`);
    }
  };

  return (
    <Field
      label="Daily work target"
      hint="Reports → Work measures each day against this, and the graph draws it as a line across the days it applies to. Only days that have already happened count towards 'met' — a Friday that has not arrived is not a day you failed. It is not pro-rated through the day either: there is no honest way to spread six hours across the hours of a morning, so it expects the whole target by the end and nothing before it."
    >
      <div className="row" style={{ flexWrap: "wrap" }}>
        <input
          type="number"
          min="0.5"
          max="24"
          step="0.5"
          value={hours}
          aria-label="Hours of work a day"
          onChange={(e) => setHours(e.target.value)}
          style={{ width: 80 }}
        />
        <span className="caption">hours a day, on</span>
        <div className="segmented" role="group" aria-label="Days the target applies to">
          {WEEKDAYS.map((d, i) => (
            <button
              key={i}
              className="btn"
              aria-pressed={(days & d.bit) !== 0}
              aria-label={`${["Monday","Tuesday","Wednesday","Thursday","Friday","Saturday","Sunday"][i]}`}
              // At least one day, always: a target that applies to no day is a
              // row in the database and nothing on any screen.
              onClick={() => setDays((v) => (v ^ d.bit) || d.bit)}
            >
              {d.label}
            </button>
          ))}
        </div>
        <button className="btn btn-primary" onClick={() => void save()}>
          {target ? "Update" : "Set target"}
        </button>
      </div>
      {loaded && !target && (
        <span className="caption">
          No daily target yet. Reports → Work shows the hours either way; a target adds the line
          and the "3 of 5 days met" reading.
        </span>
      )}
    </Field>
  );
}

/**
 * Kinds of work — "Coding", "Design", "Meeting".
 *
 * One per task, which is the whole reason the split in Reports → Work adds to
 * the total exactly once. Tags stay for slicing; this is for grouping.
 *
 * Deleting frees rather than destroys: the tasks become uncategorised and not
 * one second of recorded time moves. The confirmation says how many tasks it
 * will free, because "delete Meeting" is a different decision when the answer
 * is 2 than when it is 60.
 */
function WorkKinds() {
  const run = useApp((s) => s.run);
  const toast = useApp((s) => s.toast);
  const [cats, setCats] = useState<TaskCategoryRow[]>([]);
  const [name, setName] = useState("");

  const load = async () => {
    const rows = await ipc.getTaskCategories().catch(() => null);
    if (rows) setCats(rows);
  };
  useEffect(() => {
    void load();
  }, []);

  return (
    <Field
      label="Kinds of work"
      hint="What a task is, as opposed to what it is for. One per task, so the split in Reports → Work adds up to the total exactly once — that is the difference between this and a tag. Deleting one frees its tasks rather than deleting them, and no recorded time moves."
    >
      <div className="row" style={{ flexWrap: "wrap", gap: 6 }}>
        {cats.map((c) => (
          <span key={c.id} className="tag" style={{ borderColor: c.colour }}>
            <span className="swatch" style={{ background: c.colour }} aria-hidden="true" />
            {c.name}
            {c.taskCount > 0 && <span className="micro data">{c.taskCount}</span>}
            <button
              className="btn micro"
              aria-label={`Delete the ${c.name} kind of work`}
              onClick={async () => {
                const freed = await run(
                  () => ipc.deleteTaskCategory(c.id),
                  "Couldn't delete that.",
                );
                if (freed === null) return;
                toast(
                  freed > 0
                    ? `Removed ${c.name}. ${freed} ${freed === 1 ? "task is" : "tasks are"} now uncategorised; their time is untouched.`
                    : `Removed ${c.name}.`,
                );
                void load();
              }}
            >
              Remove
            </button>
          </span>
        ))}
      </div>
      <div className="row" style={{ marginTop: 8 }}>
        <input
          value={name}
          placeholder="Another kind of work…"
          aria-label="New kind of work"
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && name.trim() && void add()}
        />
        <button className="btn" disabled={!name.trim()} onClick={() => void add()}>
          Add
        </button>
      </div>
    </Field>
  );

  async function add() {
    const created = await run(() => ipc.createTaskCategory(name.trim()), "Couldn't add that.");
    if (!created) return;
    setName("");
    void load();
  }
}

/**
 * Focus-mode wallpapers.
 *
 * Fruit ships no photographs. It has no licence to redistribute anyone's, and a
 * bundled folder of "beautiful wallpapers" would mean putting other people's
 * work inside a paid binary. So the folder is what ships: created on first ask,
 * with a README in it explaining what to drop there and what reads well behind
 * a clock. Everything in it is yours, read from your disk, with no network call.
 *
 * The folder path is shown rather than hidden, and "Open the folder" is the
 * primary action — the whole feature is "put your own pictures here", so
 * getting there has to be one click rather than a hunt through AppData.
 */
function FocusWallpapers({
  dir,
  onPick,
}: {
  dir: string | undefined;
  onPick: (key: string, value: unknown) => Promise<void>;
}) {
  const run = useApp((s) => s.run);
  const [folder, setFolder] = useState<{ dir: string; count: number } | null>(null);

  const load = async () => {
    const f = await ipc.getWallpapers().catch(() => null);
    if (f) setFolder({ dir: f.dir, count: f.items.length });
  };
  useEffect(() => {
    void load();
    // Re-read when the configured folder changes, so the count below is never
    // the previous folder's.
  }, [dir]);

  return (
    <Field
      label="Wallpapers"
      hint="Focus mode draws one of these behind the clock instead of a phase gradient — press W in Focus, or the Photo button. Fruit ships no pictures of its own: it has no licence to redistribute photographs, so the folder starts empty and what goes in it is yours. jpg, png, webp, avif and gif, up to 16MB each. Nothing is downloaded and nothing leaves this machine."
    >
      <div className="row" style={{ flexWrap: "wrap" }}>
        <button
          className="btn btn-primary"
          onClick={async () => {
            const ok = await run(() => ipc.revealWallpaperDir(), "Couldn't open that folder.");
            if (ok !== null) await load();
          }}
        >
          Open the folder
        </button>
        <button
          className="btn"
          onClick={async () => {
            const picked = await run(() => ipc.pickWallpaperDir(), "Couldn't open the picker.");
            if (!picked) return; // cancelled is not a failure
            await onPick("focus.wallpaperDir", picked);
            await load();
          }}
        >
          Use a different folder…
        </button>
        {dir && (
          <button
            className="btn"
            onClick={async () => {
              await onPick("focus.wallpaperDir", null);
              await load();
            }}
          >
            Back to the default
          </button>
        )}
      </div>
      {folder && (
        <span className="caption">
          {/* Named, not counted-and-hidden. "0 pictures" beside a path is a
              complete answer; "no wallpapers found" is not. */}
          <span className="data">{folder.dir}</span> ·{" "}
          {folder.count === 0
            ? "no pictures yet"
            : `${folder.count} picture${folder.count === 1 ? "" : "s"}`}
        </span>
      )}
    </Field>
  );
}

/**
 * The testing reset — empties the database of everything you ever recorded, so
 * a run can start from nothing.
 *
 * Three deliberate choices:
 *
 * 1. **Type the word.** Not a second click, which is muscle memory after the
 *    third reset and stops being a decision. Typing `erase` takes a second and
 *    cannot happen by accident. This is the only control in the app that asks
 *    for it, because it is the only one with no undo.
 * 2. **No undo toast.** Every other delete in Fruit is undoable and says so.
 *    This one destroys thousands of rows across a dozen tables and the honest
 *    thing is to say it is final and hand back the path to the snapshot the
 *    core took a moment earlier — a real escape hatch, not a fake one.
 * 3. **It says what it will keep**, before you press it. A button labelled
 *    "delete everything" that silently keeps your theme is a button you cannot
 *    reason about.
 */
function ResetAllData() {
  const run = useApp((s) => s.run);
  const toast = useApp((s) => s.toast);
  const refresh = useApp((s) => s.refresh);
  const closeDetail = useApp((s) => s.closeDetail);

  const [arming, setArming] = useState(false);
  const [word, setWord] = useState("");
  const [last, setLast] = useState<ResetSummary | null>(null);
  const ready = word.trim().toLowerCase() === "erase";

  const reset = async () => {
    const summary = await run(() => ipc.resetAllData(), "Couldn't clear the data.");
    if (!summary) return;
    setArming(false);
    setWord("");
    setLast(summary);
    // The open detail panel is showing a task that no longer exists.
    closeDetail();
    await refresh();
    toast(
      `Cleared ${summary.projects} projects, ${summary.tasks} tasks, ${summary.sessions} sessions.`,
    );
  };

  return (
    <>
      <Field
        label="Clear all data"
        hint="Deletes every project, task, note, block, session, life entry, observed span, goal and day review — including anything in Recently deleted. Your settings, and the built-in life areas and observation categories, are kept: they are what the app needs to count against, and no migration will create them again. This cannot be undone from inside Fruit, so a snapshot is written to the backups folder first."
      >
        {!arming ? (
          <div className="row">
            <button className="btn btn-danger" onClick={() => setArming(true)}>
              Clear all data…
            </button>
            {last && (
              <span className="caption">
                Last cleared{" "}
                <span className="data">
                  {last.projects} projects · {last.tasks} tasks · {last.blocks} blocks ·{" "}
                  {last.sessions} sessions · {last.lifeEntries} life entries ·{" "}
                  {last.activitySpans} observed spans
                </span>
              </span>
            )}
          </div>
        ) : (
          <div className="stack">
            <label className="caption" htmlFor="reset-confirm">
              Type <strong>erase</strong> to confirm. There is no undo.
            </label>
            <div className="row">
              <input
                id="reset-confirm"
                value={word}
                autoFocus
                placeholder="erase"
                aria-label="Type erase to confirm"
                onChange={(e) => setWord(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && ready) void reset();
                  if (e.key === "Escape") {
                    setArming(false);
                    setWord("");
                  }
                }}
                style={{ width: 140 }}
              />
              <button
                className="btn"
                onClick={() => {
                  setArming(false);
                  setWord("");
                }}
              >
                Cancel <span className="kbd">Esc</span>
              </button>
              <button className="btn btn-danger" disabled={!ready} onClick={() => void reset()}>
                Erase everything
              </button>
            </div>
          </div>
        )}
      </Field>
      {last?.backupPath && (
        <Field label="Snapshot" hint="Written immediately before the last clear. Restore it with Settings → Data → Import, or by replacing fruit.db while the app is closed.">
          <span className="caption data">{last.backupPath}</span>
        </Field>
      )}
    </>
  );
}

export function Settings() {
  const theme = useApp((s) => s.theme);
  const setTheme = useApp((s) => s.setTheme);
  const hourHeight = useApp((s) => s.hourHeight);
  const setHourHeight = useApp((s) => s.setHourHeight);
  const span = useApp((s) => s.span);
  const setSpan = useApp((s) => s.setSpan);
  const setOverlay = useApp((s) => s.setOverlay);
  const run = useApp((s) => s.run);
  const toast = useApp((s) => s.toast);

  const [settings, setSettings] = useState<Record<string, unknown>>({});
  const [integrity, setIntegrity] = useState<IntegrityReport | null>(null);

  useEffect(() => {
    void ipc.getSettings().then(setSettings).catch(() => {});
  }, []);

  const put = async (key: string, value: unknown) => {
    setSettings((s) => ({ ...s, [key]: value }));
    await run(() => ipc.setSetting(key, value), "Couldn't save that setting.");
  };

  const num = (key: string, fallback: number) => (settings[key] as number) ?? fallback;

  return (
    <div className="view-pad scroll-y">
      <Section title="General">
        <Field label="Theme">
          <div className="row">
            {/* Light first: this design system is light-first, and the
                option list should open on the default rather than on the
                alternative. */}
            {(["light", "dark", "system"] as const).map((t) => (
              <button key={t} className="btn" aria-pressed={theme === t} onClick={() => setTheme(t)}>
                {t}
              </button>
            ))}
          </div>
        </Field>
        <Field label="Clock" hint="Durations always use tabular figures, so nothing jitters.">
          <div className="row">
            {[false, true].map((h12) => (
              <button
                key={String(h12)}
                className="btn"
                aria-pressed={((settings["general.hour12"] as boolean) ?? false) === h12}
                onClick={() => {
                  fmt.setHour12(h12);
                  void put("general.hour12", h12);
                }}
              >
                {h12 ? "12-hour" : "24-hour"}
              </button>
            ))}
          </div>
        </Field>
      </Section>

      <Section title="Planner">
        <Field label="Default span">
          <div className="row">
            {([1, 3, 7] as const).map((n) => (
              <button key={n} className="btn" aria-pressed={span === n} onClick={() => setSpan(n)}>
                {n} day{n > 1 ? "s" : ""}
              </button>
            ))}
          </div>
        </Field>
        <Field label="Hour height" hint="Cmd +/− does this too, between 32 and 120px.">
          <input
            type="range"
            min={32}
            max={120}
            value={hourHeight}
            onChange={(e) => setHourHeight(Number(e.target.value))}
            style={{ maxWidth: 260 }}
          />
          <span className="data caption">{hourHeight}px</span>
        </Field>
        <Field label="Snap" hint="Hold Alt while dragging for 5-minute precision.">
          <span className="data">15 minutes</span>
        </Field>
      </Section>

      <Section title="Timer">
        <Field
          label="Idle threshold"
          hint="When input stops for this long, Fruit asks. Discarding is the default — keeping is one keystroke."
        >
          <input
            type="number"
            min={1}
            max={60}
            value={num("timer.idleThresholdSec", 300) / 60}
            onChange={(e) => void put("timer.idleThresholdSec", Number(e.target.value) * 60)}
            style={{ width: 96 }}
          />
        </Field>
        <Field
          label="After sleep"
          hint="Sleep is detected by comparing wall time against the monotonic clock; it is never counted silently."
        >
          <span className="data">Ask, default not counted</span>
        </Field>
      </Section>

      <Section title="Pomodoro">
        <Field label="Work / short / long">
          <div className="row">
            {(
              [
                ["pomodoro.workSec", 25],
                ["pomodoro.shortSec", 5],
                ["pomodoro.longSec", 15],
              ] as const
            ).map(([key, def]) => (
              <input
                key={key}
                type="number"
                min={1}
                max={120}
                value={num(key, def * 60) / 60}
                onChange={(e) => void put(key, Number(e.target.value) * 60)}
                style={{ width: 72 }}
                aria-label={key}
              />
            ))}
            <span className="caption">minutes</span>
          </div>
        </Field>
        <Field label="Cycles before a long break">
          <input
            type="number"
            min={2}
            max={8}
            value={num("pomodoro.cyclesBeforeLong", 4)}
            onChange={(e) => void put("pomodoro.cyclesBeforeLong", Number(e.target.value))}
            style={{ width: 72 }}
          />
        </Field>
      </Section>

      <ActivitySettings />
      <LabelSettings />

      <Section title="Notices">
        <Field
          label="Without a break"
          hint="Only heads-down work counts. Meetings — sessions marked Attend — and personal time do not, because sitting in a two-hour review is not two hours heads-down."
        >
          <div className="row">
            {[0, 60, 90, 120].map((min) => (
              <button
                key={min}
                className="btn"
                aria-pressed={num("notices.continuousWorkSec", 0) === min * 60}
                onClick={() => void put("notices.continuousWorkSec", min * 60)}
              >
                {min === 0 ? "Never" : `${min} min`}
              </button>
            ))}
          </div>
        </Field>

        <Field
          label="A day's work ceiling"
          hint="Said once when you cross it, and not again that day."
        >
          <div className="row">
            {[0, 6, 8, 10].map((h) => (
              <button
                key={h}
                className="btn"
                aria-pressed={num("notices.dailyCeilingSec", 0) === h * 3600}
                onClick={() => void put("notices.dailyCeilingSec", h * 3600)}
              >
                {h === 0 ? "Never" : `${h} hours`}
              </button>
            ))}
          </div>
        </Field>

        <Field
          label="Off plan"
          hint="Mentions entertainment observed during an hour you plotted for something else — and never during time you did not plan, because Fruit has no standing to have an opinion about that. It is a notice, not a block: nothing is closed, denied or interrupted, and the browser extension has no permission to touch a page even if it wanted to."
        >
          <Switch
            label="Mention entertainment during plotted work"
            checked={(settings["notices.offPlan"] as boolean) ?? false}
            onChange={(v) => void put("notices.offPlan", v)}
          />
        </Field>

        <Field label="Quiet" hint="Silences all three. They come back on their own.">
          <div className="row">
            {[30, 120].map((min) => (
              <button
                key={min}
                className="btn"
                onClick={async () => {
                  const ok = await run(() => ipc.silenceNotices(min), "Couldn't silence notices.");
                  if (ok !== null) toast(`Quiet for ${min < 60 ? `${min} minutes` : "2 hours"}.`);
                }}
              >
                {min < 60 ? `${min} minutes` : "2 hours"}
              </button>
            ))}
          </div>
        </Field>
      </Section>

      <Section title="Data">
        <Field label="Export" hint="Written to your Downloads folder. JSON round-trips exactly, ids included. CSV ships tasks and sessions. ICS is export-only.">
          <div className="row">
            {(["json", "csv", "ics"] as const).map((f) => (
              <button
                key={f}
                className="btn"
                onClick={async () => {
                  const result = await run(
                    () => ipc.exportData(f, `fruit-export.${f}`, fmt.tz()),
                    "Couldn't export.",
                  );
                  // Naming the file is the whole point: an export you can't
                  // find is an export you don't trust.
                  if (result) toast(`Exported to ${result.paths.join(", ")}`);
                }}
              >
                Export {f.toUpperCase()}
              </button>
            ))}
          </div>
        </Field>
        <Field
          label="Import a calendar"
          hint="Reads a local .ics file. Meetings arrive as fixed blocks — the obligations the rest of the day has to fit around. Read-only and offline: no URL, no account, and Fruit never writes back to your calendar. Re-importing the same file updates in place instead of duplicating."
        >
          <div className="row">
            <button
              className="btn"
              onClick={async () => {
                const path = await run(() => ipc.pickIcsFile(), "Couldn't open the file picker.");
                if (!path) return; // cancelled is not a failure
                const summary = await run(
                  () => ipc.importIcs(path, fmt.tz()),
                  "Couldn't import that calendar.",
                );
                // The note names what was skipped and why — an import that
                // quietly drops half a calendar is worse than one that refuses.
                if (summary) toast(summary.note);
              }}
            >
              Choose an .ics file…
            </button>
          </div>
        </Field>
        <Field label="Integrity check" hint="Runs quick_check, verifies foreign keys, and rebuilds the tracked caches from the views.">
          <div className="row">
            <button
              className="btn"
              onClick={async () => {
                const r = await run(() => ipc.runIntegrityCheck(), "Couldn't run the check.");
                if (r) setIntegrity(r);
              }}
            >
              Run check
            </button>
            {integrity && (
              <span className="caption data">
                {integrity.quickCheck} · {integrity.foreignKeyViolations} FK violations ·{" "}
                {(integrity.dbBytes / 1024 / 1024).toFixed(1)}MB
              </span>
            )}
          </div>
        </Field>
        <Field label="Backups" hint="A VACUUM INTO snapshot on launch if the newest is over 24h old; 7 daily kept. Storing the database in Dropbox, iCloud or OneDrive is a known corruption path — don't.">
          <span className="caption">Managed automatically.</span>
        </Field>
      </Section>

      <Section title="Work">
        <StartAtLogin />
        <DailyWorkTarget />
        <WorkKinds />
      </Section>

      <Section title="Focus">
        <FocusWallpapers dir={settings["focus.wallpaperDir"] as string | undefined} onPick={put} />
      </Section>

      <Section title="Testing">
        <ResetAllData />
      </Section>

      <Section title="Shortcuts">
        <button className="btn" onClick={() => setOverlay("shortcuts")}>
          Show the full map <span className="kbd">?</span>
        </button>
      </Section>

      <Section title="About">
        <p className="caption">
          Fruit · local-first, no accounts, no telemetry. The database is plain SQLite with a documented schema, and everything in it
          is yours to export.
        </p>
      </Section>
    </div>
  );
}

/**
 * Installing the browser connector, stated plainly rather than done silently.
 *
 * Registering a native-messaging host means writing a manifest that names this
 * executable and an `HKCU` key pointing at it. An app that does that on first
 * run has installed a browser hook without being asked — which is exactly the
 * behaviour the rest of this section exists to rule out. So the app shows the
 * two paths and the person puts the files there.
 *
 * The panel is deliberately not a wizard. A wizard would imply the app can
 * verify the result, and it cannot: whether Chrome actually loaded the extension
 * is something only Chrome knows. What the app *can* honestly report is whether
 * samples have arrived, so that is the status line.
 */
function ConnectorInstall({ enabled }: { enabled: boolean }) {
  const run = useApp((s) => s.run);
  const [status, setStatus] = useState<ConnectorStatus | null>(null);
  const [extId, setExtId] = useState("");
  const [steps, setSteps] = useState<string[]>([]);

  const refresh = () => {
    void ipc.getConnectorStatus().then(setStatus).catch(() => setStatus(null));
  };

  useEffect(() => {
    if (enabled) refresh();
  }, [enabled]);

  const install = async () => {
    const done = await run(() => ipc.installConnector(extId), "Couldn't register the host.");
    // Every step is reported, successes and failures alike — a registry write
    // that was refused has to be visible, with the command to run by hand.
    if (done) {
      setSteps(done);
      refresh();
    }
  };

  // Shown even when websites are off, and saying why.
  //
  // This used to `return null`, which produced the worst possible dead end:
  // the Activity view told people to go to Settings → Activity → Browser
  // extension, and the section was not on the screen when they got there —
  // because it was hidden behind the very switch they had not turned on. A
  // control that vanishes is a control the user concludes does not exist.
  if (!enabled) {
    return (
      <Field
        label="Browser extension"
        hint="Chrome and Edge only. Fruit cannot see a tab's domain without it — an application sampler sees chrome.exe and nothing more."
      >
        <p className="caption">
          Turn on <b>Track websites</b> above first. The extension and the switch are both
          needed: the switch decides whether Fruit may store a domain at all, and the
          extension is what supplies one. With the switch off, a connected extension records
          nothing.
        </p>
      </Field>
    );
  }

  return (
    <Field
      label="Browser extension"
      hint="Chrome and Edge only. Fruit cannot see a tab's domain without it — an application sampler sees chrome.exe and nothing more."
    >
      {status ? (
        <div className="stack" style={{ gap: 6 }}>
          <p className="caption">
            1 · Open <code className="data">chrome://extensions</code>, turn on Developer
            mode, and <b>Load unpacked</b> from{" "}
            <code className="data">{status.extensionDir}</code>.
          </p>
          <p className="caption">
            2 · Copy the extension&rsquo;s <b>ID</b> from that page and paste it here. Fruit
            writes the host manifest and the per-user registry key that points Chrome and
            Edge at <code className="data">{status.exePath}</code>. It is not done on first
            run, because pointing a browser at an executable is not something an app should
            arrange without being asked.
          </p>
          <div className="row">
            <input
              value={extId}
              placeholder="32 letters, a–p"
              aria-label="Extension ID"
              onChange={(e) => setExtId(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void install()}
              style={{ maxWidth: 300 }}
            />
            <button className="btn btn-primary" onClick={() => void install()}>
              Register the host
            </button>
          </div>
          {steps.length > 0 && (
            <div className="stack" style={{ gap: 2 }}>
              {steps.map((line) => (
                <span key={line} className="caption data">
                  {line}
                </span>
              ))}
            </div>
          )}
          <p className="caption">
            {status.queuedSamples > 0
              ? "The extension is connected — samples are arriving."
              : "Nothing has arrived from a browser yet. That is expected until both steps are done and a browser window is in front."}
          </p>
        </div>
      ) : (
        <p className="caption">Checking…</p>
      )}
    </Field>
  );
}

/**
 * Activity's privacy contract, as controls (§3.5, §7.2).
 *
 * Every promise the feature makes is a switch here, in the order someone
 * worried about it would look for them: is it on, does it read titles, can I
 * pause it, what is excluded, how long is it kept, and how do I delete it all.
 * The enforcement lives in Rust — these controls set settings that
 * `record_activity` reads *before* writing, so an exclusion cannot be defeated
 * by a bug in the sampler.
 */
function ActivitySettings() {
  const status = useApp((s) => s.activityStatus);
  const put = useApp((s) => s.putActivitySetting);
  const load = useApp((s) => s.loadActivity);
  const run = useApp((s) => s.run);
  const toast = useApp((s) => s.toast);
  const [apps, setApps] = useState<string | null>(null);
  const [domains, setDomains] = useState<string | null>(null);
  const [patterns, setPatterns] = useState<string | null>(null);

  useEffect(() => {
    void load();
  }, [load]);

  if (!status) {
    return (
      <Section title="Activity">
        <p className="caption">Checking what this platform can do…</p>
      </Section>
    );
  }

  const s = status.settings;
  const supported = status.support === "full";
  // A list is edited as text and committed on blur; splitting on every
  // keystroke would delete the entry you are halfway through typing.
  const commitList = (key: string, raw: string) =>
    void put(
      key,
      raw
        .split(",")
        .map((v) => v.trim())
        .filter(Boolean),
    );

  return (
    <Section title="Activity">
      {/* The platform's own sentence, always on screen next to the switch it
          explains — never a bare "unavailable" (§3.10). */}
      <p className="caption">{status.supportNote}</p>

      <Field
        label="Track applications"
        hint="Samples which application is in front every 20 seconds, into the same local database as everything else. Nothing leaves this machine, and it is off until you turn it on."
      >
        <Switch
          label="Track applications"
          checked={s.enabled}
          disabled={!supported}
          onChange={(v) => void put("activity.enabled", v)}
        />
      </Field>

      <Field
        label="Track window titles"
        hint="A separate switch, and off even when applications are on — a title is the document name, the customer, the ticket. Turning applications off turns this off with it."
      >
        <Switch
          label="Track window titles"
          checked={s.titlesEnabled}
          disabled={!supported || !s.enabled}
          onChange={(v) => void put("activity.titlesEnabled", v)}
        />
      </Field>

      <Field label="Pause" hint="Survives a restart, so a pause before a private call stays paused.">
        <Switch
          label="Pause activity tracking"
          checked={s.paused}
          disabled={!supported || !s.enabled}
          onChange={(v) => void put("activity.paused", v)}
        />
      </Field>

      <Field
        label="Never record these apps"
        hint="Comma-separated executable or bundle names, e.g. 1Password.exe, Signal. Excluded apps are dropped before they are written, so they cannot resurface in an export."
      >
        <input
          value={apps ?? s.excludedApps.join(", ")}
          placeholder="1Password.exe, Signal"
          disabled={!supported}
          onChange={(e) => setApps(e.target.value)}
          onBlur={(e) => {
            setApps(null);
            commitList("activity.excludedApps", e.target.value);
          }}
          style={{ width: "100%", maxWidth: 420 }}
        />
      </Field>

      {/* The connector sits between titles and exclusions on purpose: it is the
          third and largest of the three things that can be observed, and this
          is where someone worried about it will already be reading. */}
      <Field
        label="Track websites (browser connector)"
        hint="Records the domain of the frontmost tab — youtube.com, never the page, the URL or the title. Its own switch, off even when applications are on, and it needs the browser extension below before it does anything."
      >
        <Switch
          label="Track websites"
          checked={s.domainsEnabled}
          disabled={!supported || !s.enabled}
          onChange={(v) => void put("activity.domainsEnabled", v)}
        />
      </Field>

      <Field
        label="Never record these websites"
        hint="Comma-separated domains, e.g. bank.co.uk. An entry covers every subdomain of itself, and an excluded domain is dropped before it is written."
      >
        <input
          value={domains ?? s.excludedDomains.join(", ")}
          placeholder="bank.co.uk, nhs.uk"
          disabled={!supported || !s.domainsEnabled}
          onChange={(e) => setDomains(e.target.value)}
          onBlur={(e) => {
            setDomains(null);
            commitList("activity.excludedDomains", e.target.value);
          }}
          style={{ width: "100%", maxWidth: 420 }}
        />
      </Field>

      <ConnectorInstall enabled={s.domainsEnabled} />

      <Field
        label="Never record titles containing"
        hint="Comma-separated fragments, matched case-insensitively. The app is still recorded; only the title is dropped."
      >
        <input
          value={patterns ?? s.excludedTitlePatterns.join(", ")}
          placeholder="salary, incognito"
          disabled={!supported || !s.titlesEnabled}
          onChange={(e) => setPatterns(e.target.value)}
          onBlur={(e) => {
            setPatterns(null);
            commitList("activity.excludedTitlePatterns", e.target.value);
          }}
          style={{ width: "100%", maxWidth: 420 }}
        />
      </Field>

      <Field
        label="Ignore anything shorter than"
        hint="Alt-tabbing to check a message is not a context switch worth a row in the day's account. A blip between two stretches of the same thing is absorbed into it; anything else is left out. Nothing is deleted — the floor is applied when the day is read, so lowering it brings the detail back."
      >
        <div className="row">
          {[0, 60, 120, 300].map((sec) => (
            <button
              key={sec}
              className="btn"
              aria-pressed={s.minSpanSec === sec}
              disabled={!supported}
              onClick={() => void put("activity.minSpanSec", sec)}
            >
              {sec === 0 ? "Nothing" : `${sec / 60} min`}
            </button>
          ))}
        </div>
      </Field>

      <Field
        label="Keep for"
        hint={
          s.retentionDays > 0 && s.nextPurgeAt
            ? `Anything older is deleted automatically. Next purge ${fmt.longDate(
                fmt.toLocalDate(new Date(s.nextPurgeAt)),
              )}.`
            : "Kept until you delete it. Nothing is purged automatically."
        }
      >
        <div className="row">
          {(
            [
              [30, "30 days"],
              [90, "90 days"],
              [0, "Forever"],
            ] as const
          ).map(([days, label]) => (
            <button
              key={days}
              className="btn"
              aria-pressed={s.retentionDays === days}
              disabled={!supported}
              onClick={() => void put("activity.retentionDays", days)}
            >
              {label}
            </button>
          ))}
        </div>
      </Field>

      <Field
        label="Delete everything recorded"
        hint="Immediate and not undoable — a privacy promise you can't act on is not a promise."
      >
        <button
          className="btn btn-danger"
          onClick={async () => {
            const removed = await run(() => ipc.clearActivity(), "Couldn't clear Activity.");
            if (removed !== null) {
              toast(`Deleted ${removed} activity ${removed === 1 ? "span" : "spans"}.`);
              await load();
            }
          }}
        >
          Delete activity data
        </button>
      </Field>
    </Section>
  );
}

/**
 * Labels, and the rules that apply them (migration 0007).
 *
 * Two lists, and both are fully editable — including the rules shipped with the
 * app. A "shipped" rule you cannot argue with is a rule that mislabels your
 * month and tells you to live with it. Editing one makes it yours: the row
 * stops being built-in rather than being shadowed by a second one.
 *
 * The **add** form matters more than it looks. Labelling from the Activity
 * screen only offers what has already been observed, which means sites are
 * unreachable until the browser extension is installed and has run for a day.
 * Typing `instagram.com → Distraction` here works immediately and applies the
 * moment the first observation arrives.
 */
function LabelSettings() {
  const run = useApp((s) => s.run);
  const toast = useApp((s) => s.toast);
  const [cats, setCats] = useState<ObservationCategory[]>([]);
  const [rules, setRules] = useState<ActivityRule[]>([]);
  const [name, setName] = useState("");

  const [kind, setKind] = useState<MatchKind>("domain");
  const [value, setValue] = useState("");
  const [pick, setPick] = useState<string>("");

  const load = async () => {
    const [c, r] = await Promise.all([
      ipc.getCategories(null, null, fmt.tz()).catch(() => []),
      ipc.getActivityRules().catch(() => []),
    ]);
    setCats(c);
    setRules(r);
    if (!pick && c.length) setPick(c[0]!.id);
  };

  useEffect(() => {
    void load();
  }, []);

  const addCategory = async () => {
    if (!name.trim()) return;
    // New categories roll up as "other" — the neutral bucket. Anything else
    // would move the month dashboard's Work or Entertainment figure the moment
    // the category was created.
    const ok = await run(() => ipc.createCategory(name.trim(), "other"), "Couldn't add that label.");
    if (ok) {
      setName("");
      void load();
    }
  };

  const addRule = async () => {
    if (!value.trim() || !pick) return;
    const saved = await run(() => ipc.setActivityRule(kind, value.trim(), pick), "Couldn't save that rule.");
    if (saved) {
      // Echo what was stored, not what was typed: `https://www.Instagram.com/x`
      // becomes `instagram.com`, and seeing that is how someone learns the rule
      // covers every page and every subdomain.
      toast(
        saved.backfilled > 0
          ? `${saved.matchValue} → ${saved.categoryName}, including ${saved.backfilled} already recorded.`
          : `${saved.matchValue} → ${saved.categoryName}. Applies from now on.`,
      );
      setValue("");
      void load();
    }
  };

  /** Repointing a rule is an upsert on the same (kind, value) — one row. */
  const repoint = async (rule: ActivityRule, categoryId: string) => {
    if (categoryId === rule.categoryId) return;
    const ok = await run(
      () => ipc.setActivityRule(rule.matchKind, rule.matchValue, categoryId),
      "Couldn't change that rule.",
    );
    if (ok) void load();
  };

  const removeRule = async (rule: ActivityRule) => {
    const ok = await run(() => ipc.deleteActivityRule(rule.id), "Couldn't delete that rule.");
    if (ok !== null) {
      toast(`Rule for ${rule.matchValue} removed. Past records keep their label.`);
      void load();
    }
  };

  return (
    <Section title="Labels">
      <Field
        label="Your labels"
        hint="What observed time can be filed as. Work and Other can be renamed but not deleted — everything unlabelled has to land somewhere. Deleting a label frees its time rather than destroying it: those intervals go back to unlabelled."
      >
        <div className="row" style={{ flexWrap: "wrap", gap: 6 }}>
          {cats.map((c) => (
            <span key={c.id} className="tag" style={{ borderColor: c.colour }}>
              <span className="swatch" style={{ background: c.colour }} aria-hidden="true" />
              {c.name}
              {!["work", "other"].includes(c.name.toLowerCase()) && (
                <button
                  className="btn micro"
                  aria-label={`Delete the ${c.name} label`}
                  onClick={async () => {
                    const ok = await run(() => ipc.deleteCategory(c.id), "Couldn't delete that label.");
                    if (ok !== null) void load();
                  }}
                >
                  Remove
                </button>
              )}
            </span>
          ))}
        </div>
        <div className="row" style={{ marginTop: 8 }}>
          <input
            value={name}
            placeholder="Add a label — e.g. AI tools"
            aria-label="New label name"
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void addCategory()}
            style={{ maxWidth: 240 }}
          />
          <button className="btn" onClick={() => void addCategory()}>
            Add label
          </button>
        </div>
      </Field>

      <Field
        label="Label a site or app"
        hint="Type it in — the site does not have to have been observed yet, so labels can be set up before the browser extension is. A site is reduced to its registrable domain, so instagram.com covers every page and every subdomain of it."
      >
        <div className="row" style={{ flexWrap: "wrap" }}>
          {(["domain", "app"] as const).map((k) => (
            <button key={k} className="btn" aria-pressed={kind === k} onClick={() => setKind(k)}>
              {k === "domain" ? "Site" : "App"}
            </button>
          ))}
          <input
            value={value}
            placeholder={kind === "domain" ? "instagram.com" : "steam.exe"}
            aria-label={kind === "domain" ? "Site to label" : "Application to label"}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void addRule()}
            style={{ maxWidth: 220 }}
          />
          <select value={pick} onChange={(e) => setPick(e.target.value)} aria-label="Label">
            {cats.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
          <button className="btn btn-primary" onClick={() => void addRule()}>
            Save rule
          </button>
        </div>
      </Field>

      <Field
        label="Rules"
        hint="One verdict per app or site, and every one of them is editable — including the ones shipped with the app. A site rule beats an app rule, always and without a setting: otherwise a browser labelled Work would make every site inside it work, which is the exact failure the browser extension was built to fix."
      >
        {rules.length === 0 ? (
          <p className="caption">No rules yet.</p>
        ) : (
          <div className="stack" style={{ gap: 4, maxHeight: 300, overflowY: "auto" }}>
            {rules.map((r) => (
              <div key={r.id} className="row">
                <span className="tag micro">{r.matchKind === "domain" ? "site" : "app"}</span>
                <span className="grow data">{r.matchValue}</span>
                {r.isBuiltin && <span className="micro caption">shipped</span>}
                {/* Editing in place rather than behind a pencil icon: there is
                    exactly one editable field on a rule, and a dropdown showing
                    its current value *is* the edit affordance. */}
                <select
                  value={r.categoryId}
                  aria-label={`Label for ${r.matchValue}`}
                  onChange={(e) => void repoint(r, e.target.value)}
                >
                  {cats.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.name}
                    </option>
                  ))}
                </select>
                <button
                  className="btn micro"
                  aria-label={`Delete the rule for ${r.matchValue}`}
                  onClick={() => void removeRule(r)}
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        )}
        <p className="caption">
          A rule fills in every stretch that had <i>no</i> label yet, whatever its date —
          filling a blank is not rewriting an answer. Anything already carrying a label
          keeps it, so a rule you change today cannot rewrite a week you have reviewed.
        </p>
      </Field>
    </Section>
  );
}
