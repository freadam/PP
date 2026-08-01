/**
 * Starting a drag from a task (§4.3).
 *
 * The spec lists four drag sources — sidebar backlog item, task row, existing
 * block, block edge — and dropping a *task* onto the grid is the primary way
 * you plot anything. The Planner owns the geometry and the drop; this hook only
 * owns "the user has begun dragging this task", which is why the drag itself
 * lives in the store: it starts in one component and lands in another.
 *
 * `S` then arrows remains a complete substitute — drag is never the only path.
 */

import type { PointerEvent as ReactPointerEvent } from "react";
import { useApp } from "../store/app";
import type { TaskRow } from "./types";

/** 8px before a drag begins, so taps and scrolls aren't misread (§4.3). */
const DRAG_THRESHOLD_PX = 8;
/** A task with no estimate still needs a length; half an hour is the smallest
 *  useful block anyone actually plots. */
const DEFAULT_BLOCK_SEC = 1800;

export function taskDragDuration(task: TaskRow): number {
  // A rollover task has no single-sitting estimate by definition, so it gets
  // the default rather than an invented number.
  if (task.isRollover || task.estimateSec == null) return DEFAULT_BLOCK_SEC;
  return Math.min(Math.max(task.estimateSec, 300), 43_200);
}

export function useStartTaskDrag() {
  return (event: ReactPointerEvent, task: TaskRow) => {
    if (event.button !== 0) return;
    const originX = event.clientX;
    const originY = event.clientY;
    let armed = false;

    const onMove = (e: PointerEvent) => {
      if (armed) return;
      if (Math.abs(e.clientX - originX) + Math.abs(e.clientY - originY) < DRAG_THRESHOLD_PX) {
        return;
      }
      armed = true;
      useApp.getState().setTaskDrag({
        taskId: task.id,
        title: task.title,
        durationSec: taskDragDuration(task),
      });
      // The planner is where a task can be dropped, so go there. Without this,
      // dragging out of the Tasks view would have nowhere to land.
      if (useApp.getState().view !== "planner") useApp.getState().go("planner");
    };

    const stop = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
      // The Planner clears the drag when it handles the drop; this only cleans
      // up a drag that never reached a target.
      setTimeout(() => {
        if (!armed) return;
        useApp.getState().setTaskDrag(null);
      }, 0);
    };

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
  };
}
