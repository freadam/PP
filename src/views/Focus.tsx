/**
 * Focus mode (§3.4).
 *
 * What distinguishes this from every other focus timer is the planner context:
 * the next scheduled block is on screen. The background is bound to the
 * Pomodoro phase, so a break is recognisable peripherally from across the room.
 */

import { useEffect, useState } from "react";
import { useApp } from "../store/app";
import * as fmt from "../lib/format";

const GRADIENTS = ["desk", "terrain", "water", "night"] as const;
type Gradient = (typeof GRADIENTS)[number];

export function Focus() {
  const timer = useApp((s) => s.timer);
  const week = useApp((s) => s.week);
  const detail = useApp((s) => s.detail);
  const setOverlay = useApp((s) => s.setOverlay);
  const toggleTimer = useApp((s) => s.toggleTimer);

  const [manual, setManual] = useState<Gradient | null>(null);
  const [hideDigits, setHideDigits] = useState(false);
  const [idleChrome, setIdleChrome] = useState(false);
  const [escSeen, setEscSeen] = useState(false);

  // Bound to the Pomodoro phase unless the user picks one by hand.
  const phase = timer.pomodoro?.phase;
  const bound: Gradient =
    phase === "shortBreak" ? "terrain" : phase === "longBreak" ? "water" : "desk";
  const background = manual ?? bound;

  /* The control strip fades after 4s of no input; any input restores it. The
     Esc hint fades last and slowest. */
  useEffect(() => {
    let t: number;
    const wake = () => {
      setIdleChrome(false);
      clearTimeout(t);
      t = window.setTimeout(() => setIdleChrome(true), 4000);
    };
    wake();
    window.addEventListener("pointermove", wake);
    window.addEventListener("keydown", wake);
    return () => {
      clearTimeout(t);
      window.removeEventListener("pointermove", wake);
      window.removeEventListener("keydown", wake);
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setEscSeen(true);
        setOverlay(null);
      } else if (e.key === " ") {
        e.preventDefault();
        if (timer.session) void toggleTimer(timer.session.taskId, timer.session.blockId);
      } else if (e.key === "h" || e.key === "H") {
        setHideDigits((v) => !v);
      } else if (/^[1-4]$/.test(e.key)) {
        setManual(GRADIENTS[Number(e.key) - 1]!);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setOverlay, timer.session, toggleTimer]);

  // The planner context: the next block that has not started yet.
  const nextBlock = week?.days
    .flatMap((d) => d.blocks)
    .filter((b) => b.block.startsAt > Date.now())
    .sort((a, b) => a.block.startsAt - b.block.startsAt)[0];

  const nextSubtask = detail?.subtasks.find((t) => t.status === "open");

  return (
    <div className="focus" data-bg={background} role="dialog" aria-label="Focus mode">
      <div className="focus-inner">
        {timer.pomodoro && (
          <span className="micro" style={{ color: "var(--muted)" }}>
            {timer.pomodoro.phase === "work"
              ? `Work · cycle ${timer.pomodoro.cycle}`
              : timer.pomodoro.phase === "shortBreak"
                ? "Short break"
                : "Long break"}
          </span>
        )}

        <div className="focus-clock" aria-live="off">
          {hideDigits ? "——:——" : fmt.stopwatch(timer.elapsedSec)}
        </div>

        <div className="title">{timer.session?.taskTitle ?? "No timer running"}</div>

        {nextSubtask && (
          <div className="caption">
            Next up: {nextSubtask.title} <span className="kbd">N</span>
          </div>
        )}

        {nextBlock && (
          <div className="caption data">
            Next plotted: {nextBlock.title} at {fmt.clock(nextBlock.block.startsAt)}
          </div>
        )}
      </div>

      <div className="focus-controls" data-hidden={idleChrome}>
        <button
          className="btn"
          onClick={() =>
            timer.session && void toggleTimer(timer.session.taskId, timer.session.blockId)
          }
        >
          {timer.phase === "running" ? "Pause" : "Resume"} <span className="kbd">Space</span>
        </button>
        <button className="btn" onClick={() => setHideDigits((v) => !v)}>
          {hideDigits ? "Show" : "Hide"} digits <span className="kbd">H</span>
        </button>
        {GRADIENTS.map((g, i) => (
          <button
            key={g}
            className="btn"
            aria-pressed={background === g}
            onClick={() => setManual(g)}
          >
            {i + 1}
          </button>
        ))}
      </div>

      {/* A persistent hint until first use is recorded (§3.4). */}
      <div className="focus-esc caption" style={{ opacity: escSeen ? 0 : 1 }}>
        <span className="kbd">Esc</span> to exit
      </div>
    </div>
  );
}
