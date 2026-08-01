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
import type { IntegrityReport } from "../lib/types";

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
            {(["dark", "light", "system"] as const).map((t) => (
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
