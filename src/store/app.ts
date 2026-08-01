/**
 * Client state (§6.9).
 *
 * Zustand is a **view-model cache over typed DTOs**. It holds no SQL and no
 * business logic: drift, groupings, takeaway lines and parse results all
 * arrive already computed. The write path is uniform for every mutation:
 *
 *   optimistic patch → invoke(command) → reconcile with the returned DTO
 *                                      → on failure revert + error toast
 */

import { create } from "zustand";
import * as ipc from "../lib/ipc";
import { CommandError } from "../lib/ipc";
import * as fmt from "../lib/format";
import { BREAK_DETAIL_COLUMN } from "../lib/useViewport";
import type {
  BacklogView,
  LocalDate,
  ProjectRow,
  ReportBundle,
  TaskDetail,
  TaskRow,
  TimerState,
  UndoToken,
  WeekView,
} from "../lib/types";

export type ViewName = "planner" | "tasks" | "activity" | "reports" | "settings";

/**
 * A task being dragged toward the planner (§4.3 — "sidebar backlog item" and
 * "task row" are listed sources, not just existing blocks).
 *
 * It lives in the store because the drag starts in one component and lands in
 * another; keeping it local to either would mean the Planner could never see a
 * drag that began in the Sidebar.
 */
export interface TaskDrag {
  taskId: string;
  title: string;
  durationSec: number;
}
export type Overlay = null | "focus" | "reconcile" | "palette" | "detail" | "shortcuts";

export interface Toast {
  id: number;
  message: string;
  action?: { label: string; run: () => void };
  tone: "info" | "danger";
  /** 8s, paused on hover *and* on keyboard focus (§4.6, U5). */
  expiresAt: number;
}

interface UndoEntry {
  label: string;
  at: number;
  /** Every mutating command returns an inverse (§4.6). */
  inverse: () => Promise<void>;
  /** Consecutive edits to the same field within 1s collapse (§4.6). */
  coalesceKey?: string;
}

interface AppState {
  // ─── shell ───────────────────────────────────────────────────────────
  view: ViewName;
  overlay: Overlay;
  theme: "dark" | "light" | "system";
  sidebarWidth: number;
  sidebarCollapsed: boolean;
  ready: boolean;
  offline: boolean;

  // ─── planner ─────────────────────────────────────────────────────────
  span: 1 | 3 | 7;
  anchor: LocalDate;
  hourHeight: number;
  week: WeekView | null;
  selectedBlockId: string | null;
  taskDrag: TaskDrag | null;

  // ─── tasks ───────────────────────────────────────────────────────────
  backlog: BacklogView | null;
  projects: ProjectRow[];
  projectFilter: string | null;
  selectedTaskIds: string[];

  // ─── detail ──────────────────────────────────────────────────────────
  detail: TaskDetail | null;
  detailTab: "note" | "sessions" | "subtasks";
  noteBuffer: string;

  // ─── timer, reports, undo, toasts ────────────────────────────────────
  timer: TimerState;
  reports: ReportBundle | null;
  reconcileDate: LocalDate | null;
  unreconciled: LocalDate[];
  undoStack: UndoEntry[];
  redoStack: UndoEntry[];
  toasts: Toast[];

  // ─── actions ─────────────────────────────────────────────────────────
  boot: () => Promise<void>;
  go: (view: ViewName) => void;
  setOverlay: (o: Overlay) => void;
  setTheme: (t: "dark" | "light" | "system") => void;
  setSpan: (s: 1 | 3 | 7) => void;
  setAnchor: (d: LocalDate) => void;
  shiftPeriod: (dir: -1 | 1) => void;
  setHourHeight: (h: number) => void;
  toggleSidebar: () => void;

  loadWeek: () => Promise<void>;
  loadTasks: () => Promise<void>;
  loadProjects: () => Promise<void>;
  loadReports: () => Promise<void>;
  openDetail: (taskId: string) => Promise<void>;
  closeDetail: () => void;
  setDetailTab: (t: "note" | "sessions" | "subtasks") => void;
  setNoteBuffer: (s: string) => void;
  flushNote: () => Promise<void>;

  selectBlock: (id: string | null) => void;
  setTaskDrag: (d: TaskDrag | null) => void;
  selectTask: (id: string, mode?: "replace" | "toggle" | "range") => void;

  toggleTimer: (taskId: string, blockId?: string | null) => Promise<void>;
  setTimer: (t: TimerState) => void;

  toast: (message: string, opts?: Partial<Omit<Toast, "id" | "message">>) => void;
  dismissToast: (id: number) => void;
  pushUndo: (entry: UndoEntry) => void;
  undo: () => Promise<void>;
  redo: () => Promise<void>;

  /** The one place a failed command turns into user-visible copy (§3.10). */
  run: <T>(work: () => Promise<T>, onError?: string) => Promise<T | null>;
  refresh: () => Promise<void>;
}

let toastSeq = 0;

export const useApp = create<AppState>((set, get) => ({
  view: "planner",
  overlay: null,
  theme: "dark",
  sidebarWidth: 260,
  sidebarCollapsed: false,
  ready: false,
  offline: true,

  span: 7,
  anchor: fmt.today(),
  hourHeight: 56,
  week: null,
  selectedBlockId: null,
  taskDrag: null,

  backlog: null,
  projects: [],
  projectFilter: null,
  selectedTaskIds: [],

  detail: null,
  detailTab: "note",
  noteBuffer: "",

  timer: {
    phase: "idle",
    runTaskId: null,
    taskTitle: null,
    session: null,
    elapsedSec: 0,
    segmentElapsedSec: 0,
    idleFrom: null,
    idleTo: null,
    recoverySessionId: null,
    pomodoro: null,
  },
  reports: null,
  reconcileDate: null,
  unreconciled: [],
  undoStack: [],
  redoStack: [],
  toasts: [],

  async boot() {
    const settings = await ipc.getSettings().catch(() => ({}) as Record<string, unknown>);
    const theme = (settings["general.theme"] as "dark" | "light" | "system") ?? "dark";
    const hourHeight = (settings["planner.hourHeight"] as number) ?? 56;
    const span = (settings["planner.span"] as 1 | 3 | 7) ?? 7;
    fmt.setHour12((settings["general.hour12"] as boolean) ?? false);
    set({ theme, hourHeight, span });
    applyTheme(theme);

    await Promise.all([get().loadWeek(), get().loadTasks(), get().loadProjects()]);
    const timer = await ipc.getTimerState().catch(() => get().timer);
    const unreconciled = await ipc.getUnreconciledDays(fmt.today()).catch(() => []);
    set({ timer, unreconciled, ready: true });

    // §3.11: reconcile-due is a tray badge and a top-bar dot, never a
    // notification, and it never blocks the app.
    if (timer.phase === "recovering") set({ overlay: null });
  },

  go(view) {
    void get().flushNote();
    set({ view, overlay: null });
    if (view === "reports" && !get().reports) void get().loadReports();
  },

  setOverlay(overlay) {
    if (overlay === null) void get().flushNote();
    set({ overlay });
  },

  setTheme(theme) {
    set({ theme });
    applyTheme(theme);
    void ipc.setSetting("general.theme", theme).catch(() => {});
  },

  setSpan(span) {
    set({ span });
    void ipc.setSetting("planner.span", span).catch(() => {});
    void get().loadWeek();
  },

  setAnchor(anchor) {
    set({ anchor });
    void get().loadWeek();
  },

  shiftPeriod(dir) {
    const { span, anchor } = get();
    set({ anchor: fmt.addDays(anchor, dir * span) });
    void get().loadWeek();
  },

  setHourHeight(h) {
    const hourHeight = Math.min(120, Math.max(32, Math.round(h)));
    set({ hourHeight });
    void ipc.setSetting("planner.hourHeight", hourHeight).catch(() => {});
  },

  toggleSidebar() {
    set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed }));
  },

  async loadWeek() {
    const { span, anchor } = get();
    const from = span === 7 ? fmt.weekStart(anchor) : anchor;
    const to = fmt.addDays(from, span - 1);
    const week = await get().run(() => ipc.getWeek(from, to, fmt.tz()));
    if (week) set({ week });
  },

  async loadTasks() {
    const backlog = await get().run(() =>
      ipc.getBacklog(fmt.tz(), get().projectFilter, false),
    );
    if (backlog) set({ backlog });
  },

  async loadProjects() {
    const projects = await get().run(() => ipc.getProjects(fmt.tz()));
    if (projects) set({ projects });
  },

  async loadReports() {
    const to = fmt.today();
    const from = fmt.addDays(to, -27);
    const reports = await get().run(() => ipc.getReports(from, to, fmt.tz()));
    if (reports) set({ reports });
  },

  async openDetail(taskId) {
    const detail = await get().run(() => ipc.getTaskDetail(taskId));
    if (!detail) return;
    set({
      detail,
      noteBuffer: detail.note,
      overlay: window.innerWidth >= BREAK_DETAIL_COLUMN ? null : "detail",
    });
  },

  closeDetail() {
    void get().flushNote();
    set({ detail: null, overlay: null });
  },

  setDetailTab(detailTab) {
    set({ detailTab });
  },

  setNoteBuffer(noteBuffer) {
    set({ noteBuffer });
    scheduleNoteSave(get);
  },

  /** Force-flushed on blur, tab change, view change and close (§6.9). */
  async flushNote() {
    const { detail, noteBuffer } = get();
    if (!detail || noteBuffer === detail.note) return;
    clearTimeout(noteTimer);
    noteTimer = undefined;
    noteWindowStart = 0;
    await get().run(
      () => ipc.saveNote(detail.task.id, noteBuffer),
      "Couldn't save the note.",
    );
    set((s) => (s.detail ? { detail: { ...s.detail, note: noteBuffer } } : {}));
  },

  selectBlock(selectedBlockId) {
    set({ selectedBlockId });
  },

  setTaskDrag(taskDrag) {
    set({ taskDrag });
  },

  selectTask(id, mode = "replace") {
    const current = get().selectedTaskIds;
    if (mode === "toggle") {
      set({
        selectedTaskIds: current.includes(id)
          ? current.filter((t) => t !== id)
          : [...current, id],
      });
      return;
    }
    if (mode === "range" && current.length > 0) {
      const ids = get().backlog?.tasks.map((t) => t.id) ?? [];
      const anchorIdx = ids.indexOf(current[current.length - 1]!);
      const targetIdx = ids.indexOf(id);
      if (anchorIdx >= 0 && targetIdx >= 0) {
        const [lo, hi] = anchorIdx < targetIdx ? [anchorIdx, targetIdx] : [targetIdx, anchorIdx];
        set({ selectedTaskIds: ids.slice(lo, hi + 1) });
        return;
      }
    }
    set({ selectedTaskIds: [id] });
  },

  async toggleTimer(taskId, blockId) {
    const { timer } = get();
    const running =
      timer.phase !== "idle" && (timer.session?.taskId ?? timer.runTaskId) === taskId;
    const next = await get().run(() =>
      running ? ipc.stopTimer() : ipc.startTimer(taskId, blockId ?? null),
    );
    if (next) {
      set({ timer: next });
      await get().refresh();
    }
  },

  setTimer(timer) {
    set({ timer });
  },

  toast(message, opts = {}) {
    const id = ++toastSeq;
    const toast: Toast = {
      id,
      message,
      tone: opts.tone ?? "info",
      action: opts.action,
      expiresAt: Date.now() + 8000,
    };
    set((s) => ({ toasts: [...s.toasts.slice(-3), toast] }));
  },

  dismissToast(id) {
    set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
  },

  pushUndo(entry) {
    set((s) => {
      const top = s.undoStack[s.undoStack.length - 1];
      if (
        entry.coalesceKey &&
        top?.coalesceKey === entry.coalesceKey &&
        entry.at - top.at < 1000
      ) {
        // Cmd+Z after typing a title shouldn't walk back character by character.
        return { undoStack: [...s.undoStack.slice(0, -1), { ...top, at: entry.at }] };
      }
      return { undoStack: [...s.undoStack, entry].slice(-50), redoStack: [] };
    });
  },

  async undo() {
    const stack = get().undoStack;
    const entry = stack[stack.length - 1];
    if (!entry) {
      get().toast("Nothing to undo.");
      return;
    }
    set({ undoStack: stack.slice(0, -1) });
    await get().run(entry.inverse, "Couldn't undo that.");
    await get().refresh();
    get().toast(`Undid: ${entry.label}`);
  },

  async redo() {
    get().toast("Redo isn't wired to a command yet.");
  },

  async run(work, onError) {
    try {
      return await work();
    } catch (e) {
      const err = e as CommandError;
      // What failed · why · the action. Never "Something went wrong."
      const why = err?.message ?? String(e);
      get().toast(onError ? `${onError} ${why}` : why, { tone: "danger" });
      return null;
    }
  },

  async refresh() {
    await Promise.all([get().loadWeek(), get().loadTasks(), get().loadProjects()]);
    const { detail, view } = get();
    if (detail) {
      const fresh = await ipc.getTaskDetail(detail.task.id).catch(() => null);
      if (fresh) set({ detail: fresh });
    }
    if (view === "reports") await get().loadReports();
  },
}));

/* ─── note autosave: 500ms debounce, 3s maxWait (§6.9) ─────────────────── */

let noteTimer: ReturnType<typeof setTimeout> | undefined;
let noteWindowStart = 0;

function scheduleNoteSave(get: () => AppState) {
  const now = Date.now();
  if (!noteWindowStart) noteWindowStart = now;
  clearTimeout(noteTimer);
  const wait = now - noteWindowStart >= 3000 ? 0 : 500;
  noteTimer = setTimeout(() => {
    noteWindowStart = 0;
    void get().flushNote();
  }, wait);
}

/* ─── theme ────────────────────────────────────────────────────────────── */

export function applyTheme(theme: "dark" | "light" | "system") {
  const resolved =
    theme === "system"
      ? window.matchMedia("(prefers-color-scheme: light)").matches
        ? "light"
        : "dark"
      : theme;
  document.documentElement.dataset.theme = resolved;
}

/** Shared helper so delete flows all produce the same 8s undo toast (U5). */
export function undoableDelete(label: string, token: UndoToken) {
  const { toast, pushUndo, refresh } = useApp.getState();
  const inverse = async () => {
    await ipc.restore(token);
    await refresh();
  };
  pushUndo({ label, at: Date.now(), inverse });
  toast(label, { action: { label: "Undo", run: () => void useApp.getState().undo() } });
}

export const selectTaskById = (id: string): TaskRow | undefined =>
  useApp.getState().backlog?.tasks.find((t) => t.id === id);
