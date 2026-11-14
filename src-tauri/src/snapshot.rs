//! Snapshot sent to the frontend (`get_snapshot` command / `snapshot` event).
//!
//! Field names follow the legacy v0.0.5 HTML snapshot JSON contract
//! (see `Readme_Architecture.md`), plus `dataSource`, which tells the UI
//! whether values are mock data or real measurements.

use crate::format::{self, MISSING};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DataSource {
    /// Read from this machine by the Rust collectors.
    Live,
    /// Generated values for UI development. Never real hardware readings.
    Mock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricStatus {
    Ready,
    /// Part of the legacy contract (startup grace period). Mock data never
    /// produces it; real collectors may.
    #[allow(dead_code)]
    Loading,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metric {
    pub display_value: String,
    pub percent: Option<f64>,
    pub raw_value: Option<f64>,
    pub status: MetricStatus,
    pub used_text: String,
    pub total_text: String,
}

impl Metric {
    fn base(status: MetricStatus) -> Self {
        Self {
            display_value: MISSING.to_string(),
            percent: None,
            raw_value: None,
            status,
            used_text: MISSING.to_string(),
            total_text: MISSING.to_string(),
        }
    }

    fn status_of(valid: bool) -> MetricStatus {
        if valid {
            MetricStatus::Ready
        } else {
            MetricStatus::Unavailable
        }
    }

    /// Percentage metric (CPU, GPU, NVENC, NVDEC).
    pub fn percent(value: Option<f64>) -> Self {
        let v = format::valid(value);
        Self {
            display_value: format::format_percent(v),
            percent: format::clamp_percent(v),
            raw_value: v,
            ..Self::base(Self::status_of(v.is_some()))
        }
    }

    /// Used / total pair in bytes (RAM, VRAM, Swap). No percentage when total is
    /// missing or zero; used/total texts are still shown when individually valid.
    pub fn pair(used: Option<f64>, total: Option<f64>) -> Self {
        let p = format::percent_of(used, total);
        let used = format::valid(used).filter(|u| *u >= 0.0);
        let total = format::valid(total).filter(|t| *t > 0.0);
        Self {
            display_value: format::format_percent(p),
            percent: format::clamp_percent(p),
            raw_value: p.and(used),
            status: Self::status_of(p.is_some()),
            used_text: format::format_bytes(used),
            total_text: format::format_bytes(total),
        }
    }

    /// Byte rate in B/s (network up/down).
    pub fn rate(bytes_per_second: Option<f64>) -> Self {
        let v = format::valid_rate(bytes_per_second);
        Self {
            display_value: format::format_rate(v),
            raw_value: v,
            ..Self::base(Self::status_of(v.is_some()))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub name: String,
    pub model: String,
    pub size_text: String,
    pub transport: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct DiskRate {
    pub read: Option<f64>,
    pub write: Option<f64>,
}

/// History series; missing samples stay `null`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    pub cpu: Vec<Option<f64>>,
    pub gpu: Vec<Option<f64>>,
    pub encode: Vec<Option<f64>>,
    pub decode: Vec<Option<f64>>,
    pub vram: Vec<Option<f64>>,
    pub ram: Vec<Option<f64>>,
    pub net_up: Vec<Option<f64>>,
    pub net_down: Vec<Option<f64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub data_source: DataSource,
    pub version: String,
    /// Read at startup and on hardware refresh; `"--"` when unknown.
    pub hostname: String,
    pub machine_model: String,
    pub cpu_name: String,
    /// Physical cores / logical CPUs; `null` when unknown.
    pub cpu_cores: Option<u32>,
    pub cpu_threads: Option<u32>,
    pub cpu: Metric,
    pub cpu_freq_text: String,
    pub cpu_temp_text: String,
    pub uptime_text: String,
    pub uptime_seconds: Option<f64>,
    pub gpu_index: u32,
    pub gpu_name: String,
    /// `nvidia` | `amd` | `intel` | `other`
    pub gpu_vendor: String,
    /// Tauri: `nvml` | `drm` | `unavailable` | `mock`
    /// (legacy widget: `nvml` | `nvidia-smi` | `ksystemstats` | `unavailable`)
    pub gpu_source: String,
    pub gpu: Metric,
    pub gpu_temp_text: String,
    pub vram: Metric,
    pub encoder: Metric,
    pub decoder: Metric,
    pub ram: Metric,
    pub ram_subtitle: String,
    pub ram_speed_text: String,
    pub swap: Metric,
    pub disks: Vec<DiskInfo>,
    pub disk_rates: BTreeMap<String, DiskRate>,
    pub net_has_interface: bool,
    pub net_ip_available: bool,
    pub net_ip: String,
    pub net_up: Metric,
    pub net_down: Metric,
    pub history: History,
}

/// Formats seconds as `H:MM:SS` / `N天 HH:MM:SS` (legacy app.js `uptimeFromSeconds`).
pub fn format_uptime(seconds: Option<f64>) -> String {
    let Some(s) = format::valid(seconds).filter(|s| *s >= 0.0) else {
        return MISSING.to_string();
    };
    let total = s.floor() as u64;
    let (days, h, m, sec) = (
        total / 86_400,
        total % 86_400 / 3600,
        total % 3600 / 60,
        total % 60,
    );
    if days > 0 {
        format!("{days}天 {h:02}:{m:02}:{sec:02}")
    } else {
        format!("{h:02}:{m:02}:{sec:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_values_never_become_zero() {
        let m = Metric::percent(None);
        assert_eq!(m.display_value, "--");
        assert_eq!(m.percent, None);
        assert_eq!(m.status, MetricStatus::Unavailable);

        let p = Metric::pair(Some(1024.0), Some(0.0));
        assert_eq!(p.percent, None);
        assert_eq!(p.display_value, "--");
        assert_eq!(p.used_text, "1.0 KiB");
        assert_eq!(p.total_text, "--");

        assert_eq!(Metric::rate(Some(-1.0)).raw_value, None);
    }

    #[test]
    fn pair_metric_values() {
        let gib = 1024.0 * 1024.0 * 1024.0;
        let m = Metric::pair(Some(6.5 * gib), Some(15.5 * gib));
        assert_eq!(m.display_value, "41.9%");
        assert_eq!(m.used_text, "6.5 GiB");
        assert_eq!(m.total_text, "15.5 GiB");
        assert_eq!(m.status, MetricStatus::Ready);
    }

    #[test]
    fn json_uses_contract_field_names() {
        let v = serde_json::to_value(Metric::percent(Some(12.0))).unwrap();
        for key in [
            "displayValue",
            "percent",
            "rawValue",
            "status",
            "usedText",
            "totalText",
        ] {
            assert!(v.get(key).is_some(), "missing {key}");
        }
        assert_eq!(v["status"], "ready");
        let h = serde_json::to_value(History::default()).unwrap();
        assert!(h.get("netUp").is_some() && h.get("netDown").is_some());
    }

    #[test]
    fn uptime_format() {
        assert_eq!(format_uptime(Some(8990.0)), "02:29:50");
        assert_eq!(format_uptime(Some(90_061.9)), "1天 01:01:01");
        assert_eq!(format_uptime(None), "--");
        assert_eq!(format_uptime(Some(-1.0)), "--");
    }
}
