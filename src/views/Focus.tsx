/**
 * Focus mode (§3.4).
 *
 * What distinguishes this from every other focus timer is the planner context:
 * the next scheduled block is on screen. The background is bound to the
 * Pomodoro phase, so a break is recognisable peripherally from across the room.
 */

import { useEffect, useState } from "react";
import { useApp } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type { Wallpaper } from "../lib/types";

const GRADIENTS = ["desk", "terrain", "water", "night"] as const;
type Gradient = (typeof GRADIENTS)[number];

/**
 * Your own pictures behind the clock.
 *
 * Fruit ships no photographs — it has no licence to redistribute anyone's, and
 * a bundled folder of "beautiful wallpapers" would mean putting other people's
 * work inside a paid binary. What it does is create the folder, explain what to
 * put in it, and draw whatever is there. Settings → Focus opens it.
 *
 * The images are loaded lazily and cached for the session: a full-screen photo
 * crosses IPC as base64, and re-reading it on every phase change would stutter
 * the one screen whose entire job is not to.
 */
function useWallpapers(enabled: boolean) {
  const [items, setItems] = useState<Wallpaper[] | null>(null);
  const [index, setIndex] = useState(0);
  const [uri, setUri] = useState<string | null>(null);
  const [cache] = useState(() => new Map<string, string>());

  useEffect(() => {
    if (!enabled) return;
    // A missing or empty folder is not an error — Focus falls back to its
    // gradients, which is why this swallows rather than toasting. Opening the
    // one screen meant to be calm on a red banner would be the wrong trade.
    void ipc
      .getWallpapers()
      .then((f) => setItems(f.items))
      .catch(() => setItems([]));
  }, [enabled]);

  const current = items && items.length > 0 ? items[index % items.length] : null;

  useEffect(() => {
    if (!current) {
      setUri(null);
      return;
    }
    const cached = cache.get(current.name);
    if (cached) {
      setUri(cached);
      return;
    }
    let live = true;
    void ipc
      .readWallpaper(current.name)
      .then((data) => {
        cache.set(current.name, data);
        if (live) setUri(data);
      })
      .catch(() => live && setUri(null));
    return () => {
      live = false;
    };
  }, [current, cache]);

  return {
    items,
    current,
    uri,
    next: () => setIndex((i) => i + 1),
    count: items?.length ?? 0,
  };
}

export function Focus() {
  const timer = useApp((s) => s.timer);
  const week = useApp((s) => s.week);
  const detail = useApp((s) => s.detail);
  const setOverlay = useApp((s) => s.setOverlay);
  const toggleTimer = useApp((s) => s.toggleTimer);

  const [manual, setManual] = useState<Gradient | null>(null);
  /* Off means the four phase gradients, which stay the default: they carry a
     meaning a photograph cannot (a break is recognisable across the room), so
     turning them off has to be a choice rather than the consequence of having
     dropped a file in a folder. */
  const [usePhoto, setUsePhoto] = useState(false);
  const photo = useWallpapers(usePhoto);
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
      } else if (e.key === "w" || e.key === "W") {
        setUsePhoto((v) => !v);
      } else if (e.key === " ") {
        e.preventDefault();
        if (timer.runTaskId) void toggleTimer(timer.runTaskId, timer.session?.blockId);
      } else if (e.key === "h" || e.key === "H") {
        setHideDigits((v) => !v);
      } else if (/^[1-4]$/.test(e.key)) {
        setManual(GRADIENTS[Number(e.key) - 1]!);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setOverlay, timer.runTaskId, timer.session, toggleTimer]);

  // The planner context: the next block that has not started yet.
  const nextBlock = week?.days
    .flatMap((d) => d.blocks)
    .filter((b) => b.block.startsAt > Date.now())
    .sort((a, b) => a.block.startsAt - b.block.startsAt)[0];

  const nextSubtask = detail?.subtasks.find((t) => t.status === "open");

  return (
    <div
      className="focus"
      data-bg={background}
      data-photo={photo.uri ? true : undefined}
      style={photo.uri ? { backgroundImage: `url("${photo.uri}")` } : undefined}
      role="dialog"
      aria-label="Focus mode"
    >
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

        <div className="title">{timer.taskTitle ?? "No timer running"}</div>

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
            timer.runTaskId && void toggleTimer(timer.runTaskId, timer.session?.blockId)
          }
        >
          {timer.phase === "running" ? "Pause" : "Resume"} <span className="kbd">Space</span>
        </button>
        <button className="btn" onClick={() => setHideDigits((v) => !v)}>
          {hideDigits ? "Show" : "Hide"} digits <span className="kbd">H</span>
        </button>
        {!usePhoto &&
          GRADIENTS.map((g, i) => (
            <button
              key={g}
              className="btn"
              aria-pressed={background === g}
              onClick={() => setManual(g)}
            >
              {i + 1}
            </button>
          ))}
        <button
          className="btn"
          aria-pressed={usePhoto}
          onClick={() => setUsePhoto((v) => !v)}
          title="Draw one of your own pictures instead of a phase gradient. Settings → Focus says where to put them."
        >
          Photo <span className="kbd">W</span>
        </button>
        {/* Only once there is more than one, because a "next" button over a
            folder of one picture is a button that does nothing. */}
        {usePhoto && photo.count > 1 && (
          <button className="btn" onClick={photo.next}>
            Next picture
          </button>
        )}
        {/* Never a blank control strip with no explanation: if the folder is
            empty the button that just did nothing has to say why. */}
        {usePhoto && photo.items !== null && photo.count === 0 && (
          <span className="caption">
            No pictures yet — Settings → Focus opens the folder to put them in.
          </span>
        )}
      </div>

      {/* A persistent hint until first use is recorded (§3.4). */}
      <div className="focus-esc caption" style={{ opacity: escSeen ? 0 : 1 }}>
        <span className="kbd">Esc</span> to exit
      </div>
    </div>
  );
}
