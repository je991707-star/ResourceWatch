//! Mock data source for UI development (`RESOURCEWATCH_MOCK=1`).
//!
//! Generates clearly labelled fake values (`dataSource: "mock"`, names prefixed
//! with "Mock"). It reads no hardware and starts no process, and must never be
//! shown as real data. The default source is the live [`crate::monitor::Monitor`].

use crate::collectors::sensors::SensorKind;
use crate::history::{disk_totals, HistoryBuffer, HistorySample};
use crate::settings::Settings;
use crate::snapshot::{
    format_uptime, DataSource, DiskInfo, DiskRate, GpuInfo, Metric, RamBreakdown, Sensor, Snapshot,
};
use crate::source::RefreshKind;
use std::collections::BTreeMap;
use std::time::Instant;

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
/// Documentation address (RFC 5737 TEST-NET-1): obviously not a real lease.
const MOCK_IPV4: &str = "192.0.2.10";

pub struct MockSource {
    started: Instant,
    tick: u64,
    history: HistoryBuffer,
    disks: Vec<DiskInfo>,
    /// Number of samples since the last disk scan; the first sample after a
    /// scan has no rate (same as the legacy counter-delta behaviour).
    samples_since_disk_scan: u64,
    ip: Option<String>,
}

impl Default for MockSource {
    fn default() -> Self {
        Self::new()
    }
}

impl MockSource {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            tick: 0,
            history: HistoryBuffer::default(),
            disks: mock_disks(),
            samples_since_disk_scan: 0,
            ip: Some(MOCK_IPV4.to_string()),
        }
    }

    /// Simulates the manual refresh actions. No hardware is queried.
    pub fn refresh(&mut self, kind: RefreshKind) {
        match kind {
            RefreshKind::Ipv4 => self.ip = Some(MOCK_IPV4.to_string()),
            RefreshKind::Disks => {
                self.disks = mock_disks();
                self.samples_since_disk_scan = 0;
            }
            RefreshKind::Hardware => {}
        }
    }

    /// Advances the mock clock by one sample and records history.
    pub fn sample(&mut self, settings: &Settings) -> Snapshot {
        self.tick += 1;
        self.samples_since_disk_scan += 1;
        self.build(settings, true)
    }

    /// Current snapshot without advancing time or history (for `get_snapshot`
    /// and refresh responses).
    pub fn peek(&mut self, settings: &Settings) -> Snapshot {
        self.build(settings, false)
    }

    fn build(&mut self, settings: &Settings, record: bool) -> Snapshot {
        let t = self.tick as f64;
        let cpu = 18.0 + 6.0 * (t * 0.5).sin() + 3.0 * (t * 0.17).sin();
        let gpu = 52.0 + 17.0 * (t * 0.25).cos();
        let enc = (12.0 + 10.0 * (t * 0.21).sin()).max(0.0);
        let dec = (6.0 + 5.0 * (t * 0.33).cos()).max(0.0);
        let vram_total = 2.0 * GIB;
        let vram_used = vram_total * (0.80 + 0.01 * (t * 0.07).sin());
        let ram_total = 15.5 * GIB;
        let ram_used = ram_total * (0.417 + 0.009 * (t * 0.12).sin());
        let up = (850_000.0 + 360_000.0 * (t * 0.4).sin()).max(0.0);
        let down = (24_000.0 + 15_000.0 * (t * 0.31).cos()).max(0.0);

        let ram_cache = ram_total * 0.31;
        let pressure = (1.5 + 1.5 * (t * 0.27).sin()).max(0.0);
        let cpu_m = Metric::percent(Some(cpu));
        let gpu_m = Metric::percent(Some(gpu));
        let enc_m = Metric::percent(Some(enc));
        let dec_m = Metric::percent(Some(dec));
        let vram_m = Metric::pair(Some(vram_used), Some(vram_total));
        let ram_m = Metric::pair(Some(ram_used), Some(ram_total));
        let net_up = Metric::rate(Some(up));
        let net_down = Metric::rate(Some(down));

        let mut disk_rates = BTreeMap::new();
        for (i, d) in self.disks.iter().enumerate() {
            let rate = if self.samples_since_disk_scan < 2 {
                DiskRate {
                    read: None,
                    write: None,
                }
            } else {
                let phase = i as f64 * 1.7;
                DiskRate {
                    read: Some((90_000.0 * (t * 0.3 + phase).sin()).max(0.0)),
                    write: Some((210_000.0 * (t * 0.2 + phase).cos()).max(0.0)),
                }
            };
            disk_rates.insert(d.name.clone(), rate);
        }

        let (disk_read, disk_write) = disk_totals(disk_rates.values());
        if record {
            self.history.push(
                HistorySample {
                    cpu: cpu_m.percent,
                    gpu: gpu_m.percent,
                    encode: enc_m.percent,
                    decode: dec_m.percent,
                    vram: vram_m.percent,
                    ram: ram_m.percent,
                    net_up: net_up.raw_value,
                    net_down: net_down.raw_value,
                    mem_pressure: Some(pressure),
                    disk_read,
                    disk_write,
                },
                settings.history_points,
            );
        }

        let uptime = 8990.0 + self.started.elapsed().as_secs_f64();
        let net_has_interface = settings.net_interface != "all";

        Snapshot {
            data_source: DataSource::Mock,
            version: env!("CARGO_PKG_VERSION").to_string(),
            hostname: "mock-host".to_string(),
            machine_model: "Mock Machine".to_string(),
            cpu_name: "Mock CPU（模擬資料）".to_string(),
            cpu_cores: Some(4),
            cpu_threads: Some(8),
            cpu: cpu_m,
            cpu_per_core: (0..8)
                .map(|i| Some((cpu + 14.0 * (t * 0.6 + i as f64).sin()).clamp(0.0, 100.0)))
                .collect(),
            cpu_freq_text: format!("{:.2} GHz", 3.0 + 0.4 * (t * 0.23).sin()),
            cpu_temp_text: format!("{:.1} °C", 46.0 + 3.0 * (t * 0.11).sin()),
            uptime_text: format_uptime(Some(uptime)),
            uptime_seconds: Some(uptime),
            gpu_index: settings.gpu_index,
            gpu_name: format!("Mock GPU {}", settings.gpu_index),
            gpu_vendor: "other".to_string(),
            gpu_source: "mock".to_string(),
            gpus: (0..2)
                .map(|i| GpuInfo {
                    index: i,
                    name: format!("Mock GPU {i}"),
                    vendor: "other".to_string(),
                })
                .collect(),
            gpu: gpu_m,
            gpu_temp_text: format!("{:.0} °C", 58.0 + 4.0 * (t * 0.09).sin()),
            vram: vram_m,
            encoder: enc_m,
            decoder: dec_m,
            ram_subtitle: ram_m.total_text.clone(),
            ram: ram_m,
            // Mock: RAM speed unknown, to exercise the missing-value path.
            ram_speed_text: "--".to_string(),
            ram_breakdown: RamBreakdown {
                app: Metric::pair(Some(ram_used * 0.9), Some(ram_total)),
                cache: Metric::pair(Some(ram_cache), Some(ram_total)),
                free: Metric::pair(
                    Some(ram_total - ram_used * 0.9 - ram_cache),
                    Some(ram_total),
                ),
            },
            mem_pressure: Some(pressure),
            swap: Metric::pair(Some(0.0), Some(7.8 * GIB)),
            disks: self.disks.clone(),
            disk_rates,
            disk_usage: self
                .disks
                .iter()
                .zip([0.62, 0.27])
                .map(|(d, share)| {
                    (
                        d.name.clone(),
                        Metric::pair(Some(share * 476.9 * GIB), Some(476.9 * GIB)),
                    )
                })
                .collect(),
            sensors: [
                (
                    "coretemp",
                    "Package id 0",
                    SensorKind::Temp,
                    49.0 + 3.0 * (t * 0.11).sin(),
                ),
                (
                    "coretemp",
                    "Core 0",
                    SensorKind::Temp,
                    47.0 + 3.0 * (t * 0.13).sin(),
                ),
                (
                    "coretemp",
                    "Core 1",
                    SensorKind::Temp,
                    46.0 + 3.0 * (t * 0.17).sin(),
                ),
                ("nvme", "Composite", SensorKind::Temp, 38.9),
                ("acpitz", "temp1", SensorKind::Temp, 27.8),
                (
                    "mock8686",
                    "CPU Fan",
                    SensorKind::Fan,
                    1200.0 + 80.0 * (t * 0.2).sin(),
                ),
                ("mock8686", "System Fan", SensorKind::Fan, 860.0),
            ]
            .into_iter()
            .map(|(chip, label, kind, v)| Sensor {
                chip: chip.to_string(),
                label: label.to_string(),
                kind,
                value: Some(v),
            })
            .collect(),
            net_has_interface,
            net_ip_available: self.ip.is_some(),
            net_ip: self.ip.clone().unwrap_or_else(|| "--".to_string()),
            net_interface_name: if net_has_interface {
                settings.net_interface.clone()
            } else {
                "mock0".to_string()
            },
            // Documentation-reserved values (RFC 7042 / RFC 5737).
            net_mac: "00:00:5e:00:53:01".to_string(),
            net_gateway: "192.0.2.1".to_string(),
            net_interfaces: vec!["mock0".to_string(), "mockwl0".to_string()],
            net_up,
            net_down,
            history: self.history.to_history(),
        }
    }
}

fn mock_disks() -> Vec<DiskInfo> {
    vec![
        DiskInfo {
            name: "mock-sda".to_string(),
            model: "Mock SATA Disk".to_string(),
            size_text: crate::format::format_bytes(Some(2_000_398_934_016.0)),
            transport: "sata".to_string(),
        },
        DiskInfo {
            name: "mock-nvme0n1".to_string(),
            model: "Mock NVMe Disk".to_string(),
            size_text: crate::format::format_bytes(Some(512_110_190_592.0)),
            transport: "nvme".to_string(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_is_labelled_and_history_bounded() {
        let mut m = MockSource::new();
        let s = Settings {
            history_points: 10,
            ..Settings::default()
        };
        let mut snap = m.sample(&s);
        for _ in 0..25 {
            snap = m.sample(&s);
        }
        assert_eq!(snap.data_source, DataSource::Mock);
        assert!(snap.cpu_name.contains("Mock"));
        assert_eq!(snap.history.cpu.len(), 10);
        assert_eq!(snap.history.net_down.len(), 10);
        let v = serde_json::to_value(&snap).unwrap();
        assert_eq!(v["dataSource"], "mock");
    }

    #[test]
    fn peek_does_not_advance_history() {
        let mut m = MockSource::new();
        let s = Settings::default();
        m.sample(&s);
        let a = m.peek(&s).history.cpu.len();
        let b = m.peek(&s).history.cpu.len();
        assert_eq!((a, b), (1, 1));
    }

    #[test]
    fn disk_rescan_yields_missing_rate_first() {
        let mut m = MockSource::new();
        let s = Settings::default();
        m.sample(&s);
        m.sample(&s);
        assert!(m.peek(&s).disk_rates["mock-sda"].read.is_some());
        m.refresh(RefreshKind::Disks);
        let first = m.sample(&s);
        assert_eq!(first.disk_rates["mock-sda"].read, None);
        assert!(m.sample(&s).disk_rates["mock-sda"].read.is_some());
    }

    #[test]
    fn shrinking_history_points_truncates() {
        let mut m = MockSource::new();
        let mut s = Settings {
            history_points: 30,
            ..Settings::default()
        };
        for _ in 0..30 {
            m.sample(&s);
        }
        s.history_points = 10;
        assert_eq!(m.sample(&s).history.gpu.len(), 10);
    }
}
