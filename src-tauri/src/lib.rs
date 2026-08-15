//! The Tauri shell.
//!
//! This crate owns windows, the tray, the clock loop and the IPC boundary —
//! and nothing else. Every rule lives in `fruit-core`, which is why that crate
//! has no Tauri dependency and this one has almost no logic (§6.8).
//!
//! Commands are intent-based, one transaction each. The renderer holds no SQL
//! strings, and the capability file lists exactly the commands below.

pub use fruit_connector_host as connector;
mod frontmost;
mod idle;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use fruit_core::clock::SystemClock;
use fruit_core::db::IntegrityReport;
use fruit_core::model::*;
use fruit_core::parser::{parse, DateOrder, ParseCtx};
use fruit_core::wallpaper::WallpaperFolder;
use fruit_core::xlsx::WorkbookInspection;
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
///
/// Both kinds: blocks and repeating life entries share a horizon, because the
/// week the planner is drawing is the same week the Day view will be asked
/// about, and a sleep entry missing from it would read as an unaccounted night.
#[tauri::command]
fn extend_series_to(state: State<'_, AppState>, through: String) -> Res<usize> {
    with(&state, |s| {
        Ok(s.extend_series_to(&through)? + s.extend_life_series_to(&through)?)
    })
}

/// Two adjacent records of the same thing becoming one.
///
/// The result says how many seconds it absorbed between them: a merge asserts
/// that the gap was part of the same thing, and one that annexed time silently
/// would be indistinguishable from a bug.
#[tauri::command]
fn merge_life_entries(state: State<'_, AppState>, ids: Vec<String>) -> Res<MergeResult> {
    with(&state, |s| s.merge_life_entries(&ids))
}

#[tauri::command]
fn merge_sessions(state: State<'_, AppState>, ids: Vec<String>) -> Res<MergeResult> {
    with(&state, |s| s.merge_sessions(&ids))
}

/// One record becoming two, at a moment inside it — merge's inverse, and the
/// verb the wireframe asks for on the Day view's detail panel.
#[tauri::command]
fn split_life_entry(
    state: State<'_, AppState>,
    id: String,
    at: i64,
) -> Res<(LifeEntryRow, LifeEntryRow)> {
    with(&state, |s| s.split_life_entry(&id, at))
}

#[tauri::command]
fn split_session(state: State<'_, AppState>, id: String, at: i64) -> Res<(SessionRow, SessionRow)> {
    with(&state, |s| s.split_session(&id, at))
}

/// Makes an existing life entry repeat. Sleep is the case this exists for.
#[tauri::command]
fn repeat_life_entry(
    state: State<'_, AppState>,
    id: String,
    rrule: String,
) -> Res<Vec<LifeEntryRow>> {
    with(&state, |s| s.repeat_life_entry(&id, &rrule))
}

/// Removing part or all of a repeating entry. The scope is explicit for the
/// same reason it is on a block: "just tonight" and "three months of sleep" are
/// different enough that inferring which was meant is a data-loss bug with a
/// friendly name.
#[tauri::command]
fn delete_life_series(
    state: State<'_, AppState>,
    id: String,
    scope: SeriesScope,
) -> Res<UndoToken> {
    with(&state, |s| s.delete_life_series(&id, scope))
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

// ─── Focus wallpapers ──────────────────────────────────────────────────
//
// Three thin commands over `fruit_core::wallpaper`, which holds the rules —
// including the containment check that stops `read_wallpaper` from becoming an
// arbitrary-file-read primitive in a webview. The renderer never sees or sends
// a path: it names the folder it was given and a file inside it.

/// The folder Focus draws from. Created on first ask, with a README beside it.
///
/// Defaults to `<app data>/wallpapers` so the feature works without a setting,
/// and honours `focus.wallpaperDir` when the user has pointed it elsewhere.
fn wallpaper_dir(app: &AppHandle, state: &State<'_, AppState>) -> PathBuf {
    let configured = with(state, |s| s.get_setting("focus.wallpaperDir"))
        .ok()
        .flatten()
        .and_then(|v| v.as_str().map(PathBuf::from))
        .filter(|p| !p.as_os_str().is_empty());
    match configured {
        Some(dir) => dir,
        // Only the default folder is created for you. A configured path that
        // has gone missing — an unplugged drive, a renamed folder — is reported
        // as empty rather than silently recreated somewhere the user did not
        // choose.
        None => fruit_core::wallpaper::ensure_dir(&data_dir(app))
            .unwrap_or_else(|_| data_dir(app).join("wallpapers")),
    }
}

#[tauri::command]
fn get_wallpapers(app: AppHandle, state: State<'_, AppState>) -> Res<WallpaperFolder> {
    let dir = wallpaper_dir(&app, &state);
    Ok(WallpaperFolder {
        items: fruit_core::wallpaper::list(&dir)?,
        dir: dir.display().to_string(),
    })
}

#[tauri::command]
fn read_wallpaper(app: AppHandle, state: State<'_, AppState>, name: String) -> Res<String> {
    let dir = wallpaper_dir(&app, &state);
    Ok(fruit_core::wallpaper::read(&dir, &name)?)
}

/// Let the user point Focus at a different folder. Returns the chosen path, or
/// `None` if they cancelled — which is not a failure.
#[tauri::command]
fn pick_wallpaper_dir(app: AppHandle) -> Res<Option<PathBuf>> {
    use tauri_plugin_dialog::DialogExt;
    Ok(app
        .dialog()
        .file()
        .blocking_pick_folder()
        .and_then(|f| f.into_path().ok()))
}

/// Open the wallpaper folder in the system file manager. The whole feature is
/// "put your own pictures here", so getting *there* has to be one click.
#[tauri::command]
fn reveal_wallpaper_dir(app: AppHandle, state: State<'_, AppState>) -> Res<()> {
    use tauri_plugin_opener::OpenerExt;
    let dir = wallpaper_dir(&app, &state);
    let _ = std::fs::create_dir_all(&dir);
    app.opener()
        .reveal_item_in_dir(&dir)
        .map_err(|e| AppError::invalid(format!("Couldn't open that folder ({e}).")))?;
    Ok(())
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
    app: AppHandle,
    state: State<'_, AppState>,
    key: String,
    value: Value,
) -> Res<ActivityStatus> {
    with(&state, |s| s.set_activity_setting(&key, value))?;
    // The sentinel the connector host reads has to move with the switch, not on
    // the next twenty-second tick: "I turned it off" must mean nothing further
    // is written, immediately.
    let dir = data_dir(&app);
    with(&state, |s| s.drain_browser_spool(&dir))?;
    get_activity_settings(state)
}

/* ─── browser connector (Plan Rev 3 §5.4) ──────────────────────────────── */

/* ─── weekly goals (migration 0008) ────────────────────────────────────── */

#[tauri::command]
fn get_week_review(state: State<'_, AppState>, date: String, tz: String) -> Res<WeekReview> {
    with(&state, |s| s.get_week_review(&date, &tz))
}

#[tauri::command]
fn get_goals(state: State<'_, AppState>, include_ended: bool) -> Res<Vec<GoalRow>> {
    with(&state, |s| s.get_goals(include_ended))
}

/// Creates a goal, replacing any live one for the same subject. The old one is
/// closed rather than deleted, so a review of the week it governed still shows
/// the number that was in force.
#[tauri::command]
fn set_goal(state: State<'_, AppState>, input: NewGoal, today: String) -> Res<GoalRow> {
    with(&state, |s| s.set_goal(input, &today))
}

#[tauri::command]
fn end_goal(state: State<'_, AppState>, id: String, today: String) -> Res<()> {
    with(&state, |s| s.end_goal(&id, &today))
}

/// Goals Fruit is prepared to suggest, each with the number it picked and where
/// it came from. A template with no history says so rather than guessing.
#[tauri::command]
fn get_goal_templates(
    state: State<'_, AppState>,
    today: String,
    tz: String,
) -> Res<Vec<GoalTemplate>> {
    with(&state, |s| s.get_goal_templates(&today, &tz))
}

/* ─── focus sessions (W3) ──────────────────────────────────────────────── */

/// Starts a run with an intended length, plotting a block for it. `minutes` of
/// `None` is an ordinary open-ended timer.
#[tauri::command]
fn start_focus(
    state: State<'_, AppState>,
    task_id: String,
    minutes: Option<i64>,
) -> Res<TimerState> {
    with(&state, |s| s.start_focus(&task_id, minutes))
}

/// "Keep going." Moves when Fruit next mentions the session and nothing else —
/// the block keeps its length, so the overrun stays visible.
#[tauri::command]
fn extend_focus(state: State<'_, AppState>, minutes: i64) -> Res<TimerState> {
    with(&state, |s| s.extend_focus(minutes))
}

/* ─── notices (W4/W5) ──────────────────────────────────────────────────── */

/// Silences every notice for a while — the "don't tell me again" the off-plan
/// nudge needs in order not to become something people learn to dismiss.
#[tauri::command]
fn silence_notices(state: State<'_, AppState>, minutes: i64) -> Res<()> {
    with(&state, |s| s.silence_notices(minutes))
}

/* ─── labelling observed time (migration 0007) ─────────────────────────── */

#[tauri::command]
fn get_categories(
    state: State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
    tz: String,
) -> Res<Vec<ObservationCategory>> {
    with(&state, |s| {
        let range = from.as_deref().zip(to.as_deref());
        s.get_categories(range, &tz)
    })
}

#[tauri::command]
fn create_category(
    state: State<'_, AppState>,
    name: String,
    colour: Option<String>,
    counts_as: DomainCategory,
) -> Res<ObservationCategory> {
    with(&state, |s| {
        s.create_category(&name, colour.as_deref(), counts_as)
    })
}

#[tauri::command]
fn update_category(
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    colour: Option<String>,
) -> Res<ObservationCategory> {
    with(&state, |s| {
        s.update_category(&id, name.as_deref(), colour.as_deref())
    })
}

#[tauri::command]
fn delete_category(state: State<'_, AppState>, id: String) -> Res<()> {
    with(&state, |s| s.delete_category(&id))
}

#[tauri::command]
fn get_activity_rules(state: State<'_, AppState>) -> Res<Vec<ActivityRuleRow>> {
    with(&state, |s| s.get_activity_rules())
}

#[tauri::command]
fn set_activity_rule(
    state: State<'_, AppState>,
    match_kind: MatchKind,
    match_value: String,
    category_id: String,
) -> Res<ActivityRuleRow> {
    with(&state, |s| {
        s.set_activity_rule(match_kind, &match_value, &category_id)
    })
}

#[tauri::command]
fn delete_activity_rule(state: State<'_, AppState>, id: String) -> Res<()> {
    with(&state, |s| s.delete_activity_rule(&id))
}

/// Relabels one observed interval without touching any rule — the YouTube case:
/// Distraction by default, and *this* video was a lecture.
#[tauri::command]
fn set_span_category(
    state: State<'_, AppState>,
    span_id: i64,
    category_id: Option<String>,
) -> Res<()> {
    with(&state, |s| {
        s.set_span_category(span_id, category_id.as_deref())
    })
}

#[tauri::command]
fn get_unlabelled(
    state: State<'_, AppState>,
    from: String,
    to: String,
    tz: String,
    limit: u32,
) -> Res<Vec<UnlabelledRow>> {
    with(&state, |s| s.get_unlabelled(&from, &to, &tz, limit))
}

#[tauri::command]
fn get_domain_totals(
    state: State<'_, AppState>,
    date: String,
    tz: String,
) -> Res<Vec<fruit_core::store::DomainTotal>> {
    with(&state, |s| s.domain_totals(&date, &tz))
}

/// What Settings needs to explain the connector: whether it is on, and the two
/// paths a person has to put a file at to install it.
///
/// Installation is not automatic and the screen says so. Writing a native-host
/// manifest means writing a registry key under `HKCU`, and an app that does that
/// silently on first run has installed a browser extension hook without asking —
/// which is precisely the behaviour the privacy contract exists to rule out.
#[tauri::command]
fn get_connector_status(app: AppHandle, state: State<'_, AppState>) -> Res<ConnectorStatus> {
    let dir = data_dir(&app);
    // The extension ships as a bundled resource, so the path shown is one that
    // actually exists on the installed machine. Printing a plausible-looking
    // path that is not there is worse than printing nothing.
    let extension_dir = app
        .path()
        .resolve("connector", tauri::path::BaseDirectory::Resource)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "(not found — reinstall Fruit)".into());
    Ok(ConnectorStatus {
        enabled: with(&state, |s| Ok(s.domains_enabled()))?,
        host_manifest_name: connector::HOST_MANIFEST_NAME.into(),
        extension_dir,
        exe_path: std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        queued_samples: fruit_core::spool::spool_path(&dir)
            .metadata()
            .map(|m| m.len())
            .unwrap_or(0),
    })
}

/// Registers the native-messaging host, on the user's explicit request.
///
/// Not on first run, and not silently — see `connector::install`. By the time
/// this is called the user has loaded the extension, read what registration
/// does, and pasted the id off `chrome://extensions`.
///
/// Every step it took is reported back, including the exact `reg.exe` command,
/// so someone whose machine refuses it has something to run by hand rather than
/// a failure with no next move.
#[tauri::command]
fn install_connector(app: AppHandle, extension_id: String) -> Res<Vec<String>> {
    let extension_id =
        connector::install::validate_extension_id(&extension_id).map_err(AppError::invalid)?;
    let exe = std::env::current_exe()
        .map_err(|e| AppError::invalid(format!("Couldn't find Fruit's own path: {e}")))?;
    let dir = data_dir(&app);

    let manifest = connector::install::write_manifest(&dir, &exe, &extension_id)
        .map_err(|e| AppError::invalid(format!("Couldn't write the host manifest: {e}")))?;
    let mut steps = vec![format!("Wrote {}", manifest.display())];

    if cfg!(windows) {
        for key in connector::install::registry_keys() {
            let args = connector::install::registry_args(&key, &manifest);
            match std::process::Command::new("reg.exe").args(&args).output() {
                Ok(out) if out.status.success() => steps.push(format!("Registered {key}")),
                Ok(out) => steps.push(format!(
                    "Could not register {key} — run: reg.exe {} ({})",
                    args.join(" "),
                    String::from_utf8_lossy(&out.stderr).trim()
                )),
                Err(err) => steps.push(format!("Could not run reg.exe ({err}). Run: reg.exe {}", args.join(" "))),
            }
        }
    } else {
        // No registry: on macOS and Linux the manifest's *location* is the
        // registration, so it is copied into each browser's directory.
        for target in connector::install::manifest_install_dirs() {
            let dest = target.join(format!("{}.json", connector::HOST_MANIFEST_NAME));
            match std::fs::create_dir_all(&target).and_then(|_| std::fs::copy(&manifest, &dest)) {
                Ok(_) => steps.push(format!("Installed {}", dest.display())),
                Err(err) => steps.push(format!("Skipped {} ({err})", dest.display())),
            }
        }
    }
    steps.push("Restart the browser, then open a tab — samples arrive within a minute.".into());
    Ok(steps)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectorStatus {
    enabled: bool,
    host_manifest_name: String,
    extension_dir: String,
    exe_path: String,
    /// Bytes waiting to be drained. Non-zero here with `enabled` false is the
    /// signature of a host still running against a switch that has gone off.
    queued_samples: u64,
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

/// The month table exactly as the workbook will contain it (wireframe screen 5).
/// The preview and the file render from one matrix, so the screen cannot
/// promise a layout the file doesn't have.
#[tauri::command]
fn preview_excel(
    state: State<'_, AppState>,
    month: String,
    tz: String,
    options: ExcelOptions,
) -> Res<ExcelPreview> {
    with(&state, |s| s.preview_excel(&month, &tz, &options))
}

#[tauri::command]
fn export_excel(
    app: AppHandle,
    state: State<'_, AppState>,
    month: String,
    tz: String,
    path: PathBuf,
    options: ExcelOptions,
) -> Res<ExcelExportResult> {
    let path = downloads_path(&app, path);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    with(&state, |s| s.write_excel(&month, &tz, &path, &options))
}

/// The Monday-morning report (W9). `None` means there is nothing to say about
/// last week, which is a legitimate answer.
#[tauri::command]
fn due_week_report(state: State<'_, AppState>, tz: String) -> Res<Option<WeekReportDue>> {
    with(&state, |s| s.due_week_report(&tz))
}

#[tauri::command]
fn get_week_report(state: State<'_, AppState>, date: String, tz: String) -> Res<WeekReport> {
    with(&state, |s| s.week_report(&date, &tz))
}

/// Writes the week's report and remembers where it went, so the card can offer
/// to open the file next time rather than writing a second copy.
#[tauri::command]
fn export_week_report(
    app: AppHandle,
    state: State<'_, AppState>,
    date: String,
    tz: String,
    path: Option<PathBuf>,
) -> Res<WeekReportResult> {
    let path = match path {
        Some(p) => downloads_path(&app, p),
        None => {
            let week = with(&state, |s| {
                Ok(s.week_report(&date, &tz)?.review.week)
            })?;
            downloads_path(&app, PathBuf::from(format!("fruit-{week}.xlsx")))
        }
    };
    with(&state, |s| s.write_week_report(&date, &tz, &path))
}

/// The card has been read. It stops appearing for this week and every week
/// before it.
#[tauri::command]
fn mark_week_report_seen(state: State<'_, AppState>, week: String) -> Res<()> {
    with(&state, |s| s.mark_week_report_seen(&week))
}

// ─── workbook import (M13, §4.8) ───────────────────────────────────────

/// Read-only. Says what the file looks like so the mapping step is a
/// confirmation instead of twenty questions.
#[tauri::command]
fn inspect_workbook(state: State<'_, AppState>, path: PathBuf) -> Res<WorkbookInspection> {
    with(&state, |s| s.inspect_workbook(&path))
}

/// Takes the path rather than the inspection: shipping a whole inspection back
/// across the boundary to have it read once is a round trip that buys nothing,
/// and re-reading a file the user just picked is cheap.
#[tauri::command]
fn suggest_import_mapping(
    state: State<'_, AppState>,
    path: PathBuf,
    sheet: String,
    tz: String,
) -> Res<ImportMapping> {
    with(&state, |s| {
        let inspection = s.inspect_workbook(&path)?;
        s.suggest_import_mapping(&inspection, &sheet, &tz)
    })
}

/// The variance preview. Reads the file; writes nothing.
#[tauri::command]
fn preview_import(
    state: State<'_, AppState>,
    path: PathBuf,
    mapping: ImportMapping,
) -> Res<ImportPreview> {
    with(&state, |s| s.preview_import(&path, &mapping))
}

/// Refuses while the preview is blocked — the check lives in the core, not
/// here, because the renderer is not a trusted caller (§6.8).
#[tauri::command]
fn commit_import(
    state: State<'_, AppState>,
    path: PathBuf,
    mapping: ImportMapping,
) -> Res<ImportResult> {
    with(&state, |s| s.commit_import(&path, &mapping))
}

#[tauri::command]
fn get_import_batches(state: State<'_, AppState>) -> Res<Vec<ImportBatch>> {
    with(&state, |s| s.get_import_batches())
}

#[tauri::command]
fn undo_import(state: State<'_, AppState>, batch_id: String) -> Res<UndoToken> {
    with(&state, |s| s.undo_import(&batch_id))
}

/// Shows a file in the OS file manager.
///
/// Reveal rather than open: the report is a spreadsheet, and launching whatever
/// the machine has associated with `.xlsx` is a bigger assumption than putting
/// the folder in front of someone. It is also the only permission the app asks
/// for here — `opener:allow-reveal-item-in-dir`, and nothing that opens URLs.
#[tauri::command]
fn reveal_path(app: AppHandle, path: String) -> Res<()> {
    use tauri_plugin_opener::OpenerExt;
    if !std::path::Path::new(&path).exists() {
        return Err(AppError::invalid(
            "That file isn't there any more. Write the report again.",
        ));
    }
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| AppError::invalid(format!("Couldn't show that file: {e}")))?;
    Ok(())
}

/// Where the workbook will land, shown before anyone commits to writing it.
#[tauri::command]
fn suggest_excel_path(app: AppHandle, file_name: String) -> Res<String> {
    Ok(downloads_path(&app, PathBuf::from(file_name))
        .display()
        .to_string())
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

/// Reclassify a plotted interval — work, an entertainment window, or life.
#[tauri::command]
fn set_block_intent(state: State<'_, AppState>, id: String, intent: BlockIntent) -> Res<BlockRow> {
    with(&state, |s| s.set_block_intent(&id, intent))
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
fn save_note(state: State<'_, AppState>, task_id: String, body: String) -> Res<()> {
    with(&state, |s| s.save_note(&task_id, &body))
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
    let path = downloads_path(&app, path);
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

/// Empty the database of everything the user recorded, for testing.
///
/// Deliberately not guarded behind a debug build: the person who needs it is
/// running a release binary against their own data, and a switch that only
/// exists in a build they never run is a switch that does not exist. The guard
/// is the confirmation in the UI and the snapshot the core takes first.
#[tauri::command]
fn reset_all_data(state: State<'_, AppState>) -> Res<ResetSummary> {
    with(&state, |s| s.reset_all_data())
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

            let tz = store.tz_or(&system_tz());

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
            // The host process resolves this directory itself, without Tauri.
            // If the two ever disagree the host spools somewhere nothing drains
            // and the connector silently records nothing — so it is checked on
            // every launch rather than left to be discovered in the field.
            let data = data_dir(&handle);
            match connector::host_data_dir() {
                Some(host_dir) if host_dir == data => {}
                other => log::error!(
                    "connector.datadir.mismatch resolved={} host={}",
                    data.display(),
                    other.map(|p| p.display().to_string()).unwrap_or_else(|| "none".into())
                ),
            }
            // Publishes the "domains may be recorded" bit the host reads. Doing
            // it at boot means a crash cannot leave the host writing after the
            // user switched it off.
            if let Err(err) = store.drain_browser_spool(&data) {
                log::error!("connector.boot.failed code={}", err.code());
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
            preview_excel,
            export_excel,
            suggest_excel_path,
            due_week_report,
            get_week_report,
            export_week_report,
            mark_week_report_seen,
            reveal_path,
            inspect_workbook,
            suggest_import_mapping,
            preview_import,
            commit_import,
            get_import_batches,
            undo_import,
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
            repeat_life_entry,
            merge_life_entries,
            split_life_entry,
            split_session,
            merge_sessions,
            delete_life_series,
            repeat_block,
            describe_rrule,
            get_rrule_presets,
            import_ics,
            pick_ics_file,
            get_wallpapers,
            read_wallpaper,
            pick_wallpaper_dir,
            reveal_wallpaper_dir,
            get_activity_settings,
            set_activity_setting,
            get_activity_day,
            clear_activity,
            get_week_review,
            get_goals,
            set_goal,
            end_goal,
            get_goal_templates,
            silence_notices,
            start_focus,
            extend_focus,
            get_categories,
            create_category,
            update_category,
            delete_category,
            get_activity_rules,
            set_activity_rule,
            delete_activity_rule,
            set_span_category,
            get_unlabelled,
            get_domain_totals,
            get_connector_status,
            install_connector,
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
            set_block_intent,
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
            reset_all_data,
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
        // The browser connector's spool rides the same tick. It is drained
        // before the foreground sample is taken, so a browser observation and
        // the "chrome.exe was frontmost" sample for the same instant arrive in
        // the order they happened rather than inverted.
        let dir = data_dir(&app);
        match store.drain_browser_spool(&dir) {
            Ok(n) if n > 0 => log::info!("connector.drained n={n}"),
            Ok(_) => {}
            Err(err) => log::error!("connector.drain.failed code={}", err.code()),
        }

        // Notices ride the same tick. `due_notices` decides *whether there is
        // anything to say* and records that it said it, so "once per crossing"
        // is a property of the core with a test rather than a hope about this
        // loop. Collected under the lock, emitted after it — a notification is
        // never worth holding the database for.
        let tz = store.tz_or(&system_tz());
        let notices = match store.due_notices(&tz) {
            Ok(n) => n,
            Err(err) => {
                log::error!("notice.failed code={}", err.code());
                Vec::new()
            }
        };

        let sampled = match frontmost::current() {
            Some(front) => {
                let sample = fruit_core::model::ActivitySample {
                    app_id: front.app_id,
                    window_title: front.window_title,
                    domain: None,
                    at: fruit_core::time::now_ms(),
                };
                match store.record_activity(sample) {
                    Ok(v) => v,
                    Err(err) => {
                        log::error!("activity.record.failed code={}", err.code());
                        false
                    }
                }
            }
            None => false,
        };

        drop(store);
        for n in &notices {
            // A notice, never a nag: no timer is interrupted, nothing is
            // blocked, and the whole lot can be silenced from Settings.
            let _ = app.emit("notice", n);
        }
        // The recording indicator in the top bar is driven by this event —
        // §3.5 requires it to be visible whenever sampling is live.
        if sampled {
            let _ = app.emit("activity:sampled", ());
        }
    });
}

/// A bare filename would land in the process's working directory — which on
/// Windows is wherever the exe was launched from, and nowhere a person would
/// look. Export is a trust feature (§6.12); "where did it go?" defeats it.
fn downloads_path(app: &AppHandle, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        return path;
    }
    app.path()
        .download_dir()
        .or_else(|_| app.path().document_dir())
        .or_else(|_| app.path().home_dir())
        .unwrap_or_else(|_| data_dir(app))
        .join(&path)
}

/// The IANA zone name this machine is in — `"Europe/London"`, not `"+01:00"`.
///
/// **The distinction is the whole point.** Every date in this database is a
/// *local* date, and every stored row carries the zone name it was written in,
/// so that a block plotted for 09:00 is still 09:00 after the clocks change. A
/// UTC offset cannot do that job: `+01:00` is London in July and Berlin in
/// January, and it silently becomes wrong twice a year.
///
/// Three sources, in order of how much they know:
///
/// 1. **`TZ`** — set explicitly, so it beats detection. Unix convention; on
///    Windows it is essentially never set, which is what made the old fallback
///    a real bug rather than a theoretical one.
/// 2. **`iana_time_zone::get_timezone()`** — asks the operating system. On
///    Windows this reads the registry and maps the Windows zone name onto its
///    IANA equivalent, which is exactly the translation the old code skipped.
/// 3. **`"UTC"`** — a name that always parses. Wrong for most people, but a
///    working app with the wrong zone beats an app whose first run fails.
///
/// The previous implementation fell back to `chrono::Local::now().offset()`,
/// which renders as `"+00:00"`. `zone()` cannot parse that, so on any machine
/// without `TZ` set — that is, every ordinary Windows machine — the first-run
/// seed failed and the user got an empty app. Found by running the binary; no
/// unit test would have caught it, because every test passes a zone name.
fn system_tz() -> String {
    if let Ok(name) = std::env::var("TZ") {
        if !name.is_empty() {
            return name;
        }
    }
    iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string())
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
