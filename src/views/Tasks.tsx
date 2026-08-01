/**
 * Tasks (§3.2) — the backlog and the estimate surface.
 *
 * Group headers show count *and* total estimate: a planner without a total is
 * useless for capacity checks. Delete is not hover-only — hover-only
 * destructive controls are unreachable by keyboard.
 */

import { useEffect, useRef, useState } from "react";
import { useApp, undoableDelete } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type { CapturePreview, TaskRow } from "../lib/types";
import { DriftBadge, DriftRailCompact } from "../components/DriftRail";
import { Empty } from "../components/chrome";
import { taskDue } from "../components/Sidebar";

export function Tasks() {
  const backlog = useApp((s) => s.backlog);
  const projectFilter = useApp((s) => s.projectFilter);

  return (
    <div className="planner">
      <CaptureBar />
      {!backlog || backlog.tasks.length === 0 ? (
        <Empty>
          {projectFilter
            ? "No open tasks in this project."
            : (
              <>
                No tasks yet. Press <span className="kbd">C</span> and type — try{" "}
                <code className="data">Fix login bug #work ~45m !!</code>
              </>
            )}
        </Empty>
      ) : (
        <div className="task-list">
          {backlog.groups
            .filter((g) => g.count > 0)
            .map((group) => (
              <section key={group.key} data-group={group.key}>
                <h2 className="group-head label">
                  {group.label}
                  <span className="data">{group.count}</span>
                  {group.estimateSec > 0 && (
                    <span className="total data">
                      {fmt.duration(group.estimateSec)} estimated
                    </span>
                  )}
                </h2>
                {group.taskIds.map((id) => {
                  const task = backlog.tasks.find((t) => t.id === id);
                  return task ? <TaskRowView key={id} task={task} /> : null;
                })}
              </section>
            ))}
        </div>
      )}
    </div>
  );
}

/**
 * §4.4 — preview chips render live beneath the input, with the matched
 * substring highlighted in place. Nobody presses Enter and discovers a due
 * date they didn't intend.
 *
 * The grammar itself lives in Rust: the resolution rules are where the bugs
 * are, and they need tests (§4.4, §6.9).
 */
export function CaptureBar({ onCreated }: { onCreated?: (t: TaskRow) => void }) {
  const [text, setText] = useState("");
  const [preview, setPreview] = useState<CapturePreview | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!text.trim()) {
      setPreview(null);
      return;
    }
    let live = true;
    // Budget: keystroke to parse preview ≤ 16ms (§7.1).
    void ipc
      .parseCapture(text, fmt.tz())
      .then((p) => live && setPreview(p))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [text]);

  const submit = async () => {
    if (!preview || !preview.title.trim()) return;
    const { run, refresh, pushUndo, toast } = useApp.getState();
    const raw = text;

    const created = await run(
      () =>
        ipc.createTask({
          title: preview.title,
          projectId: preview.projectId,
          estimateSec: preview.estimateSec,
          dueDate: preview.dueDate,
          dueAt: preview.dueAt,
          priority: preview.priority,
          tags: preview.tags,
        }),
      "Couldn't create that task.",
    );
    if (!created) return;

    // §4.4 rule 8 / U2: the raw string survives the undo window, so Cmd+Z puts
    // the text back in the box for editing.
    pushUndo({
      label: `Captured "${created.title}"`,
      at: Date.now(),
      inverse: async () => {
        await ipc.deleteTask(created.id);
        setText(raw);
        inputRef.current?.focus();
      },
    });
    toast(`Added "${created.title}"`, {
      action: { label: "Undo", run: () => void useApp.getState().undo() },
    });
    setText("");
    setPreview(null);
    await refresh();
    onCreated?.(created);
  };

  return (
    <div className="capture">
      <input
        ref={inputRef}
        id="capture-input"
        value={text}
        placeholder="Capture a task — Fix login bug #work ~45m !! ^tomorrow 9am"
        aria-label="Quick capture"
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") void submit();
          if (e.key === "Escape") {
            setText("");
            (e.target as HTMLInputElement).blur();
          }
        }}
      />
      {preview && (
        <div className="chips" aria-live="polite">
          <span className="caption">{preview.title || "…"}</span>
          {preview.chips.map((c, i) => (
            <span key={i} className="chip micro" data-kind={c.kind} data-creates={c.creates}>
              {c.display}
            </span>
          ))}
          {preview.projectCreates && (
            <span className="caption">
              <span className="kbd">Enter</span> also creates #{preview.projectName}
            </span>
          )}
        </div>
      )}
    </div>
  );
}

function TaskRowView({ task }: { task: TaskRow }) {
  const selected = useApp((s) => s.selectedTaskIds.includes(task.id));
  const selectTask = useApp((s) => s.selectTask);
  const openDetail = useApp((s) => s.openDetail);
  const toggleTimer = useApp((s) => s.toggleTimer);
  const timer = useApp((s) => s.timer);
  const projects = useApp((s) => s.projects);
  const [menu, setMenu] = useState(false);

  const project = projects.find((p) => p.id === task.projectId);
  const due = taskDue(task);
  const running = timer.phase === "running" && timer.session?.taskId === task.id;
  const planned = task.estimateSec ?? 0;
  const driftSec = planned > 0 ? task.trackedSec - planned : 0;
  const state =
    planned === 0
      ? "notStarted"
      : task.trackedSec === 0
        ? "notStarted"
        : driftSec > 60
          ? "overrun"
          : Math.abs(driftSec) <= 60
            ? "onEstimate"
            : "inProgress";

  const complete = async () => {
    const { run, refresh, pushUndo } = useApp.getState();
    const next = task.status === "done" ? "open" : "done";
    const updated = await run(() => ipc.setTaskStatus(task.id, next), "Couldn't update that task.");
    if (!updated) return;
    pushUndo({
      label: next === "done" ? `Completed "${task.title}"` : `Reopened "${task.title}"`,
      at: Date.now(),
      inverse: async () => {
        await ipc.setTaskStatus(task.id, task.status);
      },
    });
    await refresh();
  };

  const remove = async () => {
    const { run, refresh } = useApp.getState();
    const token = await run(() => ipc.deleteTask(task.id), "Couldn't delete that task.");
    if (!token) return;
    undoableDelete(token.label, token);
    await refresh();
  };

  return (
    <div
      className="task-row"
      data-selected={selected}
      data-done={task.status === "done"}
      onClick={(e) => selectTask(task.id, e.shiftKey ? "range" : e.metaKey ? "toggle" : "replace")}
      onDoubleClick={() => void openDetail(task.id)}
    >
      <button
        className="check"
        data-checked={task.status === "done"}
        aria-label={task.status === "done" ? "Mark not done" : "Mark done"}
        onClick={(e) => {
          e.stopPropagation();
          void complete();
        }}
      >
        {task.status === "done" ? "✓" : ""}
      </button>

      {task.priority > 0 && (
        <span
          className="priority-mark"
          data-p={task.priority}
          role="img"
          aria-label={`${fmt.priorityLabel(task.priority)} priority`}
        />
      )}

      <button className="task-title" onClick={() => void openDetail(task.id)}>
        {task.title}
      </button>

      {task.subtaskTotal > 0 && (
        <span className="task-meta data micro">
          {task.subtaskDone}/{task.subtaskTotal}
        </span>
      )}

      {project && <span className="dot" style={{ background: project.colour }} title={project.name} />}

      {task.tags.slice(0, 2).map((t) => (
        <span key={t.id} className="tag micro">
          <i style={{ background: t.colour ?? "var(--plot)" }} />
          {t.name}
        </span>
      ))}

      {task.isRollover ? (
        <span className="task-meta micro" title="Doesn't fit one sitting; carries across days">
          Rollover
        </span>
      ) : (
        task.estimateSec != null && (
          <span className="task-meta data">{fmt.duration(task.estimateSec)}</span>
        )
      )}

      {planned > 0 && task.trackedSec > 0 && (
        <>
          <DriftRailCompact
            planned={planned}
            tracked={task.trackedSec}
            driftSec={driftSec}
            state={state}
          />
          <DriftBadge driftSec={driftSec} state={state} />
        </>
      )}

      {due && (
        <span className="due-chip data" data-overdue={due.overdue}>
          {due.text}
        </span>
      )}

      <button
        aria-label={running ? "Stop timer" : "Start timer"}
        onClick={(e) => {
          e.stopPropagation();
          void toggleTimer(task.id);
        }}
        style={{ color: running ? "var(--track)" : "var(--muted)" }}
      >
        {running ? "■" : "▶"}
      </button>

      {/* Delete lives in the overflow menu and on Backspace — never hover-only. */}
      <div style={{ position: "relative" }}>
        <button aria-label="More actions" aria-expanded={menu} onClick={(e) => { e.stopPropagation(); setMenu(!menu); }} style={{ color: "var(--muted)" }}>
          ⋯
        </button>
        {menu && (
          <div
            className="overlay"
            style={{
              position: "absolute",
              right: 0,
              top: 20,
              zIndex: 20,
              background: "var(--surface)",
              border: "1px solid var(--line)",
              borderRadius: "var(--r-panel)",
              padding: 4,
              minWidth: 160,
            }}
          >
            <button className="palette-item" onClick={() => void openDetail(task.id)}>
              Open detail <span className="hint kbd">Enter</span>
            </button>
            <button
              className="palette-item"
              style={{ color: "var(--danger)" }}
              onClick={() => {
                setMenu(false);
                void remove();
              }}
            >
              Delete <span className="hint kbd">⌫</span>
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
