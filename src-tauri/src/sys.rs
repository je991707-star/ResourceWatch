//! Reads /proc and /sys. Parsers take text so they can be unit-tested; `Monitor`
//! keeps the previous counters so each sample can turn them into rates.
//! Missing values are `None` (JSON `null`), never 0.

use crate::gpu::{Gpu, Gpus};
use serde::Serialize;
use std::collections::HashMap;
use std::ffi::CString;
use std::fs;
use std::path::Path;
use std::time::Instant;

fn read(path: impl AsRef<Path>) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn read_num(path: impl AsRef<Path>) -> Option<f64> {
    read(path).trim().parse().ok()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    cpu: Cpu,
    mem: Mem,
    gpus: Vec<Gpu>,
    disks: Vec<Disk>,
    mounts: Vec<Mount>,
    net: Vec<Net>,
    sensors: Vec<Sensor>,
    uptime: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Cpu {
    usage: Option<f64>,
    cores: Vec<Option<f64>>,
    freq_mhz: Option<f64>,
    temp: Option<f64>,
}

#[derive(Serialize, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Mem {
    total: u64,
    used: u64,
    available: u64,
    cached: u64,
    swap_total: u64,
    swap_used: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Disk {
    name: String,
    model: String,
    size: u64,
    read_bps: Option<f64>,
    write_bps: Option<f64>,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Mount {
    mount: String,
    device: String,
    total: u64,
    used: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Net {
    name: String,
    rx_bps: Option<f64>,
    tx_bps: Option<f64>,
}

#[derive(Serialize)]
struct Sensor {
    chip: String,
    label: String,
    kind: &'static str,
    value: f64,
}

#[derive(Serialize)]
pub struct Proc {
    pid: u32,
    name: String,
    cpu: Option<f64>,
    mem: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    hostname: String,
    os: String,
    kernel: String,
    cpu_model: String,
}

pub fn system_info() -> SystemInfo {
    let os = read("/etc/os-release")
        .lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim_matches('"').to_string())
        .unwrap_or_default();
    let cpu_model = read("/proc/cpuinfo")
        .lines()
        .find_map(|l| l.strip_prefix("model name"))
        .and_then(|v| v.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_default();
    SystemInfo {
        hostname: read("/proc/sys/kernel/hostname").trim().to_string(),
        os,
        kernel: read("/proc/sys/kernel/osrelease").trim().to_string(),
        cpu_model,
    }
}

/// `cpu`/`cpuN` lines of /proc/stat → (total, idle) jiffies; first entry is the aggregate.
fn parse_stat(text: &str) -> Vec<(u64, u64)> {
    text.lines()
        .filter(|l| l.starts_with("cpu"))
        .map(|l| {
            // user nice system idle iowait irq softirq steal (guest is already in user)
            let f: Vec<u64> = l
                .split_whitespace()
                .skip(1)
                .take(8)
                .filter_map(|v| v.parse().ok())
                .collect();
            (
                f.iter().sum(),
                f.get(3).unwrap_or(&0) + f.get(4).unwrap_or(&0),
            )
        })
        .collect()
}

fn usage((t0, i0): (u64, u64), (t1, i1): (u64, u64)) -> Option<f64> {
    let dt = t1.checked_sub(t0).filter(|&d| d > 0)?;
    Some((100.0 * (1.0 - i1.saturating_sub(i0) as f64 / dt as f64)).clamp(0.0, 100.0))
}

fn parse_meminfo(text: &str) -> Mem {
    let kv: HashMap<&str, u64> = text
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            Some((k, v.split_whitespace().next()?.parse::<u64>().ok()? * 1024))
        })
        .collect();
    let g = |k| kv.get(k).copied().unwrap_or(0);
    let available = g("MemAvailable");
    Mem {
        total: g("MemTotal"),
        used: g("MemTotal").saturating_sub(available),
        available,
        cached: g("Buffers") + g("Cached") + g("SReclaimable"),
        swap_total: g("SwapTotal"),
        swap_used: g("SwapTotal").saturating_sub(g("SwapFree")),
    }
}

/// /proc/diskstats → name → (sectors read, sectors written).
fn parse_diskstats(text: &str) -> HashMap<String, (u64, u64)> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            Some((
                f.get(2)?.to_string(),
                (f.get(5)?.parse().ok()?, f.get(9)?.parse().ok()?),
            ))
        })
        .collect()
}

/// /proc/net/dev → name → (rx bytes, tx bytes).
fn parse_net_dev(text: &str) -> HashMap<String, (u64, u64)> {
    text.lines()
        .skip(2)
        .filter_map(|l| {
            let (name, rest) = l.split_once(':')?;
            let f: Vec<u64> = rest
                .split_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect();
            Some((name.trim().to_string(), (*f.first()?, *f.get(8)?)))
        })
        .collect()
}

/// /proc/self/mountinfo → one (mountpoint, device) per /dev block device. A device
/// is mounted many times (btrfs subvolumes, bind mounts such as ostree's /etc), so
/// keep the mount whose path inside the filesystem is shallowest, then the shortest.
fn parse_mounts(text: &str) -> Vec<(String, String)> {
    let mut by_dev: Vec<((usize, usize), String, String)> = Vec::new();
    for l in text.lines() {
        let Some((left, right)) = l.split_once(" - ") else {
            continue;
        };
        let f: Vec<&str> = left.split_whitespace().collect();
        let (Some(root), Some(mnt), Some(dev)) =
            (f.get(3), f.get(4), right.split_whitespace().nth(1))
        else {
            continue;
        };
        if !dev.starts_with("/dev/") || dev.starts_with("/dev/loop") {
            continue;
        }
        let mnt = mnt.replace("\\040", " ");
        let rank = (
            root.matches('/').filter(|_| *root != "/").count(),
            mnt.len(),
        );
        match by_dev.iter_mut().find(|e| e.2 == dev) {
            Some(e) if rank < e.0 => (e.0, e.1) = (rank, mnt),
            Some(_) => {}
            None => by_dev.push((rank, mnt, dev.to_string())),
        }
    }
    let mut out: Vec<_> = by_dev.into_iter().map(|(_, m, d)| (m, d)).collect();
    out.sort();
    out
}

fn statvfs(path: &str) -> Option<(u64, u64)> {
    let c = CString::new(path).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: valid C string and a zeroed out-struct of the right type.
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    let total = s.f_blocks as u64 * s.f_frsize as u64;
    Some((
        total,
        total.saturating_sub(s.f_bfree as u64 * s.f_frsize as u64),
    ))
}

/// /proc/[pid]/stat → (comm, utime+stime ticks, rss pages). comm may contain spaces and ')'.
fn parse_proc_stat(text: &str) -> Option<(String, u64, u64)> {
    let name = text.get(text.find('(')? + 1..text.rfind(')')?)?.to_string();
    let f: Vec<&str> = text[text.rfind(')')? + 1..].split_whitespace().collect();
    let ticks = f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?;
    Some((name, ticks, f.get(21)?.parse().ok()?))
}

fn sensors() -> Vec<Sensor> {
    let mut dirs: Vec<_> = fs::read_dir("/sys/class/hwmon")
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    dirs.sort();
    let mut out = Vec::new();
    for dir in dirs {
        let chip = read(dir.join("name")).trim().to_string();
        let mut files: Vec<String> = fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.ends_with("_input") && (n.starts_with("temp") || n.starts_with("fan")))
            .collect();
        files.sort_by_key(|n| (n.starts_with("fan"), n.len(), n.clone()));
        for f in files {
            let Some(raw) = read_num(dir.join(&f)) else {
                continue;
            };
            let prefix = f.trim_end_matches("_input");
            let label = read(dir.join(format!("{prefix}_label"))).trim().to_string();
            let temp = prefix.starts_with("temp");
            out.push(Sensor {
                chip: chip.clone(),
                label: if label.is_empty() {
                    prefix.to_string()
                } else {
                    label
                },
                kind: if temp { "temp" } else { "fan" },
                value: if temp { raw / 1000.0 } else { raw },
            });
        }
    }
    out
}

pub struct Monitor {
    at: Option<Instant>,
    cpu: Vec<(u64, u64)>,
    disk: HashMap<String, (u64, u64)>,
    net: HashMap<String, (u64, u64)>,
    procs_at: Option<Instant>,
    procs: HashMap<u32, u64>,
    gpus: Gpus,
}

impl Monitor {
    pub fn new() -> Self {
        Monitor {
            at: None,
            cpu: Vec::new(),
            disk: HashMap::new(),
            net: HashMap::new(),
            procs_at: None,
            procs: HashMap::new(),
            gpus: Gpus::new(),
        }
    }

    pub fn sample(&mut self) -> Snapshot {
        let now = Instant::now();
        let secs = self
            .at
            .map(|t| now.duration_since(t).as_secs_f64())
            .filter(|&s| s > 0.0);
        self.at = Some(now);
        let rate = |prev: Option<&u64>, cur: u64| Some(cur.checked_sub(*prev?)? as f64 / secs?);

        let stat = parse_stat(&read("/proc/stat"));
        let mut cpus = stat
            .iter()
            .enumerate()
            .map(|(i, &c)| self.cpu.get(i).and_then(|&p| usage(p, c)));
        let usage = cpus.next().flatten();
        let cores: Vec<_> = cpus.collect();
        self.cpu = stat;

        let freqs: Vec<f64> = read("/proc/cpuinfo")
            .lines()
            .filter_map(|l| {
                l.strip_prefix("cpu MHz")?
                    .split_once(':')?
                    .1
                    .trim()
                    .parse()
                    .ok()
            })
            .collect();
        let sensors = sensors();
        let temp = sensors
            .iter()
            .find(|s| {
                s.kind == "temp"
                    && matches!(
                        s.chip.as_str(),
                        "coretemp" | "k10temp" | "zenpower" | "cpu_thermal"
                    )
            })
            .map(|s| s.value);

        let ds = parse_diskstats(&read("/proc/diskstats"));
        let mut disks: Vec<Disk> = ds
            .iter()
            .filter(|(n, _)| Path::new(&format!("/sys/block/{n}/device")).exists())
            .map(|(n, &(r, w))| {
                let prev = self.disk.get(n);
                Disk {
                    name: n.clone(),
                    model: read(format!("/sys/block/{n}/device/model"))
                        .trim()
                        .to_string(),
                    size: read_num(format!("/sys/block/{n}/size")).unwrap_or(0.0) as u64 * 512,
                    read_bps: rate(prev.map(|p| &p.0), r).map(|v| v * 512.0),
                    write_bps: rate(prev.map(|p| &p.1), w).map(|v| v * 512.0),
                }
            })
            .collect();
        disks.sort_by(|a, b| a.name.cmp(&b.name));
        self.disk = ds;

        let nd = parse_net_dev(&read("/proc/net/dev"));
        let mut net: Vec<Net> = nd
            .iter()
            .filter(|(n, _)| Path::new(&format!("/sys/class/net/{n}/device")).exists())
            .map(|(n, &(rx, tx))| {
                let prev = self.net.get(n);
                Net {
                    name: n.clone(),
                    rx_bps: rate(prev.map(|p| &p.0), rx),
                    tx_bps: rate(prev.map(|p| &p.1), tx),
                }
            })
            .collect();
        net.sort_by(|a, b| a.name.cmp(&b.name));
        self.net = nd;

        let mounts = parse_mounts(&read("/proc/self/mountinfo"))
            .into_iter()
            .filter_map(|(mount, device)| {
                let (total, used) = statvfs(&mount).filter(|v| v.0 > 0)?;
                Some(Mount {
                    mount,
                    device,
                    total,
                    used,
                })
            })
            .collect();

        Snapshot {
            cpu: Cpu {
                usage,
                cores,
                freq_mhz: (!freqs.is_empty())
                    .then(|| freqs.iter().sum::<f64>() / freqs.len() as f64),
                temp,
            },
            mem: parse_meminfo(&read("/proc/meminfo")),
            gpus: self.gpus.sample(),
            disks,
            mounts,
            net,
            sensors,
            uptime: read("/proc/uptime")
                .split_whitespace()
                .next()
                .and_then(|v| v.parse().ok()),
        }
    }

    /// CPU% is of the whole machine (100% = all threads busy). First call has no CPU%.
    pub fn processes(&mut self) -> Vec<Proc> {
        let now = Instant::now();
        let secs = self
            .procs_at
            .map(|t| now.duration_since(t).as_secs_f64())
            .filter(|&s| s > 0.0);
        self.procs_at = Some(now);
        // SAFETY: sysconf has no preconditions.
        let (tck, page) = unsafe {
            (
                libc::sysconf(libc::_SC_CLK_TCK) as f64,
                libc::sysconf(libc::_SC_PAGESIZE) as u64,
            )
        };
        let threads = self.cpu.len().saturating_sub(1).max(1) as f64;
        let mut seen = HashMap::new();
        let list = fs::read_dir("/proc")
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let pid: u32 = e.file_name().to_str()?.parse().ok()?;
                let (name, ticks, rss) = parse_proc_stat(&read(e.path().join("stat")))?;
                seen.insert(pid, ticks);
                let cpu = self.procs.get(&pid).zip(secs).map(|(&p, s)| {
                    (ticks.saturating_sub(p) as f64 / tck / s / threads * 100.0).min(100.0)
                });
                Some(Proc {
                    pid,
                    name,
                    cpu,
                    mem: rss * page,
                })
            })
            .collect();
        self.procs = seen;
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_usage() {
        let a =
            parse_stat("cpu  100 0 100 800 0 0 0 0 5 5\ncpu0 50 0 50 400 0 0 0 0 0 0\nintr 1 2\n");
        let b = parse_stat("cpu  150 0 150 900 0 0 0 0 5 5\ncpu0 50 0 50 400 0 0 0 0 0 0\n");
        assert_eq!(a, vec![(1000, 800), (500, 400)]);
        assert_eq!(usage(a[0], b[0]), Some(50.0));
        assert_eq!(usage(a[1], b[1]), None);
    }

    #[test]
    fn meminfo() {
        let m = parse_meminfo("MemTotal: 1000 kB\nMemFree: 100 kB\nMemAvailable: 600 kB\nBuffers: 10 kB\nCached: 200 kB\nSReclaimable: 40 kB\nSwapTotal: 500 kB\nSwapFree: 400 kB\n");
        assert_eq!(
            m,
            Mem {
                total: 1024000,
                used: 409600,
                available: 614400,
                cached: 256000,
                swap_total: 512000,
                swap_used: 102400
            }
        );
    }

    #[test]
    fn diskstats_and_net() {
        let d = parse_diskstats(" 259 0 nvme0n1 10 0 2048 5 20 0 4096 9 0 0 0\n");
        assert_eq!(d["nvme0n1"], (2048, 4096));
        let n = parse_net_dev("h1\nh2\n  eth0: 100 1 0 0 0 0 0 0 200 2 0 0 0 0 0 0\n");
        assert_eq!(n["eth0"], (100, 200));
    }

    #[test]
    fn mounts_dedupe() {
        let m = parse_mounts(concat!(
            "1 0 0:1 /root/ostree/deploy/x/etc /etc rw - btrfs /dev/sda1 rw\n",
            "2 0 0:1 /home /var/home rw - btrfs /dev/sda1 rw\n",
            "3 0 0:1 /var /var rw shared:1 - btrfs /dev/sda1 rw\n",
            "4 0 0:2 / /proc rw - proc proc rw\n",
            "5 0 7:0 / /snap ro - squashfs /dev/loop0 ro\n",
            "6 0 8:17 / /mnt/my\\040disk rw - ext4 /dev/sdb1 rw\n",
        ));
        assert_eq!(
            m,
            vec![
                ("/mnt/my disk".into(), "/dev/sdb1".into()),
                ("/var".into(), "/dev/sda1".into())
            ]
        );
    }

    #[test]
    fn proc_stat() {
        let line = "42 (a (b) c) S 1 1 1 0 -1 0 0 0 0 0 7 3 0 0 20 0 1 0 100 4096 25 0";
        assert_eq!(parse_proc_stat(line), Some(("a (b) c".into(), 10, 25)));
        assert_eq!(parse_proc_stat("garbage"), None);
    }
}
