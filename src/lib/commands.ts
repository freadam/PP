/**
 * One registry for every action (§4.2, U1).
 *
 * "Any action here must also be reachable through the command palette" is only
 * true if there is a single list — two lists drift. The keyboard handler, the
 * palette and the shortcut sheet all read this file, so a command that is not
 * reachable both ways cannot exist.
 */

import { useApp } from "../store/app";
import * as ipc from "./ipc";
import * as fmt from "./format";
import { undoableDelete } from "../store/app";

export interface Command {
  id: string;
  title: string;
  group: "Global" | "Planner" | "Tasks" | "Timer" | "Data";
  /** Displayed as written; the handler below is the source of truth. */
  keys?: string;
  run: () => void | Promise<void>;
  /** Hidden from the palette when false — e.g. planner keys outside the planner. */
  when?: () => boolean;
}

const app = () => useApp.getState();

export const COMMANDS: Command[] = [
  // ─── Global ─────────────────────────────────────────────────────────
  { id: "palette", title: "Command palette", group: "Global", keys: "⌘K", run: () => app().setOverlay("palette") },
  { id: "capture", title: "Quick capture", group: "Global", keys: "C", run: focusCapture },
  { id: "search", title: "Search", group: "Global", keys: "⌘F", run: () => app().setOverlay("palette") },
  { id: "focus", title: "Focus mode", group: "Global", keys: "F", run: () => app().setOverlay("focus") },
  { id: "reconcile", title: "Reconcile a day", group: "Global", keys: "⌘R", run: () => app().setOverlay("reconcile") },
  { id: "shortcuts", title: "Shortcut sheet", group: "Global", keys: "?", run: () => app().setOverlay("shortcuts") },
  { id: "undo", title: "Undo", group: "Global", keys: "⌘Z", run: () => void app().undo() },
  { id: "go-planner", title: "Go to Planner", group: "Global", keys: "G then P", run: () => app().go("planner") },
  { id: "go-tasks", title: "Go to Tasks", group: "Global", keys: "G then T", run: () => app().go("tasks") },
  { id: "go-activity", title: "Go to Activity", group: "Global", keys: "G then A", run: () => app().go("activity") },
  { id: "go-reports", title: "Go to Reports", group: "Global", keys: "G then R", run: () => app().go("reports") },
  { id: "go-settings", title: "Settings", group: "Global", keys: "⌘,", run: () => app().go("settings") },
  {
    id: "theme",
    title: "Toggle light / dark theme",
    group: "Global",
    run: () => app().setTheme(app().theme === "dark" ? "light" : "dark"),
  },

  // ─── Timer ──────────────────────────────────────────────────────────
  {
    id: "timer-toggle",
    title: "Start or stop the timer",
    group: "Timer",
    keys: "Space",
    run: () => {
      const { timer, selectedTaskIds, backlog } = app();
      if (timer.runTaskId)
        return void app().toggleTimer(timer.runTaskId, timer.session?.blockId);
      const id = selectedTaskIds[0] ?? backlog?.tasks[0]?.id;
      if (id) void app().toggleTimer(id);
      else app().toast("Select a task first, then press Space.");
    },
  },
  {
    id: "timer-stop",
    title: "Stop the timer",
    group: "Timer",
    keys: "⌘.",
    run: async () => {
      const next = await app().run(() => ipc.stopTimer());
      if (next) {
        app().setTimer(next);
        await app().refresh();
      }
    },
  },

  // ─── Planner ────────────────────────────────────────────────────────
  { id: "span-1", title: "1-day span", group: "Planner", keys: "1", run: () => app().setSpan(1), when: inPlanner },
  { id: "span-3", title: "3-day span", group: "Planner", keys: "3", run: () => app().setSpan(3), when: inPlanner },
  { id: "span-7", title: "7-day span", group: "Planner", keys: "7", run: () => app().setSpan(7), when: inPlanner },
  { id: "today", title: "Jump to today", group: "Planner", keys: "T", run: () => app().setAnchor(fmt.today()) },
  { id: "prev", title: "Previous period", group: "Planner", keys: "←", run: () => app().shiftPeriod(-1), when: inPlanner },
  { id: "next", title: "Next period", group: "Planner", keys: "→", run: () => app().shiftPeriod(1), when: inPlanner },
  { id: "zoom-in", title: "Taller hours", group: "Planner", keys: "⌘+", run: () => app().setHourHeight(app().hourHeight + 8) },
  { id: "zoom-out", title: "Shorter hours", group: "Planner", keys: "⌘−", run: () => app().setHourHeight(app().hourHeight - 8) },
  {
    id: "block-move-later",
    title: "Move block 15 minutes later",
    group: "Planner",
    keys: "↓",
    when: hasBlock,
    run: () => shiftBlock(15),
  },
  {
    id: "block-move-earlier",
    title: "Move block 15 minutes earlier",
    group: "Planner",
    keys: "↑",
    when: hasBlock,
    run: () => shiftBlock(-15),
  },
  {
    id: "block-grow",
    title: "Grow block by 15 minutes",
    group: "Planner",
    keys: "⇧↓",
    when: hasBlock,
    run: () => resizeBlock(15),
  },
  {
    id: "block-shrink",
    title: "Shrink block by 15 minutes",
    group: "Planner",
    keys: "⇧↑",
    when: hasBlock,
    run: () => resizeBlock(-15),
  },
  {
    id: "block-duplicate",
    title: "Duplicate block",
    group: "Planner",
    keys: "D",
    when: hasBlock,
    run: async () => {
      const id = app().selectedBlockId!;
      await app().run(() => ipc.duplicateBlock(id), "Couldn't duplicate that block.");
      await app().refresh();
    },
  },
  {
    id: "block-unschedule",
    title: "Unschedule block",
    group: "Planner",
    keys: "⌫",
    when: hasBlock,
    run: async () => {
      const id = app().selectedBlockId!;
      const token = await app().run(() => ipc.unscheduleBlock(id), "Couldn't unschedule it.");
      if (token) {
        undoableDelete(token.label, token);
        app().selectBlock(null);
        await app().refresh();
      }
    },
  },
  {
    id: "schedule-selected",
    title: "Schedule the selected task",
    group: "Planner",
    keys: "S",
    run: scheduleSelectedTask,
  },

  // ─── Tasks ──────────────────────────────────────────────────────────
  {
    id: "task-complete",
    title: "Complete the selected task",
    group: "Tasks",
    keys: "X",
    when: hasTask,
    run: async () => {
      const id = app().selectedTaskIds[0]!;
      const task = app().backlog?.tasks.find((t) => t.id === id);
      await app().run(() => ipc.setTaskStatus(id, task?.status === "done" ? "open" : "done"));
      await app().refresh();
    },
  },
  {
    id: "task-open",
    title: "Open task detail",
    group: "Tasks",
    keys: "Enter",
    when: hasTask,
    run: () => void app().openDetail(app().selectedTaskIds[0]!),
  },
  {
    id: "task-delete",
    title: "Delete the selected task",
    group: "Tasks",
    keys: "⌫",
    when: hasTask,
    run: async () => {
      const id = app().selectedTaskIds[0]!;
      const token = await app().run(() => ipc.deleteTask(id), "Couldn't delete that task.");
      if (token) {
        undoableDelete(token.label, token);
        await app().refresh();
      }
    },
  },
  ...([1, 2, 3] as const).map((p) => ({
    id: `priority-${p}`,
    title: `Set priority ${["", "low", "medium", "high"][p]}`,
    group: "Tasks" as const,
    keys: String(p),
    when: hasTask,
    run: async () => {
      const id = app().selectedTaskIds[0]!;
      await app().run(() => ipc.updateTask(id, { priority: p }));
      await app().refresh();
    },
  })),

  // ─── Data ───────────────────────────────────────────────────────────
  {
    id: "integrity",
    title: "Run an integrity check",
    group: "Data",
    run: async () => {
      const r = await app().run(() => ipc.runIntegrityCheck(), "Couldn't run the check.");
      if (r) app().toast(`Integrity: ${r.quickCheck}, ${r.foreignKeyViolations} FK violations.`);
    },
  },
  {
    id: "export-json",
    title: "Export everything as JSON",
    group: "Data",
    run: async () => {
      const r = await app().run(
        () => ipc.exportData("json", "fruit-export.json", fmt.tz()),
        "Couldn't export.",
      );
      if (r) app().toast(`Exported to ${r.paths[0]}`);
    },
  },
];

function inPlanner() {
  return app().view === "planner";
}
function hasBlock() {
  return app().selectedBlockId != null;
}
function hasTask() {
  return app().selectedTaskIds.length > 0;
}

function focusCapture() {
  const { view, go } = app();
  if (view !== "tasks") go("tasks");
  requestAnimationFrame(() => document.getElementById("capture-input")?.focus());
}

function findBlock(id: string) {
  return app()
    .week?.days.flatMap((d) => d.blocks)
    .find((b) => b.block.id === id);
}

async function shiftBlock(minutes: number) {
  const id = app().selectedBlockId;
  const block = id ? findBlock(id) : null;
  if (!block) return;
  await app().run(
    () => ipc.moveBlock(block.block.id, block.block.startsAt + minutes * 60_000),
    "Couldn't move that block.",
  );
  await app().refresh();
}

async function resizeBlock(minutes: number) {
  const id = app().selectedBlockId;
  const block = id ? findBlock(id) : null;
  if (!block) return;
  await app().run(
    () => ipc.resizeBlock(block.block.id, Math.max(300, block.block.durationSec + minutes * 60)),
    "Couldn't resize that block.",
  );
  await app().refresh();
}

/**
 * `S` then arrows is a complete substitute for dragging (§4.3, F1). It drops
 * the task into the next free slot; the arrows then nudge it.
 */
async function scheduleSelectedTask() {
  const { selectedTaskIds, backlog, run, refresh, toast, go } = app();
  const id = selectedTaskIds[0];
  const task = backlog?.tasks.find((t) => t.id === id);
  if (!task) {
    toast("Select a task first, then press S.");
    return;
  }
  const now = new Date();
  const rounded = new Date(now);
  rounded.setMinutes(Math.ceil(now.getMinutes() / 15) * 15, 0, 0);
  const block = await run(
    () =>
      ipc.scheduleBlock({
        taskId: task.id,
        startsAt: rounded.getTime(),
        durationSec: task.estimateSec ?? 1800,
        tz: fmt.tz(),
      }),
    "Couldn't schedule that task.",
  );
  if (block) {
    go("planner");
    app().selectBlock(block.id);
    toast(`Plotted "${task.title}" at ${fmt.clock(block.startsAt)} — arrows nudge it 15m.`);
    await refresh();
  }
}

export const commandById = (id: string) => COMMANDS.find((c) => c.id === id);
