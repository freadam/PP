//! The DTOs that cross the IPC boundary (§6.8).
//!
//! These are the *only* shapes the renderer sees — it never holds SQL, and it
//! never reconstructs domain rules from columns. Mirrored by hand in
//! `src/lib/ipc/types.ts`; `tauri-specta` would generate that file in a build
//! that can link a webview.

use serde::{Deserialize, Serialize};

pub type Millis = i64;

// ─── projects ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub colour: String,
    pub icon: Option<String>,
    pub kind: String,
    pub sort_rank: f64,
    pub is_archived: bool,
    pub weekly_target_sec: Option<i64>,
    pub open_task_count: i64,
    /// Tracked seconds inside the current local week — feeds the sidebar's
    /// weekly-target bar (§3.2).
    pub week_tracked_sec: i64,
    pub created_at: Millis,
    pub updated_at: Millis,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewProject {
    pub name: String,
    pub colour: Option<String>,
    pub kind: Option<String>,
    pub weekly_target_sec: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPatch {
    pub name: Option<String>,
    pub colour: Option<String>,
    pub kind: Option<String>,
    pub is_archived: Option<bool>,
    #[serde(default, with = "double_option")]
    pub weekly_target_sec: Option<Option<i64>>,
}

// ─── tasks ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Open,
    Done,
    Cancelled,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::Done => "done",
            Status::Cancelled => "cancelled",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "open" => Some(Status::Open),
            "done" => Some(Status::Done),
            "cancelled" => Some(Status::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagRow {
    pub id: String,
    pub name: String,
    pub colour: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRow {
    pub id: String,
    pub project_id: Option<String>,
    pub parent_id: Option<String>,
    pub title: String,
    pub status: Status,
    pub estimate_sec: Option<i64>,
    pub due_date: Option<String>,
    pub due_at: Option<Millis>,
    pub priority: i64,
    pub energy: Option<String>,
    /// "Doesn't fit one sitting" — the top of the estimate scale, and a
    /// different state from "not estimated yet" (migration 0003).
    pub is_rollover: bool,
    pub sort_rank: f64,
    pub completed_at: Option<Millis>,
    pub tags: Vec<TagRow>,
    /// Derived from `time_session` (§6.4) — never stored on the task.
    pub tracked_sec: i64,
    pub subtask_total: i64,
    pub subtask_done: i64,
    /// True when the task has at least one block starting after now.
    pub is_scheduled: bool,
    /// The first and last instants any work was recorded against this task.
    /// Derived from `time_session`, like everything else about tracked time.
    pub first_session_at: Option<Millis>,
    pub last_session_at: Option<Millis>,
    pub created_at: Millis,
    pub updated_at: Millis,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTask {
    pub title: String,
    pub project_id: Option<String>,
    pub parent_id: Option<String>,
    pub estimate_sec: Option<i64>,
    pub due_date: Option<String>,
    pub due_at: Option<Millis>,
    pub priority: Option<i64>,
    pub energy: Option<String>,
    #[serde(default)]
    pub is_rollover: bool,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// `Option<Option<T>>` distinguishes "leave alone" from "clear it" — without
/// it there is no way to unset a due date over JSON.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPatch {
    pub title: Option<String>,
    #[serde(default, with = "double_option")]
    pub project_id: Option<Option<String>>,
    #[serde(default, with = "double_option")]
    pub estimate_sec: Option<Option<i64>>,
    #[serde(default, with = "double_option")]
    pub due_date: Option<Option<String>>,
    #[serde(default, with = "double_option")]
    pub due_at: Option<Option<Millis>>,
    pub priority: Option<i64>,
    #[serde(default, with = "double_option")]
    pub energy: Option<Option<String>>,
    pub sort_rank: Option<f64>,
    /// Setting this to true clears `estimate_sec`; setting an estimate clears
    /// this. The two are mutually exclusive by construction.
    pub is_rollover: Option<bool>,
    /// Replaces the whole tag set when present.
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDetail {
    pub task: TaskRow,
    pub note: String,
    pub note_updated_at: Option<Millis>,
    pub sessions: Vec<SessionRow>,
    pub subtasks: Vec<TaskRow>,
    pub blocks: Vec<BlockRow>,
    pub project: Option<ProjectRow>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskQuery {
    pub project_id: Option<String>,
    pub tag: Option<String>,
    pub text: Option<String>,
    /// `open` (default), `done`, `all`, `deleted`
    pub scope: Option<String>,
    /// Only top-level tasks unless true.
    #[serde(default)]
    pub include_subtasks: bool,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub rows: Vec<T>,
    pub total: i64,
}

/// §3.2 grouping. Computed in Rust so the renderer never re-derives "overdue".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskGroup {
    pub key: String,
    pub label: String,
    pub count: i64,
    pub estimate_sec: i64,
    pub task_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklogView {
    pub groups: Vec<TaskGroup>,
    pub tasks: Vec<TaskRow>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacklogFilter {
    pub project_id: Option<String>,
    pub tag: Option<String>,
    pub today: Option<String>,
    /// Exclude tasks that already have a block in the future (§6.10).
    #[serde(default)]
    pub unscheduled_only: bool,
}

// ─── blocks: the plot ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockRow {
    pub id: String,
    pub task_id: Option<String>,
    pub label: Option<String>,
    pub starts_at: Millis,
    pub duration_sec: i64,
    pub local_date: String,
    pub tz: String,
    pub is_fixed: bool,
    /// Set on every instance of a repeating series (§2.3, P2).
    pub series_id: Option<String>,
    pub rrule: Option<String>,
    /// The VEVENT UID when this block came from a `.ics` file.
    pub external_uid: Option<String>,
    pub created_at: Millis,
    pub updated_at: Millis,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewBlock {
    pub task_id: Option<String>,
    pub label: Option<String>,
    pub starts_at: Millis,
    pub duration_sec: i64,
    pub tz: String,
    #[serde(default)]
    pub is_fixed: bool,
    /// An RFC 5545 subset (§2.3, P2). Present means "make this a series".
    #[serde(default)]
    pub rrule: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CollisionPolicy {
    /// Default: land on top, the UI tints the overlap (§4.3).
    Overlap,
    /// `Shift`: push subsequent non-fixed blocks down.
    Push,
    /// `Alt`: shorten the dropped block to fit the gap.
    Shrink,
}

impl Default for CollisionPolicy {
    fn default() -> Self {
        CollisionPolicy::Overlap
    }
}

/// One block as the planner draws it: the plot, the track, and the gap.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockView {
    pub block: BlockRow,
    pub title: String,
    pub project_id: Option<String>,
    pub project_colour: Option<String>,
    pub task_status: Option<Status>,
    pub planned_sec: i64,
    pub tracked_sec: i64,
    /// `tracked − planned`. Positive is overrun, negative is underrun.
    pub drift_sec: i64,
    pub drift_state: DriftState,
    pub is_running: bool,
    /// Column index and count inside its collision group (§3.1).
    pub lane: i64,
    pub lanes: i64,
}

/// §5.6 redundancy table — the state is computed once, in Rust, so the
/// planner block, the compact task-row rail and the report bar cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DriftState {
    NotStarted,
    InProgress,
    OnEstimate,
    Overrun,
    /// Underrun, and the day is over — the plot becomes the "never happened" trace.
    UnderrunPast,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayColumn {
    pub local_date: String,
    pub is_today: bool,
    pub is_past: bool,
    pub blocks: Vec<BlockView>,
    pub planned_sec: i64,
    pub tracked_sec: i64,
    /// Sessions on this day that belong to no block — the unplanned work that
    /// Reconcile turns into retroactive blocks (§3.7).
    pub unplanned_sec: i64,
    pub is_reconciled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekView {
    pub from: String,
    pub to: String,
    pub tz: String,
    pub days: Vec<DayColumn>,
    /// Server-side "now" so the cursor never depends on a skewed renderer clock.
    pub now: Millis,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DateRange {
    pub from: String,
    pub to: String,
}

// ─── sessions: the track ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRow {
    pub id: String,
    pub task_id: String,
    pub task_title: String,
    pub block_id: Option<String>,
    pub started_at: Millis,
    pub ended_at: Option<Millis>,
    pub elapsed_sec: i64,
    pub heartbeat_at: Option<Millis>,
    pub source: String,
    pub is_confirmed: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualSession {
    pub task_id: String,
    pub block_id: Option<String>,
    pub started_at: Millis,
    pub ended_at: Millis,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPatch {
    pub started_at: Option<Millis>,
    pub ended_at: Option<Millis>,
    #[serde(default, with = "double_option")]
    pub block_id: Option<Option<String>>,
    #[serde(default, with = "double_option")]
    pub note: Option<Option<String>>,
    pub is_confirmed: Option<bool>,
}

// ─── timer ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimerPhase {
    Idle,
    Running,
    IdleChallenge,
    Break,
    Recovering,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerState {
    pub phase: TimerPhase,
    /// The task being timed, which outlives any one segment — during an idle
    /// challenge or a break the run continues with nothing recording.
    pub run_task_id: Option<String>,
    pub task_title: Option<String>,
    /// The open segment, or `None` during an idle challenge or break.
    pub session: Option<SessionRow>,
    /// Counted seconds for the whole run, across segments.
    pub elapsed_sec: i64,
    /// Counted seconds for the open segment only.
    pub segment_elapsed_sec: i64,
    /// Set in `IdleChallenge`: the exact span the user has to rule on (§4.5, U6).
    pub idle_from: Option<Millis>,
    pub idle_to: Option<Millis>,
    /// Set in `Recovering`: the orphan session found at boot.
    pub recovery_session_id: Option<String>,
    pub pomodoro: Option<PomodoroState>,
}

impl TimerState {
    pub fn idle() -> Self {
        TimerState {
            phase: TimerPhase::Idle,
            run_task_id: None,
            task_title: None,
            session: None,
            elapsed_sec: 0,
            segment_elapsed_sec: 0,
            idle_from: None,
            idle_to: None,
            recovery_session_id: None,
            pomodoro: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PomodoroState {
    pub phase: PomodoroPhase,
    pub cycle: i64,
    pub cycles_before_long: i64,
    pub phase_ends_at: Millis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PomodoroPhase {
    Work,
    ShortBreak,
    LongBreak,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum IdleAction {
    /// Keep the idle span in the session.
    Keep,
    /// The honest default (§4.5): trim the session back to the last input.
    Discard,
    /// Trim, and log the span as a break instead.
    AssignToBreak,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum RecoveryAction {
    /// Default: trim to the last heartbeat (U7, ±30s).
    TrimToHeartbeat,
    /// Believe the whole span — the user says the machine stayed awake.
    KeepAll,
    /// It never happened.
    Discard,
}

// ─── reconcile ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileItem {
    pub id: String,
    pub kind: ReconcileKind,
    pub title: String,
    pub block_id: Option<String>,
    pub task_id: Option<String>,
    pub session_id: Option<String>,
    pub planned_sec: i64,
    pub tracked_sec: i64,
    pub drift_sec: i64,
    pub starts_at: Option<Millis>,
    pub ends_at: Option<Millis>,
    pub default_action: ReconcileVerb,
    pub available: Vec<ReconcileVerb>,
    /// The next free slot big enough for the remainder, if one exists today
    /// or tomorrow (§3.7 auto-suggest).
    pub suggested_slot: Option<Millis>,
    pub suggested_duration_sec: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReconcileKind {
    Overran,
    NeverStarted,
    UntrackedGap,
    UnplannedSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReconcileVerb {
    Accept,
    RescheduleRemainder,
    Split,
    Drop,
    MarkDone,
    ReviseEstimate,
    MoveToTomorrow,
    LeaveUnscheduled,
    CreateRetroBlock,
    AssignToTask,
    LogAsBreak,
    Ignore,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileAction {
    pub item_id: String,
    pub verb: ReconcileVerb,
    /// For `RescheduleRemainder` / `MoveToTomorrow` / `CreateRetroBlock`.
    pub starts_at: Option<Millis>,
    pub duration_sec: Option<i64>,
    /// For `AssignToTask`.
    pub task_id: Option<String>,
    /// For `ReviseEstimate`.
    pub estimate_sec: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayReview {
    pub local_date: String,
    pub reconciled_at: Millis,
    pub planned_sec: i64,
    pub tracked_sec: i64,
    pub overrun_sec: i64,
    pub unplanned_sec: i64,
    pub blocks_total: i64,
    pub blocks_untouched: i64,
    pub calibration_ratio: Option<f64>,
    /// The single takeaway line shown on close (§3.7, F5).
    pub takeaway: String,
    /// Consecutive reconciled days including this one (§2.3).
    ///
    /// Returned here rather than left to Reports because closing a day is the
    /// exact moment the streak changes, and it is the only moment at which
    /// anyone is going to care.
    pub streak_days: i64,
}

// ─── reports ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationBucket {
    pub bucket: String,
    pub n: i64,
    /// Median, not mean — one abandoned task ruins a mean (§6.4).
    pub median_ratio: f64,
    /// Reported only at n ≥ 5 (F6). Below that the UI shows the count greyed.
    pub is_reportable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationReport {
    pub overall_median: Option<f64>,
    pub sample_count: i64,
    pub buckets: Vec<CalibrationBucket>,
    /// Plain language, assembled in Rust so every surface says the same thing.
    pub headline: String,
    pub worst_bucket: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWeekRow {
    pub project_id: Option<String>,
    pub project_name: String,
    pub project_colour: String,
    pub week_start: String,
    pub planned_sec: i64,
    pub tracked_sec: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyTargetRow {
    pub project_id: String,
    pub project_name: String,
    pub project_colour: String,
    pub target_sec: i64,
    pub tracked_sec: i64,
    /// Where the project *should* be by now if spend were even across the week.
    pub pace_sec: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportBundle {
    pub from: String,
    pub to: String,
    pub calibration: CalibrationReport,
    pub project_weeks: Vec<ProjectWeekRow>,
    pub weekly_targets: Vec<WeeklyTargetRow>,
    pub total_planned_sec: i64,
    pub total_tracked_sec: i64,
    pub streak_days: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportFilter {
    pub project_id: Option<String>,
    pub tz: Option<String>,
}

// ─── activity (§3.5, P2) ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySettings {
    pub enabled: bool,
    /// Separate from `enabled`, and off even when apps are on (§3.5).
    pub titles_enabled: bool,
    pub paused: bool,
    pub excluded_apps: Vec<String>,
    pub excluded_title_patterns: Vec<String>,
    /// 30 / 90 / 0 for forever.
    pub retention_days: i64,
    /// Shown in Settings, because "we delete it eventually" is not a promise.
    pub next_purge_at: Option<Millis>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySample {
    pub app_id: String,
    pub window_title: Option<String>,
    pub at: Millis,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySpanRow {
    pub id: i64,
    pub started_at: Millis,
    pub ended_at: Millis,
    pub app_id: String,
    pub window_title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppTotal {
    pub app_id: String,
    pub seconds: i64,
}

/// One plotted block against what was actually on screen underneath it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockCorrelation {
    pub block_id: String,
    pub title: String,
    pub starts_at: Millis,
    pub duration_sec: i64,
    pub top_apps: Vec<AppTotal>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityDay {
    pub local_date: String,
    pub spans: Vec<ActivitySpanRow>,
    pub by_app: Vec<AppTotal>,
    pub correlations: Vec<BlockCorrelation>,
    pub tracked_sec: i64,
}

// ─── search, undo, data ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub kind: String,
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    /// Byte offsets of the match in `title`, for highlighting in `--plot`.
    pub match_from: Option<u32>,
    pub match_to: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub query: String,
    pub hits: Vec<SearchHit>,
}

/// Returned by every soft delete. `restore` takes it back (§4.6).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoToken {
    pub entity: String,
    pub id: String,
    pub label: String,
    pub at: Millis,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedRow {
    pub entity: String,
    pub id: String,
    pub title: String,
    pub deleted_at: Millis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Json,
    Csv,
    Ics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportMode {
    Merge,
    Replace,
    Append,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub format: ExportFormat,
    pub paths: Vec<String>,
    pub projects: i64,
    pub tasks: i64,
    pub blocks: i64,
    pub sessions: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub projects: i64,
    pub tasks: i64,
    pub blocks: i64,
    pub sessions: i64,
    pub skipped: i64,
}

// ─── capture parser (§4.4) ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureChip {
    pub kind: String,
    /// The literal text matched, e.g. `~1h30m`.
    pub raw: String,
    /// What it resolved to, e.g. `1h 30m`.
    pub display: String,
    pub from: u32,
    pub to: u32,
    /// True for `#project` names that do not exist yet (§4.4 rule 7).
    pub creates: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturePreview {
    /// The raw input, retained for the length of the undo window (rule 8).
    pub raw: String,
    pub title: String,
    pub project_name: Option<String>,
    pub project_id: Option<String>,
    pub project_creates: bool,
    pub tags: Vec<String>,
    pub estimate_sec: Option<i64>,
    pub priority: i64,
    pub due_date: Option<String>,
    pub due_at: Option<Millis>,
    pub chips: Vec<CaptureChip>,
}

// ─── helper: Option<Option<T>> over JSON ───────────────────────────────

pub mod double_option {
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        Option::<T>::deserialize(d).map(Some)
    }
}
