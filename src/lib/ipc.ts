/**
 * The only place the renderer talks to the backend (§6.8).
 *
 * Every call is a typed, intent-based command. There are no SQL strings here,
 * and the capability file lists exactly the commands below — a strictly smaller
 * surface than a webview holding `sql:allow-execute` next to a text renderer.
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
  ConnectorStatus,
  DayReview,
  DeletedRow,
  ActivityRule,
  DomainCategory,

  GoalRow,
  GoalTemplate,
  NewGoal,
  WeekReview,
  DomainTotal,
  MatchKind,
  ObservationCategory,
  UnlabelledRow,
  IdleActionKind,
  IntegrityReport,
  ResetSummary,
  WallpaperFolder,
  TaskCategoryRow,
  WorkPeriod,
  WorkReport,
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
  BlockIntent,
  WeekReport,
  WeekReportDue,
  WeekReportResult,
  WorkbookInspection,
  ImportMapping,
  ImportPreview,
  ImportResult,
  ImportBatch,
  MergeResult,
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

/**
 * Reclassify a plotted interval. Changing this changes what the month
 * dashboard counts as *planned* entertainment; it touches nothing already
 * recorded against the block.
 */
export const setBlockIntent = (id: string, intent: BlockIntent) =>
  call<BlockRow>("set_block_intent", { id, intent });

export const startTimer = (taskId: string, blockId?: string | null) =>
  call<TimerState>("start_timer", { taskId, blockId: blockId ?? null });

export const stopTimer = () => call<TimerState>("stop_timer", {});

/**
 * Starts a run with an intended length — "45 minutes on this", which is a
 * different act from starting a stopwatch. Plots a block for it, so the overrun
 * is visible later. `null` minutes is an ordinary open-ended timer.
 */
export const startFocus = (taskId: string, minutes: number | null) =>
  call<TimerState>("start_focus", { taskId, minutes });

/** "Keep going." Moves the reminder and never the plan. */
export const extendFocus = (minutes: number) => call<TimerState>("extend_focus", { minutes });

export const resolveIdle = (kind: IdleActionKind) =>
  call<TimerState>("resolve_idle", { action: { kind } });

export const resolveRecovery = (id: string, kind: RecoveryActionKind) =>
  call<TimerState>("resolve_recovery", { id, a: { kind } });

/**
 * Records work by hand.
 *
 * Load-bearing rather than a fallback: the observer sees one machine, so work
 * on a second computer, an offline meeting and a task done on paper produce no
 * observation at all and can only ever arrive this way.
 */
export const addSession = (input: {
  taskId: string;
  blockId?: string | null;
  startedAt: Millis;
  endedAt: Millis;
  note?: string | null;
  /** How you were involved — very often `attend`, since the case this exists
   *  for is frequently a meeting. */
  contribution?: Contribution | null;
  /** Clear any confirmed record already covering the interval. Off by default:
   *  replacing destroys a confirmed record. */
  replaceExisting?: boolean;
}) => call<SessionRow>("add_session", { input });

export const updateSession = (
  id: string,
  patch: {
    /** Move the record to a different task. Keeps the row, its note and its
     *  contribution mode; detaches a block belonging to the old task. */
    taskId?: string;
    startedAt?: Millis;
    endedAt?: Millis;
    blockId?: string | null;
    isConfirmed?: boolean;
  },
) => call<SessionRow>("update_session", { id, p: patch });

export const deleteSession = (id: string) => call<UndoToken>("delete_session", { id });

/** Plain text, capped at 2000 characters in the core (§2, M4). */
export const saveNote = (taskId: string, body: string) =>
  call<void>("save_note", { taskId, body });

export const applyReconcile = (date: LocalDate, actions: ReconcileAction[], tz: string) =>
  call<DayReview>("apply_reconcile", { date, actions, tz });

export const setSetting = (key: string, value: unknown) =>
  call<void>("set_setting", { key, value });

export const exportData = (format: "json" | "csv" | "ics", path: string, tz: string) =>
  call<{ paths: string[] }>("export_data", { format, path, tz });

export const importData = (path: string, mode: "merge" | "replace" | "append") =>
  call<{ tasks: number; skipped: number }>("import_data", { path, mode });

export const runIntegrityCheck = () => call<IntegrityReport>("run_integrity_check", {});

/** Empties the database of everything the user recorded. Snapshots first. */
export const resetAllData = () => call<ResetSummary>("reset_all_data", {});

/* ─── launch at login ───────────────────────────────────────────────────
   Coverage, not convenience: observation only records while Fruit is running,
   so a morning that starts with "I forgot to open it" has a hole no amount of
   reconciling can fill honestly. */

export const getAutostart = () => call<boolean>("get_autostart", {});

export const setAutostart = (enabled: boolean) => call<boolean>("set_autostart", { enabled });

/* ─── kinds of work, and the work report ────────────────────────────────── */

export const getTaskCategories = () => call<TaskCategoryRow[]>("get_task_categories", {});

export const createTaskCategory = (name: string, colour?: string) =>
  call<TaskCategoryRow>("create_task_category", { name, colour: colour ?? null });

export const updateTaskCategory = (id: string, name?: string, colour?: string) =>
  call<TaskCategoryRow>("update_task_category", {
    id,
    name: name ?? null,
    colour: colour ?? null,
  });

/** Returns how many tasks were freed. Their time is untouched. */
export const deleteTaskCategory = (id: string) => call<number>("delete_task_category", { id });

export const setTaskCategory = (taskId: string, categoryId: string | null) =>
  call<void>("set_task_category", { taskId, categoryId });

/** All five work reports for one date's day, ISO week, or calendar month. */
export const getWorkReport = (date: LocalDate, period: WorkPeriod, tz: string) =>
  call<WorkReport>("get_work_report", { date, period, tz });

/* ─── Focus wallpapers ──────────────────────────────────────────────────
   The renderer holds file *names*, never paths. Rust joins each name to the
   configured folder and refuses anything that resolves outside it, so
   `readWallpaper` cannot be turned into a general file read. */

export const getWallpapers = () => call<WallpaperFolder>("get_wallpapers", {});

/** One wallpaper as a `data:` URI, ready for a CSS background. */
export const readWallpaper = (name: string) => call<string>("read_wallpaper", { name });

export const pickWallpaperDir = () => call<string | null>("pick_wallpaper_dir", {});

export const revealWallpaperDir = () => call<void>("reveal_wallpaper_dir", {});

export const rebuildCaches = () => call<void>("rebuild_caches", {});

/* ─── P2: recurrence, .ics, activity ───────────────────────────────────── */

export const scheduleRecurring = (input: NewBlock, rrule: string) =>
  call<BlockRow[]>("schedule_recurring", { input, rrule });

export const unscheduleSeries = (id: string, scope: SeriesScope) =>
  call<UndoToken>("unschedule_series", { id, scope });

/** Keeps series materialised as far as the planner is being asked to show. */
/** Two adjacent records of the same thing becoming one. Bounded at five
 *  minutes of gap, and the result says how much of it was absorbed. */
export const mergeLifeEntries = (ids: string[]) =>
  call<MergeResult>("merge_life_entries", { ids });

export const mergeSessions = (ids: string[]) => call<MergeResult>("merge_sessions", { ids });

/** One record becoming two, at a moment inside it. Returns `[earlier, later]`;
 *  the original id survives as the earlier half. */
export const splitLifeEntry = (id: string, at: Millis) =>
  call<[LifeEntryRow, LifeEntryRow]>("split_life_entry", { id, at });

export const splitSession = (id: string, at: Millis) =>
  call<[SessionRow, SessionRow]>("split_session", { id, at });

/** Makes a life entry repeat. Sleep is the case this exists for. */
export const repeatLifeEntry = (id: string, rrule: string) =>
  call<LifeEntryRow[]>("repeat_life_entry", { id, rrule });

/** Removing part or all of a repeating entry. The scope is asked, never
 *  inferred — "just tonight" and "three months of sleep" are different enough
 *  that guessing is a data-loss bug with a friendly name. */
export const deleteLifeSeries = (id: string, scope: SeriesScope) =>
  call<UndoToken>("delete_life_series", { id, scope });

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

/* ─── browser connector (Plan Rev 3 §5.4) ─────────────────────────────── */

/* ─── weekly goals ─────────────────────────────────────────────────────── */

export const getWeekReview = (date: LocalDate, tz: string) =>
  call<WeekReview>("get_week_review", { date, tz });

export const getGoals = (includeEnded = false) =>
  call<GoalRow[]>("get_goals", { includeEnded });

/** Replaces any live goal for the same subject; the old one is closed, not deleted. */
export const setGoal = (input: NewGoal, today: LocalDate) =>
  call<GoalRow>("set_goal", { input, today });

export const endGoal = (id: string, today: LocalDate) => call<void>("end_goal", { id, today });

export const getGoalTemplates = (today: LocalDate, tz: string) =>
  call<GoalTemplate[]>("get_goal_templates", { today, tz });

/** Silences every notice for a while — the "don't tell me again" a nudge needs. */
export const silenceNotices = (minutes: number) => call<void>("silence_notices", { minutes });

/* ─── labelling observed time ──────────────────────────────────────────── */

export const getCategories = (from: LocalDate | null, to: LocalDate | null, tz: string) =>
  call<ObservationCategory[]>("get_categories", { from, to, tz });

/** `colour` omitted: the core picks the next one in its palette (I1 — a
 *  component may not hold a literal colour). */
export const createCategory = (name: string, countsAs: DomainCategory = "other") =>
  call<ObservationCategory>("create_category", { name, colour: null, countsAs });

export const updateCategory = (
  id: string,
  name: string | null = null,
  colour: string | null = null,
) => call<ObservationCategory>("update_category", { id, name, colour });

export const deleteCategory = (id: string) => call<void>("delete_category", { id });

export const getActivityRules = () => call<ActivityRule[]>("get_activity_rules", {});

/** Creates or replaces the rule for one app or domain — there is only ever one. */
export const setActivityRule = (matchKind: MatchKind, matchValue: string, categoryId: string) =>
  call<ActivityRule>("set_activity_rule", { matchKind, matchValue, categoryId });

export const deleteActivityRule = (id: string) => call<void>("delete_activity_rule", { id });

/**
 * Relabels one observed interval without touching any rule.
 *
 * The YouTube case: Distraction by default, and *this* video was a lecture.
 * Fruit never sees the URL or the page title, so only you can say.
 */
/** Labels the whole stretch the id belongs to, and returns the seconds that
 *  moved — a label that quietly moves less than the row it was clicked on is
 *  the failure this return value exists to surface. */
export const setSpanCategory = (spanId: number, categoryId: string | null) =>
  call<number>("set_span_category", { spanId, categoryId });

export const getUnlabelled = (from: LocalDate, to: LocalDate, tz: string, limit = 12) =>
  call<UnlabelledRow[]>("get_unlabelled", { from, to, tz, limit });

export const getDomainTotals = (date: LocalDate, tz: string) =>
  call<DomainTotal[]>("get_domain_totals", { date, tz });

export const getConnectorStatus = () => call<ConnectorStatus>("get_connector_status", {});

/**
 * Registers the native-messaging host — on request, never on first run.
 *
 * Returns one line per step it took, including the exact `reg.exe` command, so
 * a machine that refuses gives the user something to run rather than a dead end.
 */
export const installConnector = (extensionId: string) =>
  call<string[]>("install_connector", { extensionId });

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

/* ─── the Monday-morning report (W9) ───────────────────────────────────── */

/**
 * Is a report waiting? `null` means there is nothing to say about last week —
 * a legitimate answer, and the one an empty week gets. Never about the week in
 * progress: a report on a week that is still happening is one that will be
 * wrong by Friday.
 */
export const dueWeekReport = (tz: string) =>
  call<WeekReportDue | null>("due_week_report", { tz });

export const getWeekReport = (date: LocalDate, tz: string) =>
  call<WeekReport>("get_week_report", { date, tz });

/** Writes the file. `path` omitted lands it in Downloads, named by ISO week. */
export const exportWeekReport = (date: LocalDate, tz: string, path?: string) =>
  call<WeekReportResult>("export_week_report", { date, tz, path: path ?? null });

export const markWeekReportSeen = (week: string) =>
  call<void>("mark_week_report_seen", { week });

/** Shows a written file in the OS file manager. Reveal, never open: launching
 *  whatever the machine associates with `.xlsx` is a bigger assumption than
 *  putting the folder in front of someone. */
export const revealPath = (path: string) => call<void>("reveal_path", { path });

/* ─── workbook import (M13, §4.8) ──────────────────────────────────────── */

/** Read-only. Says what the file looks like; decides nothing. */
export const inspectWorkbook = (path: string) =>
  call<WorkbookInspection>("inspect_workbook", { path });

/** A starting mapping. Every label it cannot place exactly is left unmapped —
 *  and unmapped blocks the commit, which is the whole point. */
export const suggestImportMapping = (path: string, sheet: string, tz: string) =>
  call<ImportMapping>("suggest_import_mapping", { path, sheet, tz });

export const previewImport = (path: string, mapping: ImportMapping) =>
  call<ImportPreview>("preview_import", { path, mapping });

export const commitImport = (path: string, mapping: ImportMapping) =>
  call<ImportResult>("commit_import", { path, mapping });

export const getImportBatches = () => call<ImportBatch[]>("get_import_batches", {});

export const undoImport = (batchId: string) => call<UndoToken>("undo_import", { batchId });
