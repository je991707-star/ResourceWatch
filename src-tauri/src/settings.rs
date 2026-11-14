//! User settings: defaults, ranges and validation.
//!
//! Keys, defaults and ranges follow the legacy `main.xml` contract.
//! Phase 1: settings live in memory only; persistence is a Phase 3 task.

use serde::{Deserialize, Serialize};
use std::fmt;

pub const UPDATE_INTERVAL_MIN: u32 = 100;
pub const UPDATE_INTERVAL_MAX: u32 = 10_000;
pub const UPDATE_INTERVAL_STEP: u32 = 50;
pub const HISTORY_POINTS_MIN: u32 = 10;
pub const HISTORY_POINTS_MAX: u32 = 120;
pub const HISTORY_POINTS_STEP: u32 = 10;
pub const GPU_INDEX_MAX: u32 = 15;
/// Linux IFNAMSIZ (16) minus the terminating NUL.
pub const NET_INTERFACE_MAX_LEN: usize = 15;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// The single shared update interval (ms) for all live metrics,
    /// history sampling and snapshot events.
    pub update_interval: u32,
    pub history_points: u32,
    pub gpu_index: u32,
    /// `"all"` = aggregate of all interfaces.
    pub net_interface: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            update_interval: 500,
            history_points: 30,
            gpu_index: 0,
            net_interface: "all".to_string(),
        }
    }
}

/// Partial update sent by the frontend. Unknown keys are rejected.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    pub update_interval: Option<u32>,
    pub history_points: Option<u32>,
    pub gpu_index: Option<u32>,
    pub net_interface: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsError {
    pub field: &'static str,
    pub message: String,
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl std::error::Error for SettingsError {}

fn stepped(
    field: &'static str,
    value: u32,
    min: u32,
    max: u32,
    step: u32,
) -> Result<u32, SettingsError> {
    if value < min || value > max || (value - min) % step != 0 {
        return Err(SettingsError {
            field,
            message: format!("必須介於 {min}–{max}，步進 {step}（收到 {value}）"),
        });
    }
    Ok(value)
}

/// Accepts `"all"` (also for empty input) or a plausible Linux interface name.
/// The value is never passed to a shell.
pub fn normalize_net_interface(input: &str) -> Result<String, SettingsError> {
    let name = input.trim();
    if name.is_empty() || name == "all" {
        return Ok("all".to_string());
    }
    let ok_chars = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':' | '@'));
    if name.len() > NET_INTERFACE_MAX_LEN || !ok_chars || name == "." || name == ".." {
        return Err(SettingsError {
            field: "netInterface",
            message: format!(
                "介面名稱需為 1–{NET_INTERFACE_MAX_LEN} 個英數字或 - _ . : @，或填 all"
            ),
        });
    }
    Ok(name.to_string())
}

impl Settings {
    /// Returns a new validated settings value; `self` is unchanged on error.
    pub fn apply(&self, patch: &SettingsPatch) -> Result<Settings, SettingsError> {
        let mut next = self.clone();
        if let Some(v) = patch.update_interval {
            next.update_interval = stepped(
                "updateInterval",
                v,
                UPDATE_INTERVAL_MIN,
                UPDATE_INTERVAL_MAX,
                UPDATE_INTERVAL_STEP,
            )?;
        }
        if let Some(v) = patch.history_points {
            next.history_points = stepped(
                "historyPoints",
                v,
                HISTORY_POINTS_MIN,
                HISTORY_POINTS_MAX,
                HISTORY_POINTS_STEP,
            )?;
        }
        if let Some(v) = patch.gpu_index {
            next.gpu_index = stepped("gpuIndex", v, 0, GPU_INDEX_MAX, 1)?;
        }
        if let Some(v) = &patch.net_interface {
            next.net_interface = normalize_net_interface(v)?;
        }
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch_interval(v: u32) -> SettingsPatch {
        SettingsPatch {
            update_interval: Some(v),
            ..Default::default()
        }
    }

    #[test]
    fn defaults_match_legacy_main_xml() {
        let s = Settings::default();
        assert_eq!(s.update_interval, 500);
        assert_eq!(s.history_points, 30);
        assert_eq!(s.gpu_index, 0);
        assert_eq!(s.net_interface, "all");
    }

    #[test]
    fn update_interval_bounds_and_step() {
        let s = Settings::default();
        for ok in [100, 150, 500, 9950, 10_000] {
            assert_eq!(s.apply(&patch_interval(ok)).unwrap().update_interval, ok);
        }
        for bad in [0, 50, 99, 120, 501, 10_050, u32::MAX] {
            let err = s.apply(&patch_interval(bad)).unwrap_err();
            assert_eq!(err.field, "updateInterval");
        }
    }

    #[test]
    fn history_points_and_gpu_index() {
        let s = Settings::default();
        let p = SettingsPatch {
            history_points: Some(120),
            gpu_index: Some(15),
            ..Default::default()
        };
        let n = s.apply(&p).unwrap();
        assert_eq!((n.history_points, n.gpu_index), (120, 15));
        for bad in [0, 5, 15, 130] {
            let p = SettingsPatch {
                history_points: Some(bad),
                ..Default::default()
            };
            assert_eq!(s.apply(&p).unwrap_err().field, "historyPoints");
        }
        let p = SettingsPatch {
            gpu_index: Some(16),
            ..Default::default()
        };
        assert_eq!(s.apply(&p).unwrap_err().field, "gpuIndex");
    }

    #[test]
    fn failed_patch_leaves_settings_unchanged() {
        let s = Settings::default();
        let p = SettingsPatch {
            update_interval: Some(1000),
            history_points: Some(7),
            ..Default::default()
        };
        assert!(s.apply(&p).is_err());
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn net_interface_validation() {
        assert_eq!(normalize_net_interface("").unwrap(), "all");
        assert_eq!(normalize_net_interface("  all ").unwrap(), "all");
        assert_eq!(normalize_net_interface(" enp3s0 ").unwrap(), "enp3s0");
        assert_eq!(normalize_net_interface("wlp2s0.100").unwrap(), "wlp2s0.100");
        for bad in [
            "eth0; rm -rf /",
            "a b",
            "x/y",
            "$(id)",
            "..",
            "abcdefghijklmnop",
        ] {
            assert!(normalize_net_interface(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn patch_rejects_unknown_keys() {
        let r: Result<SettingsPatch, _> = serde_json::from_str(r#"{"updateIntervalCpu":500}"#);
        assert!(r.is_err());
        let r: Result<SettingsPatch, _> = serde_json::from_str(r#"{"updateInterval":500.5}"#);
        assert!(r.is_err());
    }
}
