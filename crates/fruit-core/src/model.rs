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
    /// Work contribution mode. Work records only — see `Contribution`.
    pub contribution: Option<Contribution>,
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

// ─── life time (Plan Rev 3 §7) ─────────────────────────────────────────

/// What a life area is *for*, which is what the summaries group by. The
/// workbook's core-versus-entertainment split, plus rest, which is neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AreaKind {
    Core,
    Entertainment,
    Rest,
    Other,
}

impl AreaKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "core" => Self::Core,
            "entertainment" => Self::Entertainment,
            "rest" => Self::Rest,
            "other" => Self::Other,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Entertainment => "entertainment",
            Self::Rest => "rest",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifeAreaRow {
    pub id: String,
    pub name: String,
    pub colour: String,
    pub kind: AreaKind,
    pub monthly_target_sec: Option<i64>,
    pub sort_rank: f64,
    /// Built-in areas can be renamed and retargeted but never deleted — an
    /// imported workbook maps onto them, and a missing one is silent data loss.
    pub is_builtin: bool,
    pub is_archived: bool,
    /// Confirmed seconds in the current local month, for target-versus-actual.
    pub month_tracked_sec: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewLifeArea {
    pub name: String,
    pub colour: Option<String>,
    pub kind: Option<String>,
    pub monthly_target_sec: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifeAreaPatch {
    pub name: Option<String>,
    pub colour: Option<String>,
    pub kind: Option<String>,
    #[serde(default, with = "double_option")]
    pub monthly_target_sec: Option<Option<i64>>,
    pub is_archived: Option<bool>,
}

/// Confirmed non-work time. Unlike a session this has no accumulator and no
/// open state — it is always a closed interval asserted after the fact, because
/// there is no timer for sleeping.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifeEntryRow {
    pub id: String,
    pub life_area_id: String,
    pub area_name: String,
    pub area_colour: String,
    pub area_kind: AreaKind,
    pub label: Option<String>,
    pub started_at: Millis,
    pub ended_at: Millis,
    pub local_date: String,
    pub tz: String,
    /// Accounted for, but nothing recorded about it.
    pub is_private: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewLifeEntry {
    pub life_area_id: String,
    pub label: Option<String>,
    pub started_at: Millis,
    pub ended_at: Millis,
    pub tz: String,
    #[serde(default)]
    pub is_private: bool,
    pub note: Option<String>,
    /// Clear any confirmed record already covering this interval.
    ///
    /// Replacing is destructive, so it is never the default: the caller has to
    /// have shown the user what is about to go (M9).
    #[serde(default)]
    pub replace_existing: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifeEntryPatch {
    pub life_area_id: Option<String>,
    pub label: Option<String>,
    pub started_at: Option<Millis>,
    pub ended_at: Option<Millis>,
    pub is_private: Option<bool>,
    pub note: Option<String>,
}

/// Work contribution mode (Plan Rev 3 §7). Work records only — there is
/// deliberately no counterpart on `life_entry`, so "contribution never applies
/// to personal time" is a fact about the schema rather than a rule the UI is
/// trusted to remember.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Contribution {
    None,
    Attend,
    Support,
    Own,
    Assist,
}

impl Contribution {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "none" => Self::None,
            "attend" => Self::Attend,
            "support" => Self::Support,
            "own" => Self::Own,
            "assist" => Self::Assist,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Attend => "attend",
            Self::Support => "support",
            Self::Own => "own",
            Self::Assist => "assist",
        }
    }
}

// ─── the unified day (Plan Rev 3 §7, §8.1) ─────────────────────────────

/// What owns one segment of the day, after precedence is applied.
///
/// Exactly one owner per segment, which is the whole point: two owners would
/// be two durations for the same second.
#[derive(Debug, Clone, PartialEq, Serialize)]
// `rename_all` renames *variants*; struct-variant **fields** need
// `rename_all_fields`. Without the second attribute this enum goes over the
// wire as `app_id`/`area_name` while every other DTO is camelCase — a silent,
// per-variant contract break. `wire_shape_is_camel_case` guards it.
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
pub enum SlotOwner {
    /// Confirmed non-work time.
    Life {
        entry_id: String,
        area_id: String,
        area_name: String,
        area_colour: String,
        area_kind: AreaKind,
        label: Option<String>,
        is_private: bool,
    },
    /// Confirmed work.
    Work {
        session_id: String,
        task_id: String,
        task_title: String,
        project_id: Option<String>,
        project_colour: Option<String>,
        contribution: Option<Contribution>,
    },
    /// The machine saw an application, and nobody has confirmed what it meant.
    Observed {
        app_id: String,
        domain: Option<String>,
        category: Option<String>,
    },
    /// Observed, and the observation was "nobody was here".
    Idle,
    /// Nothing at all. A real state with a real duration — not an absent row.
    Empty,
}

impl SlotOwner {
    /// Rank in the §7 precedence order. Lower wins.
    pub fn rank(&self) -> u8 {
        match self {
            SlotOwner::Life { .. } => 0,
            SlotOwner::Work { .. } => 1,
            SlotOwner::Observed { .. } => 2,
            SlotOwner::Idle => 3,
            SlotOwner::Empty => 4,
        }
    }

    pub fn is_confirmed(&self) -> bool {
        matches!(self, SlotOwner::Life { .. } | SlotOwner::Work { .. })
    }
}

/// One contiguous run of the day with a single owner. Segments tile the day
/// exactly: no gaps, no overlaps, and their durations sum to the day's length.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySegment {
    pub from: Millis,
    pub to: Millis,
    pub owner: SlotOwner,
    /// Applications seen during this segment. **Evidence, not duration** — a
    /// segment owned by a confirmed session carries what the observer saw
    /// without the day summing to more than a day (M8).
    pub evidence: Vec<AppTotal>,
    /// Confirmed work with entertainment observed inside it — the wireframe's
    /// "Work + distraction". It is the one classification the user cannot
    /// arrive at by looking at either layer alone, and the reason the two
    /// layers are drawn on the same row.
    pub has_distraction: bool,
}

/// The plan overlay. Deliberately outside the precedence order: an intention
/// that silently becomes actual time is how a planner starts lying to you.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayPlan {
    pub block_id: String,
    pub title: String,
    pub project_colour: Option<String>,
    pub starts_at: Millis,
    pub duration_sec: i64,
    pub tracked_sec: i64,
    pub drift_sec: i64,
    pub drift_state: DriftState,
    pub is_fixed: bool,
    pub series_id: Option<String>,
}

/// One row of the Day table. The slot grid is a lens for the eye; the segments
/// above are the arithmetic, which is why a ten-minute session inside a
/// thirty-minute slot contributes ten minutes and not thirty.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySlot {
    pub index: i64,
    pub starts_at: Millis,
    pub ends_at: Millis,
    /// Every segment overlapping this slot, longest first.
    pub segments: Vec<DaySegment>,
    /// Plan overlay for this slot, if any.
    pub plans: Vec<DayPlan>,
    /// The dominant owner, for the single-glance read down the column.
    pub state: SlotState,
}

/// The one word that describes a slot (Plan Rev 3 §7 "required time states").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SlotState {
    /// No record of any kind, and no plan either.
    Empty,
    /// A block covers it and nothing happened. The most actionable state on
    /// the screen: it is the difference between intending and doing.
    PlannedNotStarted,
    ConfirmedWork,
    ConfirmedLife,
    /// Accounted for, deliberately unrecorded.
    Private,
    /// The machine saw something; nobody has said what it was.
    ObservedOnly,
    Idle,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AreaTotal {
    pub area_id: String,
    pub name: String,
    pub colour: String,
    pub kind: AreaKind,
    pub seconds: i64,
    pub monthly_target_sec: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTotal {
    pub project_id: Option<String>,
    pub name: String,
    pub colour: Option<String>,
    pub seconds: i64,
}

/// The day's arithmetic, summed from segments.
///
/// The invariant that matters: `confirmed_work + confirmed_life + private +
/// observed_only + idle + empty == day_sec`, where `day_sec` is 24 hours — or
/// 23 or 25 across a DST transition.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayTotals {
    pub day_sec: i64,
    pub planned_sec: i64,
    pub confirmed_work_sec: i64,
    pub confirmed_life_sec: i64,
    /// Confirmed life time in a `rest` area. A subset of `confirmed_life_sec`,
    /// broken out because the workbook reports sleep on its own line and a
    /// third of the month landing in one bucket makes every other one unreadable.
    pub sleep_sec: i64,
    pub private_sec: i64,
    pub observed_only_sec: i64,
    pub idle_sec: i64,
    pub empty_sec: i64,
    /// Confirmed life time in an `entertainment` area, plus observed-only time
    /// the classifier called entertainment.
    pub entertainment_sec: i64,
    /// Every second the machine was observed in front of someone, whether or
    /// not it also belongs to a confirmed record. **Overlaps the totals above
    /// on purpose** — it answers "how much of this day was at the PC", which is
    /// a different question from "how was this day spent".
    pub pc_sec: i64,
    pub by_area: Vec<AreaTotal>,
    pub by_project: Vec<ProjectTotal>,
    pub by_app: Vec<AppTotal>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayView {
    pub local_date: String,
    pub tz: String,
    pub slot_minutes: i64,
    pub starts_at: Millis,
    pub ends_at: Millis,
    pub slots: Vec<DaySlot>,
    pub segments: Vec<DaySegment>,
    pub totals: DayTotals,
    pub is_reconciled: bool,
    pub is_today: bool,
    pub now: Millis,
}

// ─── the month (Plan Rev 3 §8.5, wireframe screen 3) ───────────────────

/// One day's arithmetic, for the month's per-day panels.
///
/// Deliberately a summary rather than a whole `DayView`: the dashboard draws
/// 31 of these, and shipping 31 × 48 slots to render a heatmap would be a
/// megabyte of payload for 31 numbers.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthDay {
    pub local_date: String,
    pub day_of_month: i64,
    pub day_sec: i64,
    pub confirmed_work_sec: i64,
    pub confirmed_life_sec: i64,
    pub sleep_sec: i64,
    pub private_sec: i64,
    pub observed_only_sec: i64,
    pub idle_sec: i64,
    pub empty_sec: i64,
    pub entertainment_sec: i64,
    /// Entertainment that fell inside a plotted block. Zero until entertainment
    /// windows exist — see `MonthView::planned_entertainment_note`.
    pub planned_entertainment_sec: i64,
    pub is_reconciled: bool,
    /// The day has ended. A day that has not arrived is not "unreviewed" — it
    /// is simply not yet, and marking it as a problem is how a dashboard trains
    /// someone to ignore its warnings.
    pub has_happened: bool,
    /// `0.0`–`1.0`. What the data-quality heatmap shades by.
    pub accounted_ratio: f64,
}

/// One line of the "Monthly findings" panel: a fact, its number, and why it is
/// on the list. Computed in Rust so the renderer never decides what counts as a
/// finding — that judgement is the panel's whole content.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthFinding {
    pub key: String,
    pub label: String,
    /// Pre-formatted, because "+38%" and "14h 20m" are different kinds of number.
    pub value: String,
    pub detail: Option<String>,
    /// True when this is something to act on rather than merely notice.
    pub is_warning: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthView {
    /// `YYYY-MM`.
    pub month: String,
    pub label: String,
    pub from: String,
    pub to: String,
    pub tz: String,
    pub days: Vec<MonthDay>,
    /// The same shape the Day view uses, summed over the month — so a figure
    /// here and the same figure on a day can never be computed two ways.
    pub totals: DayTotals,
    /// Everything except unaccounted, over the days that have **happened**.
    pub accounted_ratio: f64,
    /// How much of the month has actually elapsed, and how much of *that* is
    /// unaccounted for. The whole-month `totals` include days that have not
    /// arrived, so a card reading "unaccounted 696h" beside "accounted 40%"
    /// would be two readings of the same month. These are the pair that agree.
    pub elapsed_sec: i64,
    pub elapsed_empty_sec: i64,
    pub unreconciled_days: i64,
    pub findings: Vec<MonthFinding>,
    /// Why the planned-entertainment series is flat. Stated rather than drawn
    /// as an empty axis with no explanation.
    pub planned_entertainment_note: Option<String>,
}
