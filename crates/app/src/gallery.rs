//! Data for the component gallery (`--gallery`): synthetic series exercise the
//! chart rules (gaps, zero stubs, mirrored pairs); the process table uses the
//! real process list (or the mock source) so sorting runs on real values.

use crate::charts::{bar_path, pair_max, stack, Dir, Layout};
use crate::processes;
use crate::{Gallery, KeyValue, LegendItem, ProcessRow, StackPart, Theme};
use resourcewatch_core::collectors::process::ProcessInfo;
use resourcewatch_core::format::{format_bytes, format_rate};
use resourcewatch_core::settings::Settings;
use resourcewatch_core::source::Source;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Deterministic wave in 0..scale with a gap every 37 samples and a zero run.
fn series(len: usize, phase: f64, scale: f64) -> Vec<Option<f64>> {
    (0..len)
        .map(|i| match i {
            _ if i % 37 == 36 => None,
            200..=205 => Some(0.0),
            _ => {
                let t = i as f64 / 9.0 + phase;
                Some(scale * (0.45 + 0.35 * t.sin() + 0.15 * (t * 2.7).cos()).max(0.0))
            }
        })
        .collect()
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

fn kv(pairs: &[(&str, &str)]) -> ModelRc<KeyValue> {
    model(
        pairs
            .iter()
            .map(|(k, v)| KeyValue {
                key: (*k).into(),
                value: (*v).into(),
            })
            .collect(),
    )
}

fn temps(fahrenheit: bool) -> ModelRc<KeyValue> {
    let fmt = |c: f64| {
        if fahrenheit {
            format!("{:.0} °F", c * 9.0 / 5.0 + 32.0)
        } else {
            format!("{c:.1} °C")
        }
    };
    let rows = [
        ("CPU Package", 48.6),
        ("Core 0", 46.0),
        ("GPU", 61.0),
        ("NVMe", 39.9),
    ];
    model(
        rows.iter()
            .map(|(k, c)| KeyValue {
                key: (*k).into(),
                value: fmt(*c).into(),
            })
            .collect(),
    )
}

fn rows(list: &[ProcessInfo]) -> Vec<ProcessRow> {
    list.iter()
        .map(|p| ProcessRow {
            name: p.name.as_str().into(),
            pid: p.pid.to_string().into(),
            cpu: p.cpu.map_or("--".into(), |c| format!("{c:.1}%")).into(),
            memory: format_bytes(Some(p.memory as f64)).into(),
        })
        .collect()
}

pub fn setup(ui: &Gallery) {
    let g = ui.global::<Theme>();
    g.set_glass(false);

    let cpu = series(240, 0.0, 100.0);
    let up = series(240, 1.3, 2.4e6);
    let down = series(240, 2.1, 6.0e6);
    let detail = series(240, 0.6, 100.0);
    let max = pair_max(&up, &down);
    ui.set_up_max(format_rate(Some(max)).into());
    let redraw = {
        let weak = ui.as_weak();
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let (w, h) = (ui.get_cpu_w(), ui.get_cpu_h());
            ui.set_cpu_path(bar_path(&cpu, 100.0, Dir::Full, Layout::Fixed, w, h).into());
            let (w, h) = (ui.get_net_w(), ui.get_net_h());
            ui.set_up_path(bar_path(&up, max, Dir::Up, Layout::Fixed, w, h).into());
            ui.set_down_path(bar_path(&down, max, Dir::Down, Layout::Fixed, w, h).into());
            let (w, h) = (ui.get_detail_w(), ui.get_detail_h());
            let slots = Layout::Slots(Settings::default().history_points as usize);
            ui.set_detail_path(bar_path(&detail, 100.0, Dir::Full, slots, w, h).into());
        }
    };
    redraw();
    ui.on_charts_resized(redraw);

    let colors = [
        g.get_ram(),
        g.get_gpu(),
        g.get_net_up(),
        g.get_text_tertiary(),
    ];
    let parts = [Some(6.1), Some(5.2), Some(3.3), Some(0.9)];
    ui.set_mem_parts(model(
        stack(&parts, 15.5)
            .into_iter()
            .zip(colors)
            .map(|((start, width), color)| StackPart {
                start,
                width,
                color,
            })
            .collect(),
    ));
    let labels = ["App", "Cache", "Free", "Other"];
    ui.set_mem_legend(model(
        labels
            .iter()
            .zip(parts)
            .zip(colors)
            .map(|((label, gib), color)| LegendItem {
                label: (*label).into(),
                value: SharedString::from(format!("{:.1} GiB", gib.unwrap_or(0.0))),
                color,
            })
            .collect(),
    ));
    ui.set_disk_kv(kv(&[
        ("Total", "931.5 GiB"),
        ("Used", "412.0 GiB"),
        ("Free", "519.5 GiB"),
    ]));
    ui.set_temps(temps(false));
    let weak = ui.as_weak();
    ui.on_temp_unit_selected(move |i| {
        if let Some(ui) = weak.upgrade() {
            ui.set_temps(temps(i == 1));
        }
    });

    // Two scans so CPU shares exist (the first scan has none).
    let mut source = Source::from_env(&Settings::default());
    source.processes(Instant::now());
    std::thread::sleep(Duration::from_millis(250));
    let list = Rc::new(RefCell::new(source.processes(Instant::now())));
    processes::sort(&mut list.borrow_mut(), processes::COL_CPU, false);
    let table = Rc::new(VecModel::from(rows(&list.borrow())));
    ui.set_rows(ModelRc::from(table.clone()));
    ui.on_sort_changed(move |column, ascending| {
        processes::sort(&mut list.borrow_mut(), column, ascending);
        table.set_vec(rows(&list.borrow()));
    });
}
