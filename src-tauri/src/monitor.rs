//! Live monitor: owns the collectors, the startup-time static information and
//! the history, and turns readings into a [`Snapshot`].
//!
//! - every sample: CPU, memory, uptime, GPU, disk I/O, network
//! - startup and manual refresh only: CPU model, RAM speed, GPU list/name,
//!   disk list, IPv4

use crate::collectors::{
    cpu::{self, CpuCollector, CpuReading},
    disk::{self, DiskCollector, DiskDevice, DiskRateReading},
    gpu::{GpuCollector, GpuReading},
    memory::{self, MemReading},
    network::{self, NetCollector, NetReading},
    system, Roots,
};
use crate::format::{clamp_percent, format_bytes, MISSING};
use crate::history::{disk_totals, HistoryBuffer, HistorySample};
use crate::settings::Settings;
use crate::snapshot::{
    format_uptime, DataSource, DiskInfo, DiskRate, History, Metric, MetricStatus, RamBreakdown,
    Snapshot,
};
use crate::source::RefreshKind;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Like the legacy widget: missing values show "loading" for the first
/// seconds (rates need two samples), then "unavailable".
const GRACE: Duration = Duration::from_secs(5);

/// Filesystem usage changes slowly; statvfs every cycle would be wasted work.
const USAGE_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Default)]
struct Readings {
    cpu: CpuReading,
    mem: MemReading,
    gpu: GpuReading,
    disks: BTreeMap<String, DiskRateReading>,
    net: NetReading,
    uptime: Option<f64>,
    mem_pressure: Option<f64>,
}

pub struct Monitor {
    roots: Roots,
    started: Instant,
    cpu: CpuCollector,
    disk_io: DiskCollector,
    net: NetCollector,
    gpu: GpuCollector,
    net_interface: String,
    cpu_model: Option<String>,
    cpu_topology: (Option<u32>, Option<u32>),
    hostname: Option<String>,
    machine_model: Option<String>,
    ram_speed: Option<String>,
    disks: Vec<DiskDevice>,
    /// disk → (used, total) bytes of its mounted filesystems.
    disk_usage: BTreeMap<String, (u64, u64)>,
    usage_at: Instant,
    ip: Option<String>,
    last: Readings,
    history: HistoryBuffer,
}

// No extra finiteness check: collectors only produce finite values here
// (hwmon/NVML integers already range-checked, cpufreq is an average of
// positive integers), so `None` is the only "missing" case.
fn temp_text(c: Option<f64>, decimals: usize) -> String {
    c.map_or_else(|| MISSING.to_string(), |c| format!("{c:.decimals$} °C"))
}

fn text_or_missing(v: &Option<String>) -> String {
    v.clone().unwrap_or_else(|| MISSING.to_string())
}

fn freq_text(mhz: Option<f64>) -> String {
    mhz.map_or_else(|| MISSING.to_string(), |m| format!("{:.2} GHz", m / 1000.0))
}

impl Monitor {
    pub fn new(settings: &Settings) -> Self {
        Self::with_roots(Roots::default(), settings)
    }

    pub fn with_roots(roots: Roots, settings: &Settings) -> Self {
        Self {
            cpu: CpuCollector::new(&roots),
            disk_io: DiskCollector::new(&roots),
            net: NetCollector::new(&roots, &settings.net_interface),
            gpu: GpuCollector::new(&roots, settings.gpu_index),
            net_interface: settings.net_interface.clone(),
            cpu_model: cpu::cpu_model(&roots),
            cpu_topology: cpu::cpu_topology(&roots),
            hostname: system::hostname(&roots),
            machine_model: system::machine_model(&roots),
            ram_speed: system::ram_speed_text(&roots, system::ram_cache_path().as_deref()),
            disks: disk::discover(&roots),
            disk_usage: BTreeMap::new(),
            usage_at: Instant::now(),
            ip: network::ipv4_address(&settings.net_interface),
            started: Instant::now(),
            last: Readings::default(),
            history: HistoryBuffer::default(),
            roots,
        }
        .with_usage()
    }

    fn with_usage(mut self) -> Self {
        self.disk_usage = disk::usage(&self.roots, &self.disks);
        self
    }

    /// Reacts to `gpuIndex` / `netInterface` changes. Choosing another
    /// interface is an explicit user action, so its IPv4 is looked up once.
    pub fn apply_settings(&mut self, s: &Settings) {
        if s.gpu_index != self.gpu.index() {
            self.gpu.select(s.gpu_index);
            self.last.gpu = GpuReading::default();
        }
        if s.net_interface != self.net_interface {
            self.net_interface = s.net_interface.clone();
            self.net.select(&s.net_interface);
            self.last.net = NetReading::default();
            self.ip = network::ipv4_address(&s.net_interface);
        }
    }

    pub fn refresh(&mut self, kind: RefreshKind, s: &Settings) {
        match kind {
            RefreshKind::Ipv4 => self.ip = network::ipv4_address(&s.net_interface),
            RefreshKind::Disks => {
                self.disks = disk::discover(&self.roots);
                self.disk_io.reset();
                self.last.disks.clear();
                self.disk_usage = disk::usage(&self.roots, &self.disks);
                self.usage_at = Instant::now();
            }
            RefreshKind::Hardware => {
                self.cpu_model = cpu::cpu_model(&self.roots);
                self.cpu_topology = cpu::cpu_topology(&self.roots);
                self.hostname = system::hostname(&self.roots);
                self.machine_model = system::machine_model(&self.roots);
                self.ram_speed =
                    system::ram_speed_text(&self.roots, system::ram_cache_path().as_deref());
                self.gpu.rediscover();
            }
        }
    }

    /// One monitoring cycle: reads every per-interval collector with the
    /// cycle's shared timestamp `now`, records history, returns a snapshot.
    pub fn sample(&mut self, now: Instant, s: &Settings) -> Snapshot {
        if now.saturating_duration_since(self.usage_at) >= USAGE_INTERVAL {
            self.disk_usage = disk::usage(&self.roots, &self.disks);
            self.usage_at = now;
        }
        self.last = Readings {
            cpu: self.cpu.sample(),
            mem: memory::sample(&self.roots),
            gpu: self.gpu.sample(),
            disks: self.disk_io.sample_at(&self.disks, now),
            net: self.net.sample_at(now),
            uptime: system::uptime_seconds(&self.roots),
            mem_pressure: memory::pressure(&self.roots),
        };
        let mut snap = self.build(s);
        let (disk_read, disk_write) = disk_totals(snap.disk_rates.values());
        self.history.push(
            HistorySample {
                cpu: snap.cpu.percent,
                gpu: snap.gpu.percent,
                encode: snap.encoder.percent,
                decode: snap.decoder.percent,
                vram: snap.vram.percent,
                ram: snap.ram.percent,
                net_up: snap.net_up.raw_value,
                net_down: snap.net_down.raw_value,
                mem_pressure: snap.mem_pressure,
                disk_read,
                disk_write,
            },
            s.history_points,
        );
        // History is materialised once per cycle (build() leaves it empty).
        snap.history = self.history.to_history();
        snap
    }

    /// Snapshot of the last readings; performs no I/O on the collectors.
    pub fn peek(&self, s: &Settings) -> Snapshot {
        let mut snap = self.build(s);
        snap.history = self.history.to_history();
        snap
    }

    fn build(&self, s: &Settings) -> Snapshot {
        let grace = self.started.elapsed() < GRACE;
        let g = |m: Metric| {
            if grace && m.status == MetricStatus::Unavailable {
                Metric {
                    status: MetricStatus::Loading,
                    ..m
                }
            } else {
                m
            }
        };
        let r = &self.last;
        let ram = g(Metric::pair(r.mem.ram_used, r.mem.ram_total));
        let disk_rates = self
            .disks
            .iter()
            .map(|d| {
                let rate = r.disks.get(&d.name);
                (
                    d.name.clone(),
                    DiskRate {
                        read: rate.and_then(|x| x.read),
                        write: rate.and_then(|x| x.write),
                    },
                )
            })
            .collect();
        Snapshot {
            data_source: DataSource::Live,
            version: env!("CARGO_PKG_VERSION").to_string(),
            hostname: text_or_missing(&self.hostname),
            machine_model: text_or_missing(&self.machine_model),
            cpu_name: self
                .cpu_model
                .clone()
                .unwrap_or_else(|| MISSING.to_string()),
            cpu_cores: self.cpu_topology.0,
            cpu_threads: self.cpu_topology.1,
            cpu: g(Metric::percent(r.cpu.usage)),
            cpu_per_core: r.cpu.per_core.iter().map(|v| clamp_percent(*v)).collect(),
            cpu_freq_text: freq_text(r.cpu.freq_mhz),
            cpu_temp_text: temp_text(r.cpu.temp_c, 1),
            uptime_text: format_uptime(r.uptime),
            uptime_seconds: r.uptime,
            gpu_index: s.gpu_index,
            gpu_name: self.gpu.name().unwrap_or(MISSING).to_string(),
            gpu_vendor: self
                .gpu
                .vendor()
                .map_or("other", |v| v.as_str())
                .to_string(),
            gpu_source: self.gpu.source().to_string(),
            gpu: g(Metric::percent(r.gpu.usage)),
            gpu_temp_text: temp_text(r.gpu.temp_c, 0),
            vram: g(Metric::pair(r.gpu.vram_used, r.gpu.vram_total)),
            encoder: g(Metric::percent(r.gpu.encoder)),
            decoder: g(Metric::percent(r.gpu.decoder)),
            ram_subtitle: ram.total_text.clone(),
            ram,
            ram_speed_text: self
                .ram_speed
                .clone()
                .unwrap_or_else(|| MISSING.to_string()),
            ram_breakdown: RamBreakdown {
                app: g(Metric::pair(r.mem.ram_app, r.mem.ram_total)),
                cache: g(Metric::pair(r.mem.ram_cache, r.mem.ram_total)),
                free: g(Metric::pair(r.mem.ram_free, r.mem.ram_total)),
            },
            mem_pressure: r.mem_pressure,
            swap: g(Metric::pair(r.mem.swap_used, r.mem.swap_total)),
            disks: self
                .disks
                .iter()
                .map(|d| DiskInfo {
                    name: d.name.clone(),
                    model: d.model.clone().unwrap_or_else(|| MISSING.to_string()),
                    size_text: format_bytes(Some(d.size_bytes as f64)),
                    transport: d.transport.clone(),
                })
                .collect(),
            disk_rates,
            disk_usage: self
                .disks
                .iter()
                .filter_map(|d| {
                    let (used, total) = self.disk_usage.get(&d.name)?;
                    Some((
                        d.name.clone(),
                        Metric::pair(Some(*used as f64), Some(*total as f64)),
                    ))
                })
                .collect(),
            net_has_interface: s.net_interface != "all",
            net_ip_available: self.ip.is_some(),
            net_ip: self.ip.clone().unwrap_or_else(|| MISSING.to_string()),
            net_up: g(Metric::rate(r.net.up)),
            net_down: g(Metric::rate(r.net.down)),
            history: History::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::testutil::{roots, temp_root, write};

    #[test]
    fn empty_system_yields_missing_not_zero() {
        let r = temp_root("monitor-empty");
        let s = Settings::default();
        let mut m = Monitor::with_roots(roots(&r), &s);
        let snap = m.sample(Instant::now(), &s);
        assert_eq!(snap.data_source, DataSource::Live);
        assert_eq!(snap.cpu.percent, None);
        assert_eq!(
            snap.cpu.status,
            MetricStatus::Loading,
            "within grace period"
        );
        assert_eq!(snap.ram.display_value, "--");
        assert_eq!(snap.cpu_name, "--");
        assert_eq!(snap.gpu_source, "unavailable");
        assert!(snap.disks.is_empty());
        assert_eq!(snap.history.cpu, vec![None]);
    }

    #[test]
    fn fake_system_end_to_end() {
        let r = temp_root("monitor");
        write(&r, "proc/stat", "cpu  100 0 100 800 0 0 0 0 0 0\n");
        write(
            &r,
            "proc/meminfo",
            "MemTotal: 1000 kB\nMemAvailable: 250 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB\n",
        );
        write(&r, "proc/uptime", "100.5 1.0\n");
        write(&r, "proc/cpuinfo", "model name : Test CPU\n");
        write(&r, "proc/sys/kernel/hostname", "testhost\n");
        let s = Settings::default();
        let mut m = Monitor::with_roots(roots(&r), &s);
        m.sample(Instant::now(), &s);
        write(&r, "proc/stat", "cpu  200 0 100 900 0 0 0 0 0 0\n");
        let snap = m.sample(Instant::now(), &s);
        assert_eq!(snap.cpu.display_value, "50%");
        assert_eq!(snap.ram.display_value, "75%");
        assert_eq!(snap.cpu_name, "Test CPU");
        assert_eq!(snap.hostname, "testhost");
        assert_eq!(snap.machine_model, "--");
        assert_eq!(snap.uptime_seconds, Some(100.5));
        assert_eq!(snap.swap.percent, None, "no swap configured: no percentage");
        assert_eq!(snap.history.cpu, vec![None, Some(50.0)]);
        assert!(
            snap.cpu_per_core.is_empty(),
            "no cpuN lines in the fake stat"
        );
        // peek repeats the last readings without touching history.
        assert_eq!(m.peek(&s).history.cpu.len(), 2);
    }
}
