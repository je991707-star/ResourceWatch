//! ResourceWatch Slint app (v0.2.0 redesign, docs/REDESIGN_V0.2.md).
//!
//! `resourcewatch-slint`                    open the window
//! `resourcewatch-slint --snapshot out.png [--theme dark|light] [--collapsed] [--page N]`
//!                                          render headless with the software renderer to PNG
//! `RESOURCEWATCH_MOCK=1`                   mock data source (from core)

#[cfg(target_os = "linux")]
mod blur;

use resourcewatch_core::scheduler::Scheduler;
use resourcewatch_core::settings::{Settings, SettingsPatch};
use resourcewatch_core::settings_store::SettingsStore;
use resourcewatch_core::snapshot::{DataSource, Snapshot};
use resourcewatch_core::source::Source;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

slint::include_modules!();

/// Same directory as the Tauri app (`app_config_dir` of its identifier), so both
/// share one `config.json` during the redesign.
const CONFIG_DIR_NAME: &str = "io.github.local.resourcewatch";
/// Must match the window `radius` in `ui/app.slint`.
const WINDOW_RADIUS: f32 = 16.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--snapshot") {
        let out = args.get(i + 1).ok_or("--snapshot needs a path")?;
        return snapshot(out, &args);
    }

    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .select()?;
    // Needs a selected platform; must precede window creation. Equals the
    // installed .desktop file name so KWin shows the app icon.
    slint::set_xdg_app_id("resourcewatch")?;

    let prefs = Rc::new(RefCell::new(Prefs::load(config_dir())));
    let settings = prefs.borrow().current.clone();
    let ui = App::new()?;
    ui.set_version(env!("CARGO_PKG_VERSION").into());
    apply_prefs(&ui, &settings);

    ui.on_close_window(|| {
        let _ = slint::quit_event_loop();
    });
    let weak = ui.as_weak();
    let p = prefs.clone();
    ui.on_prefs_changed(move || {
        let ui = weak.unwrap();
        p.borrow_mut().update(&SettingsPatch {
            theme: Some(ui.global::<Theme>().get_mode().into()),
            sidebar_collapsed: Some(ui.get_sidebar_collapsed()),
            ..Default::default()
        });
    });

    if settings.sidebar_glass {
        start_blur(&ui)?;
    }

    let source = Arc::new(Mutex::new(Source::from_env(&settings)));
    let weak = ui.as_weak();
    let interval = Duration::from_millis(settings.update_interval.into());
    // shortcut: the scheduler samples with the settings read at startup; Phase 6
    // (settings page) must pass changed intervals/devices to it like Tauri's commit_settings.
    let scheduler = Scheduler::start(interval, move |now| {
        let host = Host::from(&lock(&source).sample(now, &settings));
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                host.apply(&ui);
            }
        });
    })?;

    ui.run()?;
    scheduler.stop();
    Ok(())
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// `$XDG_CONFIG_HOME/<id>` or `~/.config/<id>`; `None` keeps settings in memory.
fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|d| d.join(CONFIG_DIR_NAME))
}

/// Settings plus where they persist (core `settings_store`: same file, same
/// tolerance rules as the Tauri app).
struct Prefs {
    store: Option<SettingsStore>,
    current: Settings,
}

impl Prefs {
    fn load(dir: Option<PathBuf>) -> Self {
        let store = dir.map(|d| SettingsStore::new(&d));
        let current = match &store {
            Some(s) => {
                let loaded = s.load();
                if let Some(w) = &loaded.warning {
                    eprintln!("ResourceWatch: settings: {w}");
                }
                loaded.settings
            }
            None => {
                eprintln!("ResourceWatch: no config directory; settings kept in memory");
                Settings::default()
            }
        };
        Self { store, current }
    }

    /// Validates and saves; on any error the previous value stays in use.
    fn update(&mut self, patch: &SettingsPatch) {
        let next = match self.current.apply(patch) {
            Ok(n) if n != self.current => n,
            Ok(_) => return,
            Err(e) => return eprintln!("ResourceWatch: settings: {e}"),
        };
        if let Some(store) = &self.store {
            if let Err(e) = store.save(&next) {
                return eprintln!("ResourceWatch: cannot save {}: {e}", store.path().display());
            }
        }
        self.current = next;
    }
}

fn apply_prefs(ui: &App, s: &Settings) {
    ui.global::<Theme>().set_mode(s.theme.as_str().into());
    ui.global::<Theme>().set_glass(s.sidebar_glass);
    ui.set_sidebar_collapsed(s.sidebar_collapsed);
}

/// Title-bar host summary; plain strings so it can cross from the scheduler
/// thread to the UI thread.
struct Host {
    hostname: String,
    uptime: String,
    machine_model: String,
    cpu_name: String,
    mock: bool,
}

impl From<&Snapshot> for Host {
    fn from(s: &Snapshot) -> Self {
        Self {
            hostname: s.hostname.clone(),
            uptime: s.uptime_text.clone(),
            machine_model: s.machine_model.clone(),
            cpu_name: s.cpu_name.clone(),
            mock: s.data_source == DataSource::Mock,
        }
    }
}

impl Host {
    /// Slint skips unchanged values, so only the uptime text repaints each cycle.
    fn apply(self, ui: &App) {
        ui.set_hostname(self.hostname.into());
        ui.set_uptime(self.uptime.into());
        ui.set_machine_model(self.machine_model.into());
        ui.set_cpu_name(self.cpu_name.into());
        ui.set_mock(self.mock);
    }
}

#[cfg(target_os = "linux")]
fn start_blur(ui: &App) -> Result<(), slint::EventLoopError> {
    use slint::winit_030::winit::event::WindowEvent;
    use slint::winit_030::{EventResult, WinitWindowAccessor};

    let weak = ui.as_weak();
    slint::spawn_local(async move {
        let Some(ui) = weak.upgrade() else { return };
        let result = match ui.window().winit_window().await {
            Ok(w) => request_blur(&w),
            Err(e) => Err(e.to_string()),
        };
        match result {
            Err(e) => {
                eprintln!("ResourceWatch: blur unavailable: {e}; using an opaque window");
                ui.global::<Theme>().set_glass(false);
            }
            Ok(blur) => {
                let w = ui.window();
                shape_blur(&blur, w, w.size());
                // Re-shape on every resize (also covers maximize/restore).
                w.on_winit_window_event(move |w, event| {
                    if let WindowEvent::Resized(size) = event {
                        shape_blur(&blur, w, slint::PhysicalSize::new(size.width, size.height));
                    }
                    EventResult::Propagate
                });
            }
        }
    })
    .map(|_| ())
}

#[cfg(not(target_os = "linux"))]
fn start_blur(ui: &App) -> Result<(), slint::EventLoopError> {
    ui.global::<Theme>().set_glass(false);
    Ok(())
}

/// Blur only the rounded window shape so the backdrop outside the corners stays sharp.
#[cfg(target_os = "linux")]
fn shape_blur(blur: &blur::wayland::Blur, w: &slint::Window, size: slint::PhysicalSize) {
    let logical = size.to_logical(w.scale_factor());
    let radius = if w.is_maximized() { 0.0 } else { WINDOW_RADIUS };
    blur.set_shape(
        logical.width.round() as i32,
        logical.height.round() as i32,
        radius as i32,
    );
}

#[cfg(target_os = "linux")]
fn request_blur(
    w: &slint::winit_030::winit::window::Window,
) -> Result<blur::wayland::Blur, String> {
    use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
    let display = w.display_handle().map_err(|e| e.to_string())?.as_raw();
    let window = w.window_handle().map_err(|e| e.to_string())?.as_raw();
    match (display, window) {
        (RawDisplayHandle::Wayland(d), RawWindowHandle::Wayland(s)) => {
            // SAFETY: winit owns both objects and keeps them alive while the window exists.
            unsafe { blur::wayland::blur(d.display.as_ptr(), s.surface.as_ptr()) }
        }
        _ => Err("not a native Wayland window".into()),
    }
}

/// Headless software-renderer platform for PNG snapshots.
struct Headless(Rc<MinimalSoftwareWindow>);

impl slint::platform::Platform for Headless {
    fn create_window_adapter(
        &self,
    ) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

/// Renders one frame with real (or `RESOURCEWATCH_MOCK=1`) host data, without
/// a window: visual checks avoid KWin focus-stealing prevention.
fn snapshot(out: &str, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
    };
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(window.clone())))
        .map_err(|e| format!("{e:?}"))?;
    let ui = App::new()?;
    let settings = Settings {
        theme: value("--theme").cloned().unwrap_or_else(|| "dark".into()),
        sidebar_collapsed: args.iter().any(|a| a == "--collapsed"),
        ..Settings::default()
    };
    apply_prefs(&ui, &settings);
    ui.set_version(env!("CARGO_PKG_VERSION").into());
    ui.set_page(value("--page").and_then(|p| p.parse().ok()).unwrap_or(0));
    Host::from(&Source::from_env(&settings).sample(Instant::now(), &settings)).apply(&ui);
    window.set_size(slint::PhysicalSize::new(1220, 790));
    ui.show()?;
    let buf = ui.window().take_snapshot()?;
    let mut enc = png::Encoder::new(std::fs::File::create(out)?, buf.width(), buf.height());
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(buf.as_bytes())?;
    eprintln!("wrote {out} ({}x{})", buf.width(), buf.height());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_save_only_valid_changes() {
        let dir = std::env::temp_dir().join(format!("rw-app-prefs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut p = Prefs::load(Some(dir.clone()));
        assert_eq!(p.current, Settings::default(), "first start → defaults");
        let file = dir.join("config.json");

        p.update(&SettingsPatch::default());
        assert!(!file.exists(), "no change → no write");

        p.update(&SettingsPatch {
            theme: Some("light".into()),
            sidebar_collapsed: Some(true),
            ..Default::default()
        });
        let reloaded = Prefs::load(Some(dir.clone())).current;
        assert_eq!(
            (reloaded.theme.as_str(), reloaded.sidebar_collapsed),
            ("light", true)
        );

        p.update(&SettingsPatch {
            theme: Some("purple".into()),
            ..Default::default()
        });
        assert_eq!(
            p.current.theme, "light",
            "invalid value keeps the previous one"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
