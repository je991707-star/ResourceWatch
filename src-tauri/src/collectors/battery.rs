//! Laptop battery from `/sys/class/power_supply/*` (`type` = `Battery`;
//! peripheral batteries with `scope` = `Device` — mice, headsets — are
//! skipped). The path is found at startup / hardware refresh; each sample
//! reads a few small attribute files.

use super::{list_dir, read_trimmed, read_u64, Roots};
use std::path::{Path, PathBuf};

pub fn discover(roots: &Roots) -> Option<PathBuf> {
    list_dir(&roots.sys("class/power_supply"), |_| true)
        .into_iter()
        .find(|d| {
            read_trimmed(&d.join("type")).as_deref() == Some("Battery")
                && read_trimmed(&d.join("scope")).as_deref() != Some("Device")
        })
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct BatteryReading {
    /// 0–100
    pub percent: Option<f64>,
    /// `charging` | `discharging` | `full` | `notCharging` | `unknown`
    pub status: &'static str,
    pub cycles: Option<u32>,
    /// Remaining / full / design capacity in Wh (from µWh), or Ah (from µAh)
    /// when the driver only reports charge; see `unit`.
    pub now: Option<f64>,
    pub full: Option<f64>,
    pub design: Option<f64>,
    pub unit: &'static str,
    /// Time to empty (discharging) or to full (charging), seconds.
    pub time_seconds: Option<f64>,
}

fn micro(dir: &Path, name: &str) -> Option<f64> {
    read_u64(&dir.join(name)).map(|v| v as f64 / 1_000_000.0)
}

pub fn read(dir: &Path) -> BatteryReading {
    let status = match read_trimmed(&dir.join("status")).as_deref() {
        Some("Charging") => "charging",
        Some("Discharging") => "discharging",
        Some("Full") => "full",
        Some("Not charging") => "notCharging",
        _ => "unknown",
    };
    // energy_* (µWh) with power_now (µW), else charge_* (µAh) with current_now (µA).
    let (prefix, rate, unit) = if dir.join("energy_full").exists() {
        ("energy", "power_now", "Wh")
    } else {
        ("charge", "current_now", "Ah")
    };
    let now = micro(dir, &format!("{prefix}_now"));
    let full = micro(dir, &format!("{prefix}_full")).filter(|v| *v > 0.0);
    let design = micro(dir, &format!("{prefix}_full_design")).filter(|v| *v > 0.0);
    let rate = micro(dir, rate).filter(|v| *v > 0.0);
    let hours = match (status, now, full, rate) {
        ("discharging", Some(n), _, Some(r)) => Some(n / r),
        ("charging", Some(n), Some(f), Some(r)) if f >= n => Some((f - n) / r),
        _ => None,
    };
    let percent = read_u64(&dir.join("capacity"))
        .filter(|p| *p <= 100)
        .map(|p| p as f64)
        .or_else(|| now.zip(full).map(|(n, f)| (n / f * 100.0).min(100.0)));
    BatteryReading {
        percent,
        status,
        // Many drivers report 0 when they do not track cycles.
        cycles: read_u64(&dir.join("cycle_count"))
            .filter(|c| *c > 0)
            .and_then(|c| u32::try_from(c).ok()),
        now,
        full,
        design,
        unit,
        // Guard against nonsense rates right after plugging in/out (> 4 days).
        time_seconds: hours
            .map(|h| h * 3600.0)
            .filter(|s| s.is_finite() && *s <= 96.0 * 3600.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::testutil::{roots, temp_root, write};

    #[test]
    fn energy_battery_discharging() {
        let r = temp_root("bat");
        write(&r, "sys/class/power_supply/AC/type", "Mains\n");
        write(
            &r,
            "sys/class/power_supply/hidpp_battery_0/type",
            "Battery\n",
        );
        write(
            &r,
            "sys/class/power_supply/hidpp_battery_0/scope",
            "Device\n",
        );
        let b = "sys/class/power_supply/BAT0";
        for (f, v) in [
            ("type", "Battery"),
            ("status", "Discharging"),
            ("capacity", "61"),
            ("cycle_count", "283"),
            ("energy_now", "38000000"),
            ("energy_full", "62000000"),
            ("energy_full_design", "65000000"),
            ("power_now", "16000000"),
        ] {
            write(&r, &format!("{b}/{f}"), &format!("{v}\n"));
        }
        let dir = discover(&roots(&r)).expect("BAT0, not the mouse");
        assert!(dir.ends_with("BAT0"));
        let x = read(&dir);
        assert_eq!(
            (x.percent, x.status, x.cycles),
            (Some(61.0), "discharging", Some(283))
        );
        assert_eq!(
            (x.now, x.full, x.design, x.unit),
            (Some(38.0), Some(62.0), Some(65.0), "Wh")
        );
        assert_eq!(x.time_seconds, Some(38.0 / 16.0 * 3600.0));
    }

    #[test]
    fn charge_battery_charging_and_missing_values() {
        let r = temp_root("bat-charge");
        let b = "sys/class/power_supply/BAT1";
        for (f, v) in [
            ("type", "Battery"),
            ("status", "Charging"),
            ("cycle_count", "0"),
            ("charge_now", "2000000"),
            ("charge_full", "4000000"),
            ("current_now", "1000000"),
        ] {
            write(&r, &format!("{b}/{f}"), &format!("{v}\n"));
        }
        let x = read(&discover(&roots(&r)).unwrap());
        assert_eq!(x.unit, "Ah");
        assert_eq!(
            x.percent,
            Some(50.0),
            "derived from charge when capacity is absent"
        );
        assert_eq!(x.cycles, None, "0 means not tracked");
        assert_eq!(x.design, None);
        assert_eq!(x.time_seconds, Some(2.0 * 3600.0));
        assert_eq!(discover(&roots(&temp_root("no-bat"))), None);
    }
}
