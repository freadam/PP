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
    /// What kind of work this is — "Coding", "Meeting" (migration 0013). One
    /// per task, which is what makes the split in the work report add to the
    /// total exactly once.
    pub category_id: Option<String>,
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
    /// What the interval was plotted *for* (migration 0009).
    pub intent: BlockIntent,
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
    /// What the interval was plotted *for*. Defaults to work, because every
    /// block that existed before this column was one.
    #[serde(default)]
    pub intent: Option<BlockIntent>,
}

/// What a plotted interval was for (migration 0009).
///
/// An evening plotted for a film is a plan for an interval, which is exactly
/// what a block is — so this is a column rather than a second table the Planner,
/// the Day view and the reconciler would each have to learn about separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BlockIntent {
    #[default]
    Work,
    /// The half of M11 that was missing: entertainment you *meant* to have.
    Entertainment,
    Life,
}

impl BlockIntent {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "work" => Self::Work,
            "entertainment" => Self::Entertainment,
            "life" => Self::Life,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::Entertainment => "entertainment",
            Self::Life => "life",
        }
    }
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
    /// How you were involved. Settable here rather than only afterwards,
    /// because the case manual entry exists for is very often a meeting — and
    /// "I attended two hours" and "I did two hours" are the distinction
    /// contribution exists to draw.
    #[serde(default)]
    pub contribution: Option<Contribution>,
    /// Clear any confirmed record already covering this interval.
    ///
    /// The same flag `NewLifeEntry` carries, for the same reason and with the
    /// same default. Recording work by hand is usually filling a gap, but it is
    /// sometimes correcting an hour the app got wrong, and the second case
    /// needs the old record gone or the day would hold both.
    #[serde(default)]
    pub replace_existing: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPatch {
    /// Move this record to a different task.
    ///
    /// The commonest correction there is: the interval is right, the hour is
    /// right, and it was filed against the wrong thing. Reassigning rather than
    /// deleting-and-re-adding keeps the row's id, its note and its contribution
    /// mode, so a fix costs one field instead of retyping the record.
    pub task_id: Option<String>,
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
    /// When a **focus session's** intended length runs out (W3).
    ///
    /// Advisory, never enforcing: the timer does not stop, nothing is blocked,
    /// and going past it shows in drift as the overrun it is. Starting *"45
    /// minutes on this"* is a different act from starting a stopwatch, and this
    /// is the difference.
    pub focus_ends_at: Option<Millis>,
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
            focus_ends_at: None,
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
    /// The sentence under the title: what this item is and why it is on the
    /// list. Written here so the decision copy has one author.
    pub explanation: String,
    /// Why the default verb is the default. Shown on the recommended choice.
    pub recommendation: Option<String>,
    /// Where the claim came from — populated for observed items, which are the
    /// only ones whose subject is something the machine asserted.
    pub evidence: Option<ReconcileEvidence>,
}

/// The evidence panel (wireframe screen 4, right column).
///
/// It exists because an observed item asks the user to accept a *machine's*
/// claim, and the only fair way to ask is to show the claim's provenance. The
/// `storage` line is the privacy promise restated at the moment it matters —
/// which is the moment someone is looking at a record of what they did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileEvidence {
    pub source: String,
    pub subject: String,
    pub confidence: String,
    /// What sits either side of this interval, for orientation.
    pub adjacent: Vec<String>,
    pub storage: String,
    /// The registrable domain this claim came from, when it came from the
    /// browser connector. Present only when a *prospective rule* is possible —
    /// the sheet offers "apply this to future activity" exactly when there is
    /// something durable to key a rule on, which an application name is not.
    pub domain: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReconcileKind {
    Overran,
    NeverStarted,
    UntrackedGap,
    UnplannedSession,
    /// The machine saw an application and nobody said what the time was (M10).
    /// The only item kind whose *subject* is a machine claim rather than a
    /// human one, which is why it is the one that ships an evidence panel.
    ObservedOnly,
    /// An hour of the day with no record and no observation at all (M10).
    /// Reconciling it is the difference between a day that is 60% accounted for
    /// and one that is 60% accounted for *and you know why*.
    Empty,
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
    /// Confirm an interval as life time in a named area — the verb that turns
    /// an observation into a record.
    RecordAsLife,
    /// Accounted for on purpose, nothing recorded about it.
    MarkPrivate,
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
    /// For `RecordAsLife`.
    pub life_area_id: Option<String>,
    /// "Apply my choice to future activity in this context" (wireframe 4).
    ///
    /// Set to the item's observed domain to write a rule alongside the decision,
    /// so tomorrow's YouTube is classified without being asked about again.
    /// `None` means "this interval only", which stays the default: a rule made
    /// by accident is a rule that quietly mislabels a month.
    #[serde(default)]
    pub rule_for_domain: Option<String>,
    /// Which label the rule should apply. Optional: when absent the verb
    /// decides, so the checkbox alone is enough for the common case and the
    /// picker only appears when the user wants something more specific than
    /// "work" or "not work".
    #[serde(default)]
    pub rule_category_id: Option<String>,
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
    /// Whether the browser connector's domains may be recorded. Independent of
    /// `enabled`, and off by default.
    pub domains_enabled: bool,
    pub paused: bool,
    pub excluded_apps: Vec<String>,
    /// Registrable domains never written, matched after reduction so an entry
    /// covers every subdomain.
    pub excluded_domains: Vec<String>,
    pub excluded_title_patterns: Vec<String>,
    /// 30 / 90 / 0 for forever.
    pub retention_days: i64,
    /// Observation shorter than this is not reported. 0 disables the floor.
    pub min_span_sec: i64,
    /// Shown in Settings, because "we delete it eventually" is not a promise.
    pub next_purge_at: Option<Millis>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySample {
    pub app_id: String,
    pub window_title: Option<String>,
    /// Set only by the browser connector, and only ever a registrable domain.
    /// `None` from the foreground-window sampler, which cannot see one.
    #[serde(default)]
    pub domain: Option<String>,
    pub at: Millis,
}

/// What an observed domain is taken to mean. Deliberately three values, not the
/// four of `AreaKind`: `rest` is something a person asserts about themselves and
/// no domain implies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DomainCategory {
    Core,
    Entertainment,
    Other,
}

impl DomainCategory {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "core" => Self::Core,
            "entertainment" => Self::Entertainment,
            "other" => Self::Other,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Entertainment => "entertainment",
            Self::Other => "other",
        }
    }
}

/// A label the user can put on observed time.
///
/// Replaces `DomainCategory` as the thing rules point at. That enum survives as
/// [`ObservationCategory::counts_as`] — the roll-up every existing report reads,
/// so adding "Study" can never move a number that was already on screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservationCategory {
    pub id: String,
    pub name: String,
    pub colour: String,
    /// Which of the three original verdicts this rolls up into. Fixed once, for
    /// the reason above.
    pub counts_as: DomainCategory,
    pub is_builtin: bool,
    pub sort_rank: f64,
    /// Observed seconds carrying this label, over whatever range was asked for.
    /// Zero when the caller did not ask for totals.
    pub seconds: i64,
}

/// Built-in category ids. Fixed rather than generated so migration 0007 can
/// seed them and backfill in one transaction — see the migration's own note.
pub mod category {
    pub const WORK: &str = "00000000-0000-7000-8000-000000000001";
    pub const STUDY: &str = "00000000-0000-7000-8000-000000000002";
    pub const DISTRACTION: &str = "00000000-0000-7000-8000-000000000003";
    pub const LIFE: &str = "00000000-0000-7000-8000-000000000004";
    /// Where observation lands when the user has decided it is nothing in
    /// particular. Distinct from *unlabelled*, which is `NULL`.
    pub const OTHER: &str = "00000000-0000-7000-8000-000000000005";
}

/// What a rule matches against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchKind {
    /// The executable or bundle name — `chrome.exe`, `Code.exe`.
    App,
    /// A registrable domain, already reduced — `instagram.com`.
    Domain,
}

impl MatchKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "app" => Self::App,
            "domain" => Self::Domain,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Domain => "domain",
        }
    }
}

/// One row of "this thing means this".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityRuleRow {
    pub id: String,
    pub match_kind: MatchKind,
    pub match_value: String,
    pub category_id: String,
    pub category_name: String,
    pub category_colour: String,
    /// Shipped with the app and never edited. A rule the user has touched is
    /// theirs, and the Settings list says so rather than silently replacing it
    /// on the next update.
    pub is_builtin: bool,
    /// How many already-recorded but **unlabelled** stretches this rule just
    /// filled in. Only meaningful on the row returned by `set_activity_rule`;
    /// zero when listing.
    ///
    /// Reported so the app can say "also labelled 3 earlier stretches" — a rule
    /// that quietly changes what is on screen is worse than one that says so.
    #[serde(default)]
    pub backfilled: i64,
}

/// One continuous stretch of observation, as a thing a person can point at.
///
/// The aggregate below answers "what should this *always* be"; this answers
/// "what was *that*". They need different verbs — a rule and a relabel — and
/// conflating them is how someone turns one YouTube lecture into a standing
/// declaration that YouTube is study.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservedInterval {
    pub id: i64,
    pub started_at: Millis,
    pub ended_at: Millis,
    pub seconds: i64,
    pub app_id: String,
    /// The site, where the connector saw one.
    pub domain: Option<String>,
    /// What the window was called — the video, the document, the ticket. Only
    /// present when window titles are switched on, and it is the field that
    /// tells two stretches of the same site apart.
    pub window_title: Option<String>,
    pub category_id: Option<String>,
}

/// Something observed that no rule covers, ranked by how much of your time it
/// took. The list the labelling UI is built on: the app names the few things
/// worth naming instead of presenting an empty taxonomy.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlabelledRow {
    pub match_kind: MatchKind,
    pub match_value: String,
    pub seconds: i64,
    /// How many separate stretches. One 4-hour stretch and forty 6-minute ones
    /// are different problems wearing the same total.
    pub occurrences: i64,
    /// The stretches themselves, longest first, so each can be labelled on its
    /// own without first committing to a rule about the whole site.
    pub stretches: Vec<ObservedInterval>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySpanRow {
    pub id: i64,
    pub started_at: Millis,
    pub ended_at: Millis,
    pub app_id: String,
    pub window_title: Option<String>,
    /// The site, where the connector saw one. What the row should be named
    /// after — `chrome.exe` is not an answer to "what was I doing".
    pub domain: Option<String>,
    /// The label in force when this was written. `None` is unlabelled.
    pub category_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppTotal {
    pub app_id: String,
    pub seconds: i64,
}

/// Confirmed work, split by how you were involved (§3.5, §4.8).
///
/// `None` is a real row, not a missing one: it means *work with no contribution
/// mode recorded*, which §5.8 distinguishes from non-work time. Non-work has no
/// contribution field at all, so it can never appear here — the split is
/// structural rather than filtered.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributionTotal {
    pub contribution: Option<Contribution>,
    pub seconds: i64,
}

/// Observed browser time per domain. The month's version of `domain_totals`,
/// carried on the day so the month gets it by summing rather than by running a
/// second query that could disagree.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainSeconds {
    pub domain: String,
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
    /// The longest few, for the bar. **Truncated**, so this does not sum to
    /// `observed_sec` and must never be used to work out coverage.
    pub top_apps: Vec<AppTotal>,
    /// Every second observed under the block, before the truncation above.
    ///
    /// The figure that makes the comparison honest. Without it a caller can
    /// only add up `top_apps` and normalise against that, which reports *what
    /// the observed time was made of* while looking exactly like *how much of
    /// the block was observed* — so three hours with nineteen minutes under
    /// them draw the same full bar as three hours with three hours under them.
    ///
    /// Always ≤ `duration_sec`: the overlaps are clipped to the block.
    pub observed_sec: i64,
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

// ─── weekly goals (PLAN-WEEKLY-GOALS.md W1/W2) ─────────────────────────

/// What a goal is measured against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GoalSubject {
    /// A column of the day's own totals — see [`GoalMetric`].
    Metric,
    LifeArea,
    Project,
    /// Observed time carrying one label (migration 0007).
    Category,
}

impl GoalSubject {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "metric" => Self::Metric,
            "lifeArea" => Self::LifeArea,
            "project" => Self::Project,
            "category" => Self::Category,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Metric => "metric",
            Self::LifeArea => "lifeArea",
            Self::Project => "project",
            Self::Category => "category",
        }
    }
}

/// The whole-day quantities a goal can be set against. Named constants rather
/// than free text so a typo cannot become a goal that silently measures nothing.
pub mod metric {
    pub const ALL_WORK: &str = "allWork";
    pub const LIFE: &str = "life";
    pub const SLEEP: &str = "sleep";
    pub const ENTERTAINMENT: &str = "entertainment";
    pub const UNACCOUNTED: &str = "unaccounted";
    pub const ALL: [&str; 5] = [ALL_WORK, LIFE, SLEEP, ENTERTAINMENT, UNACCOUNTED];
}

/// **First-class, not a sign convention.** "At most 5h of entertainment" is a
/// goal you succeed at by being under it, and a bar that turns red when you do
/// the right thing teaches people to ignore bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GoalDirection {
    AtLeast,
    AtMost,
}

impl GoalDirection {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "atLeast" => Self::AtLeast,
            "atMost" => Self::AtMost,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AtLeast => "atLeast",
            Self::AtMost => "atMost",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalRow {
    pub id: String,
    pub subject_kind: GoalSubject,
    pub subject_id: String,
    /// Resolved for display — the area's name, the project's, the label's, or
    /// the metric's own wording. The renderer never looks one up.
    pub subject_name: String,
    pub direction: GoalDirection,
    pub target_sec: i64,
    /// `day` | `week` | `month` (migration 0013). A daily and a weekly target
    /// on the same subject are not a contradiction — "6h a day, and no more
    /// than 32h a week" is a coherent pair — so the period is part of a goal's
    /// identity, not a display detail.
    pub period: String,
    /// Bitmask, Monday = 1 … Sunday = 64.
    pub applies_days: i64,
    pub starts_week: String,
    pub ends_week: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewGoal {
    pub subject_kind: Option<GoalSubject>,
    pub subject_id: String,
    /// `day` | `week` | `month`. Defaults to `week`, which is what every goal
    /// written before migration 0013 was.
    pub period: Option<String>,
    pub direction: Option<GoalDirection>,
    pub target_sec: i64,
    pub applies_days: Option<i64>,
}

/// How a goal is doing, in the only terms that let someone act on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GoalState {
    /// `atLeast` and the target is reached; `atMost` and the week is over
    /// without breaching it.
    Met,
    /// Inside the band the elapsed part of the week implies.
    OnPace,
    /// `atLeast`, and behind where the elapsed days say you would be.
    Behind,
    /// `atMost`, over the pace but not yet past the target — still savable.
    AtRisk,
    /// `atMost`, and the budget is spent.
    Over,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalProgress {
    pub goal: GoalRow,
    pub actual_sec: i64,
    /// `target × (elapsed applicable days ÷ all applicable days)`, today clipped
    /// to the clock. **The future is never a shortfall** — a goal at zero on
    /// Monday morning is on pace, and has to say so.
    pub expected_sec: i64,
    /// Positive is good in both directions: ahead of pace, or under budget.
    pub delta_sec: i64,
    pub state: GoalState,
    /// The row that changes behaviour. `"3h 20m a day for the remaining 3 days"`
    /// is a decision you can make at breakfast; "62% of target" is a fact.
    /// `None` once the week is out of applicable days.
    pub per_day_needed_sec: Option<i64>,
    pub applicable_days: i64,
    pub days_left: i64,
    /// Pre-formatted, because the sentence differs by direction and by state and
    /// deriving it in the renderer would be a second implementation of the rules.
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekReview {
    /// ISO week, `YYYY-Www`.
    pub week: String,
    pub from: String,
    pub to: String,
    pub tz: String,
    /// The same shape the Day view uses, summed over the week — so a figure here
    /// and the same figure on a day cannot be computed two ways.
    pub totals: DayTotals,
    pub days: Vec<MonthDay>,
    pub elapsed_sec: i64,
    pub elapsed_empty_sec: i64,
    pub unreconciled_days: i64,
    pub goals: Vec<GoalProgress>,
    /// Summed over the week, and against the week before — direction of travel
    /// is the point. One week's longest stretch in isolation says nothing.
    pub fragmentation: Fragmentation,
    pub previous_fragmentation: Fragmentation,
    /// Goals whose history says the number has stopped being a goal. Offered,
    /// never applied.
    pub calibration: Vec<GoalCalibration>,
}

/// What a merge did. `absorbed_sec` is the part worth saying out loud: a merge
/// asserts that the gap between two records was part of the same thing, and a
/// merge that annexed time silently would be indistinguishable from a bug.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeResult {
    /// The row that survived — always the earliest, so anything already
    /// pointing at it still points at something.
    pub id: String,
    pub merged: i64,
    pub seconds: i64,
    pub absorbed_sec: i64,
}

// ─── workbook import (M13, §4.8) ───────────────────────────────────────

/// What a label in the sheet means. There are exactly four answers, and one of
/// them is "nothing" — because the alternative to an explicit *ignore* is an
/// implicit one, and §4.8's "no silent loss" is a rule about silence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
// `rename_all` renames the *variants*; `rename_all_fields` renames what is
// inside them. Both are needed, and forgetting the second is silent — the JSON
// simply carries `life_area_id` and the renderer reads `undefined`.
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
pub enum ImportTarget {
    /// Read, counted, and deliberately not written. The preview says how much.
    Ignore,
    /// `task_id` omitted means "a task named after the label", created once and
    /// reused — otherwise a second import of the same month doubles the backlog
    /// rather than the hours.
    Work {
        #[serde(default)]
        task_id: Option<String>,
    },
    Life {
        life_area_id: String,
    },
    /// Intentionally untracked: the interval is accounted for, nothing is
    /// recorded about it.
    Private,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRule {
    pub label: String,
    pub target: ImportTarget,
}

/// Everything the importer needs, all of it confirmed by a person.
///
/// `suggest_import_mapping` fills this in from what the file looks like; every
/// field remains the user's to change, and the ones it cannot fill honestly it
/// leaves empty rather than guessing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportMapping {
    pub sheet: String,
    /// Zero-based, as the sheet is indexed.
    pub header_row: usize,
    pub time_col: usize,
    pub columns: Vec<crate::xlsx::DayColumn>,
    pub slot_minutes: i64,
    /// `YYYY-MM`. Empty blocks the import: a sheet headed "1 … 31" does not say
    /// which month it is, and picking one for the user is picking wrong.
    pub month: String,
    pub tz: String,
    pub rules: Vec<ImportRule>,
    /// The user's answer to a conflict. False refuses; true replaces.
    #[serde(default)]
    pub replace_existing: bool,
}

/// The variance preview — §4.8's precondition for importing anything.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub sheet: String,
    pub month: String,
    pub tz: String,
    pub slot_minutes: i64,
    pub total_sec: i64,
    /// Time a rule says to drop. Reported because dropping it was a decision.
    pub ignored_sec: i64,
    pub days: Vec<ImportDay>,
    /// Labels nobody has placed. Non-empty blocks the commit.
    pub unmapped: Vec<crate::xlsx::LabelCount>,
    pub conflicting_dates: Vec<String>,
    /// Everything standing between this preview and a commit, in sentences.
    /// Empty means the import will go through.
    pub blocking: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDay {
    pub local_date: String,
    pub slots: i64,
    /// What the sheet holds for this day, once the rules are applied.
    pub seconds: i64,
    /// What Fruit already holds. The pair is the point.
    pub existing_sec: i64,
    /// `seconds − existing_sec`. Signed, because "12h of variance" without a
    /// direction is a number nobody can act on.
    pub variance_sec: i64,
    pub conflicts: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub batch_id: String,
    pub month: String,
    pub sessions_written: i64,
    pub life_entries_written: i64,
    /// Rows the import replaced, because the user chose to replace them.
    pub records_replaced: i64,
    pub total_sec: i64,
}

/// One past import. Kept so it can be named, and undone.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBatch {
    pub id: String,
    pub source_path: String,
    pub sheet: String,
    pub month: String,
    pub tz: String,
    pub sessions_written: i64,
    pub life_entries_written: i64,
    pub created_at: Millis,
}

// ─── the Monday-morning report (W9) ────────────────────────────────────

/// Everything the weekly report says, in the order it says it.
///
/// The card in the app and the workbook on disk are both rendered from this, so
/// what Fruit tells you is waiting and what the file contains cannot disagree.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekReport {
    /// The top of the page, and the only part most readings get to.
    pub headline: WeekHeadline,
    pub review: WeekReview,
    /// Where the plan and the week disagreed most, and on which day.
    pub divergence: Option<WeekDivergence>,
    /// W8's ranked list, trimmed to what fits a skim.
    pub unlabelled: Vec<UnlabelledRow>,
}

/// *"The most important information is right at the top — the Work hours from
/// last week and the breakdown into categories."* This struct is that sentence
/// and nothing else, deliberately: anything that creeps in here pushes the two
/// figures the reader came for below the fold.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekHeadline {
    pub work_sec: i64,
    pub categories: Vec<WeekCategory>,
    /// The week has ended. False means the report is about a week still
    /// happening, and it must not call itself "last week".
    pub is_complete: bool,
}

/// One row of the breakdown. `share` is carried rather than left to the
/// renderer because the rows partition the week, and a percentage computed
/// against the wrong denominator is the classic way that stops being true.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekCategory {
    pub name: String,
    pub seconds: i64,
    pub share: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DivergenceKind {
    /// A day whose tracked work ran past what was plotted for it.
    Overrun,
    /// Entertainment outside any window that was plotted for it.
    UnplannedEntertainment,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekDivergence {
    pub local_date: String,
    pub kind: DivergenceKind,
    pub seconds: i64,
    /// Pre-formatted, because the sentence differs by kind and deriving it in
    /// the renderer would be a second implementation of the same rule.
    pub detail: String,
}

/// A report waiting to be read. `None` from `due_week_report` means there is
/// nothing to say, which is a legitimate answer and not an error.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekReportDue {
    /// ISO week, `YYYY-Www`.
    pub week: String,
    pub from: String,
    pub to: String,
    pub headline: WeekHeadline,
    /// Where the file went, if one has been written and is still there. `None`
    /// means the card can offer to write it.
    pub written_to: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekReportResult {
    pub path: String,
    pub week: String,
    pub headline: String,
    pub sheets: Vec<String>,
}

/// A goal Fruit is prepared to suggest, with the number it picked and where it
/// came from.
///
/// Rize ships eight templates and its reviewer used one, which is the strongest
/// evidence available that templates beat a blank goal form. The difference here
/// is that **every template states the number it chose and why**: a template that
/// opens with an invented round figure is one people dismiss, and a goal you did
/// not believe when you set it is one you will not honour on Thursday.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalTemplate {
    pub key: String,
    pub name: String,
    pub subject_kind: GoalSubject,
    pub subject_id: String,
    pub direction: GoalDirection,
    /// `None` when there is not enough history to pick honestly. The template
    /// still appears, and says so — asking beats guessing.
    pub target_sec: Option<i64>,
    pub applies_days: i64,
    /// Where the number came from, in words. Empty templates explain the gap.
    pub rationale: String,
}

/// "A goal you miss every week has stopped being a goal."
///
/// The same discipline the estimate calibration already holds itself to:
/// trailing median, reported only at **n ≥ 5**, so five weeks of noise cannot
/// move a recommendation.
///
/// **Offered, never applied.** The user may have a good reason the last three
/// weeks were atypical, and an app that quietly lowers your ambitions is worse
/// than one that says nothing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalCalibration {
    pub goal_id: String,
    /// The subject, so applying the suggestion is `set_goal` on the same
    /// quantity. Without it the caller would have to reach back through the
    /// goal id, and the obvious mistake — passing the goal id *as* the subject
    /// — fails validation at the boundary rather than at the point of use.
    pub subject_kind: GoalSubject,
    pub subject_id: String,
    pub direction: GoalDirection,
    pub subject_name: String,
    pub target_sec: i64,
    /// Median of the completed weeks looked at.
    pub median_sec: i64,
    pub weeks: i64,
    /// How many of those weeks the goal was met in.
    pub weeks_met: i64,
    /// What the history suggests instead. Never further from reality than the
    /// current target, and never a change of direction.
    pub suggested_sec: i64,
    pub summary: String,
}

// ─── notices (PLAN-WEEKLY-GOALS.md W4/W5) ──────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NoticeKind {
    /// A focus session reached the length it was started for (W3).
    FocusElapsed,
    /// Heads-down for a while. Meetings and personal time excluded.
    ContinuousWork,
    /// Past the day's work ceiling.
    DailyCeiling,
    /// Entertainment observed during an hour you plotted for something else.
    /// A notice, never a block — see `store::notices`.
    OffPlan,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub kind: NoticeKind,
    /// Identifies the crossing, not the notice. Recorded so a threshold is
    /// mentioned once and a second crossing can still speak.
    pub key: String,
    pub title: String,
    pub body: String,
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

// ─── kinds of work (migration 0013) ────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCategoryRow {
    pub id: String,
    pub name: String,
    pub colour: String,
    pub sort_rank: f64,
    /// Shipped rather than typed. Cleared the moment the user renames it.
    pub is_builtin: bool,
    /// Live tasks carrying this category, so a delete can say what it frees.
    pub task_count: i64,
    pub created_at: Millis,
    pub updated_at: Millis,
}

// ─── the work report (migration 0013) ──────────────────────────────────

/// The five work reports, over one range, in one round trip.
///
/// One bundle rather than five commands because they are always read together
/// and they share a range: five calls would mean five range parses, five
/// timezone resolutions, and five chances for the panels on one screen to
/// disagree about which week it is.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkReport {
    pub from: String,
    pub to: String,
    pub tz: String,
    /// `day` | `week` | `month` — what the caller asked for, echoed back so the
    /// renderer never has to re-derive which button is pressed.
    pub period: String,
    /// Confirmed work in the range, and the daily target it is measured
    /// against when one is set.
    pub total_work_sec: i64,
    pub target_sec: Option<i64>,
    /// Days in the range on which the daily target applied and was met. `None`
    /// when there is no daily target — "0 of 0 days met" is not a fact.
    pub target_days_met: Option<i64>,
    pub target_days_applicable: Option<i64>,
    /// One entry per local date in the range, oldest first. Always complete —
    /// a day with no work is a zero, never a missing point, or the graph draws
    /// a straight line through a week off.
    pub days: Vec<WorkDay>,
    pub by_category: Vec<WorkSlice>,
    pub by_project: Vec<WorkSlice>,
    pub focus: FocusSummary,
    pub apps: Vec<AppUsage>,

    /// Confirmed work by hour of local day, 0–23, summed across the range.
    ///
    /// The one chart that answers "*when* do I actually work", which no total
    /// can. A week whose hours are identical to last week's but shifted three
    /// hours later is a different week, and this is the only place that shows.
    pub by_hour: Vec<i64>,
    /// The same range over the previous four comparable periods, averaged.
    ///
    /// `None` when there is not enough history — and that is reported rather
    /// than papered over. A "vs. average" arrow computed from one prior week is
    /// noise wearing the costume of a trend, so the baseline states how many
    /// periods it actually had.
    pub baseline: Option<WorkBaseline>,
    pub score: WorkScore,
    /// Sentences, generated from the figures above.
    ///
    /// A dashboard makes you do the reading; a report does the reading for you
    /// and shows its working. Every finding here names the number it came from,
    /// so it can be checked against the panel beside it.
    ///
    /// They state facts and stop. Rize's equivalent ends "Consider taking this
    /// week lightly if possible" — this app does not tell you what to do about
    /// your own week, because it does not know what your week was for.
    pub findings: Vec<Finding>,

    /// Observed time by the label you gave it, over the whole range.
    ///
    /// Unlike everything above it, this is *observed* rather than confirmed —
    /// what the machine saw, not what anyone signed off. It sits beside the
    /// confirmed panels rather than among them, and the screen says which is
    /// which, because a work total that quietly absorbed "Chrome was open" is
    /// the one dishonesty this app exists to avoid.
    ///
    /// Empty when Activity is off or nothing in the range carries a label.
    pub by_label: Vec<ObservationCategory>,
    /// Each plotted block in the range against the apps that were actually in
    /// front of you while it ran, newest last.
    ///
    /// The only place an intention meets an observation rather than another
    /// record of itself. Blocks with nothing observed under them are left out —
    /// an empty bar says "you did nothing", when what happened is that nothing
    /// was watching.
    pub correlations: Vec<BlockCorrelation>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Stable, so the renderer can key on it without matching prose.
    pub id: String,
    /// The claim, in one line.
    pub headline: String,
    /// Where the number came from, and what it is measured against.
    pub detail: String,
    /// `neutral` | `good` | `watch`. Never the only carrier — the headline
    /// says the thing, and the tone only decides which rule it gets.
    pub tone: String,
}

/// What the last few comparable periods looked like, so every headline figure
/// can say whether it is up or down.
///
/// Averaged over periods **with something recorded in them**. A week you did
/// not use Fruit is silence, not a zero, and averaging it in would manufacture
/// a downward trend out of a holiday — the same rule the goal calibration
/// already applies.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkBaseline {
    /// How many prior periods actually contributed. Shown, not hidden: "vs. the
    /// last 2 weeks" and "vs. the last 4" are different claims.
    pub periods: i64,
    pub total_work_sec: i64,
    pub focus_sec: i64,
    pub focus_sessions: i64,
    pub score: i64,
    /// Average length of a working day, over days with any work in them.
    pub day_length_sec: i64,
}

/// One legible number for how the period went, and the arithmetic behind it.
///
/// Rize ships a "focus quality score" derived from twenty-plus attributes and
/// shows none of them. A score you cannot open is a score you cannot argue
/// with, and this app's whole position is that a figure you cannot audit is a
/// figure you do not believe — so this one has four inputs, each of which is
/// reported with its own value and weight, and the panel prints the sum.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkScore {
    /// 0–100.
    pub value: i64,
    /// A word, so the number is never the only carrier.
    pub rating: String,
    pub components: Vec<ScoreComponent>,
    /// False when there was nothing to score. A score of 0 for a week you were
    /// on holiday is a verdict the app has no business delivering.
    pub scored: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreComponent {
    pub label: String,
    /// What this input scored, 0–100.
    pub value: i64,
    /// Its share of the total, 0–1.
    pub weight: f64,
    /// The sentence that says where the number came from.
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkDay {
    pub date: String,
    pub work_sec: i64,
    /// Whether the daily target applied on this weekday, so the graph can draw
    /// the target line only where it means something.
    pub target_applies: bool,
    pub focus_sec: i64,
    /// When work first started and last stopped on this day, local ms.
    ///
    /// The *span* of the working day, which is a different fact from its
    /// length: six hours between 09:00 and 15:00 and six hours between 09:00
    /// and 23:00 are the same total and very different days.
    pub first_at: Option<Millis>,
    pub last_at: Option<Millis>,
    /// Confirmed non-work life time, so the day bar can show what the rest of
    /// the day went to rather than implying the remainder was idle.
    pub life_sec: i64,
}

/// A named share of the work total. Used for both the category and project
/// splits, because they are the same shape and the same chart.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkSlice {
    /// `None` is the uncategorised / no-project bucket, which is always shown.
    /// Hiding it would make the percentages add to less than the total and
    /// leave the user unable to find the time that went missing.
    pub id: Option<String>,
    pub name: String,
    pub colour: String,
    pub seconds: i64,
    pub share: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusSummary {
    pub sessions: i64,
    /// What the sessions were *plotted* for.
    pub planned_sec: i64,
    /// What was actually tracked against them. The pair is the point: a focus
    /// session is an intention, and the gap between the two is the same drift
    /// the rest of the app is built around.
    pub tracked_sec: i64,
    pub longest_sec: i64,
    pub completed: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUsage {
    pub app_id: String,
    pub name: String,
    pub seconds: i64,
    pub share: f64,
    /// The verdict stamped on the spans at write time — never recomputed here.
    pub category: Option<String>,
}

/// What `reset_all_data` removed. Counted *before* the delete, because the
/// point of the figures is to say what was destroyed — a confirmation reading
/// "0 projects, 0 tasks" after the fact tells the user nothing about whether
/// the thing they were worried about is gone.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetSummary {
    pub projects: i64,
    pub tasks: i64,
    pub blocks: i64,
    pub sessions: i64,
    pub life_entries: i64,
    pub activity_spans: i64,
    /// The snapshot taken immediately before the delete. `None` only for an
    /// in-memory store, which has no file to snapshot.
    pub backup_path: Option<String>,
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
    /// Set on every instance of a repeating entry (migration 0012). Sleep is
    /// the case this exists for: the largest block of anyone's week, and the
    /// one that is genuinely the same every night.
    pub series_id: Option<String>,
    /// The rule itself, carried by the seed instance.
    pub rrule: Option<String>,
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
// `Hash` so it can key the per-contribution totals; `Ord` so those totals sort
// the same way twice rather than in whatever order a hash seed produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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
        /// Resolved for display, beside the colour that is already here. The
        /// renderer never looks a project up — the Day view's per-project
        /// filter reads this, and a filter that had to join against a separate
        /// list would show the wrong name for an archived project.
        project_name: Option<String>,
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
    /// What the interval was plotted *for* (migration 0009). An evening blocked
    /// out for a film is a plan, and the Day view draws it like one — tinted
    /// differently, so a plan-versus-record comparison does not read a planned
    /// two hours of television as two hours of missing work.
    pub intent: BlockIntent,
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
    /// The part of `planned_sec` that was plotted as entertainment — an evening
    /// you *meant* to spend on a film. A subset, not an extra bucket: it does
    /// not participate in the counting invariant, because plans are an overlay
    /// on the timeline rather than part of it.
    pub planned_entertainment_sec: i64,
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
    /// The part of `entertainment_sec` that fell inside a plotted entertainment
    /// window. `entertainment_sec` minus this is the unplanned part — the two
    /// reconcile by construction, which is what M11 asks for.
    pub entertainment_in_window_sec: i64,
    /// Every second the machine was observed in front of someone, whether or
    /// not it also belongs to a confirmed record. **Overlaps the totals above
    /// on purpose** — it answers "how much of this day was at the PC", which is
    /// a different question from "how was this day spent".
    pub pc_sec: i64,
    pub by_area: Vec<AreaTotal>,
    pub by_project: Vec<ProjectTotal>,
    pub by_app: Vec<AppTotal>,
    /// Confirmed work by involvement. Work records only, structurally.
    pub by_contribution: Vec<ContributionTotal>,
    /// Observed browser time by domain — the source of the YouTube/Twitch
    /// figures §4.8 asks the export to carry.
    pub by_domain: Vec<DomainSeconds>,
}

/// How broken up a day's work was — the components, deliberately **not** a
/// score.
///
/// Rize returns a "focus score". This app's whole argument is that its
/// arithmetic can be checked by eye — the counting invariant, Excel totals as
/// formulas rather than pasted numbers, a reconciliation table putting the app's
/// figure beside the sheet's own. A weighted 0–100 would be the single number in
/// Fruit nobody could verify, and the recurring complaint about scores is
/// exactly that their inputs are hidden.
///
/// And one thing an app that only watches window focus structurally cannot do:
/// **planned and unplanned switches are counted separately.** A switch landing
/// on a block boundary is you executing your intention; one that does not is an
/// interruption. For someone whose work genuinely requires switching, that
/// distinction is the entire question.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fragmentation {
    /// The longest unbroken run of confirmed work on **one task**. The figure
    /// that actually tracks deep work.
    pub longest_stretch_sec: i64,
    /// How many separate runs of confirmed work there were.
    pub stretches: i64,
    /// Boundaries that fall on a plotted block's own start or end.
    pub planned_switches: i64,
    pub unplanned_switches: i64,
    /// Confirmed work sitting in runs shorter than the threshold. Time that
    /// counts and accomplished little.
    pub fragmented_sec: i64,
    /// The threshold that produced `fragmented_sec`, so the figure can never be
    /// read without the rule that made it.
    pub fragment_threshold_sec: i64,
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
    pub fragmentation: Fragmentation,
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
    /// The part of `entertainment_sec` that happened inside a plotted window.
    /// The rest is unplanned — the solid line on the dashboard.
    pub entertainment_in_window_sec: i64,
    /// Time plotted as an entertainment window (a block with
    /// `intent = entertainment`, migration 0009) — entertainment you *meant*
    /// to have. Compare it with `entertainment_sec`, which is what actually
    /// happened; the gap between the two is the point of the dashboard.
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
}

// ─── Excel export (Plan Rev 3 §10, wireframe screen 5) ─────────────────

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcelOptions {
    /// Mark slots the machine saw but nobody confirmed.
    #[serde(default = "yes")]
    pub include_observed: bool,
    /// Keep blank slots as visible "Gap" cells rather than empty ones.
    #[serde(default = "yes")]
    pub include_unaccounted: bool,
    /// Off by default. Private duration is always included; this only controls
    /// whether the *area* behind it is named.
    #[serde(default)]
    pub include_private_labels: bool,
}

fn yes() -> bool {
    true
}

/// One row of the reconciliation table (wireframe screen 5).
///
/// The whole point of the export screen: a figure from the app beside the same
/// figure computed from the sheet's own cells, and the difference. Success
/// measure 7 is "exports reconcile with no unexplained variance", and this is
/// what makes that checkable rather than asserted.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcelVariance {
    pub measure: String,
    pub app_sec: i64,
    pub sheet_sec: i64,
    pub variance_sec: i64,
}

/// One cell of the month table, for the on-screen preview *and* the sheet —
/// both render from this, so the preview cannot promise a layout the file
/// doesn't have.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcelCell {
    pub label: String,
    /// `work` · `life` · `entertainment` · `rest` · `observed` · `gap` · `private`
    pub kind: String,
    pub colour: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcelPreview {
    pub month: String,
    pub label: String,
    pub file_name: String,
    /// Column headers: "1 Sat", "2 Sun", …
    pub day_headers: Vec<String>,
    /// Row labels: "06:00", "06:30", …
    pub slot_labels: Vec<String>,
    /// `rows[slot][day]`. The full month matrix, blanks included.
    pub rows: Vec<Vec<ExcelCell>>,
    pub variances: Vec<ExcelVariance>,
    pub unreconciled_days: i64,
    /// What the sheet says about itself — carried into the workbook as a note.
    pub source_note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcelExportResult {
    pub path: String,
    pub sheets: Vec<String>,
    pub rows_written: i64,
    pub variances: Vec<ExcelVariance>,
}
