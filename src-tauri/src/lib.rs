//! ResourceWatch Tauri application.
//!
//! Snapshots come from the live `monitor::Monitor` (Rust collectors reading
//! `/proc`, sysfs, hwmon and NVML), or from `mock::MockSource` when
//! `RESOURCEWATCH_MOCK=1` is set for UI development. One
//! `scheduler::Scheduler` thread drives every monitoring cycle.
//!
//! IPC contract
//! - Commands: `get_snapshot`, `get_settings`, `get_settings_info`,
//!   `update_settings`, `reset_settings`, `refresh_devices`,
//!   `get_scheduler_stats`
//! - Events:   `snapshot` (Snapshot), `settings-changed` (Settings)

mod collectors;
mod format;
mod history;
mod mock;
mod monitor;
#[cfg(test)]
mod robustness;
mod scheduler;
mod settings;
mod settings_store;
mod snapshot;
mod source;

use scheduler::{Scheduler, SchedulerStats};
use serde::Serialize;
use settings::{Settings, SettingsError, SettingsPatch};
use settings_store::SettingsStore;
use snapshot::Snapshot;
use source::{RefreshKind, Source};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, RunEvent, Runtime, State};

pub const EVENT_SNAPSHOT: &str = "snapshot";
pub const EVENT_SETTINGS: &str = "settings-changed";

/// `RESOURCEWATCH_DEBUG_STATS=1` prints scheduler statistics to stderr every
/// 10 s and at exit (used for the performance tests).
const DEBUG_STATS_ENV: &str = "RESOURCEWATCH_DEBUG_STATS";
const DEBUG_STATS_EVERY: Duration = Duration::from_secs(10);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding a lock must not take the whole UI down.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn interval_of(s: &Settings) -> Duration {
    Duration::from_millis(s.update_interval.into())
}

struct SettingsCell {
    /// Shared with the scheduler thread by cheap `Arc` clone (no per-cycle
    /// `String` copies).
    current: Arc<Settings>,
    /// `None` when no config directory could be resolved (memory only).
    store: Option<SettingsStore>,
    load_warning: Option<String>,
}

/// Lock order: `settings` → `source`. The scheduler cycle never holds
/// `settings` while taking `source`, so the two cannot deadlock.
pub struct AppState {
    settings: Mutex<SettingsCell>,
    source: Mutex<Source>,
    /// Last snapshot produced by the scheduler; `get_snapshot` returns it
    /// without touching hardware.
    latest: Mutex<Option<Snapshot>>,
    scheduler: OnceLock<Scheduler>,
}

impl AppState {
    fn new(store: Option<SettingsStore>, store_warning: Option<String>) -> Self {
        let (settings, load_warning) = match &store {
            Some(s) => {
                let loaded = s.load();
                (loaded.settings, loaded.warning)
            }
            None => (Settings::default(), store_warning),
        };
        if let Some(w) = &load_warning {
            eprintln!("ResourceWatch: settings: {w}");
        }
        Self {
            source: Mutex::new(Source::from_env(&settings)),
            settings: Mutex::new(SettingsCell {
                current: Arc::new(settings),
                store,
                load_warning,
            }),
            latest: Mutex::new(None),
            scheduler: OnceLock::new(),
        }
    }

    fn settings(&self) -> Arc<Settings> {
        Arc::clone(&lock(&self.settings).current)
    }

    /// Stops and joins the scheduler thread (app exit).
    fn shutdown(&self) {
        if let Some(s) = self.scheduler.get() {
            s.stop();
            if debug_stats() {
                eprintln!("{}", format_stats("RW_STATS final", &s.stats()));
                eprintln!("ResourceWatch: scheduler stopped");
            }
        }
    }
}

fn debug_stats() -> bool {
    std::env::var(DEBUG_STATS_ENV).is_ok_and(|v| v == "1")
}

fn format_stats(tag: &str, s: &SchedulerStats) -> String {
    let avg = |total: u64| total.checked_div(s.ticks).unwrap_or(0);
    format!(
        "{tag} interval_ms={} ticks={} skipped={} panics={} avg_duration_us={} max_duration_us={} avg_lateness_us={} max_lateness_us={}",
        s.interval_ms,
        s.ticks,
        s.skipped,
        s.panics,
        avg(s.total_duration_us),
        s.max_duration_us,
        avg(s.total_lateness_us),
        s.max_lateness_us
    )
}

/// Error returned to the frontend by commands.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    code: &'static str,
    field: Option<&'static str>,
    message: String,
}

impl From<SettingsError> for CommandError {
    fn from(e: SettingsError) -> Self {
        Self {
            code: "invalidSetting",
            field: Some(e.field),
            message: e.message,
        }
    }
}

fn emit<R: Runtime, T: Serialize + Clone>(app: &AppHandle<R>, event: &str, payload: &T) {
    if let Err(e) = app.emit(event, payload) {
        eprintln!("ResourceWatch: failed to emit {event}: {e}");
    }
}

#[tauri::command]
fn get_snapshot(state: State<'_, AppState>) -> Snapshot {
    if let Some(s) = lock(&state.latest).as_ref() {
        return s.clone();
    }
    let settings = state.settings();
    lock(&state.source).peek(&settings)
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    (*state.settings()).clone()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsInfo {
    /// Absolute path of `config.json`, or `null` when settings are memory-only.
    path: Option<String>,
    /// Problem found while loading the file at startup, if any.
    load_warning: Option<String>,
}

#[tauri::command]
fn get_settings_info(state: State<'_, AppState>) -> SettingsInfo {
    let cell = lock(&state.settings);
    SettingsInfo {
        path: cell.store.as_ref().map(|s| s.path().display().to_string()),
        load_warning: cell.load_warning.clone(),
    }
}

/// Validate → write to disk → apply to source and scheduler → publish.
///
/// Everything happens under the settings lock, so concurrent updates are
/// serialised and memory, disk and scheduler cannot diverge: if the file
/// cannot be written, nothing is applied and the error is returned.
fn commit_settings<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    make: impl FnOnce(&Settings) -> Result<Settings, CommandError>,
) -> Result<Settings, CommandError> {
    let mut cell = lock(&state.settings);
    let next = make(&cell.current)?;
    if next == *cell.current {
        return Ok(next); // nothing to write or reschedule
    }
    if let Some(store) = &cell.store {
        store.save(&next).map_err(|e| CommandError {
            code: "settingsWriteFailed",
            field: None,
            message: format!("無法儲存設定檔 {}：{e}", store.path().display()),
        })?;
    }
    lock(&state.source).apply_settings(&next);
    let interval_changed = next.update_interval != cell.current.update_interval;
    cell.current = Arc::new(next.clone());
    if interval_changed {
        if let Some(s) = state.scheduler.get() {
            s.set_interval(interval_of(&next));
        }
    }
    drop(cell);
    emit(app, EVENT_SETTINGS, &next);
    Ok(next)
}

#[tauri::command]
fn update_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<Settings, CommandError> {
    commit_settings(&app, &state, |cur| Ok(cur.apply(&patch)?))
}

#[tauri::command]
fn reset_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Result<Settings, CommandError> {
    commit_settings(&app, &state, |_| Ok(Settings::default()))
}

/// Manual refresh (IPv4 / disk list / static hardware info). Runs on the
/// caller's thread, serialised with the scheduler cycle by the source lock;
/// the scheduler keeps running. Returns and emits the refreshed snapshot.
#[tauri::command]
fn refresh_devices<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    kind: RefreshKind,
) -> Snapshot {
    let settings = state.settings();
    let snapshot = {
        let mut src = lock(&state.source);
        src.refresh(kind, &settings);
        src.peek(&settings)
    };
    emit(&app, EVENT_SNAPSHOT, &snapshot);
    *lock(&state.latest) = Some(snapshot.clone());
    snapshot
}

#[tauri::command]
fn get_scheduler_stats(state: State<'_, AppState>) -> Option<SchedulerStats> {
    state.scheduler.get().map(Scheduler::stats)
}

/// Starts the single scheduler. Each cycle: sample all collectors (source
/// lock only) → emit the snapshot → keep it as `latest`.
fn start_scheduler<R: Runtime>(app: &AppHandle<R>) -> std::io::Result<()> {
    let state = app.state::<AppState>();
    let handle = app.clone();
    let debug = debug_stats();
    let mut last_report = Instant::now();
    let scheduler = Scheduler::start(interval_of(&state.settings()), move |now| {
        let state = handle.state::<AppState>();
        let settings = state.settings();
        let snapshot = lock(&state.source).sample(now, &settings);
        emit(&handle, EVENT_SNAPSHOT, &snapshot);
        *lock(&state.latest) = Some(snapshot);
        if debug && last_report.elapsed() >= DEBUG_STATS_EVERY {
            last_report = Instant::now();
            if let Some(s) = state.scheduler.get() {
                eprintln!("{}", format_stats("RW_STATS", &s.stats()));
            }
        }
    })?;
    if state.scheduler.set(scheduler).is_err() {
        eprintln!("ResourceWatch: scheduler already running; not starting another");
    }
    Ok(())
}

/// Registers state, the scheduler and all commands. `config_dir`: directory
/// for `config.json`; `None` resolves Tauri's app config directory
/// (`$XDG_CONFIG_HOME/<identifier>`, usually
/// `~/.config/io.github.local.resourcewatch`). Tests pass a temp directory.
pub fn configure_with<R: Runtime>(
    builder: tauri::Builder<R>,
    config_dir: Option<PathBuf>,
) -> tauri::Builder<R> {
    builder
        .setup(move |app| {
            let dir = match config_dir {
                Some(d) => Ok(d),
                None => app.path().app_config_dir(),
            };
            let (store, warning) = match dir {
                Ok(dir) => (Some(SettingsStore::new(&dir)), None),
                Err(e) => (
                    None,
                    Some(format!("無法取得設定目錄（{e}），設定只保存在記憶體")),
                ),
            };
            app.manage(AppState::new(store, warning));
            start_scheduler(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            get_settings,
            get_settings_info,
            update_settings,
            reset_settings,
            refresh_devices,
            get_scheduler_stats
        ])
}

pub fn configure<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    configure_with(builder, None)
}

/// Headless sampling for cross-checking against system tools
/// (`cargo run --example sample`). Returns `count` live snapshots taken
/// `interval` apart; no window, no Tauri runtime, no settings file.
pub fn sample_live(count: usize, interval: Duration) -> Vec<serde_json::Value> {
    let settings = Settings::default();
    let mut m = monitor::Monitor::new(&settings);
    (0..count)
        .map(|i| {
            if i > 0 {
                std::thread::sleep(interval);
            }
            serde_json::to_value(m.sample(Instant::now(), &settings)).unwrap_or_default()
        })
        .collect()
}

/// WebKitGTK's DMA-BUF renderer fails on X11/XWayland with the NVIDIA
/// proprietary driver ("Failed to create GBM buffer", blank window). Native
/// Wayland works. Returns whether the workaround variable should be set:
/// only when the user has not set it, the GTK backend is X11, and the NVIDIA
/// kernel module is loaded.
fn needs_dmabuf_workaround(
    var: impl Fn(&str) -> Option<String>,
    nvidia_module_loaded: bool,
) -> bool {
    if var("WEBKIT_DISABLE_DMABUF_RENDERER").is_some() || !nvidia_module_loaded {
        return false;
    }
    let present = |k: &str| var(k).is_some_and(|v| !v.is_empty());
    let x11 = match var("GDK_BACKEND").as_deref() {
        Some(b) if b.starts_with("x11") => true,
        Some(b) if b.starts_with("wayland") => false,
        _ => !present("WAYLAND_DISPLAY") && present("DISPLAY"),
    };
    x11
}

/// Must be called at the very start of `main`, before any thread exists
/// (`std::env::set_var` is not thread-safe).
pub fn prepare_environment() {
    let nvidia = std::path::Path::new("/sys/module/nvidia").exists();
    if needs_dmabuf_workaround(|k| std::env::var(k).ok(), nvidia) {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        eprintln!("ResourceWatch: X11 + NVIDIA detected, set WEBKIT_DISABLE_DMABUF_RENDERER=1");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    configure(tauri::Builder::default())
        .build(tauri::generate_context!())
        .expect("error while building ResourceWatch")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                if let Some(state) = app.try_state::<AppState>() {
                    state.shutdown();
                }
            }
        });
}

#[cfg(test)]
mod env_tests {
    use super::needs_dmabuf_workaround;
    use std::collections::HashMap;

    fn check(vars: &[(&str, &str)], nvidia: bool) -> bool {
        let m: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        needs_dmabuf_workaround(|k| m.get(k).cloned(), nvidia)
    }

    #[test]
    fn dmabuf_workaround_decision() {
        assert!(check(
            &[("GDK_BACKEND", "x11"), ("WAYLAND_DISPLAY", "wayland-1")],
            true
        ));
        assert!(check(&[("DISPLAY", ":0")], true), "plain X11 session");
        assert!(
            !check(&[("WAYLAND_DISPLAY", "wayland-1"), ("DISPLAY", ":0")], true),
            "native Wayland"
        );
        assert!(!check(
            &[("GDK_BACKEND", "wayland"), ("DISPLAY", ":0")],
            true
        ));
        assert!(!check(&[("GDK_BACKEND", "x11")], false), "no NVIDIA driver");
        assert!(
            !check(
                &[
                    ("GDK_BACKEND", "x11"),
                    ("WEBKIT_DISABLE_DMABUF_RENDERER", "0")
                ],
                true
            ),
            "user choice wins"
        );
    }
}
