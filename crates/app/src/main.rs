//! ResourceWatch Slint app (v0.2.0 redesign, docs/REDESIGN_V0.2.md).
//!
//! `resourcewatch-slint`                    open the window
//! `resourcewatch-slint --snapshot out.png [--theme dark|light] [--collapsed] [--page N] [--gallery] [--size WxH]`
//!                                          render headless with the software renderer to PNG
//! `resourcewatch-slint --gallery [--theme dark|light]`
//!                                          open the component gallery (Phase 3 components)
//! `RESOURCEWATCH_MOCK=1`                   mock data source (from core)

#[cfg(target_os = "linux")]
mod blur;
mod charts;
mod dashboard;
mod gallery;
mod processes;

use resourcewatch_core::scheduler::Scheduler;
use resourcewatch_core::settings::{Settings, SettingsPatch};
use resourcewatch_core::settings_store::SettingsStore;
use resourcewatch_core::snapshot::{DataSource, Snapshot};
use resourcewatch_core::source::Source;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::ComponentHandle;
use std::cell::{OnceCell, RefCell};
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

    if args.iter().any(|a| a == "--gallery") {
        let ui = Gallery::new()?;
        ui.global::<Theme>()
            .set_mode(flag_value(&args, "--theme").map_or("system".into(), |t| t.into()));
        gallery::setup(&ui);
        return Ok(ui.run()?);
    }

    let prefs = Prefs::load(config_dir());
    let settings = prefs.current.clone();
    let ui = App::new()?;
    ui.set_version(env!("CARGO_PKG_VERSION").into());
    apply_prefs(&ui, &settings);
    let state = Rc::new(State {
        shared: Arc::new(Shared {
            source: Mutex::new(Source::from_env(&settings)),
            settings: Mutex::new(Arc::new(settings.clone())),
        }),
        prefs: RefCell::new(prefs),
        scheduler: OnceCell::new(),
        latest: RefCell::new(None),
        view: RefCell::new(dashboard::View::default()),
    });
    STATE.with(|s| s.set(state.clone()).ok());
    connect(&ui, &state);

    if settings.sidebar_glass {
        start_blur(&ui)?;
    }
    let weak = ui.as_weak();
    slint::Timer::single_shot(Duration::from_millis(300), move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_animated(true);
        }
    });

    // Process list: polled only while the dashboard shows it (like the Tauri
    // frontend's watchProcesses); the first scan has no CPU share, so a second
    // one follows after 500 ms.
    let poll = {
        let (weak, state) = (ui.as_weak(), state.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            if ui.get_page() != 0 || ui.window().is_minimized() {
                return;
            }
            let list = lock(&state.shared.source).processes(Instant::now());
            dashboard::apply_processes(&ui, &list);
        }
    };
    poll();
    slint::Timer::single_shot(Duration::from_millis(500), poll.clone());
    let proc_timer = slint::Timer::default();
    proc_timer.start(slint::TimerMode::Repeated, Duration::from_secs(2), poll);

    let shared = state.shared.clone();
    let weak = ui.as_weak();
    let interval = Duration::from_millis(settings.update_interval.into());
    let scheduler = Scheduler::start(interval, move |now| {
        // Lock order settings → source (never both held here).
        let settings = lock(&shared.settings).clone();
        let snapshot = lock(&shared.source).sample(now, &settings);
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            let (Some(ui), Some(state)) = (weak.upgrade(), STATE.with(|s| s.get().cloned())) else {
                return;
            };
            *state.latest.borrow_mut() = Some(snapshot);
            // Minimized: keep sampling (history continues) but skip UI work.
            if !ui.window().is_minimized() {
                state.render(&ui);
            }
        });
    })?;
    let _ = state.scheduler.set(scheduler);

    ui.run()?;
    if let Some(s) = state.scheduler.get() {
        s.stop();
    }
    Ok(())
}

thread_local! {
    /// UI-thread state for closures that arrive via `invoke_from_event_loop`
    /// (which must be `Send`, so they cannot capture the `Rc`).
    static STATE: OnceCell<Rc<State>> = const { OnceCell::new() };
}

/// Shared with the scheduler thread. Lock order: `settings` → `source`.
struct Shared {
    settings: Mutex<Arc<Settings>>,
    source: Mutex<Source>,
}

/// UI-thread state.
struct State {
    shared: Arc<Shared>,
    prefs: RefCell<Prefs>,
    scheduler: OnceCell<Scheduler>,
    latest: RefCell<Option<Snapshot>>,
    view: RefCell<dashboard::View>,
}

impl State {
    /// Validate → save → apply to source and scheduler (like Tauri's
    /// `commit_settings`); nothing changes if validation or saving fails.
    fn commit(&self, patch: &SettingsPatch) {
        let mut prefs = self.prefs.borrow_mut();
        let old_interval = prefs.current.update_interval;
        if !prefs.update(patch) {
            return;
        }
        let next = prefs.current.clone();
        let mut settings = lock(&self.shared.settings);
        lock(&self.shared.source).apply_settings(&next);
        *settings = Arc::new(next.clone());
        drop(settings);
        if next.update_interval != old_interval {
            if let Some(s) = self.scheduler.get() {
                s.set_interval(Duration::from_millis(next.update_interval.into()));
            }
        }
    }

    /// Pushes the latest snapshot to the UI (title bar + dashboard).
    fn render(&self, ui: &App) {
        let latest = self.latest.borrow();
        let Some(snapshot) = latest.as_ref() else {
            return;
        };
        Host::from(snapshot).apply(ui);
        if ui.get_page() == 0 {
            let settings = self.prefs.borrow().current.clone();
            dashboard::apply(ui, snapshot, &settings, &mut self.view.borrow_mut());
        }
    }
}

fn connect(ui: &App, state: &Rc<State>) {
    ui.on_close_window(|| {
        let _ = slint::quit_event_loop();
    });
    let (weak, st) = (ui.as_weak(), state.clone());
    ui.on_prefs_changed(move || {
        let ui = weak.unwrap();
        st.commit(&SettingsPatch {
            theme: Some(ui.global::<Theme>().get_mode().into()),
            sidebar_collapsed: Some(ui.get_sidebar_collapsed()),
            ..Default::default()
        });
    });
    // Returning to the dashboard shows the latest values at once.
    let (weak, st) = (ui.as_weak(), state.clone());
    ui.on_page_changed(move || st.render(&weak.unwrap()));

    let d = ui.global::<Dash>();
    let st = state.clone();
    d.on_net_selected(move |i| {
        let value = st.view.borrow().net_values.get(i as usize).cloned();
        if let Some(v) = value {
            st.commit(&SettingsPatch {
                net_interface: Some(v),
                ..Default::default()
            });
        }
    });
    let st = state.clone();
    d.on_gpu_selected(move |i| {
        let value = st.view.borrow().gpu_values.get(i as usize).copied();
        if let Some(v) = value {
            st.commit(&SettingsPatch {
                gpu_index: Some(v),
                ..Default::default()
            });
        }
    });
    let (weak, st) = (ui.as_weak(), state.clone());
    d.on_temp_unit_selected(move |i| {
        st.commit(&SettingsPatch {
            temp_unit: Some(if i == 1 { "f" } else { "c" }.into()),
            ..Default::default()
        });
        st.render(&weak.unwrap());
    });
    let (weak, st) = (ui.as_weak(), state.clone());
    d.on_sum_disk_step(move |delta| {
        let count = st.latest.borrow().as_ref().map_or(0, |s| s.disks.len());
        if count > 0 {
            let mut view = st.view.borrow_mut();
            view.sum_disk =
                (view.sum_disk as i64 + i64::from(delta)).rem_euclid(count as i64) as usize;
        }
        st.render(&weak.unwrap());
    });
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

    /// Validates and saves; returns whether the settings changed. On any error
    /// the previous value stays in use.
    fn update(&mut self, patch: &SettingsPatch) -> bool {
        let next = match self.current.apply(patch) {
            Ok(n) if n != self.current => n,
            Ok(_) => return false,
            Err(e) => {
                eprintln!("ResourceWatch: settings: {e}");
                return false;
            }
        };
        if let Some(store) = &self.store {
            if let Err(e) = store.save(&next) {
                eprintln!("ResourceWatch: cannot save {}: {e}", store.path().display());
                return false;
            }
        }
        self.current = next;
        true
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

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
}

/// Renders one frame with real (or `RESOURCEWATCH_MOCK=1`) host data, without
/// a window: visual checks avoid KWin focus-stealing prevention.
fn snapshot(out: &str, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(window.clone())))
        .map_err(|e| format!("{e:?}"))?;
    let theme: slint::SharedString =
        flag_value(args, "--theme").map_or("dark".into(), |t| t.into());
    let (w, h) = flag_value(args, "--size")
        .and_then(|v| v.split_once('x'))
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .unwrap_or((1220, 790));
    window.set_size(slint::PhysicalSize::new(w, h));
    if args.iter().any(|a| a == "--gallery") {
        let ui = Gallery::new()?;
        ui.global::<Theme>().set_mode(theme);
        ui.show()?;
        gallery::setup(&ui);
        return write_png(ui.window(), out);
    }
    let ui = App::new()?;
    let settings = Settings {
        theme: theme.into(),
        sidebar_collapsed: args.iter().any(|a| a == "--collapsed"),
        ..Settings::default()
    };
    apply_prefs(&ui, &settings);
    ui.set_version(env!("CARGO_PKG_VERSION").into());
    ui.set_page(
        flag_value(args, "--page")
            .and_then(|p| p.parse().ok())
            .unwrap_or(0),
    );
    // A few cycles so rates, CPU shares and some history exist.
    let mut source = Source::from_env(&settings);
    let mut snap = source.sample(Instant::now(), &settings);
    source.processes(Instant::now());
    for _ in 0..4 {
        std::thread::sleep(Duration::from_millis(250));
        snap = source.sample(Instant::now(), &settings);
    }
    Host::from(&snap).apply(&ui);
    ui.show()?;
    dashboard::apply(&ui, &snap, &settings, &mut dashboard::View::default());
    dashboard::apply_processes(&ui, &source.processes(Instant::now()));
    write_png(ui.window(), out)
}

fn write_png(window: &slint::Window, out: &str) -> Result<(), Box<dyn std::error::Error>> {
    let buf = window.take_snapshot()?;
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

        assert!(!p.update(&SettingsPatch::default()));
        assert!(!file.exists(), "no change → no write");

        assert!(p.update(&SettingsPatch {
            theme: Some("light".into()),
            sidebar_collapsed: Some(true),
            ..Default::default()
        }));
        let reloaded = Prefs::load(Some(dir.clone())).current;
        assert_eq!(
            (reloaded.theme.as_str(), reloaded.sidebar_collapsed),
            ("light", true)
        );

        assert!(!p.update(&SettingsPatch {
            theme: Some("purple".into()),
            ..Default::default()
        }));
        assert_eq!(
            p.current.theme, "light",
            "invalid value keeps the previous one"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
