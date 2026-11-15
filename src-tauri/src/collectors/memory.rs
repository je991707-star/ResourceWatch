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
    /// Breakdown of `MemTotal` (sums to it): app = AnonPages (process
    /// memory), cache = Buffers + Cached + SReclaimable, free = MemFree,
    /// other = the rest (kernel, unreclaimable slab, page tables, drivers).
    /// Without AnonPages, app is the whole rest and other is `None`.
    pub ram_app: Option<f64>,
    pub ram_other: Option<f64>,
    pub ram_cache: Option<f64>,
    pub ram_free: Option<f64>,
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
    let free = field(text, "MemFree");
    // SReclaimable is missing on very old kernels; count it as 0 there.
    let cache = field(text, "Buffers")
        .zip(field(text, "Cached"))
        .and_then(|(b, c)| {
            b.checked_add(c)?
                .checked_add(field(text, "SReclaimable").unwrap_or(0))
        });
    let app = match (total, free, cache) {
        (Some(t), Some(f), Some(c)) => t.checked_sub(f).and_then(|r| r.checked_sub(c)),
        _ => None,
    };
    let consistent = app.is_some();
    let anon = field(text, "AnonPages").and_then(|a| Some(a.min(app?)));
    MemReading {
        ram_app: anon.or(app).map(|v| v as f64),
        ram_other: app.zip(anon).map(|(rest, a)| (rest - a) as f64),
        ram_cache: cache.filter(|_| consistent).map(|v| v as f64),
        ram_free: free.filter(|_| consistent).map(|v| v as f64),
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

/// `some avg10` of `/proc/pressure/memory` (PSI, kernel ≥ 4.20 with PSI
/// enabled): share of the last 10 s in which some task waited for memory.
pub fn parse_pressure(text: &str) -> Option<f64> {
    let line = text.lines().find(|l| l.starts_with("some "))?;
    let v: f64 = line
        .split_whitespace()
        .find_map(|kv| kv.strip_prefix("avg10="))?
        .parse()
        .ok()?;
    (0.0..=100.0).contains(&v).then_some(v)
}

pub fn pressure(roots: &Roots) -> Option<f64> {
    std::fs::read_to_string(roots.proc("pressure/memory"))
        .ok()
        .and_then(|t| parse_pressure(&t))
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
        assert_eq!(m.ram_app, None, "Cached missing: no breakdown");
    }

    #[test]
    fn breakdown_sums_to_total() {
        let text = "MemTotal: 1000 kB\nMemFree: 100 kB\nMemAvailable: 600 kB\nBuffers: 50 kB\n\
                    Cached: 300 kB\nSReclaimable: 50 kB\n";
        let m = parse_meminfo(text);
        let k = 1024.0;
        assert_eq!(m.ram_cache, Some(400.0 * k));
        assert_eq!(m.ram_free, Some(100.0 * k));
        assert_eq!(m.ram_app, Some(500.0 * k));
        assert_eq!(m.ram_other, None, "no AnonPages: app is the whole rest");
        let split = parse_meminfo(&format!("{text}AnonPages: 300 kB\n"));
        assert_eq!(
            (split.ram_app, split.ram_other),
            (Some(300.0 * k), Some(200.0 * k))
        );
        // Contradictory numbers (cache larger than total) give no breakdown.
        let bad = parse_meminfo("MemTotal: 10 kB\nMemFree: 5 kB\nBuffers: 0 kB\nCached: 50 kB\n");
        assert_eq!(
            (bad.ram_app, bad.ram_cache, bad.ram_free),
            (None, None, None)
        );
    }

    #[test]
    fn pressure_avg10() {
        let psi = "some avg10=1.25 avg60=0.03 avg300=0.02 total=27245771\n\
                   full avg10=0.50 avg60=0.02 avg300=0.01 total=26016186\n";
        assert_eq!(parse_pressure(psi), Some(1.25));
        assert_eq!(parse_pressure("full avg10=3.00\n"), None);
        assert_eq!(parse_pressure("some avg10=nan\n"), None);
        assert_eq!(parse_pressure("some avg10=250\n"), None);
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
