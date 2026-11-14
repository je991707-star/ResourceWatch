//! RAM / Swap from `/proc/meminfo`.
//!
//! used = MemTotal − MemAvailable, swap used = SwapTotal − SwapFree.
//! Verified identical to KSystemStats `memory/physical/used` and
//! `memory/swap/used` (the legacy source) on the development machine.

use super::Roots;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MemReading {
    pub ram_used: Option<f64>,
    pub ram_total: Option<f64>,
    pub swap_used: Option<f64>,
    pub swap_total: Option<f64>,
}

/// Value of `Key:   1234 kB` in bytes.
fn field(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let rest = line.strip_prefix(key)?.strip_prefix(':')?;
        let mut it = rest.split_whitespace();
        let n: u64 = it.next()?.parse().ok()?;
        match it.next() {
            Some("kB") => n.checked_mul(1024),
            None => Some(n),
            Some(_) => None,
        }
    })
}

pub fn parse_meminfo(text: &str) -> MemReading {
    let total = field(text, "MemTotal").filter(|t| *t > 0);
    let avail = field(text, "MemAvailable");
    let swap_total = field(text, "SwapTotal");
    let swap_free = field(text, "SwapFree");
    MemReading {
        ram_used: match (total, avail) {
            (Some(t), Some(a)) if a <= t => Some((t - a) as f64),
            _ => None,
        },
        ram_total: total.map(|t| t as f64),
        swap_used: match (swap_total, swap_free) {
            (Some(t), Some(f)) if f <= t => Some((t - f) as f64),
            _ => None,
        },
        // SwapTotal 0 is a real value (no swap configured): kept as 0 so the
        // UI shows "0 B / --" and no percentage, rather than "unknown".
        swap_total: swap_total.map(|t| t as f64),
    }
}

pub fn sample(roots: &Roots) -> MemReading {
    std::fs::read_to_string(roots.proc("meminfo"))
        .map(|t| parse_meminfo(&t))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_ksystemstats_definition() {
        let text = "MemTotal:       16291920 kB\nMemFree:          514256 kB\nMemAvailable:    8821808 kB\n\
                    Buffers:            1504 kB\nSwapTotal:       8145916 kB\nSwapFree:         437856 kB\n";
        let m = parse_meminfo(text);
        assert_eq!(m.ram_total, Some(16291920.0 * 1024.0));
        assert_eq!(m.ram_used, Some((16291920.0 - 8821808.0) * 1024.0));
        assert_eq!(m.swap_used, Some((8145916.0 - 437856.0) * 1024.0));
    }

    #[test]
    fn missing_fields_are_none() {
        let m = parse_meminfo("MemTotal: 1000 kB\n");
        assert_eq!(m.ram_used, None);
        assert_eq!(m.swap_total, None);
        assert_eq!(parse_meminfo(""), MemReading::default());
        // MemAvailable larger than total is inconsistent: reject.
        assert_eq!(
            parse_meminfo("MemTotal: 10 kB\nMemAvailable: 20 kB\n").ram_used,
            None
        );
        // Prefix must match the whole key.
        assert_eq!(parse_meminfo("MemTotalX: 10 kB\n").ram_total, None);
    }
}
