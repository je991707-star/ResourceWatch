//! Hardware collectors: they only read data. They never decide when to read
//! (that is the sampling loop's job) and never format text for the UI.
//!
//! Every collector takes a [`Roots`] so tests can point it at a fake
//! `/proc` + `/sys` tree. Missing files, unreadable files and malformed
//! values yield `None`, never `0`.

pub mod cpu;
pub mod disk;
pub mod gpu;
pub mod memory;
pub mod network;
pub mod system;

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Filesystem roots for Linux pseudo filesystems.
#[derive(Debug, Clone)]
pub struct Roots {
    pub proc_dir: PathBuf,
    pub sys_dir: PathBuf,
}

impl Default for Roots {
    fn default() -> Self {
        Self {
            proc_dir: PathBuf::from("/proc"),
            sys_dir: PathBuf::from("/sys"),
        }
    }
}

impl Roots {
    pub fn proc(&self, rel: &str) -> PathBuf {
        self.proc_dir.join(rel)
    }
    pub fn sys(&self, rel: &str) -> PathBuf {
        self.sys_dir.join(rel)
    }
}

/// Whole file as trimmed text; `None` if missing, unreadable or empty.
pub fn read_trimmed(path: &Path) -> Option<String> {
    let s = fs::read_to_string(path).ok()?;
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

pub fn read_u64(path: &Path) -> Option<u64> {
    read_trimmed(path)?.parse().ok()
}

pub fn read_i64(path: &Path) -> Option<i64> {
    read_trimmed(path)?.parse().ok()
}

/// First line only (avoids reading per-CPU lines of `/proc/stat`).
pub fn read_first_line(path: &Path) -> Option<String> {
    let mut line = String::new();
    BufReader::new(fs::File::open(path).ok()?)
        .read_line(&mut line)
        .ok()?;
    Some(line)
}

/// Sorted directory entries whose file name matches `filter`.
pub fn list_dir(path: &Path, filter: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(path)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| e.file_name().to_str().is_some_and(&filter))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    out.sort_by_key(|p| natural_key(p));
    out
}

/// Sort key so that `card10` follows `card9`, `hwmon10` follows `hwmon2`.
fn natural_key(p: &Path) -> (String, u64) {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let digits = name.len() - name.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (stem, num) = name.split_at(name.len() - digits);
    (stem.to_string(), num.parse().unwrap_or(0))
}

/// Byte/second rate from two cumulative counters. `None` when the counter
/// went backwards (reset, device re-attached) or no time elapsed.
pub fn counter_rate(prev: u64, cur: u64, seconds: f64) -> Option<f64> {
    if cur < prev || seconds.is_nan() || seconds <= 0.0 {
        return None;
    }
    Some((cur - prev) as f64 / seconds)
}

#[cfg(test)]
pub mod testutil {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Fresh empty directory under the system temp dir.
    pub fn temp_root(tag: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("rw-test-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub fn write(root: &Path, rel: &str, content: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    pub fn roots(root: &Path) -> super::Roots {
        super::Roots {
            proc_dir: root.join("proc"),
            sys_dir: root.join("sys"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_rate_handles_reset_and_zero_time() {
        assert_eq!(counter_rate(100, 300, 2.0), Some(100.0));
        assert_eq!(counter_rate(100, 100, 1.0), Some(0.0));
        assert_eq!(counter_rate(300, 100, 1.0), None);
        assert_eq!(counter_rate(100, 300, 0.0), None);
        assert_eq!(counter_rate(100, 300, f64::NAN), None);
    }

    #[test]
    fn list_dir_natural_order() {
        let root = testutil::temp_root("ls");
        for n in ["card10", "card2", "card1", "renderD128"] {
            std::fs::create_dir_all(root.join(n)).unwrap();
        }
        let names: Vec<_> = list_dir(&root, |n| n.starts_with("card"))
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert_eq!(names, ["card1", "card2", "card10"]);
    }
}
