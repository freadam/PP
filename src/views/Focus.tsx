/**
 * Focus mode (§3.4).
 *
 * What distinguishes this from every other focus timer is the planner context:
 * the next scheduled block is on screen. The background is bound to the
 * Pomodoro phase, so a break is recognisable peripherally from across the room.
 */

import { useCallback, useEffect, useState } from "react";
import { useApp } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type { Wallpaper } from "../lib/types";

const GRADIENTS = ["desk", "terrain", "water", "night"] as const;
type Gradient = (typeof GRADIENTS)[number];

/* Both remembered, because Focus is a mode you enter many times a day and a
   preference you have to re-set on every entry is not a preference. */
const USE_PHOTO = "focus.usePhoto";
const LAST_PICTURE = "focus.wallpaper";

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
 *
 * The picture is tracked **by name, not by index**. The folder is a folder on
 * disk that changes between sessions, and an index into it is a promise about
 * ordering that a single new file breaks — drop one in that sorts first and
 * "the picture I was on" silently becomes a different picture.
 */
function useWallpapers(enabled: boolean, remembered: string | null) {
  const [items, setItems] = useState<Wallpaper[] | null>(null);
  const [name, setName] = useState<string | null>(null);
  const [uri, setUri] = useState<string | null>(null);
  const [cache] = useState(() => new Map<string, string>());

  useEffect(() => {
    if (!enabled) return;
    // A missing or empty folder is not an error — Focus falls back to its
    // gradients, which is why this swallows rather than toasting. Opening the
    // one screen meant to be calm on a red banner would be the wrong trade.
    void ipc
      .getWallpapers()
      .then((f) => {
        setItems(f.items);
        // The remembered picture can have been renamed, moved or deleted since
        // Focus last closed. Falling back to the first one keeps the folder
        // working; honouring a name that is no longer there would show the
        // gradients and no reason why.
        const still = remembered && f.items.some((i) => i.name === remembered);
        setName((n) => n ?? (still ? remembered : (f.items[0]?.name ?? null)));
      })
      .catch(() => setItems([]));
  }, [enabled, remembered]);

  const current = items?.find((i) => i.name === name) ?? null;

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

  /* Written when the picture resolves, not only when the user changes it — so
     a remembered name that has gone from the folder is replaced by the one
     actually being drawn, rather than left dangling for every future open. */
  useEffect(() => {
    if (!enabled || !name) return;
    void ipc.setSetting(LAST_PICTURE, name).catch(() => {});
  }, [enabled, name]);

  return {
    items,
    current,
    /* Only while photos are on. The loaded picture and the folder listing are
       both kept across a toggle so coming back is instant, but reporting the
       uri regardless would leave the photograph painted after the user turned
       photos off — the controls would say gradient and the screen would say
       otherwise. */
    uri: enabled ? uri : null,
    next: () =>
      setName((n) => {
        if (!items || items.length === 0) return n;
        // -1 for an unknown name lands on the first, which is where a "next"
        // from nowhere should go.
        const at = items.findIndex((i) => i.name === n);
        return items[(at + 1) % items.length]!.name;
      }),
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
  /* `null` while the remembered choice is still being read. The gradient
     buttons only exist in photo-off, so rendering a guess would flash four
     buttons and take them away again on the one screen that must not flicker.
     Photos are the default: the folder is the point of the feature, and one
     you have to switch on at every entry is one you stop using. The gradients
     remain a keystroke away, and still take over on their own when the folder
     is empty. */
  const [usePhoto, setUsePhoto] = useState<boolean | null>(null);
  const [remembered, setRemembered] = useState<string | null>(null);
  const photo = useWallpapers(usePhoto === true, remembered);

  useEffect(() => {
    void ipc
      .getSettings()
      .then((s) => {
        setRemembered((s[LAST_PICTURE] as string | undefined) ?? null);
        setUsePhoto((s[USE_PHOTO] as boolean | undefined) ?? true);
      })
      .catch(() => setUsePhoto(true));
  }, []);

  const togglePhoto = useCallback(() => {
    const next = !usePhoto;
    setUsePhoto(next);
    void ipc.setSetting(USE_PHOTO, next).catch(() => {});
  }, [usePhoto]);
  const [hideDigits, setHideDigits] = useState(false);
  const [idleChrome, setIdleChrome] = useState(false);
  const [escSeen, setEscSeen] = useState(false);

  // Bound to the Pomodoro phase unless the user picks one by hand.
  const phase = timer.pomodoro?.phase;
  const bound: Gradient =
    phase === "shortBreak" ? "terrain" : phase === "longBreak" ? "water" : "desk";
  const background = manual ?? bound;

  /* A folder that is known to hold nothing — not one still being read, which
     is why this asks `items` rather than `uri`. Gating on `uri` would flash
     the gradient buttons in during every picture load. */
  const noPictures = photo.items !== null && photo.count === 0;
  /* The gradient is what is actually on screen whenever there is no picture to
     draw, so its controls belong on screen too. Photo mode over an empty
     folder used to hide them, which left someone whose folder is empty with no
     way to pick a gradient without first turning photos off. */
  const showGradients = usePhoto === false || (usePhoto === true && noPictures);

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
        togglePhoto();
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
  }, [setOverlay, timer.runTaskId, timer.session, toggleTimer, togglePhoto]);

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
        {showGradients &&
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
          aria-pressed={usePhoto === true}
          onClick={togglePhoto}
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
            empty the button that just did nothing has to say why. Now that
            photos are the default this is also the first thing a new user sees
            here, so it has to name the fix rather than just the problem. */}
        {usePhoto && noPictures && (
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
