/**
 * The contextual sidebar (§2.2, §3.2).
 *
 * On the Planner it is the backlog you plot from; on Tasks it is projects,
 * tags and Recently deleted. Collapses to a 48px icon strip below 1130px so
 * seven day columns still clear the 110px minimum legible width (§5.8).
 */

import { useEffect, useState } from "react";
import { useApp } from "../store/app";
import * as ipc from "../lib/ipc";
import * as fmt from "../lib/format";
import type { DeletedRow, TaskRow } from "../lib/types";
import { undoableDelete } from "../store/app";
import { BREAK_SIDEBAR_COLLAPSE, useViewportWidth } from "../lib/useViewport";
import { useStartTaskDrag } from "../lib/useTaskDrag";

export function Sidebar() {
  const view = useApp((s) => s.view);
  const manuallyCollapsed = useApp((s) => s.sidebarCollapsed);
  const width = useApp((s) => s.sidebarWidth);
  const viewport = useViewportWidth();

  // Below 1130px the sidebar collapses to an icon strip — the *content*
  // switches too, not only the width, or it would just overflow its box.
  const collapsed = manuallyCollapsed || viewport < BREAK_SIDEBAR_COLLAPSE;

  if (view === "reports" || view === "settings" || view === "activity") return null;

  return (
    <aside
      className="sidebar"
      data-collapsed={collapsed}
      /* A custom property rather than an inline `width`, so the collapsed
         rule in the stylesheet still wins. */
      style={{ ["--sidebar-w" as string]: `${width}px` }}
      aria-label="Sidebar"
    >
      {collapsed ? <CollapsedStrip /> : view === "planner" ? <BacklogPane /> : <ProjectsPane />}
    </aside>
  );
}

function CollapsedStrip() {
  const toggle = useApp((s) => s.toggleSidebar);
  return (
    <button className="rail-item" onClick={toggle} aria-label="Expand sidebar">
      »
    </button>
  );
}

/** The backlog you plot from. Dragging is one path; `S` then arrows is the other. */
function BacklogPane() {
  const backlog = useApp((s) => s.backlog);
  const openDetail = useApp((s) => s.openDetail);
  const startTaskDrag = useStartTaskDrag();

  const unscheduled = (backlog?.tasks ?? []).filter((t) => !t.isScheduled).slice(0, 60);

  return (
    <>
      <div className="row spread">
        <span className="label sidebar-group-title">Backlog</span>
        <span className="caption">{unscheduled.length}</span>
      </div>
      {unscheduled.length === 0 ? (
        <p className="caption">
          Everything with a date is plotted. Press <span className="kbd">C</span> to capture
          something new.
        </p>
      ) : (
        <div className="stack" style={{ gap: 2 }}>
          {unscheduled.map((t) => (
            <button
              key={t.id}
              className="sidebar-row draggable"
              onPointerDown={(e) => startTaskDrag(e, t)}
              onClick={() => void openDetail(t.id)}
              title="Drag onto the planner, or select it and press S"
            >
              <span className="grow" style={{ overflow: "hidden", textOverflow: "ellipsis" }}>
                {t.title}
              </span>
              {t.estimateSec != null && (
                <span className="data" style={{ color: "var(--muted)" }}>
                  {fmt.duration(t.estimateSec)}
                </span>
              )}
            </button>
          ))}
        </div>
      )}
    </>
  );
}

function ProjectsPane() {
  const projects = useApp((s) => s.projects);
  const filter = useApp((s) => s.projectFilter);
  const backlog = useApp((s) => s.backlog);
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  /** The project being renamed, and the text so far. */
  const [renaming, setRenaming] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [deleted, setDeleted] = useState<DeletedRow[]>([]);

  useEffect(() => {
    void ipc.getDeleted().then(setDeleted).catch(() => setDeleted([]));
  }, [backlog]);

  const setFilter = (id: string | null) => {
    useApp.setState({ projectFilter: id });
    void useApp.getState().loadTasks();
  };

  const create = async () => {
    if (!name.trim()) return setCreating(false);
    const { run, loadProjects } = useApp.getState();
    await run(() => ipc.createProject(name.trim()), "Couldn't create that project.");
    setName("");
    setCreating(false);
    await loadProjects();
  };

  /**
   * Commit a rename.
   *
   * A project's name is read in more places than this list — the capture
   * parser matches `#project` against it, and the Day, Reports and detail
   * views all print it — so this refreshes everything rather than only the
   * sidebar it was typed into.
   */
  const rename = async (id: string, before: string) => {
    const next = draft.trim();
    setRenaming(null);
    if (!next || next === before) return;
    const { run, refresh } = useApp.getState();
    const ok = await run(() => ipc.updateProject(id, { name: next }), "Couldn't rename that project.");
    if (ok) await refresh();
  };

  return (
    <>
      <div className="stack" style={{ gap: 2 }}>
        <button
          className="sidebar-row"
          aria-selected={filter === null}
          onClick={() => setFilter(null)}
        >
          All tasks
          <span className="count data">{backlog?.tasks.length ?? 0}</span>
        </button>
        <button
          className="sidebar-row"
          aria-selected={filter === "inbox"}
          onClick={() => setFilter("inbox")}
        >
          Inbox
        </button>
      </div>

      <div>
        <div className="label sidebar-group-title">Projects</div>
        <div className="stack" style={{ gap: 2 }}>
          {projects.map((p) => {
            const pct =
              p.weeklyTargetSec && p.weeklyTargetSec > 0
                ? Math.min(100, (p.weekTrackedSec / p.weeklyTargetSec) * 100)
                : null;
            return (
              <div key={p.id}>
                {renaming === p.id ? (
                  <input
                    autoFocus
                    value={draft}
                    aria-label={`Rename ${p.name}`}
                    onChange={(e) => setDraft(e.target.value)}
                    onBlur={() => void rename(p.id, p.name)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void rename(p.id, p.name);
                      if (e.key === "Escape") setRenaming(null);
                    }}
                    style={{ width: "100%" }}
                  />
                ) : (
                  <div className="sidebar-row-group">
                    <button
                      className="sidebar-row"
                      aria-selected={filter === p.id}
                      onClick={() => setFilter(p.id)}
                      /* The shortcut for people who already expect it. The
                         button beside it is the discoverable path — this is
                         not the only way in. */
                      onDoubleClick={() => {
                        setDraft(p.name);
                        setRenaming(p.id);
                      }}
                    >
                      <span className="dot" style={{ background: p.colour }} />
                      <span
                        className="grow"
                        style={{ overflow: "hidden", textOverflow: "ellipsis" }}
                      >
                        {p.name}
                      </span>
                      <span className="micro" style={{ color: "var(--faint)" }}>
                        {p.kind}
                      </span>
                      <span className="count data">{p.openTaskCount}</span>
                    </button>
                    <button
                      className="btn btn-quiet btn-icon row-action"
                      aria-label={`Rename ${p.name}`}
                      title="Rename"
                      onClick={() => {
                        setDraft(p.name);
                        setRenaming(p.id);
                      }}
                    >
                      ✎
                    </button>
                  </div>
                )}
                {pct != null && (
                  <div
                    className="target-bar"
                    title={`${fmt.duration(p.weekTrackedSec)} of ${fmt.duration(p.weeklyTargetSec!)} this week`}
                  >
                    <i style={{ width: `${pct}%` }} />
                    <b style={{ left: "100%" }} />
                  </div>
                )}
              </div>
            );
          })}
        </div>

        {creating ? (
          <input
            autoFocus
            value={name}
            placeholder="Project name"
            onChange={(e) => setName(e.target.value)}
            onBlur={() => void create()}
            onKeyDown={(e) => {
              if (e.key === "Enter") void create();
              if (e.key === "Escape") setCreating(false);
            }}
            style={{ width: "100%", marginTop: 8 }}
          />
        ) : (
          <button className="btn" style={{ marginTop: 8 }} onClick={() => setCreating(true)}>
            + New project
          </button>
        )}
      </div>

      <div>
        <div className="label sidebar-group-title">Recently deleted</div>
        {deleted.length === 0 ? (
          <p className="caption">Nothing deleted in the last 30 days.</p>
        ) : (
          <div className="stack" style={{ gap: 2 }}>
            {deleted.slice(0, 8).map((d) => (
              <button
                key={`${d.entity}:${d.id}`}
                className="sidebar-row"
                onClick={() => {
                  undoableDelete(`Restored ${d.title}`, {
                    entity: d.entity,
                    id: d.id,
                    label: d.title,
                    at: Date.now(),
                  });
                  void ipc
                    .restore({ entity: d.entity, id: d.id, label: d.title, at: Date.now() })
                    .then(() => useApp.getState().refresh());
                }}
              >
                <span className="grow" style={{ overflow: "hidden", textOverflow: "ellipsis" }}>
                  {d.title}
                </span>
                <span className="micro" style={{ color: "var(--faint)" }}>
                  restore
                </span>
              </button>
            ))}
          </div>
        )}
      </div>
    </>
  );
}

export function taskDue(task: TaskRow): { text: string; overdue: boolean } | null {
  const date = task.dueDate ?? (task.dueAt ? fmt.toLocalDate(new Date(task.dueAt)) : null);
  if (!date) return null;
  const overdue = date < fmt.today() && task.status === "open";
  const text = task.dueAt ? fmt.clock(task.dueAt) : date === fmt.today() ? "today" : date.slice(5);
  return { text, overdue };
}
