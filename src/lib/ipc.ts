/**
 * The only place the renderer talks to the backend (§6.8).
 *
 * Every call is a typed, intent-based command. There are no SQL strings here,
 * and the capability file lists exactly the commands below — a strictly smaller
 * surface than a webview holding `sql:allow-execute` next to a markdown renderer.
 *
 * Outside a Tauri window (`npm run dev` in a browser) this falls back to a
 * **fixture** produced by the real Rust code — see `src/dev/fixtures.json` and
 * `cargo run --bin dump-fixtures`. It is a recording, not a second
 * implementation: writes are refused with an explanatory error rather than
 * being simulated, because a JS reimplementation of the command layer is
 * exactly the drift this architecture exists to prevent.
 */

import type {
  BacklogView,
  CapturePreview,
  CollisionPolicy,
  DayReview,
  DeletedRow,
  IdleActionKind,
  IntegrityReport,
  LocalDate,
  Millis,
  NewTask,
  Page,
  ProjectRow,
  ReconcileAction,
  ReconcileItem,
  RecoveryActionKind,
  ReportBundle,
  SearchResults,
  SessionRow,
  Status,
  TagRow,
  TaskDetail,
  TaskPatch,
  TaskQuery,
  TaskRow,
  TimerState,
  UndoToken,
  WeekView,
  WireError,
} from "./types";
import type {
  ActivityDay,
  AreaKind,
  Contribution,
  DayView,
  ExcelOptions,
  ExcelPreview,
  ExcelExportResult,
  LifeAreaRow,
  LifeEntryRow,
  MonthView,
  NewLifeEntry,
  ActivityStatus,
  BlockRow,
  IcsImportSummary,
  NewBlock,
  RrulePreset,
  SeriesScope,
} from "./types";

type Args = Record<string, unknown>;

export const isDesktop = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export class CommandError extends Error {
  code: string;
  constructor(wire: WireError) {
    super(wire.message);
    this.code = wire.code;
    this.name = "CommandError";
  }
}

let fixtures: Record<string, unknown> | null = null;

async function loadFixtures(): Promise<Record<string, unknown>> {
  if (!fixtures) {
    fixtures = (await import("../dev/fixtures.json")).default as Record<string, unknown>;
  }
  return fixtures;
}

async function call<T>(command: string, args: Args = {}): Promise<T> {
  if (isDesktop()) {
    const { invoke } = await import("@tauri-apps/api/core");
    try {
      return (await invoke(command, args)) as T;
    } catch (raw) {
      if (raw && typeof raw === "object" && "code" in raw && "message" in raw) {
        throw new CommandError(raw as WireError);
      }
      throw new CommandError({ code: "unknown", message: String(raw) });
    }
  }

  const data = await loadFixtures();
  if (command in data) return structuredClone(data[command]) as T;
  throw new CommandError({
    code: "fixture",
    // §3.10 error copy: what failed · why · the action.
    message: `Can't ${command.replace(/_/g, " ")} in browser preview. This build has no backend attached. Run \`npm run app\` for the real thing.`,
  });
}

/** Events pushed from Rust — the renderer never polls (§6.8). */
export async function listen<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  if (!isDesktop()) return () => {};
  const { listen: tauriListen } = await import("@tauri-apps/api/event");
  const un = await tauriListen<T>(event, (e) => handler(e.payload));
  return un;
}

/* ─── read ─────────────────────────────────────────────────────────────── */

export const getWeek = (from: LocalDate, to: LocalDate, tz: string) =>
  call<WeekView>("get_week", { range: { from, to }, tz });

export const getBacklog = (tz: string, projectId?: string | null, unscheduledOnly = false) =>
  call<BacklogView>("get_backlog", { filter: { projectId, unscheduledOnly }, tz });

export const getTasks = (query: TaskQuery) => call<Page<TaskRow>>("get_tasks", { query });

export const getTaskDetail = (id: string) => call<TaskDetail>("get_task_detail", { id });

export const getProjects = (tz: string) => call<ProjectRow[]>("get_projects", { tz });

export const getTags = () => call<TagRow[]>("get_tags", {});

export const getReports = (from: LocalDate, to: LocalDate, tz: string, projectId?: string | null) =>
  call<ReportBundle>("get_reports", { range: { from, to }, filter: { tz, projectId } });

export const getReconcileItems = (date: LocalDate, tz: string) =>
  call<ReconcileItem[]>("get_reconcile_items", { date, tz });

export const getUnreconciledDays = (before: LocalDate) =>
  call<LocalDate[]>("get_unreconciled_days", { before });

export const search = (q: string, limit = 30) => call<SearchResults>("search", { q, limit });

export const parseCapture = (text: string, tz: string) =>
  call<CapturePreview>("parse_capture", { text, tz });

export const getSettings = () => call<Record<string, unknown>>("get_settings", {});

export const getDeleted = () => call<DeletedRow[]>("get_deleted", {});

export const getTimerState = () => call<TimerState>("get_timer_state", {});

/* ─── write ────────────────────────────────────────────────────────────── */

export const createTask = (input: NewTask) => call<TaskRow>("create_task", { input });

export const updateTask = (id: string, patch: TaskPatch) =>
  call<TaskRow>("update_task", { id, patch });

export const setTaskStatus = (id: string, status: Status) =>
  call<TaskRow>("set_task_status", { id, s: status });

export const deleteTask = (id: string) => call<UndoToken>("delete_task", { id });

export const restore = (token: UndoToken) => call<void>("restore", { token });

export const createProject = (name: string, colour?: string, weeklyTargetSec?: number | null) =>
  call<ProjectRow>("create_project", { input: { name, colour, weeklyTargetSec } });

export const deleteProject = (id: string, moveTasksToInbox: boolean) =>
  call<UndoToken>("delete_project", { id, moveTasksToInbox });

export const scheduleBlock = (input: NewBlock) => call<BlockRow>("schedule_block", { input });

export const moveBlock = (id: string, startsAt: Millis, policy: CollisionPolicy = "overlap") =>
  call<BlockRow[]>("move_block", { id, startsAt, policy });

export const resizeBlock = (id: string, durationSec: number) =>
  call<BlockRow>("resize_block", { id, durationSec });

export const unscheduleBlock = (id: string) => call<UndoToken>("unschedule_block", { id });

export const duplicateBlock = (id: string) => call<BlockRow>("duplicate_block", { id });

export const setBlockFixed = (id: string, isFixed: boolean) =>
  call<BlockRow>("set_block_fixed", { id, isFixed });

export const startTimer = (taskId: string, blockId?: string | null) =>
  call<TimerState>("start_timer", { taskId, blockId: blockId ?? null });

export const stopTimer = () => call<TimerState>("stop_timer", {});

export const resolveIdle = (kind: IdleActionKind) =>
  call<TimerState>("resolve_idle", { action: { kind } });

export const resolveRecovery = (id: string, kind: RecoveryActionKind) =>
  call<TimerState>("resolve_recovery", { id, a: { kind } });

export const addSession = (input: {
  taskId: string;
  blockId?: string | null;
  startedAt: Millis;
  endedAt: Millis;
  note?: string | null;
}) => call<SessionRow>("add_session", { input });

export const updateSession = (
  id: string,
  patch: { startedAt?: Millis; endedAt?: Millis; blockId?: string | null; isConfirmed?: boolean },
) => call<SessionRow>("update_session", { id, p: patch });

export const deleteSession = (id: string) => call<UndoToken>("delete_session", { id });

export const saveNote = (taskId: string, markdown: string) =>
  call<void>("save_note", { taskId, markdown });

export const applyReconcile = (date: LocalDate, actions: ReconcileAction[], tz: string) =>
  call<DayReview>("apply_reconcile", { date, actions, tz });

export const setSetting = (key: string, value: unknown) =>
  call<void>("set_setting", { key, value });

export const exportData = (format: "json" | "csv" | "ics", path: string, tz: string) =>
  call<{ paths: string[] }>("export_data", { format, path, tz });

export const importData = (path: string, mode: "merge" | "replace" | "append") =>
  call<{ tasks: number; skipped: number }>("import_data", { path, mode });

export const runIntegrityCheck = () => call<IntegrityReport>("run_integrity_check", {});

export const rebuildCaches = () => call<void>("rebuild_caches", {});

/* ─── P2: recurrence, .ics, activity ───────────────────────────────────── */

export const scheduleRecurring = (input: NewBlock, rrule: string) =>
  call<BlockRow[]>("schedule_recurring", { input, rrule });

export const unscheduleSeries = (id: string, scope: SeriesScope) =>
  call<UndoToken>("unschedule_series", { id, scope });

/** Keeps series materialised as far as the planner is being asked to show. */
export const extendSeriesTo = (through: LocalDate) =>
  call<number>("extend_series_to", { through });

/** Makes a block that already exists the seed of a series, in place. */
export const repeatBlock = (id: string, rrule: string) =>
  call<BlockRow[]>("repeat_block", { id, rrule });

export const describeRrule = (rrule: string) => call<string>("describe_rrule", { rrule });

export const getRrulePresets = () => call<RrulePreset[]>("get_rrule_presets", {});

export const importIcs = (path: string, tz: string) =>
  call<IcsImportSummary>("import_ics", { path, tz });

/** The OS picker. `null` when the user cancelled — not an error. */
export const pickIcsFile = () => call<string | null>("pick_ics_file", {});

export const getActivitySettings = () => call<ActivityStatus>("get_activity_settings", {});

export const setActivitySetting = (key: string, value: unknown) =>
  call<ActivityStatus>("set_activity_setting", { key, value });

export const getActivityDay = (date: LocalDate, tz: string) =>
  call<ActivityDay>("get_activity_day", { date, tz });

export const clearActivity = () => call<number>("clear_activity", {});

/* ─── the unified day and life time (Plan Rev 3 §7, §8.1) ──────────────── */

export const getDay = (date: LocalDate, tz: string, slotMinutes?: number) =>
  call<DayView>("get_day", { date, tz, slotMinutes: slotMinutes ?? null });

export const getLifeAreas = (tz: string, includeArchived = false) =>
  call<LifeAreaRow[]>("get_life_areas", { tz, includeArchived });

export const createLifeArea = (input: {
  name: string;
  colour?: string;
  kind?: AreaKind;
  monthlyTargetSec?: number | null;
}) => call<LifeAreaRow>("create_life_area", { input });

export const updateLifeArea = (
  id: string,
  patch: { name?: string; colour?: string; kind?: AreaKind; monthlyTargetSec?: number | null; isArchived?: boolean },
) => call<LifeAreaRow>("update_life_area", { id, patch });

export const deleteLifeArea = (id: string) => call<UndoToken>("delete_life_area", { id });

export const getLifeEntries = (date: LocalDate, tz: string) =>
  call<LifeEntryRow[]>("get_life_entries", { date, tz });

export const addLifeEntry = (input: NewLifeEntry) =>
  call<LifeEntryRow>("add_life_entry", { input });

export const updateLifeEntry = (
  id: string,
  patch: {
    lifeAreaId?: string;
    label?: string | null;
    startedAt?: Millis;
    endedAt?: Millis;
    isPrivate?: boolean;
    note?: string | null;
  },
) => call<LifeEntryRow>("update_life_entry", { id, patch });

export const deleteLifeEntry = (id: string) => call<UndoToken>("delete_life_entry", { id });

/** Work records only — there is no life-entry equivalent, by design. */
export const setSessionContribution = (id: string, contribution: Contribution | null) =>
  call<SessionRow>("set_session_contribution", { id, contribution });

export const convertSessionToLife = (id: string, lifeAreaId: string, tz: string) =>
  call<LifeEntryRow>("convert_session_to_life", { id, lifeAreaId, tz });

/** The month dashboard. `month` is `YYYY-MM`, or any date inside it. */
export const getMonth = (month: string, tz: string) =>
  call<MonthView>("get_month", { month, tz });

/* ─── Excel export (Plan Rev 3 §10) ────────────────────────────────────── */

/** The month table exactly as the workbook will contain it. */
export const previewExcel = (month: string, tz: string, options: ExcelOptions) =>
  call<ExcelPreview>("preview_excel", { month, tz, options });

export const exportExcel = (
  month: string,
  tz: string,
  path: string,
  options: ExcelOptions,
) => call<ExcelExportResult>("export_excel", { month, tz, path, options });

export const suggestExcelPath = (fileName: string) =>
  call<string>("suggest_excel_path", { fileName });
