//! ResourceWatch Tauri application.
//!
//! Snapshots come from the live `monitor::Monitor` (Rust collectors reading
//! `/proc`, sysfs, hwmon and NVML), or from `mock::MockSource` when
//! `RESOURCEWATCH_MOCK=1` is set for UI development.
//!
//! IPC contract
//! - Commands: `get_snapshot`, `get_settings`, `update_settings`,
//!   `reset_settings`, `refresh_devices`
//! - Events:   `snapshot` (Snapshot), `settings-changed` (Settings)

mod collectors;
mod format;
mod history;
mod mock;
mod monitor;
mod settings;
mod snapshot;
mod source;

use serde::Serialize;
use settings::{Settings, SettingsError, SettingsPatch};
use snapshot::Snapshot;
use source::{RefreshKind, Source};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

pub const EVENT_SNAPSHOT: &str = "snapshot";
pub const EVENT_SETTINGS: &str = "settings-changed";

struct Inner {
    settings: Settings,
    source: Source,
    /// Set when `updateInterval` changes so the loop reschedules immediately.
    reschedule: bool,
}

pub struct AppState {
    inner: Mutex<Inner>,
    wake: Condvar,
}

impl AppState {
    fn new() -> Self {
        let settings = Settings::default();
        Self {
            inner: Mutex::new(Inner {
                source: Source::from_env(&settings),
                settings,
                reschedule: false,
            }),
            wake: Condvar::new(),
        }
    }

    /// A panic while holding the lock must not take the whole UI down.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
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
    let mut g = state.lock();
    let settings = g.settings.clone();
    g.source.peek(&settings)
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.lock().settings.clone()
}

fn store_settings<R: Runtime>(app: &AppHandle<R>, state: &AppState, next: Settings) -> Settings {
    {
        let mut g = state.lock();
        if g.settings.update_interval != next.update_interval {
            g.reschedule = true;
        }
        g.source.apply_settings(&next);
        g.settings = next.clone();
    }
    state.wake.notify_all();
    emit(app, EVENT_SETTINGS, &next);
    next
}

#[tauri::command]
fn update_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<Settings, CommandError> {
    let next = state.lock().settings.apply(&patch)?;
    Ok(store_settings(&app, &state, next))
}

#[tauri::command]
fn reset_settings<R: Runtime>(app: AppHandle<R>, state: State<'_, AppState>) -> Settings {
    store_settings(&app, &state, Settings::default())
}

/// Manual refresh (IPv4 / disk list / static hardware info). Returns and emits
/// the refreshed snapshot immediately, without waiting for the next tick.
#[tauri::command]
fn refresh_devices<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    kind: RefreshKind,
) -> Snapshot {
    let snapshot = {
        let mut g = state.lock();
        let settings = g.settings.clone();
        g.source.refresh(kind, &settings);
        g.source.peek(&settings)
    };
    emit(&app, EVENT_SNAPSHOT, &snapshot);
    snapshot
}

/// Temporary sampling loop: one thread, sleeps until the next due time,
/// takes one sample of every collector and emits one snapshot per
/// `updateInterval`. No catch-up after a long stall (the next due time is
/// always computed from "now"). The unified scheduler of Phase 3 replaces this.
fn spawn_sampler<R: Runtime>(app: AppHandle<R>) -> std::io::Result<()> {
    std::thread::Builder::new()
        .name("rw-sampler".into())
        .spawn(move || {
            let state = app.state::<AppState>();
            let mut next_due = Instant::now();
            loop {
                let snapshot = {
                    let mut g = state.lock();
                    loop {
                        let now = Instant::now();
                        if g.reschedule {
                            g.reschedule = false;
                            next_due =
                                now + Duration::from_millis(g.settings.update_interval.into());
                        }
                        if now >= next_due {
                            break;
                        }
                        g = match state.wake.wait_timeout(g, next_due - now) {
                            Ok((g, _)) => g,
                            Err(e) => e.into_inner().0,
                        };
                    }
                    let settings = g.settings.clone();
                    next_due =
                        Instant::now() + Duration::from_millis(settings.update_interval.into());
                    g.source.sample(&settings)
                };
                emit(&app, EVENT_SNAPSHOT, &snapshot);
            }
        })
        .map(|_| ())
}

/// Registers state, the mock loop and all commands. Shared by `run()` and the
/// IPC integration tests (which use Tauri's mock runtime).
pub fn configure<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
        .manage(AppState::new())
        .setup(|app| {
            spawn_sampler(app.handle().clone())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            get_settings,
            update_settings,
            reset_settings,
            refresh_devices
        ])
}

/// Headless sampling for cross-checking against system tools
/// (`cargo run --example sample`). Returns `count` live snapshots taken
/// `interval` apart; no window, no Tauri runtime.
pub fn sample_live(count: usize, interval: Duration) -> Vec<serde_json::Value> {
    let settings = Settings::default();
    let mut m = monitor::Monitor::new(&settings);
    (0..count)
        .map(|i| {
            if i > 0 {
                std::thread::sleep(interval);
            }
            serde_json::to_value(m.sample(&settings)).unwrap_or_default()
        })
        .collect()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    configure(tauri::Builder::default())
        .run(tauri::generate_context!())
        .expect("error while running ResourceWatch");
}
