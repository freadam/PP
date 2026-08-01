/**
 * The keyboard map (§4.2).
 *
 * Fruit is modeless except for the four overlays: the audience likes vim, but
 * a modal text app confuses everyone who isn't currently thinking about vim.
 * The only sequence is `G` then a letter.
 *
 * `Cmd+Q`, `Cmd+W`, `Cmd+M`, `Cmd+C/V/X` and `Tab` are reserved and never bound.
 */

import { useEffect, useRef } from "react";
import { useApp } from "../store/app";
import { commandById } from "./commands";

const RESERVED = new Set(["q", "w", "m", "c", "v", "x"]);

function isTyping(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el) return false;
  return (
    el.tagName === "INPUT" ||
    el.tagName === "TEXTAREA" ||
    el.tagName === "SELECT" ||
    el.isContentEditable
  );
}

export function useKeyboard() {
  const pendingG = useRef(false);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const app = useApp.getState();
      const mod = e.metaKey || e.ctrlKey;
      const typing = isTyping(e.target);

      // Esc dismisses whatever is open, innermost first, and is never swallowed.
      if (e.key === "Escape") {
        if (app.overlay) {
          app.setOverlay(null);
          return;
        }
        if (app.detail) {
          app.closeDetail();
          return;
        }
        if (app.selectedBlockId) {
          app.selectBlock(null);
          return;
        }
        return;
      }

      if (mod && RESERVED.has(e.key.toLowerCase()) && !["k", "f", "r", "z"].includes(e.key.toLowerCase())) {
        return; // leave the OS clipboard and window keys alone
      }

      if (mod) {
        const key = e.key.toLowerCase();
        const run = (id: string) => {
          e.preventDefault();
          void commandById(id)?.run();
        };
        if (key === "k") return run("palette");
        if (key === "f") return run("search");
        if (key === "r") return run("reconcile");
        if (key === "z") return run(e.shiftKey ? "undo" : "undo");
        if (key === ",") return run("go-settings");
        if (key === ".") return run("timer-stop");
        if (key === "=" || key === "+") return run("zoom-in");
        if (key === "-") return run("zoom-out");
        return;
      }

      // Text fields keep their keys; only Esc and the modifier combos above
      // reach through.
      if (typing) return;

      // `G` then P/T/A/R/S
      if (pendingG.current) {
        pendingG.current = false;
        const map: Record<string, string> = {
          p: "go-planner",
          t: "go-tasks",
          a: "go-activity",
          r: "go-reports",
          s: "go-settings",
        };
        const id = map[e.key.toLowerCase()];
        if (id) {
          e.preventDefault();
          void commandById(id)?.run();
          return;
        }
      }
      if (e.key === "g" || e.key === "G") {
        pendingG.current = true;
        setTimeout(() => (pendingG.current = false), 1200);
        return;
      }

      const single: Record<string, string> = {
        c: "capture",
        f: "focus",
        "?": "shortcuts",
        " ": "timer-toggle",
        t: "today",
        d: "block-duplicate",
        r: "block-repeat",
        s: "schedule-selected",
        x: "task-complete",
        "1": app.view === "planner" ? "span-1" : "priority-1",
        "2": "priority-2",
        "3": app.view === "planner" ? "span-3" : "priority-3",
        "7": "span-7",
        Enter: "task-open",
        Backspace: app.selectedBlockId ? "block-unschedule" : "task-delete",
        ArrowLeft: "prev",
        ArrowRight: "next",
      };

      // Arrows move a selected block; without one they page the planner.
      if (e.key === "ArrowUp" || e.key === "ArrowDown") {
        if (!app.selectedBlockId) return;
        e.preventDefault();
        const down = e.key === "ArrowDown";
        const id = e.shiftKey
          ? down
            ? "block-grow"
            : "block-shrink"
          : down
            ? "block-move-later"
            : "block-move-earlier";
        void commandById(id)?.run();
        return;
      }

      // J / K move the selection in whichever list is in front.
      if (e.key === "j" || e.key === "k" || e.key === "J" || e.key === "K") {
        e.preventDefault();
        moveSelection(e.key.toLowerCase() === "j" ? 1 : -1, e.shiftKey);
        return;
      }

      const id = single[e.key];
      if (!id) return;
      const command = commandById(id);
      if (!command || (command.when && !command.when())) return;
      e.preventDefault();
      void command.run();
    };

    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}

function moveSelection(delta: number, extend: boolean) {
  const app = useApp.getState();
  if (app.view === "planner") {
    const blocks = app.week?.days.flatMap((d) => d.blocks) ?? [];
    if (!blocks.length) return;
    const i = blocks.findIndex((b) => b.block.id === app.selectedBlockId);
    const next = blocks[Math.max(0, Math.min(blocks.length - 1, (i < 0 ? 0 : i) + delta))];
    if (next) app.selectBlock(next.block.id);
    return;
  }
  const tasks = app.backlog?.tasks ?? [];
  if (!tasks.length) return;
  const current = app.selectedTaskIds[app.selectedTaskIds.length - 1];
  const i = tasks.findIndex((t) => t.id === current);
  const next = tasks[Math.max(0, Math.min(tasks.length - 1, (i < 0 ? 0 : i) + delta))];
  if (next) app.selectTask(next.id, extend ? "toggle" : "replace");
}
