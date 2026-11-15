//! Phase 0 spike (docs/REDESIGN_V0.2.md): transparent frameless Slint window
//! with compositor blur, custom title bar and a 500 ms bar chart.
//!
//! `slint-poc`                    open the window (renderer: SLINT_RENDERER, default software)
//! `slint-poc --snapshot out.png` render headless with the software renderer to PNG
//! `POC_NO_GLASS=1`               opaque window, no blur request

mod blur;
mod icons;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::winit_030::winit::event::WindowEvent;
use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

slint::include_modules!();

const KEPT_POINTS: usize = 240;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--snapshot") {
        let out = args.get(i + 1).ok_or("--snapshot needs a path")?;
        return snapshot(out);
    }

    let renderer = std::env::var("SLINT_RENDERER").unwrap_or_else(|_| "software".into());
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name(renderer)
        .select()?;
    // Needs a selected platform; must precede window creation.
    slint::set_xdg_app_id("resourcewatch")?;

    let ui = Poc::new()?;
    let want_glass = std::env::var_os("POC_NO_GLASS").is_none();
    ui.set_glass(want_glass);
    if let [min, max, restore, close] = &icons::load(&[
        "window-minimize",
        "window-maximize",
        "window-restore",
        "window-close",
    ])[..]
    {
        ui.set_icon_minimize(min.clone());
        ui.set_icon_maximize(max.clone());
        ui.set_icon_restore(restore.clone());
        ui.set_icon_close(close.clone());
    }

    let weak = ui.as_weak();
    ui.on_minimize(move || weak.unwrap().window().set_minimized(true));
    let weak = ui.as_weak();
    ui.on_toggle_maximize(move || {
        let w = weak.unwrap();
        let max = !w.window().is_maximized();
        w.window().set_maximized(max);
        w.set_maximized_state(max);
    });
    ui.on_close_window(|| {
        let _ = slint::quit_event_loop();
    });

    if want_glass {
        let weak = ui.as_weak();
        slint::spawn_local(async move {
            let Some(ui) = weak.upgrade() else { return };
            let result = match ui.window().winit_window().await {
                Ok(w) => request_blur(&w),
                Err(e) => Err(e.to_string()),
            };
            match result {
                Err(e) => {
                    eprintln!("blur unavailable: {e}; using opaque window");
                    ui.set_glass(false);
                }
                Ok(blur) => {
                    eprintln!("blur requested");
                    let w = ui.window();
                    shape_blur(&blur, w, w.size());
                    // Re-shape on every resize (also covers maximize/restore).
                    w.on_winit_window_event(move |w, event| {
                        if let WindowEvent::Resized(size) = event {
                            shape_blur(&blur, w, slint::PhysicalSize::new(size.width, size.height));
                        }
                        slint::winit_030::EventResult::Propagate
                    });
                }
            }
        })?;
    }

    let mut cpu = CpuSampler::default();
    let history = Rc::new(RefCell::new(VecDeque::<f32>::with_capacity(KEPT_POINTS)));
    let redraw = {
        let (weak, history) = (ui.as_weak(), history.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let path = bars_path(
                &history.borrow(),
                ui.get_chart_width(),
                ui.get_chart_height(),
            );
            ui.set_bars_path(path.into());
        }
    };
    ui.on_chart_resized(redraw.clone());
    let weak = ui.as_weak();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(500),
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let Some(v) = cpu.sample() else { return };
            {
                let mut history = history.borrow_mut();
                if history.len() == KEPT_POINTS {
                    history.pop_front();
                }
                history.push_back(v);
            }
            // Compositor-driven maximize (e.g. KWin shortcut) also updates the corners.
            ui.set_maximized_state(ui.window().is_maximized());
            ui.set_cpu_text(format!("{:.0}%", v * 100.0).into());
            redraw();
        },
    );

    ui.run()?;
    Ok(())
}

/// Must match the window `border-radius` in `ui/poc.slint`.
const WINDOW_RADIUS: f32 = 16.0;

fn shape_blur(blur: &blur::wayland::Blur, w: &slint::Window, size: slint::PhysicalSize) {
    let logical = size.to_logical(w.scale_factor());
    let radius = if w.is_maximized() { 0.0 } else { WINDOW_RADIUS };
    blur.set_shape(
        logical.width.round() as i32,
        logical.height.round() as i32,
        radius as i32,
    );
}

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

/// Bars in pixel units: 2 px wide, 1 px gap, newest at the right edge.
fn bars_path(history: &VecDeque<f32>, width: f32, height: f32) -> String {
    let mut s = String::new();
    let mut x = width - 2.0;
    for &v in history.iter().rev() {
        if x < 0.0 {
            break;
        }
        let top = height - (v.clamp(0.0, 1.0) * height).max(1.0);
        s += &format!(
            "M{x} {height}L{x} {top}L{} {top}L{} {height}Z",
            x + 2.0,
            x + 2.0
        );
        x -= 3.0;
    }
    s
}

/// Whole-system CPU busy fraction from /proc/stat deltas.
#[derive(Default)]
struct CpuSampler {
    last: Option<(u64, u64)>,
}

impl CpuSampler {
    fn sample(&mut self) -> Option<f32> {
        let stat = std::fs::read_to_string("/proc/stat").ok()?;
        let nums: Vec<u64> = stat
            .lines()
            .next()?
            .split_whitespace()
            .skip(1)
            .filter_map(|n| n.parse().ok())
            .collect();
        let total: u64 = nums.iter().sum();
        let idle = nums.get(3)? + nums.get(4).unwrap_or(&0);
        let prev = self.last.replace((total, idle));
        let (pt, pi) = prev?;
        let dt = total.saturating_sub(pt);
        (dt > 0).then(|| 1.0 - idle.saturating_sub(pi) as f32 / dt as f32)
    }
}

/// Headless software-renderer platform used for PNG snapshots.
struct Headless(Rc<MinimalSoftwareWindow>);

impl slint::platform::Platform for Headless {
    fn create_window_adapter(
        &self,
    ) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

fn snapshot(out: &str) -> Result<(), Box<dyn std::error::Error>> {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(window.clone())))
        .map_err(|e| format!("{e:?}"))?;
    let ui = Poc::new()?;
    window.set_size(slint::PhysicalSize::new(520, 340));
    ui.show()?;
    let history: VecDeque<f32> = (0..KEPT_POINTS)
        .map(|i| 0.35 + 0.3 * (i as f32 / 9.0).sin())
        .collect();
    ui.set_cpu_text("42%".into());
    ui.set_bars_path(bars_path(&history, ui.get_chart_width(), ui.get_chart_height()).into());
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
    fn bars_are_right_aligned_and_clipped() {
        let h: VecDeque<f32> = [0.5, 1.0].into_iter().collect();
        let p = bars_path(&h, 10.0, 20.0);
        // Newest (1.0) at x=8..10 reaching the top, older at x=5..7 half height.
        assert!(p.starts_with("M8 20L8 0L10 0L10 20Z"));
        assert!(p.contains("M5 20L5 10L7 10L7 20Z"));
        let many: VecDeque<f32> = std::iter::repeat_n(0.5, 100).collect();
        assert_eq!(bars_path(&many, 10.0, 20.0).matches('M').count(), 3);
    }
}
