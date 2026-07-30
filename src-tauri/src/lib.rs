//! The Tauri shell.
//!
//! This crate owns windows, the tray, the clock loop and the IPC boundary —
//! and nothing else. Every rule lives in `fruit-core`, which is why that crate
//! has no Tauri dependency and this one has almost no logic (§6.8).
//!
//! Commands are intent-based, one transaction each. The renderer holds no SQL
//! strings, and the capability file lists exactly the commands below.

mod idle;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use fruit_core::clock::SystemClock;
use fruit_core::db::IntegrityReport;
use fruit_core::model::*;
use fruit_core::parser::{parse, DateOrder, ParseCtx};
use fruit_core::store::IdleReport;
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
    state: State<'_, AppState>,
    format: ExportFormat,
    path: PathBuf,
    tz: String,
) -> Res<ExportSummary> {
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

            spawn_timer_loop(handle);
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
