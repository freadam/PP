/**
 * The persistent shell: brand mark, top bar, nav rail, timer chip, Pomodoro
 * strip, toasts, empty states (§2.2, §5.7).
 */

import { useEffect, useState } from "react";
import { useApp } from "../store/app";
import type { ViewName } from "../store/app";
import * as fmt from "../lib/format";
import * as ipc from "../lib/ipc";
import type { PomodoroState, TimerState } from "../lib/types";

/**
 * §5.10 — the mark is the drift rail as a monogram: two vertical strokes, one
 * dashed and one solid, the solid one running slightly longer. Legible at
 * 16px, which is the size that matters most: the tray icon showing a running
 * timer. It is literally the product's idea.
 */
export function BrandMark({ size = 20, state = "idle" }: { size?: number; state?: "idle" | "running" | "overrun" }) {
  const solid = state === "running" ? "var(--track)" : state === "overrun" ? "var(--over)" : "var(--muted)";
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" aria-hidden="true">
      <line x1="6" y1="3" x2="6" y2="15" stroke="var(--plot)" strokeWidth="1.5" strokeDasharray="3 3" />
      <line x1="12" y1="3" x2="12" y2="18" stroke={solid} strokeWidth="3" />
    </svg>
  );
}

const NAV: { view: ViewName; label: string; key: string; icon: string }[] = [
  { view: "planner", label: "Planner", key: "P", icon: "▤" },
  { view: "tasks", label: "Tasks", key: "T", icon: "☰" },
  { view: "activity", label: "Activity", key: "A", icon: "◷" },
  { view: "reports", label: "Reports", key: "R", icon: "◫" },
  { view: "settings", label: "Settings", key: "S", icon: "⚙" },
];

export function NavRail() {
  const view = useApp((s) => s.view);
  const go = useApp((s) => s.go);
  const unreconciled = useApp((s) => s.unreconciled);

  return (
    <nav className="rail-nav" aria-label="Views">
      {NAV.map((item) => (
        <button
          key={item.view}
          className="rail-item"
          aria-current={view === item.view ? "page" : undefined}
          // The label is always present in the accessible name, not only on hover.
          aria-label={`${item.label} (G then ${item.key})`}
          title={`${item.label} — G then ${item.key}`}
          onClick={() => go(item.view)}
        >
          <span aria-hidden="true" style={{ fontSize: 18 }}>
            {item.icon}
          </span>
          {/* §3.11: reconcile-due is a dot, not a notification. */}
          {item.view === "planner" && unreconciled.length > 0 && <span className="rail-dot" />}
        </button>
      ))}
    </nav>
  );
}

export function PomodoroStrip({ pomodoro }: { pomodoro: PomodoroState | null }) {
  // Four dots, not eight: an 8-dot strip is unreadable at 6px and nobody counts
  // past four (§5.7).
  const total = 4;
  const cycle = pomodoro ? ((pomodoro.cycle - 1) % total) + 1 : 0;
  return (
    <span
      className="pomodoro"
      role="img"
      aria-label={
        pomodoro
          ? `Pomodoro ${pomodoro.phase}, cycle ${cycle} of ${total}`
          : "Pomodoro not running"
      }
    >
      {Array.from({ length: total }, (_, i) => {
        const n = i + 1;
        const state = n < cycle ? "done" : n === cycle ? "current" : "upcoming";
        return <span key={n} className="pomo-dot" data-state={state} data-long={n === total} />;
      })}
    </span>
  );
}

export function TimerChip({ timer }: { timer: TimerState }) {
  const openDetail = useApp((s) => s.openDetail);
  const toggleTimer = useApp((s) => s.toggleTimer);
  const running = timer.phase === "running";

  // The chip follows the *run*, not the segment: a segment closes whenever the
  // machine sleeps or you walk away, and a chip that blanks out at that moment
  // would be reporting a bookkeeping detail as if it were news.
  if (!timer.runTaskId) {
    return (
      <span className="timer-chip" data-idle="true">
        <span className="caption">No timer running</span>
        <span className="kbd">Space</span>
      </span>
    );
  }

  return (
    <span className="timer-chip" data-idle={!running} aria-live="polite">
      <span
        className="dot"
        style={{ background: running ? "var(--track)" : "var(--muted)" }}
      />
      <button
        className="chip-title"
        onClick={() => timer.runTaskId && void openDetail(timer.runTaskId)}
      >
        {timer.taskTitle ?? "Timing"}
      </button>
      <span className="data">{fmt.stopwatch(timer.elapsedSec)}</span>
      {!running && (
        <span className="micro" style={{ color: "var(--muted)" }}>
          {timer.phase === "idleChallenge" ? "paused" : timer.phase}
        </span>
      )}
      <button
        aria-label={running ? "Stop timer" : "Start timer"}
        onClick={() =>
          timer.runTaskId && void toggleTimer(timer.runTaskId, timer.session?.blockId)
        }
        style={{ color: "var(--muted)" }}
      >
        {running ? "■" : "▶"}
      </button>
    </span>
  );
}

export function TopBar() {
  const timer = useApp((s) => s.timer);
  const setOverlay = useApp((s) => s.setOverlay);
  const unreconciled = useApp((s) => s.unreconciled);

  return (
    <header className="topbar">
      <span className="brand">
        <BrandMark
          state={timer.phase === "running" ? "running" : "idle"}
        />
        Fruit
      </span>
      {/* No network calls in the core loop — the badge is a statement of fact. */}
      <span className="micro badge-offline" title="Fruit never talks to a server">
        Offline
      </span>

      <span className="topbar-centre">
        <TimerChip timer={timer} />
        <PomodoroStrip pomodoro={timer.pomodoro} />
      </span>

      {unreconciled.length > 0 && (
        <button className="btn" onClick={() => setOverlay("reconcile")}>
          <span className="dot" style={{ background: "var(--track)" }} />
          Reconcile <span className="kbd">⌘R</span>
        </button>
      )}
      <button className="btn" onClick={() => setOverlay("focus")}>
        Focus <span className="kbd">F</span>
      </button>
    </header>
  );
}

/**
 * §5.7 — centred, `caption` `muted`, one sentence with an inline keyboard hint.
 * An empty screen is an invitation to act (§3.10).
 */
export function Empty({ children }: { children: React.ReactNode }) {
  return (
    <div className="empty">
      <p className="caption">{children}</p>
    </div>
  );
}

export function Toasts() {
  const toasts = useApp((s) => s.toasts);
  const dismiss = useApp((s) => s.dismissToast);

  useEffect(() => {
    if (!toasts.length) return;
    const timers = toasts.map((t) =>
      setTimeout(() => dismiss(t.id), Math.max(0, t.expiresAt - Date.now())),
    );
    return () => timers.forEach(clearTimeout);
  }, [toasts, dismiss]);

  if (!toasts.length) return null;
  return (
    <div className="toast-stack" role="status" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className="toast" data-tone={t.tone}>
          <span className="grow">{t.message}</span>
          {t.action && (
            <button className="btn" onClick={t.action.run}>
              {t.action.label}
            </button>
          )}
          <button aria-label="Dismiss" onClick={() => dismiss(t.id)} style={{ color: "var(--muted)" }}>
            ✕
          </button>
          <span className="toast-progress" />
        </div>
      ))}
    </div>
  );
}

/**
 * §4.5 `recovering` — an in-app modal, because it needs a decision before the
 * numbers can be trusted. This is the one surface that uses `assertive`.
 */
export function RecoveryModal() {
  const timer = useApp((s) => s.timer);
  const setTimer = useApp((s) => s.setTimer);
  const refresh = useApp((s) => s.refresh);
  const run = useApp((s) => s.run);
  if (timer.phase !== "recovering" || !timer.recoverySessionId) return null;

  const resolve = async (kind: "trimToHeartbeat" | "keepAll" | "discard") => {
    const next = await run(() => ipc.resolveRecovery(timer.recoverySessionId!, kind));
    if (next) {
      setTimer(next);
      await refresh();
    }
  };

  return (
    <>
      <div className="scrim" />
      <div className="modal overlay" role="alertdialog" aria-modal="true" aria-live="assertive">
        <div className="sheet-head">
          <strong className="title">Fruit closed while a timer was running</strong>
        </div>
        <div className="sheet-body stack">
          <p className="caption">
            The last thing Fruit knew for certain was the heartbeat it wrote every 30 seconds.
            Trimming to it is the honest default — keeping the whole span would count any time
            the machine spent asleep.
          </p>
          <div className="row">
            <button className="btn btn-primary" onClick={() => void resolve("trimToHeartbeat")}>
              Trim to last heartbeat
            </button>
            <button className="btn" onClick={() => void resolve("keepAll")}>
              Keep all of it
            </button>
            <button className="btn btn-danger" onClick={() => void resolve("discard")}>
              Discard
            </button>
          </div>
        </div>
      </div>
    </>
  );
}

/**
 * §4.5 `idle_challenge` — in-app, non-modal, resolvable later. Never a
 * notification (§3.11).
 */
export function IdleBanner() {
  const timer = useApp((s) => s.timer);
  const setTimer = useApp((s) => s.setTimer);
  const run = useApp((s) => s.run);
  if (timer.phase !== "idleChallenge" || timer.idleFrom == null || timer.idleTo == null) return null;

  const span = Math.round((timer.idleTo - timer.idleFrom) / 60000);
  const resolve = async (kind: "keep" | "discard" | "assignToBreak") => {
    const next = await run(() => ipc.resolveIdle(kind));
    if (next) setTimer(next);
  };

  return (
    <div className="toast-stack" style={{ bottom: 72 }}>
      <div className="toast" role="status">
        <span className="grow">
          No input for {span}m — {fmt.clock(timer.idleFrom)} to {fmt.clock(timer.idleTo)}.
        </span>
        <button className="btn btn-primary" onClick={() => void resolve("discard")}>
          Discard
        </button>
        <button className="btn" onClick={() => void resolve("keep")}>
          Keep
        </button>
        <button className="btn" onClick={() => void resolve("assignToBreak")}>
          Break
        </button>
      </div>
    </div>
  );
}

/** Browser preview only: says plainly that writes are not wired up. */
export function PreviewNotice() {
  const [dismissed, setDismissed] = useState(false);
  if (ipc.isDesktop() || dismissed) return null;
  return (
    <div className="toast-stack" style={{ left: "auto", right: 16, transform: "none" }}>
      <div className="toast">
        <span className="grow caption">
          Browser preview — reading recorded output from the Rust command layer. Writes need the
          desktop app (<code className="data">npm run app</code>).
        </span>
        <button onClick={() => setDismissed(true)} aria-label="Dismiss">
          ✕
        </button>
      </div>
    </div>
  );
}
