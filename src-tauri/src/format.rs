//! Pure formatting helpers ported from the legacy `FormatUtils.js` and the
//! legacy QML/JS byte/rate formatters. Invalid input yields `None` / `"--"`,
//! never `0`.

pub const MISSING: &str = "--";

/// Finite number check (mirrors `FormatUtils.isValidNumber`).
pub fn valid(v: Option<f64>) -> Option<f64> {
    v.filter(|x| x.is_finite())
}

/// Valid non-negative rate (mirrors `FormatUtils.isValidRate`).
pub fn valid_rate(v: Option<f64>) -> Option<f64> {
    valid(v).filter(|x| *x >= 0.0)
}

/// Visual clamp to 0..=100 (mirrors `FormatUtils.clampPercent`).
pub fn clamp_percent(v: Option<f64>) -> Option<f64> {
    valid(v).map(|x| x.clamp(0.0, 100.0))
}

/// `used / total * 100`; `None` when total <= 0 or used < 0
/// (mirrors `FormatUtils.percentOf`).
pub fn percent_of(used: Option<f64>, total: Option<f64>) -> Option<f64> {
    match (valid(used), valid(total)) {
        (Some(u), Some(t)) if t > 0.0 && u >= 0.0 => Some(u / t * 100.0),
        _ => None,
    }
}

/// One decimal, trailing `.0` dropped, e.g. `18.4%`, `68%`
/// (mirrors `FormatUtils.formatPercent`).
pub fn format_percent(p: Option<f64>) -> String {
    match valid(p) {
        Some(x) => format!("{}%", trim_number((x * 10.0).round() / 10.0)),
        None => MISSING.to_string(),
    }
}

fn trim_number(x: f64) -> String {
    let s = format!("{x:.1}");
    let s = s.strip_suffix(".0").unwrap_or(&s).to_string();
    if s == "-0" {
        "0".to_string()
    } else {
        s
    }
}

fn scaled(value: f64, units: &[&str]) -> String {
    let mut v = value;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{v:.0} {}", units[i])
    } else {
        format!("{v:.1} {}", units[i])
    }
}

/// Binary byte size, e.g. `476.9 GiB` (legacy `_formatDiskSize`).
pub fn format_bytes(bytes: Option<f64>) -> String {
    match valid(bytes).filter(|b| *b >= 0.0) {
        Some(b) => scaled(b, &["B", "KiB", "MiB", "GiB", "TiB", "PiB"]),
        None => MISSING.to_string(),
    }
}

/// Binary byte rate, e.g. `13.9 KiB/s` (legacy `_formatDiskRate` / app.js `formatRate`).
pub fn format_rate(bytes_per_second: Option<f64>) -> String {
    match valid_rate(bytes_per_second) {
        Some(b) => scaled(b, &["B/s", "KiB/s", "MiB/s", "GiB/s"]),
        None => MISSING.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_formatting_matches_legacy() {
        assert_eq!(format_percent(Some(18.44)), "18.4%");
        assert_eq!(format_percent(Some(68.0)), "68%");
        assert_eq!(format_percent(Some(0.0)), "0%");
        assert_eq!(format_percent(Some(99.96)), "100%");
        assert_eq!(format_percent(None), "--");
        assert_eq!(format_percent(Some(f64::NAN)), "--");
        assert_eq!(format_percent(Some(f64::INFINITY)), "--");
    }

    #[test]
    fn percent_of_rejects_invalid_totals() {
        assert_eq!(percent_of(Some(1.0), Some(4.0)), Some(25.0));
        assert_eq!(percent_of(Some(1.0), Some(0.0)), None);
        assert_eq!(percent_of(Some(-1.0), Some(4.0)), None);
        assert_eq!(percent_of(None, Some(4.0)), None);
        assert_eq!(percent_of(Some(1.0), None), None);
    }

    #[test]
    fn clamp_keeps_missing_missing() {
        assert_eq!(clamp_percent(Some(120.0)), Some(100.0));
        assert_eq!(clamp_percent(Some(-5.0)), Some(0.0));
        assert_eq!(clamp_percent(None), None);
    }

    #[test]
    fn byte_and_rate_units() {
        assert_eq!(format_bytes(Some(512.0)), "512 B");
        assert_eq!(format_bytes(Some(1536.0)), "1.5 KiB");
        assert_eq!(format_bytes(Some(2_000_398_934_016.0)), "1.8 TiB");
        assert_eq!(format_bytes(Some(-1.0)), "--");
        assert_eq!(format_rate(Some(0.0)), "0 B/s");
        assert_eq!(format_rate(Some(14_234.0)), "13.9 KiB/s");
        assert_eq!(format_rate(Some(1_363_149.0)), "1.3 MiB/s");
        assert_eq!(format_rate(Some(-3.0)), "--");
        assert_eq!(format_rate(None), "--");
    }
}
