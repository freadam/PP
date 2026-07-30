/**
 * Settings (§3.8) and Activity (§3.5).
 *
 * The Activity section states the platform truth rather than pretending:
 * Wayland cannot report the frontmost window at all, so Linux ships with
 * Activity disabled and a sentence explaining why.
 */

import { useEffect, useState } from "react";
import { useApp } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type { IntegrityReport } from "../lib/types";
import { Empty } from "../components/chrome";

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="panel">
      <h2>{title}</h2>
      <div className="stack">{children}</div>
    </section>
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

      <Section title="Activity">
        <Field
          label="Window tracking"
          hint="App-level tracking and window-title tracking are separate switches, and titles stay off even when apps are on."
        >
          <p className="caption">
            {navigator.platform.toLowerCase().includes("linux")
              ? "Activity tracking isn't available on Wayland. It works on X11 sessions."
              : "Activity tracking is off. It is opt-in, sampled locally, and never leaves this machine."}
          </p>
        </Field>
      </Section>

      <Section title="Data">
        <Field label="Export" hint="JSON round-trips exactly, ids included. CSV ships tasks and sessions. ICS is export-only.">
          <div className="row">
            {(["json", "csv", "ics"] as const).map((f) => (
              <button
                key={f}
                className="btn"
                onClick={() =>
                  void run(
                    () => ipc.exportData(f, `fruit-export.${f}`, fmt.tz()),
                    "Couldn't export.",
                  )
                }
              >
                Export {f.toUpperCase()}
              </button>
            ))}
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

export function Activity() {
  return (
    <Empty>
      Activity tracking is off. Turn it on in Settings → Activity. It is opt-in, sampled locally,
      and never leaves this machine.
    </Empty>
  );
}
