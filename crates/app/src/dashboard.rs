//! Snapshot → dashboard values (`Dash` global). Formatting follows the Tauri
//! frontend (`frontend/app.js` render()): missing values stay "--", texts the
//! backend already formats are passed through, temperatures convert to °F
//! only for display.

use crate::charts::{bar_path, pair_max, stack, Dir, Layout};
use crate::{App, ChartCanvas, Dash, DiskRow, KeyValue, LegendItem, StackPart, Theme};
use resourcewatch_core::collectors::process::ProcessInfo;
use resourcewatch_core::collectors::sensors::SensorKind;
use resourcewatch_core::format::{format_bytes, format_rate};
use resourcewatch_core::settings::Settings;
use resourcewatch_core::snapshot::{Metric, Snapshot};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

const MISSING: &str = "--";

/// UI-side state the snapshot does not carry.
#[derive(Default)]
pub struct View {
    /// Disk shown in the SUMMARY storage pager.
    pub sum_disk: usize,
    /// `netInterface` values behind the NIC pill options ("all" first).
    pub net_values: Vec<String>,
    /// `gpuIndex` values behind the GPU pill options.
    pub gpu_values: Vec<u32>,
}

/// Updates a list property in place: same length → only changed rows are
/// replaced, so Slint keeps the repeated elements instead of rebuilding them
/// (and their layout and paint) on every cycle.
fn sync<T: Clone + PartialEq + 'static>(
    current: ModelRc<T>,
    set: impl FnOnce(ModelRc<T>),
    new: Vec<T>,
) {
    if let Some(vm) = current.as_any().downcast_ref::<VecModel<T>>() {
        if vm.row_count() == new.len() {
            for (i, v) in new.into_iter().enumerate() {
                if vm.row_data(i).as_ref() != Some(&v) {
                    vm.set_row_data(i, v);
                }
            }
        } else {
            vm.set_vec(new);
        }
        return;
    }
    set(ModelRc::new(VecModel::from(new)));
}

fn ss(s: &str) -> SharedString {
    if s.is_empty() {
        MISSING.into()
    } else {
        s.into()
    }
}

fn kv(key: &str, value: impl AsRef<str>) -> KeyValue {
    KeyValue {
        key: key.into(),
        value: ss(value.as_ref()),
    }
}

fn percent(m: &Metric) -> f32 {
    m.percent.map_or(-1.0, |p| p as f32)
}

fn pair_text(m: &Metric) -> String {
    format!("{} / {}", m.used_text, m.total_text)
}

/// "48.6 °C" → "120 °F" when Fahrenheit is chosen (backend texts are °C).
pub fn temp_text(text: &str, fahrenheit: bool) -> String {
    match text.strip_suffix(" °C").and_then(|n| n.parse::<f64>().ok()) {
        Some(c) if fahrenheit => format!("{:.0} °F", c * 9.0 / 5.0 + 32.0),
        _ => text.to_string(),
    }
}

pub fn format_temp(c: Option<f64>, fahrenheit: bool) -> String {
    match c.filter(|v| v.is_finite()) {
        None => MISSING.into(),
        Some(c) if fahrenheit => format!("{:.0} °F", c * 9.0 / 5.0 + 32.0),
        Some(c) => format!("{c:.1} °C"),
    }
}

/// Network card: bits per second, e.g. "10.25 Mbps".
pub fn format_mbps(bytes_per_second: Option<f64>) -> String {
    match bytes_per_second.filter(|v| v.is_finite() && *v >= 0.0) {
        None => MISSING.into(),
        Some(b) => {
            let mbps = b * 8.0 / 1e6;
            if mbps >= 100.0 {
                format!("{mbps:.1} Mbps")
            } else {
                format!("{mbps:.2} Mbps")
            }
        }
    }
}

/// Process memory: "1.2 GiB" from 1 GiB up, else whole MiB.
pub fn proc_memory(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} GiB", b / (1024.0 * 1024.0 * 1024.0))
    } else {
        format!("{} MiB", (b / (1024.0 * 1024.0)).round())
    }
}

pub fn proc_cpu(cpu: Option<f64>) -> String {
    cpu.filter(|c| c.is_finite())
        .map_or(MISSING.into(), |c| format!("{c:.1}%"))
}

/// "NVIDIA GeForce …": adds the vendor brand when the name lacks it.
fn labeled_gpu_name(name: &str, vendor: &str) -> String {
    let brand = match vendor.to_ascii_lowercase().as_str() {
        "nvidia" => "NVIDIA",
        "amd" => "AMD",
        "intel" => "Intel",
        _ => return name.to_string(),
    };
    if name == MISSING
        || name
            .to_ascii_lowercase()
            .contains(&brand.to_ascii_lowercase())
    {
        name.to_string()
    } else {
        format!("{brand} {name}")
    }
}

/// Free space of a used/total pair: total = used / percent.
fn free_text(m: &Metric) -> String {
    match (m.raw_value, m.percent) {
        (Some(used), Some(p)) if p > 0.0 => format_bytes(Some(used * (100.0 / p - 1.0))),
        _ => MISSING.into(),
    }
}

fn canvas_path(ui: &App, values: &[Option<f64>], max: f64, dir: Dir) -> SharedString {
    let w = ui.global::<ChartCanvas>().get_width();
    bar_path(values, max, dir, Layout::Fixed, w, 100.0).into()
}

pub fn apply(ui: &App, s: &Snapshot, settings: &Settings, view: &mut View) {
    let d = ui.global::<Dash>();
    let t = ui.global::<Theme>();
    let f = settings.temp_unit == "f";

    // CPU
    d.set_cpu_percent(percent(&s.cpu));
    d.set_cpu_usage(ss(&s.cpu.display_value));
    d.set_cpu_temp(temp_text(&s.cpu_temp_text, f).into());
    d.set_cpu_speed(ss(&s.cpu_freq_text));
    sync(
        d.get_cores(),
        |m| d.set_cores(m),
        s.cpu_per_core
            .iter()
            .map(|v| v.filter(|x| x.is_finite()).map_or(-1.0, |x| x as f32))
            .collect(),
    );
    let h = &s.history;
    d.set_cpu_path(canvas_path(ui, &h.cpu, 100.0, Dir::Full));

    // GPU
    view.gpu_values = s.gpus.iter().map(|g| g.index).collect();
    sync(
        d.get_gpu_options(),
        |m| d.set_gpu_options(m),
        s.gpus
            .iter()
            .map(|g| SharedString::from(format!("GPU{}", g.index)))
            .collect(),
    );
    d.set_gpu_index(
        view.gpu_values
            .iter()
            .position(|&i| i == s.gpu_index)
            .unwrap_or(0) as i32,
    );
    d.set_gpu_percent(percent(&s.gpu));
    d.set_gpu_usage(ss(&s.gpu.display_value));
    d.set_gpu_temp(temp_text(&s.gpu_temp_text, f).into());
    d.set_gpu_clock(ss(&s.gpu_clock_text));
    d.set_gpu_name(labeled_gpu_name(&s.gpu_name, &s.gpu_vendor).into());
    d.set_gpu_vram(pair_text(&s.vram).into());
    d.set_gpu_vram_fraction(s.vram.percent.map_or(-1.0, |p| (p / 100.0) as f32));
    d.set_encoder(ss(&s.encoder.display_value));
    d.set_decoder(ss(&s.decoder.display_value));
    let gpu_temp = temp_text(&s.gpu_temp_text, f);
    sync(
        d.get_gpu_kv(),
        |m| d.set_gpu_kv(m),
        vec![
            kv("Temp", &gpu_temp),
            kv("Encode", &s.encoder.display_value),
            kv("Decode", &s.decoder.display_value),
        ],
    );

    // Memory
    d.set_ram_percent(percent(&s.ram));
    d.set_ram_usage(ss(&s.ram.display_value));
    d.set_ram_total(ss(&s.ram_subtitle));
    d.set_ram_speed(ss(&s.ram_speed_text));
    d.set_mem_pressure(
        s.mem_pressure
            .filter(|p| p.is_finite())
            .map_or(MISSING.into(), |p| format!("{p:.2}%"))
            .into(),
    );
    sync(
        d.get_memory_kv(),
        |m| d.set_memory_kv(m),
        vec![
            kv("Total", &s.ram_subtitle),
            kv("Speed", &s.ram_speed_text),
            kv("Pressure", d.get_mem_pressure()),
        ],
    );
    let b = &s.ram_breakdown;
    let parts = [&b.app, &b.cache, &b.free, &b.other];
    let colors = [
        t.get_ram(),
        t.get_gpu(),
        t.get_net_up(),
        t.get_text_tertiary(),
    ];
    sync(
        d.get_mem_parts(),
        |m| d.set_mem_parts(m),
        stack(&parts.map(|m| m.percent), 100.0)
            .into_iter()
            .zip(colors)
            .map(|((start, width), color)| StackPart {
                start,
                width,
                color,
            })
            .collect(),
    );
    sync(
        d.get_mem_legend(),
        |m| d.set_mem_legend(m),
        ["Used", "Cached", "Free", "Other"]
            .iter()
            .zip(parts)
            .zip(colors)
            .map(|((label, m), color)| LegendItem {
                label: (*label).into(),
                value: ss(&m.used_text),
                color,
            })
            .collect(),
    );
    // Pressure is usually near 0: scale to the recent peak (at least 10 %).
    let pressure_max = h
        .mem_pressure
        .iter()
        .flatten()
        .filter(|v| v.is_finite())
        .fold(10.0_f64, |m, &v| m.max(v));
    d.set_pressure_path(canvas_path(ui, &h.mem_pressure, pressure_max, Dir::Full));

    // Storage pager and disks
    d.set_disk_count(s.disks.len() as i32);
    view.sum_disk = view.sum_disk.min(s.disks.len().saturating_sub(1));
    match s.disks.get(view.sum_disk) {
        Some(disk) => {
            let name = [disk.name.to_uppercase(), disk.model.clone()]
                .into_iter()
                .filter(|v| !v.is_empty() && v != MISSING)
                .collect::<Vec<_>>()
                .join(" · ");
            d.set_sum_disk_name(name.into());
            let usage = s.disk_usage.get(&disk.name);
            sync(
                d.get_sum_disk_kv(),
                |m| d.set_sum_disk_kv(m),
                vec![
                    kv("Total", usage.map_or(MISSING, |u| &u.total_text)),
                    kv("Used", usage.map_or(MISSING, |u| &u.used_text)),
                    kv("Free", usage.map_or(MISSING.into(), free_text)),
                ],
            );
        }
        None => {
            d.set_sum_disk_name("No disk detected".into());
            sync(
                d.get_sum_disk_kv(),
                |m| d.set_sum_disk_kv(m),
                vec![
                    kv("Total", MISSING),
                    kv("Used", MISSING),
                    kv("Free", MISSING),
                ],
            );
        }
    }
    sync(
        d.get_disks(),
        |m| d.set_disks(m),
        s.disks
            .iter()
            .map(|disk| {
                let rate = s.disk_rates.get(&disk.name);
                let usage = s.disk_usage.get(&disk.name);
                let text = |v: &str| {
                    if v.is_empty() {
                        MISSING.to_string()
                    } else {
                        v.to_string()
                    }
                };
                DiskRow {
                    name: ss(&disk.name),
                    model: [
                        text(&disk.model),
                        text(&disk.size_text),
                        text(&disk.transport).to_uppercase(),
                    ]
                    .join(" · ")
                    .into(),
                    read: format_rate(rate.and_then(|r| r.read)).into(),
                    write: format_rate(rate.and_then(|r| r.write)).into(),
                    usage: usage
                        .and_then(|u| u.percent)
                        .map_or(-1.0, |p| (p / 100.0) as f32),
                    usage_text: usage.map_or("未掛載".to_string(), pair_text).into(),
                }
            })
            .collect(),
    );
    // Paired series share one scale; the label shows the peak without headroom.
    let disk_max = pair_max(&h.disk_read, &h.disk_write);
    d.set_disk_max(format_rate(Some(disk_max / 1.12)).into());
    d.set_disk_read_path(canvas_path(ui, &h.disk_read, disk_max, Dir::Up));
    d.set_disk_write_path(canvas_path(ui, &h.disk_write, disk_max, Dir::Down));
    let last = |v: &[Option<f64>]| v.last().copied().flatten();
    d.set_disk_read_now(format_rate(last(&h.disk_read)).into());
    d.set_disk_write_now(format_rate(last(&h.disk_write)).into());

    // Network
    view.net_values = std::iter::once("all".to_string())
        .chain(s.net_interfaces.iter().cloned())
        .collect();
    // A saved interface that is currently missing (unplugged) stays selectable.
    if !view.net_values.contains(&settings.net_interface) {
        view.net_values.push(settings.net_interface.clone());
    }
    sync(
        d.get_net_options(),
        |m| d.set_net_options(m),
        view.net_values
            .iter()
            .map(|v| SharedString::from(v.as_str()))
            .collect(),
    );
    d.set_net_index(
        view.net_values
            .iter()
            .position(|v| *v == settings.net_interface)
            .unwrap_or(0) as i32,
    );
    d.set_net_ip(ss(&s.net_ip));
    d.set_net_mac(ss(&s.net_mac));
    d.set_net_gateway(ss(&s.net_gateway));
    d.set_net_gateway_mac(ss(&s.net_gateway_mac));
    let net_max = pair_max(&h.net_up, &h.net_down);
    d.set_net_up_path(canvas_path(ui, &h.net_up, net_max, Dir::Up));
    d.set_net_down_path(canvas_path(ui, &h.net_down, net_max, Dir::Down));
    d.set_net_up_mbps(format_mbps(s.net_up.raw_value).into());
    d.set_net_down_mbps(format_mbps(s.net_down.raw_value).into());

    // Sensors (first 8, "chip · label")
    d.set_temp_unit(i32::from(f));
    sync(
        d.get_temps_kv(),
        |m| d.set_temps_kv(m),
        vec![
            kv("CPU", temp_text(&s.cpu_temp_text, f)),
            kv("GPU", &gpu_temp),
        ],
    );
    sync(
        d.get_sensors_short(),
        |m| d.set_sensors_short(m),
        s.sensors
            .iter()
            .take(8)
            .map(|x| KeyValue {
                key: format!("{} · {}", x.chip, x.label).into(),
                value: match x.kind {
                    SensorKind::Fan => x
                        .value
                        .filter(|v| v.is_finite())
                        .map_or(MISSING.into(), |v| format!("{} RPM", v.round())),
                    SensorKind::Temp => format_temp(x.value, f),
                }
                .into(),
            })
            .collect(),
    );

    // Battery
    d.set_has_battery(s.battery.is_some());
    if let Some(bat) = &s.battery {
        let p = bat.percent.filter(|v| v.is_finite());
        d.set_battery_percent(
            p.map_or(MISSING.into(), |v| format!("{}%", v.round()))
                .into(),
        );
        d.set_battery_fraction(p.map_or(-1.0, |v| (v / 100.0) as f32));
        let status = match bat.status.as_str() {
            "charging" => "Charging",
            "discharging" => "On battery",
            "full" => "Full",
            "notCharging" => "Not charging",
            _ => "Unknown",
        };
        d.set_battery_status(status.into());
        let prefix = match bat.status.as_str() {
            "charging" => Some("Full in "),
            "discharging" => Some("Remaining "),
            _ => None,
        };
        d.set_battery_time(
            prefix
                .filter(|_| bat.time_text != MISSING)
                .map_or(MISSING.into(), |p| format!("{p}{}", bat.time_text))
                .into(),
        );
        sync(
            d.get_battery_kv(),
            |m| d.set_battery_kv(m),
            vec![
                kv(
                    "Health",
                    bat.health
                        .filter(|v| v.is_finite())
                        .map_or(MISSING.into(), |v| format!("{v:.0}%")),
                ),
                kv(
                    "Cycles",
                    bat.cycles.map_or(MISSING.into(), |c| c.to_string()),
                ),
                kv("Current", &bat.now_text),
                kv("Full", &bat.full_text),
                kv("Design", &bat.design_text),
            ],
        );
    }
}

/// Top 5 by CPU and by memory (CPU missing on the first scan sorts last).
pub fn apply_processes(ui: &App, list: &[ProcessInfo]) {
    let d = ui.global::<Dash>();
    let mut by_cpu: Vec<&ProcessInfo> = list.iter().collect();
    by_cpu.sort_by(|a, b| b.cpu.unwrap_or(-1.0).total_cmp(&a.cpu.unwrap_or(-1.0)));
    sync(
        d.get_top_cpu(),
        |m| d.set_top_cpu(m),
        by_cpu
            .iter()
            .take(5)
            .map(|p| kv(&p.name, proc_cpu(p.cpu)))
            .collect(),
    );
    let mut by_mem = by_cpu;
    by_mem.sort_by_key(|p| std::cmp::Reverse(p.memory));
    sync(
        d.get_top_memory(),
        |m| d.set_top_memory(m),
        by_mem
            .iter()
            .take(5)
            .map(|p| kv(&p.name, proc_memory(p.memory)))
            .collect(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_formats_match_the_frontend() {
        assert_eq!(temp_text("48.6 °C", true), "119 °F");
        assert_eq!(temp_text("48.6 °C", false), "48.6 °C");
        assert_eq!(temp_text("--", true), "--");
        assert_eq!(format_temp(Some(39.94), false), "39.9 °C");
        assert_eq!(format_temp(Some(f64::NAN), true), "--");
        assert_eq!(format_mbps(Some(1_281_250.0)), "10.25 Mbps");
        assert_eq!(format_mbps(Some(15e6)), "120.0 Mbps");
        assert_eq!(format_mbps(None), "--");
        assert_eq!(proc_memory(3 * 1024 * 1024 * 1024 / 2), "1.5 GiB");
        assert_eq!(proc_memory(200 * 1024 * 1024 + 600 * 1024), "201 MiB");
        assert_eq!(proc_cpu(None), "--");
        assert_eq!(
            labeled_gpu_name("GeForce GTX 1060", "nvidia"),
            "NVIDIA GeForce GTX 1060"
        );
        assert_eq!(
            labeled_gpu_name("NVIDIA GeForce", "nvidia"),
            "NVIDIA GeForce"
        );
        assert_eq!(labeled_gpu_name("--", "amd"), "--");
    }

    #[test]
    fn free_space_from_used_and_percent() {
        let m = Metric::pair(
            Some(25.0 * 1024.0 * 1024.0 * 1024.0),
            Some(100.0 * 1024.0 * 1024.0 * 1024.0),
        );
        assert_eq!(free_text(&m), "75.0 GiB");
        assert_eq!(free_text(&Metric::pair(None, Some(1.0))), "--");
    }
}
