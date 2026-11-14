//! Per-process CPU and resident memory from `/proc/<pid>/stat` (one read per
//! process). Processes that exit mid-scan or cannot be read are skipped;
//! kernel threads are left out.

use super::Roots;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    /// Kernel `comm` (at most 15 characters).
    pub name: String,
    /// Share of the whole machine's CPU time (all logical CPUs = 100 %) since
    /// the previous scan; `None` the first time a process is seen.
    pub cpu: Option<f64>,
    /// Resident set size in bytes.
    pub memory: u64,
}

/// The fields of `/proc/<pid>/stat` used here.
#[derive(Debug, PartialEq)]
pub struct Stat {
    pub name: String,
    /// utime + stime, clock ticks.
    pub ticks: u64,
    pub rss_pages: u64,
    pub kernel_thread: bool,
}

const PF_KTHREAD: u64 = 0x0020_0000;

pub fn parse_stat(text: &str) -> Option<Stat> {
    // comm may contain spaces and ')' itself, so it ends at the last ')'.
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let name = text.get(open + 1..close)?.to_string();
    let fields: Vec<&str> = text.get(close + 1..)?.split_whitespace().collect();
    // fields[0] is field 3 (state) in proc(5) numbering.
    let num = |n: usize| fields.get(n - 3)?.parse::<u64>().ok();
    Some(Stat {
        name,
        ticks: num(14)?.checked_add(num(15)?)?,
        rss_pages: num(24)?,
        kernel_thread: num(9)? & PF_KTHREAD != 0,
    })
}

pub struct ProcessCollector {
    roots: Roots,
    prev: HashMap<u32, u64>,
    prev_at: Option<Instant>,
    tick_hz: f64,
    page_size: u64,
    cpus: f64,
}

fn sysconf(name: libc::c_int) -> Option<u64> {
    // SAFETY: sysconf only reads a system constant.
    let v = unsafe { libc::sysconf(name) };
    u64::try_from(v).ok().filter(|v| *v > 0)
}

impl ProcessCollector {
    pub fn new(roots: &Roots) -> Self {
        Self {
            roots: roots.clone(),
            prev: HashMap::new(),
            prev_at: None,
            tick_hz: sysconf(libc::_SC_CLK_TCK).unwrap_or(100) as f64,
            page_size: sysconf(libc::_SC_PAGESIZE).unwrap_or(4096),
            cpus: sysconf(libc::_SC_NPROCESSORS_ONLN).unwrap_or(1) as f64,
        }
    }

    pub fn sample(&mut self, now: Instant) -> Vec<ProcessInfo> {
        let elapsed = self
            .prev_at
            .map(|t| now.saturating_duration_since(t).as_secs_f64())
            .filter(|s| *s > 0.0);
        let mut ticks = HashMap::with_capacity(self.prev.len());
        let mut out = Vec::with_capacity(self.prev.len());
        let entries = fs::read_dir(&self.roots.proc_dir).into_iter().flatten();
        for entry in entries.flatten() {
            let Some(pid) = entry.file_name().to_str().and_then(|s| s.parse().ok()) else {
                continue;
            };
            let Some(st) = fs::read_to_string(entry.path().join("stat"))
                .ok()
                .as_deref()
                .and_then(parse_stat)
            else {
                continue;
            };
            if st.kernel_thread {
                continue;
            }
            // A reused PID with fewer ticks reads as 0 % for one scan.
            let cpu = elapsed.zip(self.prev.get(&pid)).map(|(secs, &before)| {
                let used = st.ticks.saturating_sub(before) as f64 / self.tick_hz;
                (used / secs / self.cpus * 100.0).min(100.0)
            });
            ticks.insert(pid, st.ticks);
            out.push(ProcessInfo {
                pid,
                name: st.name,
                cpu,
                memory: st.rss_pages.saturating_mul(self.page_size),
            });
        }
        self.prev = ticks;
        self.prev_at = Some(now);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const LINE: &str = "1234 (Web Content (x)) S 1 1234 1234 0 -1 4194560 100 0 0 0 250 50 0 0 20 0 30 0 5000 900000000 2048 18446744073709551615";

    #[test]
    fn parses_name_with_parens_ticks_and_rss() {
        let st = parse_stat(LINE).unwrap();
        assert_eq!(st.name, "Web Content (x)");
        assert_eq!(st.ticks, 300);
        assert_eq!(st.rss_pages, 2048);
        assert!(!st.kernel_thread);
        let kthread = LINE.replace(" 4194560 ", " 2129984 ");
        assert!(parse_stat(&kthread).unwrap().kernel_thread);
        assert_eq!(parse_stat("1 (x) S 1 2"), None);
        assert_eq!(parse_stat("garbage"), None);
    }

    #[test]
    fn cpu_share_between_scans() {
        let dir = std::env::temp_dir().join(format!("rw-proc-{}", std::process::id()));
        let write = |pid: &str, line: &str| {
            fs::create_dir_all(dir.join(pid)).unwrap();
            fs::write(dir.join(pid).join("stat"), line).unwrap();
        };
        write("1234", LINE);
        write("self", LINE); // non-numeric entries are ignored
        let roots = Roots {
            proc_dir: dir.clone(),
            ..Roots::default()
        };
        let mut c = ProcessCollector::new(&roots);
        (c.tick_hz, c.cpus, c.page_size) = (100.0, 2.0, 4096);
        let t0 = Instant::now();
        let first = c.sample(t0);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].cpu, None);
        assert_eq!(first[0].memory, 2048 * 4096);
        // +100 ticks = 1 s of CPU over 1 s on 2 CPUs → 50 %.
        write("1234", &LINE.replace(" 250 50 ", " 350 50 "));
        let second = c.sample(t0 + Duration::from_secs(1));
        assert_eq!(second[0].cpu, Some(50.0));
        fs::remove_dir_all(&dir).unwrap();
    }
}
