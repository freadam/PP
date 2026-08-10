/**
 * Task detail (§3.3). Third column at ≥1280px, overlay sheet below.
 *
 * The Sessions tab is the one that makes the record honest: every interval is
 * editable and manually addable, and recovered sessions stay visually flagged
 * until the user confirms them.
 */

import { useState } from "react";
import { useApp } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import { Note } from "../components/Note";
import type { SessionRow } from "../lib/types";

export function TaskDetail({ mode }: { mode: "column" | "sheet" }) {
  const detail = useApp((s) => s.detail);
  const tab = useApp((s) => s.detailTab);
  const setTab = useApp((s) => s.setDetailTab);
  const close = useApp((s) => s.closeDetail);
  if (!detail) return null;

  const body = (
    <>
      <div className="sheet-head">
        <TitleField />
        <button className="btn btn-quiet btn-icon" aria-label="Close detail" onClick={close}>
          ✕
        </button>
        <span className="kbd">Esc</span>
      </div>

      <div className="row" style={{ padding: "8px 12px 0", gap: 4 }} role="tablist">
        {(["note", "sessions", "subtasks"] as const).map((t) => (
          <button
            key={t}
            role="tab"
            aria-selected={tab === t}
            className="btn"
            onClick={() => setTab(t)}
          >
            {t[0]!.toUpperCase() + t.slice(1)}
            {t === "sessions" && detail.sessions.length > 0 && (
              <span className="data micro">{detail.sessions.length}</span>
            )}
            {t === "subtasks" && detail.subtasks.length > 0 && (
              <span className="data micro">
                {detail.task.subtaskDone}/{detail.task.subtaskTotal}
              </span>
            )}
          </button>
        ))}
      </div>

      <div className="sheet-body">
        <Fields />
        {tab === "note" && <NoteTab />}
        {tab === "sessions" && <SessionsTab />}
        {tab === "subtasks" && <SubtasksTab />}
      </div>
    </>
  );

  if (mode === "sheet") {
    return (
      <>
        <div className="scrim" onClick={close} />
        <aside className="sheet overlay" role="dialog" aria-label="Task detail">
          {body}
        </aside>
      </>
    );
  }
  return (
    <aside
      className="sheet"
      style={{ position: "relative", width: 360, animation: "none", borderLeft: "1px solid var(--line)" }}
      aria-label="Task detail"
    >
      {body}
    </aside>
  );
}

function TitleField() {
  const detail = useApp((s) => s.detail)!;
  const [value, setValue] = useState(detail.task.title);

  const save = async () => {
    if (value.trim() === detail.task.title || !value.trim()) return;
    const { run, refresh, pushUndo } = useApp.getState();
    const before = detail.task.title;
    await run(() => ipc.updateTask(detail.task.id, { title: value.trim() }), "Couldn't rename it.");
    pushUndo({
      label: "Renamed task",
      at: Date.now(),
      coalesceKey: `title:${detail.task.id}`,
      inverse: async () => {
        await ipc.updateTask(detail.task.id, { title: before });
      },
    });
    await refresh();
  };

  return (
    <input
      className="title grow"
      value={value}
      aria-label="Task title"
      onChange={(e) => setValue(e.target.value)}
      onBlur={() => void save()}
      onKeyDown={(e) => e.key === "Enter" && void save()}
      style={{ background: "transparent", border: 0 }}
    />
  );
}

/** Every field is a real control, keyboard-reachable, saved optimistically. */
function Fields() {
  const detail = useApp((s) => s.detail)!;
  const projects = useApp((s) => s.projects);

  const patch = async (p: Parameters<typeof ipc.updateTask>[1]) => {
    const { run, refresh } = useApp.getState();
    await run(() => ipc.updateTask(detail.task.id, p), "Couldn't save that change.");
    await refresh();
  };

  return (
    <div className="stack" style={{ marginBottom: 16 }}>
      <div className="row">
        <label className="label" style={{ width: 76, color: "var(--muted)" }} htmlFor="d-project">
          Project
        </label>
        <select
          id="d-project"
          className="grow"
          value={detail.task.projectId ?? ""}
          onChange={(e) => void patch({ projectId: e.target.value || null })}
        >
          <option value="">Inbox</option>
          {projects.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </div>

      <div className="row">
        <label className="label" style={{ width: 76, color: "var(--muted)" }} htmlFor="d-estimate">
          Estimate
        </label>
        {/* A fixed ladder, 30 min → 4 Hrs → Rollover. Any off-ladder value the
            capture parser produced (`~45m`) appears as an extra rung rather
            than being rounded away — see `estimateOptions`. */}
        <select
          id="d-estimate"
          className="grow data"
          value={
            detail.task.isRollover
              ? "rollover"
              : detail.task.estimateSec != null
                ? String(detail.task.estimateSec)
                : ""
          }
          onChange={(e) => {
            const v = e.target.value;
            if (v === "rollover") void patch({ isRollover: true });
            else void patch({ estimateSec: v === "" ? null : Number(v) });
          }}
        >
          {fmt.estimateOptions(detail.task.estimateSec).map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
              {o.offLadder ? " (from capture)" : ""}
            </option>
          ))}
        </select>
      </div>

      <div className="row">
        <label className="label" style={{ width: 76, color: "var(--muted)" }} htmlFor="d-priority">
          Priority
        </label>
        <select
          id="d-priority"
          className="grow"
          value={detail.task.priority}
          onChange={(e) => void patch({ priority: Number(e.target.value) })}
        >
          <option value={0}>None</option>
          <option value={1}>Low</option>
          <option value={2}>Medium</option>
          <option value={3}>High</option>
        </select>
      </div>

      <div className="row">
        <label className="label" style={{ width: 76, color: "var(--muted)" }} htmlFor="d-due">
          Due
        </label>
        <input
          id="d-due"
          type="date"
          className="grow"
          value={detail.task.dueDate ?? ""}
          onChange={(e) => void patch({ dueDate: e.target.value || null })}
        />
      </div>

      <div className="row caption">
        <span>
          Tracked <span className="data">{fmt.duration(detail.task.trackedSec)}</span>
        </span>
        {detail.task.isRollover ? (
          <span>· rollover, no single-sitting estimate</span>
        ) : (
          detail.task.estimateSec != null && (
            <span>
              of <span className="data">{fmt.duration(detail.task.estimateSec)}</span> estimated
            </span>
          )
        )}
      </div>
    </div>
  );
}

/** What "compact" means, mirrored from `NOTE_MAX_CHARS` in the core. The core
 *  is the authority — this only decides when to start showing the count. */
const NOTE_MAX = 2000;

function NoteTab() {
  const buffer = useApp((s) => s.noteBuffer);
  const setBuffer = useApp((s) => s.setNoteBuffer);
  const flush = useApp((s) => s.flushNote);
  const detail = useApp((s) => s.detail)!;
  const [preview, setPreview] = useState(false);
  const used = [...buffer].length;
  const over = used > NOTE_MAX;

  const makeSubtask = async (line: string) => {
    const { run, refresh } = useApp.getState();
    if (!line.trim()) return;
    await run(
      () => ipc.createTask({ title: line.trim(), parentId: detail.task.id }),
      "Couldn't create that subtask.",
    );
    await refresh();
  };

  return (
    <div className="stack">
      <div className="row spread">
        <span className="label" style={{ color: "var(--muted)" }}>
          Note
        </span>
        {/* The count appears once it is close to mattering. Shown before the
            save, not after it: a limit you only meet at the moment you lose
            work is not a limit, it is a trap. */}
        {(over || used > NOTE_MAX * 0.8) && (
          <span className="data micro" style={{ color: over ? "var(--over)" : "var(--muted)" }}>
            {used} / {NOTE_MAX} characters{over ? " — too long to save" : ""}
          </span>
        )}
        <button className="btn" aria-pressed={preview} onClick={() => setPreview(!preview)}>
          {preview ? "Edit" : "Read"}
        </button>
      </div>
      {preview ? (
        <Note source={buffer} />
      ) : (
        <textarea
          value={buffer}
          aria-label="Note"
          placeholder="A note, in plain text. Cmd+Enter turns the current line into a subtask."
          onChange={(e) => setBuffer(e.target.value)}
          onBlur={() => void flush()}
          onKeyDown={(e) => {
            if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
              e.preventDefault();
              const el = e.currentTarget;
              const upto = el.value.slice(0, el.selectionStart ?? 0);
              const line = upto.slice(upto.lastIndexOf("\n") + 1);
              void makeSubtask(line.replace(/^[-*\s]+/, ""));
            }
          }}
          style={{ minHeight: 220, width: "100%", resize: "vertical", lineHeight: 1.6 }}
        />
      )}
    </div>
  );
}

function SessionsTab() {
  const detail = useApp((s) => s.detail)!;
  const [adding, setAdding] = useState(false);

  if (detail.sessions.length === 0 && !adding) {
    return (
      <div className="stack">
        <p className="caption">
          No tracked time yet. Press <span className="kbd">Space</span> to start the timer.
        </p>
        <button className="btn" onClick={() => setAdding(true)}>
          Add a session by hand
        </button>
      </div>
    );
  }

  return (
    <div className="stack">
      {detail.sessions.map((s) => (
        <SessionRowView key={s.id} session={s} />
      ))}
      {adding ? (
        <ManualSessionForm taskId={detail.task.id} onDone={() => setAdding(false)} />
      ) : (
        <button className="btn" onClick={() => setAdding(true)}>
          + Add session
        </button>
      )}
    </div>
  );
}

function SessionRowView({ session }: { session: SessionRow }) {
  const confirm = async () => {
    const { run, refresh } = useApp.getState();
    await run(() => ipc.updateSession(session.id, { isConfirmed: true }), "Couldn't confirm it.");
    await refresh();
  };
  const remove = async () => {
    const { run, refresh, toast } = useApp.getState();
    const token = await run(() => ipc.deleteSession(session.id), "Couldn't delete that session.");
    if (token) {
      toast("Deleted session", {
        action: {
          label: "Undo",
          run: () => void ipc.restore(token).then(() => useApp.getState().refresh()),
        },
      });
      await refresh();
    }
  };

  return (
    <div
      className="row"
      style={{
        borderBottom: "1px solid var(--line)",
        paddingBottom: 6,
        // Recovered sessions stay flagged until the user confirms them (§3.3).
        borderLeft: session.isConfirmed ? undefined : "2px solid var(--over)",
        paddingLeft: session.isConfirmed ? undefined : 6,
      }}
    >
      <span className="data">
        {fmt.clock(session.startedAt)}
        {session.endedAt ? `–${fmt.clock(session.endedAt)}` : " · running"}
      </span>
      <span className="data" style={{ color: "var(--muted)" }}>
        {fmt.duration(session.elapsedSec)}
      </span>
      <span className="micro" style={{ color: "var(--faint)" }}>
        {session.source}
      </span>
      {session.blockId && (
        <span className="micro" style={{ color: "var(--plot)" }} title="Linked to a planned block">
          plotted
        </span>
      )}
      <span className="grow" />
      {!session.isConfirmed && (
        <button className="btn" onClick={() => void confirm()}>
          Confirm
        </button>
      )}
      <button className="btn btn-quiet btn-icon" aria-label="Delete session" onClick={() => void remove()}>
        ✕
      </button>
    </div>
  );
}

function ManualSessionForm({ taskId, onDone }: { taskId: string; onDone: () => void }) {
  const [date, setDate] = useState(fmt.today());
  const [from, setFrom] = useState("09:00");
  const [to, setTo] = useState("10:00");

  const save = async () => {
    const { run, refresh } = useApp.getState();
    const startedAt = new Date(`${date}T${from}:00`).getTime();
    const endedAt = new Date(`${date}T${to}:00`).getTime();
    const created = await run(
      () => ipc.addSession({ taskId, startedAt, endedAt }),
      "Couldn't add that session.",
    );
    if (created) {
      await refresh();
      onDone();
    }
  };

  return (
    <div className="row" style={{ flexWrap: "wrap" }}>
      <input type="date" value={date} onChange={(e) => setDate(e.target.value)} aria-label="Date" />
      <input type="time" value={from} onChange={(e) => setFrom(e.target.value)} aria-label="From" />
      <input type="time" value={to} onChange={(e) => setTo(e.target.value)} aria-label="To" />
      <button className="btn btn-primary" onClick={() => void save()}>
        Add
      </button>
      <button className="btn" onClick={onDone}>
        Cancel
      </button>
    </div>
  );
}

function SubtasksTab() {
  const detail = useApp((s) => s.detail)!;
  const openDetail = useApp((s) => s.openDetail);
  const [title, setTitle] = useState("");

  const add = async () => {
    if (!title.trim()) return;
    const { run, refresh } = useApp.getState();
    await run(
      () => ipc.createTask({ title: title.trim(), parentId: detail.task.id }),
      "Couldn't add that subtask.",
    );
    setTitle("");
    await refresh();
  };

  const trackedTotal = detail.subtasks.reduce((n, t) => n + t.trackedSec, 0);
  const estimateTotal = detail.subtasks.reduce((n, t) => n + (t.estimateSec ?? 0), 0);

  return (
    <div className="stack">
      {detail.subtasks.length > 0 && (
        <p className="caption data">
          {detail.task.subtaskDone}/{detail.task.subtaskTotal} · {fmt.duration(trackedTotal)} of{" "}
          {fmt.duration(estimateTotal)}
        </p>
      )}
      {detail.subtasks.map((t) => (
        <div key={t.id} className="row">
          <button
            className="check"
            data-checked={t.status === "done"}
            aria-label="Toggle subtask"
            onClick={() => {
              const { run, refresh } = useApp.getState();
              void run(() => ipc.setTaskStatus(t.id, t.status === "done" ? "open" : "done")).then(
                refresh,
              );
            }}
          >
            {t.status === "done" ? "✓" : ""}
          </button>
          <button className="grow task-title" onClick={() => void openDetail(t.id)}>
            {t.title}
          </button>
          {t.trackedSec > 0 && <span className="data caption">{fmt.duration(t.trackedSec)}</span>}
        </div>
      ))}
      <input
        value={title}
        placeholder="Add a subtask"
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && void add()}
      />
    </div>
  );
}
