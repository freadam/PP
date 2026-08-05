//! The Tauri shell.
//!
//! This crate owns windows, the tray, the clock loop and the IPC boundary —
//! and nothing else. Every rule lives in `fruit-core`, which is why that crate
//! has no Tauri dependency and this one has almost no logic (§6.8).
//!
//! Commands are intent-based, one transaction each. The renderer holds no SQL
//! strings, and the capability file lists exactly the commands below.

mod frontmost;
mod idle;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use fruit_core::clock::SystemClock;
use fruit_core::db::IntegrityReport;
use fruit_core::model::*;
use fruit_core::parser::{parse, DateOrder, ParseCtx};
use fruit_core::store::{
    IcsImportSummary, IdleReport, SeriesScope, ACTIVITY_ENABLED, ACTIVITY_PAUSED,
    SAMPLE_INTERVAL_MS,
};
use fruit_core::{AppError, Store};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

/// The renderer reports its own last input as a fallback where the OS cannot
/// (Linux). See `idle.rs`.
pub struct AppState {
    store: Mutex<Store>,
    last_window_input_ms: Mutex<i64>,
}

type Res<T> = Result<T, AppError>;

fn with<T>(state: &State<'_, AppState>, f: impl FnOnce(&mut Store) -> Res<T>) -> Res<T> {
    let mut store = state
        .store
        .lock()
        .map_err(|_| AppError::invalid("The database lock was poisoned. Restart Fruit."))?;
    f(&mut store)
}

/* ─── read ─────────────────────────────────────────────────────────────── */

#[tauri::command]
fn get_week(state: State<'_, AppState>, range: DateRange, tz: String) -> Res<WeekView> {
    with(&state, |s| s.get_week(&range, &tz))
}

#[tauri::command]
fn get_backlog(state: State<'_, AppState>, filter: BacklogFilter, tz: String) -> Res<BacklogView> {
    with(&state, |s| s.get_backlog(filter, &tz))
}

#[tauri::command]
fn get_tasks(state: State<'_, AppState>, query: TaskQuery) -> Res<Page<TaskRow>> {
    with(&state, |s| s.get_tasks(query))
}

#[tauri::command]
fn get_task_detail(state: State<'_, AppState>, id: String) -> Res<TaskDetail> {
    with(&state, |s| s.get_task_detail(&id))
}

#[tauri::command]
fn get_projects(state: State<'_, AppState>, tz: String) -> Res<Vec<ProjectRow>> {
    with(&state, |s| s.get_projects(&tz))
}

#[tauri::command]
fn get_tags(state: State<'_, AppState>) -> Res<Vec<TagRow>> {
    with(&state, |s| s.get_tags())
}

#[tauri::command]
fn get_reports(
    state: State<'_, AppState>,
    range: DateRange,
    filter: ReportFilter,
) -> Res<ReportBundle> {
    with(&state, |s| s.get_reports(&range, &filter))
}

#[tauri::command]
fn get_reconcile_items(
    state: State<'_, AppState>,
    date: String,
    tz: String,
) -> Res<Vec<ReconcileItem>> {
    with(&state, |s| s.get_reconcile_items(&date, &tz))
}

#[tauri::command]
fn get_unreconciled_days(state: State<'_, AppState>, before: String) -> Res<Vec<String>> {
    with(&state, |s| s.unreconciled_days(&before, 30))
}

#[tauri::command]
fn search(state: State<'_, AppState>, q: String, limit: u32) -> Res<SearchResults> {
    with(&state, |s| s.search(&q, limit))
}

/// The capture grammar runs in Rust so the resolution rules in §4.4 have tests,
/// and so the same parser serves capture, the estimate field and the palette.
#[tauri::command]
fn parse_capture(state: State<'_, AppState>, text: String, tz: String) -> Res<CapturePreview> {
    with(&state, |s| {
        let zone = fruit_core::time::zone(&tz)?;
        let projects = s.project_names()?;
        Ok(parse(
            &text,
            &ParseCtx {
                now: s.now(),
                tz: zone,
                order: locale_date_order(),
                known_projects: &projects,
            },
        ))
    })
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Res<Value> {
    with(&state, |s| Ok(Value::Object(s.all_settings()?)))
}

#[tauri::command]
fn get_deleted(state: State<'_, AppState>) -> Res<Vec<DeletedRow>> {
    with(&state, |s| s.deleted_rows())
}

#[tauri::command]
fn get_timer_state(state: State<'_, AppState>) -> Res<TimerState> {
    with(&state, |s| s.timer_state())
}

/* ─── write ────────────────────────────────────────────────────────────── */

#[tauri::command]
fn create_task(state: State<'_, AppState>, input: NewTask) -> Res<TaskRow> {
    with(&state, |s| s.create_task(input))
}

#[tauri::command]
fn update_task(state: State<'_, AppState>, id: String, patch: TaskPatch) -> Res<TaskRow> {
    with(&state, |s| s.update_task(&id, patch))
}

#[tauri::command]
fn set_task_status(state: State<'_, AppState>, id: String, s: Status) -> Res<TaskRow> {
    with(&state, |store| store.set_task_status(&id, s))
}

#[tauri::command]
fn delete_task(state: State<'_, AppState>, id: String) -> Res<UndoToken> {
    with(&state, |s| s.delete_task(&id))
}

#[tauri::command]
fn restore(state: State<'_, AppState>, token: UndoToken) -> Res<()> {
    with(&state, |s| s.restore(&token))
}

#[tauri::command]
fn create_project(state: State<'_, AppState>, input: NewProject) -> Res<ProjectRow> {
    with(&state, |s| s.create_project(input))
}

#[tauri::command]
fn update_project(state: State<'_, AppState>, id: String, patch: ProjectPatch) -> Res<ProjectRow> {
    with(&state, |s| s.update_project(&id, patch))
}

/// Deleting a project asks one question — delete its tasks, or move them to
/// Inbox — and never guesses (§4.6). The answer arrives as this flag.
#[tauri::command]
fn delete_project(
    state: State<'_, AppState>,
    id: String,
    move_tasks_to_inbox: bool,
) -> Res<UndoToken> {
    with(&state, |s| s.delete_project(&id, move_tasks_to_inbox))
}

#[tauri::command]
fn schedule_block(state: State<'_, AppState>, input: NewBlock) -> Res<BlockRow> {
    with(&state, |s| s.schedule_block(input))
}

#[tauri::command]
fn move_block(
    state: State<'_, AppState>,
    id: String,
    starts_at: i64,
    policy: CollisionPolicy,
) -> Res<Vec<BlockRow>> {
    with(&state, |s| s.move_block(&id, starts_at, policy))
}

#[tauri::command]
fn resize_block(state: State<'_, AppState>, id: String, duration_sec: i64) -> Res<BlockRow> {
    with(&state, |s| s.resize_block(&id, duration_sec))
}

#[tauri::command]
fn unschedule_block(state: State<'_, AppState>, id: String) -> Res<UndoToken> {
    with(&state, |s| s.unschedule_block(&id))
}

/// P2 (§2.3): a repeating series. `schedule_block` routes here when the input
/// carries a rule, so both paths share one validation.
#[tauri::command]
fn schedule_recurring(
    state: State<'_, AppState>,
    input: NewBlock,
    rrule: String,
) -> Res<Vec<BlockRow>> {
    with(&state, |s| s.schedule_recurring(input, &rrule))
}

#[tauri::command]
fn unschedule_series(
    state: State<'_, AppState>,
    id: String,
    scope: SeriesScope,
) -> Res<UndoToken> {
    with(&state, |s| s.unschedule_series(&id, scope))
}

/// Keeps series materialised as far as the planner is being asked to show.
#[tauri::command]
fn extend_series_to(state: State<'_, AppState>, through: String) -> Res<usize> {
    with(&state, |s| s.extend_series_to(&through))
}

/// Turns a block that already exists into the seed of a series, in place —
/// it keeps its task, its duration and anything tracked against it.
#[tauri::command]
fn repeat_block(state: State<'_, AppState>, id: String, rrule: String) -> Res<Vec<BlockRow>> {
    with(&state, |s| s.repeat_block(&id, &rrule))
}

#[tauri::command]
fn describe_rrule(rrule: String) -> Res<String> {
    Ok(fruit_core::rrule::Rrule::parse(&rrule)?.describe())
}

/// The repeat presets, described by the same code that parses them, so the
/// picker can never offer a rule the engine would refuse.
#[tauri::command]
fn get_rrule_presets() -> Res<Vec<fruit_core::rrule::RrulePreset>> {
    Ok(fruit_core::rrule::presets())
}

/// P2 (§2.3): read a local `.ics`. Read-only and offline — no URL, no
/// account, and Fruit never writes back to a calendar (§1.4).
#[tauri::command]
fn import_ics(state: State<'_, AppState>, path: PathBuf, tz: String) -> Res<IcsImportSummary> {
    with(&state, |s| s.import_ics(&path, &tz))
}

/// The OS file picker, so the renderer never has to ask anyone to type a path.
///
/// This is the *only* way a path enters the import: `fs:*` is not in the
/// capability file (§7.3), so the webview cannot read a file itself — it can
/// only hand a user-chosen path to a Rust command that knows what to do with it.
#[tauri::command]
fn pick_ics_file(app: AppHandle) -> Res<Option<PathBuf>> {
    use tauri_plugin_dialog::DialogExt;
    // Commands run off the main thread, so blocking here is safe and keeps the
    // renderer from having to model a dialog's lifetime.
    Ok(app
        .dialog()
        .file()
        .add_filter("Calendar", &["ics"])
        .blocking_pick_file()
        .and_then(|f| f.into_path().ok()))
}

// ─── activity (§3.5, P2) ───────────────────────────────────────────────

#[tauri::command]
fn get_activity_settings(state: State<'_, AppState>) -> Res<ActivityStatus> {
    let settings = with(&state, |s| s.activity_settings())?;
    Ok(ActivityStatus {
        support: frontmost::support(),
        support_note: frontmost::support().describe().to_string(),
        settings,
    })
}

#[tauri::command]
fn set_activity_setting(
    state: State<'_, AppState>,
    key: String,
    value: Value,
) -> Res<ActivityStatus> {
    with(&state, |s| s.set_activity_setting(&key, value))?;
    get_activity_settings(state)
}

#[tauri::command]
fn get_activity_day(state: State<'_, AppState>, date: String, tz: String) -> Res<ActivityDay> {
    with(&state, |s| s.get_activity_day(&date, &tz))
}

#[tauri::command]
fn clear_activity(state: State<'_, AppState>) -> Res<i64> {
    with(&state, |s| s.clear_activity())
}

// ─── the unified day and life time (Plan Rev 3 §7, §8.1) ───────────────

/// The Day view: 24 hours of one date, with the four record types resolved
/// into a single non-double-counted timeline.
#[tauri::command]
fn get_day(
    state: State<'_, AppState>,
    date: String,
    tz: String,
    slot_minutes: Option<i64>,
) -> Res<DayView> {
    with(&state, |s| s.get_day(&date, &tz, slot_minutes))
}

/// The month dashboard (wireframe screen 3). Month is the plan's default
/// reporting horizon, so this is what Reports opens to.
#[tauri::command]
fn get_month(state: State<'_, AppState>, month: String, tz: String) -> Res<MonthView> {
    with(&state, |s| s.get_month(&month, &tz))
}

#[tauri::command]
fn get_life_areas(
    state: State<'_, AppState>,
    tz: String,
    include_archived: Option<bool>,
) -> Res<Vec<LifeAreaRow>> {
    with(&state, |s| {
        s.get_life_areas(&tz, include_archived.unwrap_or(false))
    })
}

#[tauri::command]
fn create_life_area(state: State<'_, AppState>, input: NewLifeArea) -> Res<LifeAreaRow> {
    with(&state, |s| s.create_life_area(input))
}

#[tauri::command]
fn update_life_area(
    state: State<'_, AppState>,
    id: String,
    patch: LifeAreaPatch,
) -> Res<LifeAreaRow> {
    with(&state, |s| s.update_life_area(&id, patch))
}

#[tauri::command]
fn delete_life_area(state: State<'_, AppState>, id: String) -> Res<UndoToken> {
    with(&state, |s| s.delete_life_area(&id))
}

#[tauri::command]
fn get_life_entries(
    state: State<'_, AppState>,
    date: String,
    tz: String,
) -> Res<Vec<LifeEntryRow>> {
    with(&state, |s| s.life_entries_on(&date, &tz))
}

#[tauri::command]
fn add_life_entry(state: State<'_, AppState>, input: NewLifeEntry) -> Res<LifeEntryRow> {
    with(&state, |s| s.add_life_entry(input))
}

#[tauri::command]
fn update_life_entry(
    state: State<'_, AppState>,
    id: String,
    patch: LifeEntryPatch,
) -> Res<LifeEntryRow> {
    with(&state, |s| s.update_life_entry(&id, patch))
}

#[tauri::command]
fn delete_life_entry(state: State<'_, AppState>, id: String) -> Res<UndoToken> {
    with(&state, |s| s.delete_life_entry(&id))
}

/// Work records only — there is deliberately no life-entry equivalent.
#[tauri::command]
fn set_session_contribution(
    state: State<'_, AppState>,
    id: String,
    contribution: Option<Contribution>,
) -> Res<SessionRow> {
    with(&state, |s| s.set_session_contribution(&id, contribution))
}

/// Reclassifies confirmed work as confirmed life time. The contribution mode
/// does not travel, because the destination has nowhere to put one.
#[tauri::command]
fn convert_session_to_life(
    state: State<'_, AppState>,
    id: String,
    life_area_id: String,
    tz: String,
) -> Res<LifeEntryRow> {
    with(&state, |s| s.convert_session_to_life(&id, &life_area_id, &tz))
}

#[tauri::command]
fn duplicate_block(state: State<'_, AppState>, id: String) -> Res<BlockRow> {
    with(&state, |s| s.duplicate_block(&id))
}

#[tauri::command]
fn set_block_fixed(state: State<'_, AppState>, id: String, is_fixed: bool) -> Res<BlockRow> {
    with(&state, |s| s.set_block_fixed(&id, is_fixed))
}

#[tauri::command]
fn start_timer(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
    block_id: Option<String>,
) -> Res<TimerState> {
    let next = with(&state, |s| s.start_timer(&task_id, block_id.as_deref()))?;
    let _ = app.emit("timer:state", &next);
    update_tray(&app, &next);
    Ok(next)
}

#[tauri::command]
fn stop_timer(app: AppHandle, state: State<'_, AppState>) -> Res<TimerState> {
    let next = with(&state, |s| s.stop_timer())?;
    let _ = app.emit("timer:state", &next);
    update_tray(&app, &next);
    Ok(next)
}

#[tauri::command]
fn resolve_idle(app: AppHandle, state: State<'_, AppState>, action: IdleAction) -> Res<TimerState> {
    let next = with(&state, |s| s.resolve_idle(action))?;
    let _ = app.emit("timer:state", &next);
    Ok(next)
}

#[tauri::command]
fn resolve_recovery(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    a: RecoveryAction,
) -> Res<TimerState> {
    let next = with(&state, |s| s.resolve_recovery(&id, a))?;
    let _ = app.emit("timer:state", &next);
    Ok(next)
}

#[tauri::command]
fn add_session(state: State<'_, AppState>, input: ManualSession) -> Res<SessionRow> {
    with(&state, |s| s.add_session(input))
}

#[tauri::command]
fn update_session(state: State<'_, AppState>, id: String, p: SessionPatch) -> Res<SessionRow> {
    with(&state, |s| s.update_session(&id, p))
}

#[tauri::command]
fn delete_session(state: State<'_, AppState>, id: String) -> Res<UndoToken> {
    with(&state, |s| s.delete_session(&id))
}

#[tauri::command]
fn save_note(state: State<'_, AppState>, task_id: String, markdown: String) -> Res<()> {
    with(&state, |s| s.save_note(&task_id, &markdown))
}

#[tauri::command]
fn apply_reconcile(
    state: State<'_, AppState>,
    date: String,
    actions: Vec<ReconcileAction>,
    tz: String,
) -> Res<DayReview> {
    with(&state, |s| s.apply_reconcile(&date, actions, &tz))
}

#[tauri::command]
fn set_setting(state: State<'_, AppState>, key: String, value: Value) -> Res<()> {
    with(&state, |s| s.set_setting(&key, &value))
}

#[tauri::command]
fn export_data(
    app: AppHandle,
    state: State<'_, AppState>,
    format: ExportFormat,
    path: PathBuf,
    tz: String,
) -> Res<ExportSummary> {
    // A bare filename would land in the process's working directory — which on
    // Windows is wherever the exe was launched from, and nowhere a person would
    // look. Export is a trust feature (§6.12); "where did it go?" defeats it.
    let path = if path.is_absolute() {
        path
    } else {
        app.path()
            .download_dir()
            .or_else(|_| app.path().document_dir())
            .or_else(|_| app.path().home_dir())
            .unwrap_or_else(|_| data_dir(&app))
            .join(&path)
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    with(&state, |s| s.export_to(format, &path, &tz))
}

#[tauri::command]
fn import_data(
    state: State<'_, AppState>,
    path: PathBuf,
    mode: ImportMode,
) -> Res<ImportSummary> {
    with(&state, |s| s.import_file(&path, mode))
}

#[tauri::command]
fn run_integrity_check(state: State<'_, AppState>) -> Res<IntegrityReport> {
    with(&state, |s| s.run_integrity_check())
}

#[tauri::command]
fn rebuild_caches(state: State<'_, AppState>) -> Res<()> {
    with(&state, |s| {
        s.run_integrity_check()?;
        Ok(())
    })
}

/// Fallback input signal for platforms where the OS won't report idle time.
#[tauri::command]
fn report_input(state: State<'_, AppState>) {
    if let Ok(mut last) = state.last_window_input_ms.lock() {
        *last = fruit_core::time::now_ms();
    }
}

/* ─── setup ────────────────────────────────────────────────────────────── */

fn locale_date_order() -> DateOrder {
    // §4.4 rule 3: `d/m` vs `m/d` follows the OS locale. ISO is unambiguous and
    // preferred in docs precisely because this guess is a guess.
    let locale = std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .unwrap_or_default()
        .to_lowercase();
    if locale.starts_with("en_us") || locale.starts_with("en-us") {
        DateOrder::MonthDay
    } else {
        DateOrder::DayMonth
    }
}

fn data_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// §5.10 — the tray communicates state through the mark itself, not a badge:
/// idle (both strokes hairline), running (solid stroke `track`), overrun
/// (`over`). Here we can at least keep the tooltip honest.
fn update_tray(app: &AppHandle, timer: &TimerState) {
    let title = match timer.phase {
        TimerPhase::Running => timer
            .session
            .as_ref()
            .map(|s| format!("{} · {}", s.task_title, fmt_elapsed(timer.elapsed_sec)))
            .unwrap_or_else(|| "Fruit".into()),
        TimerPhase::IdleChallenge => "Fruit · idle, awaiting a decision".into(),
        TimerPhase::Recovering => "Fruit · unresolved session".into(),
        _ => "Fruit".into(),
    };
    if let Some(tray) = app.tray_by_id("fruit") {
        let _ = tray.set_tooltip(Some(&title));
    }
}

fn fmt_elapsed(sec: i64) -> String {
    format!("{}:{:02}", sec / 3600, (sec % 3600) / 60)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Two processes on one SQLite file is both a corruption path and a
        // duplicate-timer bug (§7.3). A second launch focuses the first window.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                // 5 files × 2MB, info in release (§7.2). Task titles and note
                // contents never enter the log — ids only.
                .max_file_size(2 * 1024 * 1024)
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            let db_path = data_dir(&handle).join("fruit.db");

            let mut store = match Store::open(&db_path) {
                Ok(store) => store,
                Err(err) => {
                    // §3.10: what failed · why · the action. Never a blank app.
                    log::error!("db.open.failed code={}", err.code());
                    let _ = handle.emit("db:integrity-failed", err.to_string());
                    return Err(Box::new(err) as Box<dyn std::error::Error>);
                }
            };

            let tz = chrono::Local::now().offset().to_string();
            let tz = store.tz_or(&system_tz().unwrap_or(tz));

            // First run seeds a project containing one already-drifted block
            // (§3.9, U8), so the signature rail is on screen immediately.
            if let Err(err) = store.seed_first_run(&tz) {
                log::warn!("seed.failed code={}", err.code());
            }
            match store.rotate_backups() {
                Ok(Some(path)) => log::info!("backup.snapshot.written path_len={}", path.as_os_str().len()),
                Ok(None) => {}
                Err(err) => {
                    log::error!("backup.snapshot.failed code={}", err.code());
                    let _ = handle.emit("backup:failed", err.to_string());
                }
            }
            let _ = store.purge_expired();
            match store.purge_activity() {
                Ok(n) if n > 0 => log::info!("activity.purge.removed n={n}"),
                _ => {}
            }
            let _ = store.auto_accept_stale_days(&tz);

            // Boot found an open session? No timer may start until it is
            // resolved (§4.5 `recovering`).
            match store.recover_on_boot() {
                Ok(state) if state.phase == TimerPhase::Recovering => {
                    log::info!("timer.recovery.required");
                    let _ = handle.emit("timer:recovery-required", &state);
                }
                Ok(_) => {}
                Err(err) => log::error!("timer.recovery.failed code={}", err.code()),
            }

            app.manage(AppState {
                store: Mutex::new(store),
                last_window_input_ms: Mutex::new(fruit_core::time::now_ms()),
            });

            spawn_timer_loop(handle.clone());
            spawn_activity_loop(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_week,
            get_backlog,
            get_tasks,
            get_task_detail,
            get_projects,
            get_tags,
            get_reports,
            get_reconcile_items,
            get_day,
            get_month,
            get_life_areas,
            create_life_area,
            update_life_area,
            delete_life_area,
            get_life_entries,
            add_life_entry,
            update_life_entry,
            delete_life_entry,
            set_session_contribution,
            convert_session_to_life,
            schedule_recurring,
            unschedule_series,
            extend_series_to,
            repeat_block,
            describe_rrule,
            get_rrule_presets,
            import_ics,
            pick_ics_file,
            get_activity_settings,
            set_activity_setting,
            get_activity_day,
            clear_activity,
            get_unreconciled_days,
            search,
            parse_capture,
            get_settings,
            get_deleted,
            get_timer_state,
            create_task,
            update_task,
            set_task_status,
            delete_task,
            restore,
            create_project,
            update_project,
            delete_project,
            schedule_block,
            move_block,
            resize_block,
            unschedule_block,
            duplicate_block,
            set_block_fixed,
            start_timer,
            stop_timer,
            resolve_idle,
            resolve_recovery,
            add_session,
            update_session,
            delete_session,
            save_note,
            apply_reconcile,
            set_setting,
            export_data,
            import_data,
            run_integrity_check,
            rebuild_caches,
            report_input,
        ])
        .on_window_event(|window, event| {
            // §6.7 crash flush: close the running session and checkpoint WAL
            // before the process goes away.
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if let Some(state) = window.try_state::<AppState>() {
                    if let Ok(mut store) = state.store.lock() {
                        let _ = store.stop_timer();
                        let _ = fruit_core::db::checkpoint(store.connection());
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Fruit");
}

/// What Settings needs to render the Activity section: the switches, and what
/// this platform is actually capable of.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivityStatus {
    support: frontmost::Support,
    support_note: String,
    settings: fruit_core::model::ActivitySettings,
}

/// Samples the frontmost application (§3.5, P2).
///
/// Opt-in, off by default, and it exits early on every tick when disabled — so
/// a user who never turns it on pays one settings read every 20 seconds and
/// nothing else. The filtering (exclusions, titles, pause) lives in
/// `fruit-core`, so a bug here cannot record something the user excluded.
fn spawn_activity_loop(app: AppHandle) {
    if !frontmost::support().available() {
        log::info!("activity.unsupported platform={}", std::env::consts::OS);
        return;
    }
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(SAMPLE_INTERVAL_MS as u64));
        let Some(state) = app.try_state::<AppState>() else {
            continue;
        };
        let Ok(mut store) = state.store.lock() else {
            continue;
        };
        let enabled = matches!(
            store.get_setting(ACTIVITY_ENABLED),
            Ok(Some(Value::Bool(true)))
        );
        let paused = matches!(
            store.get_setting(ACTIVITY_PAUSED),
            Ok(Some(Value::Bool(true)))
        );
        if !enabled || paused {
            continue;
        }
        let Some(front) = frontmost::current() else {
            continue;
        };
        let sample = fruit_core::model::ActivitySample {
            app_id: front.app_id,
            window_title: front.window_title,
            at: fruit_core::time::now_ms(),
        };
        match store.record_activity(sample) {
            // The recording indicator in the top bar is driven by this event —
            // §3.5 requires it to be visible whenever sampling is live.
            Ok(true) => {
                drop(store);
                let _ = app.emit("activity:sampled", ());
            }
            Ok(false) => {}
            Err(err) => log::error!("activity.record.failed code={}", err.code()),
        }
    });
}

fn system_tz() -> Option<String> {
    std::env::var("TZ").ok().filter(|s| !s.is_empty())
}

/// One second is the only interval in the app, it runs **only while a timer is
/// running**, and it lives in Rust: the renderer never owns elapsed time
/// (§6.9). Idle CPU with no timer stays under the §7.1 budget because this
/// loop sleeps a full second and does nothing when the phase is `idle`.
fn spawn_timer_loop(app: AppHandle) {
    std::thread::spawn(move || {
        let _clock = Arc::new(SystemClock::default());
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let Some(state) = app.try_state::<AppState>() else {
                continue;
            };

            let window_input = state
                .last_window_input_ms
                .lock()
                .map(|v| *v)
                .unwrap_or_else(|_| fruit_core::time::now_ms());
            let last_input_at = match idle::os_idle_seconds() {
                Some(secs) => fruit_core::time::now_ms() - (secs as i64) * 1000,
                None => window_input,
            };

            let Ok(mut store) = state.store.lock() else {
                continue;
            };
            let before = store.timer_state().map(|s| s.phase).unwrap_or(TimerPhase::Idle);
            if before != TimerPhase::Running {
                continue;
            }
            let next = match store.tick(Some(IdleReport { last_input_at })) {
                Ok(next) => next,
                Err(err) => {
                    log::error!("timer.tick.failed code={}", err.code());
                    continue;
                }
            };
            drop(store);

            if next.phase == TimerPhase::IdleChallenge {
                log::info!("timer.idle.detected");
                let _ = app.emit("timer:idle-detected", &next);
            } else {
                let _ = app.emit("timer:tick", &next);
            }
            update_tray(&app, &next);
        }
    });
}
