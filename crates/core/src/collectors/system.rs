//! Uptime (`/proc/uptime`) and RAM module speed (SMBIOS type 17 or the
//! user cache written by the legacy `hardware_info.py --cache-ram-from-stdin`).

use super::Roots;
use std::path::{Path, PathBuf};

/// Seconds since boot including suspend (same clock as CLOCK_BOOTTIME).
pub fn uptime_seconds(roots: &Roots) -> Option<f64> {
    let text = std::fs::read_to_string(roots.proc("uptime")).ok()?;
    let v: f64 = text.split_whitespace().next()?.parse().ok()?;
    (v.is_finite() && v >= 0.0).then_some(v)
}

/// Configured (else rated) speed in MT/s of an installed DIMM from one raw
/// SMBIOS type 17 structure. Port of legacy `parse_smbios_type17`.
/// Kernel host name (`/proc/sys/kernel/hostname`).
pub fn hostname(roots: &Roots) -> Option<String> {
    super::read_trimmed(&roots.proc("sys/kernel/hostname"))
}

/// Firmware filler that vendors leave in DMI fields.
fn is_placeholder(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    [
        "to be filled",
        "system product name",
        "default string",
        "not applicable",
        "not specified",
        "none",
        "o.e.m.",
    ]
    .iter()
    .any(|p| l.contains(p))
}

/// Machine model from DMI (`product_name`, else `board_name`), readable
/// without root; firmware placeholders are treated as missing.
pub fn machine_model(roots: &Roots) -> Option<String> {
    ["product_name", "board_name"].iter().find_map(|f| {
        super::read_trimmed(&roots.sys(&format!("class/dmi/id/{f}"))).filter(|v| !is_placeholder(v))
    })
}

pub fn parse_smbios_type17(blob: &[u8]) -> Option<u32> {
    if blob.len() < 0x17 || blob[0] != 17 {
        return None;
    }
    let size = u16::from_le_bytes([blob[0x0c], blob[0x0d]]);
    if size == 0 || size == 0xffff {
        return None; // empty or unknown slot
    }
    let len = usize::from(blob[1]).min(blob.len());
    let word = |at: usize| {
        (len >= at + 2).then(|| u32::from(u16::from_le_bytes([blob[at], blob[at + 1]])))
    };
    let rated = word(0x15).unwrap_or(0);
    let configured = word(0x20).unwrap_or(0);
    [configured, rated]
        .into_iter()
        .find(|s| (100..=10_000).contains(s))
}

/// Usually root-only on Fedora/Bazzite; unreadable entries are skipped.
pub fn read_smbios_speeds(roots: &Roots) -> Vec<u32> {
    super::list_dir(&roots.sys("firmware/dmi/entries"), |n| n.starts_with("17-"))
        .iter()
        .filter_map(|d| std::fs::read(d.join("raw")).ok())
        .filter_map(|b| parse_smbios_type17(&b))
        .collect()
}

/// `${XDG_STATE_HOME:-$HOME/.local/state}/resourcewatch/ram_speed.json`
pub fn ram_cache_path() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(state.join("resourcewatch/ram_speed.json"))
}

/// Cache format: `{"speedsMTs": [1600, 1600]}`.
pub fn read_ram_cache(path: &Path) -> Vec<u32> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    value["speedsMTs"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_u64())
                .filter(|v| (100..=10_000).contains(v))
                .map(|v| v as u32)
                .collect()
        })
        .unwrap_or_default()
}

/// `1600 MT/s`, or `2133–3200 MT/s` for mixed modules; `None` if nothing valid.
pub fn format_speeds(speeds: &[u32]) -> Option<String> {
    let mut v: Vec<u32> = speeds
        .iter()
        .copied()
        .filter(|s| (100..=10_000).contains(s))
        .collect();
    v.sort_unstable();
    v.dedup();
    match v.as_slice() {
        [] => None,
        [one] => Some(format!("{one} MT/s")),
        [lo, .., hi] => Some(format!("{lo}–{hi} MT/s")),
    }
}

/// SMBIOS first, then the user cache. No root access is requested.
pub fn ram_speed_text(roots: &Roots, cache: Option<&Path>) -> Option<String> {
    let speeds = read_smbios_speeds(roots);
    if let Some(t) = format_speeds(&speeds) {
        return Some(t);
    }
    format_speeds(&read_ram_cache(cache?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::testutil::{roots, temp_root, write};

    fn dimm(size: u16, rated: u16, configured: u16) -> Vec<u8> {
        let mut b = vec![0u8; 0x28];
        b[0] = 17;
        b[1] = 0x28;
        b[0x0c..0x0e].copy_from_slice(&size.to_le_bytes());
        b[0x15..0x17].copy_from_slice(&rated.to_le_bytes());
        b[0x20..0x22].copy_from_slice(&configured.to_le_bytes());
        b
    }

    #[test]
    fn hostname_and_machine_model() {
        let r = temp_root("dmi");
        write(&r, "proc/sys/kernel/hostname", "H97M\n");
        write(
            &r,
            "sys/class/dmi/id/product_name",
            "To be filled by O.E.M.\n",
        );
        write(&r, "sys/class/dmi/id/board_name", "H97M-D3H\n");
        let rt = roots(&r);
        assert_eq!(hostname(&rt).as_deref(), Some("H97M"));
        assert_eq!(
            machine_model(&rt).as_deref(),
            Some("H97M-D3H"),
            "placeholder skipped"
        );
        write(&r, "sys/class/dmi/id/product_name", "ThinkPad T14\n");
        assert_eq!(machine_model(&rt).as_deref(), Some("ThinkPad T14"));
        let empty = roots(&temp_root("dmi-none"));
        assert_eq!((hostname(&empty), machine_model(&empty)), (None, None));
    }

    #[test]
    fn smbios_type17() {
        assert_eq!(parse_smbios_type17(&dimm(8192, 1600, 1333)), Some(1333));
        assert_eq!(parse_smbios_type17(&dimm(8192, 1600, 0)), Some(1600));
        assert_eq!(
            parse_smbios_type17(&dimm(0, 1600, 1600)),
            None,
            "empty slot"
        );
        assert_eq!(parse_smbios_type17(&dimm(0xffff, 1600, 1600)), None);
        let mut short = dimm(8192, 1600, 2400);
        short[1] = 0x17; // old structure without configured speed
        assert_eq!(parse_smbios_type17(&short), Some(1600));
        assert_eq!(parse_smbios_type17(&[16u8; 0x28]), None, "wrong type");
    }

    #[test]
    fn speed_formatting() {
        assert_eq!(
            format_speeds(&[1600, 1600, 1600, 1600]).as_deref(),
            Some("1600 MT/s")
        );
        assert_eq!(
            format_speeds(&[3200, 2133]).as_deref(),
            Some("2133–3200 MT/s")
        );
        assert_eq!(format_speeds(&[]), None);
        assert_eq!(format_speeds(&[50, 20_000]), None);
    }

    #[test]
    fn cache_fallback_and_uptime() {
        let r = temp_root("sys");
        write(&r, "proc/uptime", "173501.45 1299177.73\n");
        write(
            &r,
            "cache/ram_speed.json",
            r#"{"speedsMTs": [1600, 1600, 1600, 1600]}"#,
        );
        write(&r, "bad.json", "not json");
        let rt = roots(&r);
        assert_eq!(uptime_seconds(&rt), Some(173501.45));
        assert_eq!(
            ram_speed_text(&rt, Some(&r.join("cache/ram_speed.json"))).as_deref(),
            Some("1600 MT/s")
        );
        assert_eq!(ram_speed_text(&rt, Some(&r.join("bad.json"))), None);
        assert_eq!(ram_speed_text(&rt, None), None);
        assert_eq!(uptime_seconds(&roots(&temp_root("none"))), None);
    }
}
