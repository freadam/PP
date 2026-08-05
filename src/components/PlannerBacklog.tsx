/**
 * The Planner's backlog column (wireframe screen 2).
 *
 * The same tasks the global sidebar lists, but *inside the canvas*, next to the
 * grid they get dragged onto. That placement is the point: "what still needs a
 * slot" and "where are the slots" are one question, and answering it should not
 * involve looking at two different regions of the window.
 *
 * It is deliberately short — capture, then Today / This week / No date. Anything
 * longer belongs on Projects, and a backlog you scroll is one you stop reading.
 */

import { useState } from "react";
import { useApp } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type { TaskRow } from "../lib/types";
import { useStartTaskDrag } from "../lib/useTaskDrag";

/** The wireframe's three groups. Overdue folds into Today — it is still today's problem. */
const GROUPS = ["Today", "This week", "No date"] as const;

function groupOf(task: TaskRow, today: string, weekEnd: string): (typeof GROUPS)[number] | null {
  if (!task.dueDate) return task.isScheduled ? null : "No date";
  if (task.dueDate <= today) return "Today";
  if (task.dueDate <= weekEnd) return "This week";
  return null;
}

export function PlannerBacklog() {
  const backlog = useApp((s) => s.backlog);
  const selected = useApp((s) => s.selectedTaskIds);
  const selectTask = useApp((s) => s.selectTask);
  const run = useApp((s) => s.run);
  const refresh = useApp((s) => s.refresh);
  const startDrag = useStartTaskDrag();
  const [capture, setCapture] = useState("");

  const today = fmt.today();
  const weekEnd = fmt.addDays(today, 7);
  const open = (backlog?.tasks ?? []).filter((t) => t.status === "open");

  const submit = async () => {
    const title = capture.trim();
    if (!title) return;
    setCapture("");
    // The full grammar, not a bare title: `~45m #dev !!` all work here, because
    // there is one parser and it lives in Rust.
    const parsed = await run(() => ipc.parseCapture(title, fmt.tz()));
    await run(
      () =>
        ipc.createTask({
          title: parsed?.title ?? title,
          estimateSec: parsed?.estimateSec ?? null,
          dueDate: parsed?.dueDate ?? null,
          priority: parsed?.priority ?? 0,
        }),
      "Couldn't capture that.",
    );
    await refresh();
  };

  return (
    <aside className="planner-backlog">
      <h3>Backlog</h3>
      <input
        value={capture}
        placeholder="Capture a task…"
        onChange={(e) => setCapture(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") void submit();
        }}
      />

      {GROUPS.map((group) => {
        const rows = open.filter((t) => groupOf(t, today, weekEnd) === group);
        if (!rows.length) return null;
        return (
          <div key={group} className="stack" style={{ gap: 4 }}>
            <span className="micro" style={{ color: "var(--muted)" }}>
              {group}
            </span>
            {rows.slice(0, 6).map((t) => (
              <button
                key={t.id}
                className="task-card"
                aria-selected={selected.includes(t.id)}
                onClick={() => selectTask(t.id)}
                // Drag is an alternative, never the only path: `S` plots the
                // selected task and the arrows nudge it (§4.3, U1).
                onPointerDown={(e) => startDrag(e, t)}
                title="Drag onto the grid, or press S to plot it"
              >
                <b>{t.title}</b>
                <span className="micro" style={{ color: "var(--muted)" }}>
                  {t.isRollover
                    ? "Rollover"
                    : t.estimateSec
                      ? `~ ${fmt.duration(t.estimateSec)}`
                      : "no estimate"}
                  {t.priority > 0 && ` · P${4 - t.priority}`}
                </span>
              </button>
            ))}
          </div>
        );
      })}

      {open.length === 0 && (
        <p className="caption">
          Nothing waiting for a slot. Capture something above, or press{" "}
          <span className="kbd">C</span>.
        </p>
      )}
    </aside>
  );
}
