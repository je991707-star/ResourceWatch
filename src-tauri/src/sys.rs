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
    free: u64,
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
    /// Sum over the filesystems mounted from this disk; `None` when nothing is mounted.
    used: Option<u64>,
    total: Option<u64>,
    mounts: Vec<String>,
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
    up: bool,
    mac: String,
    ip: Option<String>,
    gateway: Option<String>,
    rx_total: u64,
    tx_total: u64,
    rx_bps: Option<f64>,
    tx_bps: Option<f64>,
}

#[derive(Serialize)]
struct Sensor {
    chip: String,
    device: String,
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
    /// os-release NAME + VERSION_ID, e.g. "Bazzite 45" (PRETTY_NAME when NAME is missing).
    os: String,
    /// Desktop environment and version, e.g. "KDE Plasma 6.8.2"; None outside a known desktop session.
    desktop: Option<String>,
    kernel: String,
    cpu_model: String,
    cpu_cores: Option<usize>,
    cpu_threads: usize,
    home: String,
    os_id: String,
    logo: Option<String>,
    /// Installed RAM from SMBIOS, e.g. "16 GB DDR3 1600"; None when udev has no memory devices (VMs).
    mem_spec: Option<String>,
}

pub fn system_info() -> SystemInfo {
    let release = read("/etc/os-release");
    let field = |k: &str| {
        release
            .lines()
            .find_map(|l| l.strip_prefix(k)?.strip_prefix('='))
            .map(|v| v.trim_matches('"').to_string())
            .unwrap_or_default()
    };
    let os = match (field("NAME"), field("VERSION_ID")) {
        (n, _) if n.is_empty() => field("PRETTY_NAME"),
        (n, v) if v.is_empty() => n,
        (n, v) => format!("{n} {v}"),
    };
    let cpuinfo = read("/proc/cpuinfo");
    let (cpu_model, cpu_cores, cpu_threads) = parse_cpuinfo(&cpuinfo);
    SystemInfo {
        hostname: read("/proc/sys/kernel/hostname").trim().to_string(),
        os,
        desktop: desktop(),
        kernel: read("/proc/sys/kernel/osrelease").trim().to_string(),
        cpu_model,
        cpu_cores,
        cpu_threads,
        home: std::env::var("HOME").unwrap_or_else(|_| "/".into()),
        os_id: field("ID"),
        logo: distro_logo(&field("LOGO")),
        mem_spec: parse_mem_spec(&read("/run/udev/data/+dmi:id")),
    }
}

/// XDG_CURRENT_DESKTOP (first entry) → display name, plus the version its shell binary prints for
/// `--version`. Unknown desktops show their XDG name; a missing binary just drops the version.
fn desktop() -> Option<String> {
    let xdg = std::env::var("XDG_CURRENT_DESKTOP").ok()?;
    let id = xdg.split(':').next()?.trim();
    let (name, bin) = match id.to_ascii_uppercase().as_str() {
        "" => return None,
        "KDE" => ("KDE Plasma", "plasmashell"),
        "GNOME" => ("GNOME", "gnome-shell"),
        "X-CINNAMON" | "CINNAMON" => ("Cinnamon", "cinnamon"),
        "XFCE" => ("Xfce", "xfce4-session"),
        "MATE" => ("MATE", "mate-session"),
        "LXQT" => ("LXQt", "lxqt-session"),
        "BUDGIE" => ("Budgie", "budgie-desktop"),
        _ => return Some(id.to_string()),
    };
    let out = std::process::Command::new(bin)
        .arg("--version")
        .output()
        .ok();
    let version = out.and_then(|o| first_version(&String::from_utf8_lossy(&o.stdout)));
    Some(version.map_or(name.to_string(), |v| format!("{name} {v}")))
}

/// First token that looks like a dotted version ("plasmashell 6.8.2" → "6.8.2").
fn first_version(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == ',')
        .find(|w| {
            w.contains('.')
                && w.split('.')
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        })
        .map(str::to_string)
}

/// systemd-udev's world-readable copy of SMBIOS type 17 (`E:MEMORY_DEVICE_<n>_<KEY>=<v>`; the raw DMI
/// tables are root-only) → "<total> GB <type> <speed>". Speed is the slowest configured speed (what mixed
/// modules actually run at), else the slowest rated one.
fn parse_mem_spec(text: &str) -> Option<String> {
    let (mut size, mut kind) = (0u64, None);
    let (mut conf, mut rated) = (None::<u64>, None::<u64>);
    let min = |slot: &mut Option<u64>, v: &str| {
        if let Ok(mts) = v.parse::<u64>() {
            *slot = Some(slot.map_or(mts, |s| s.min(mts)));
        }
    };
    for (k, v) in text
        .lines()
        .filter_map(|l| l.strip_prefix("E:MEMORY_DEVICE_")?.split_once('='))
    {
        let key = k.split_once('_').map_or(k, |(_, key)| key);
        match key {
            "SIZE" => size += v.parse::<u64>().unwrap_or(0),
            "TYPE" if kind.is_none() && v != "Unknown" => kind = Some(v),
            "CONFIGURED_SPEED_MTS" => min(&mut conf, v),
            "SPEED_MTS" => min(&mut rated, v),
            _ => {}
        }
    }
    if size == 0 {
        return None;
    }
    let speed = conf.or(rated); // older SMBIOS has no configured speed
    let gb = size as f64 / (1u64 << 30) as f64;
    let mut out = if gb.fract() == 0.0 {
        format!("{gb:.0} GB")
    } else {
        format!("{gb:.1} GB")
    };
    for part in [kind.map(str::to_string), speed.map(|s| s.to_string())]
        .into_iter()
        .flatten()
    {
        out += &format!(" {part}");
    }
    Some(out)
}

/// os-release `LOGO` (an icon name) → data: URL. The page's CSP only allows 'self' and data: images,
/// and the icon lives outside the app. Prefers the exact name, then `<name>-icon`, then SVG, then the largest PNG.
fn distro_logo(name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    fn files(dir: &Path, depth: u8, out: &mut Vec<std::path::PathBuf>) {
        for e in fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() && depth > 0 {
                files(&p, depth - 1, out);
            } else {
                out.push(p);
            }
        }
    }
    let mut all = Vec::new();
    files(Path::new("/usr/share/icons/hicolor"), 3, &mut all);
    files(Path::new("/usr/share/pixmaps"), 0, &mut all);
    let best = all
        .into_iter()
        .filter_map(|p| {
            let stem = p.file_stem()?.to_str()?;
            let svg = match p.extension()?.to_str()? {
                "svg" => true,
                "png" => false,
                _ => return None,
            };
            let rank = match stem {
                s if s == name => 0,
                s if s == format!("{name}-icon") => 1,
                s if s.starts_with(name) && !s.contains("white") => 2,
                _ => return None,
            };
            // hicolor size from a path part like "256x256"
            let size = p
                .components()
                .find_map(|c| {
                    c.as_os_str()
                        .to_str()?
                        .split_once('x')?
                        .0
                        .parse::<u32>()
                        .ok()
                })
                .unwrap_or(0);
            Some(((rank, !svg, u32::MAX - size), p))
        })
        .min_by(|a, b| a.0.cmp(&b.0))?
        .1;
    let data = fs::read(&best).ok().filter(|d| d.len() < 1 << 20)?;
    let mime = if best.extension()? == "svg" {
        "image/svg+xml"
    } else {
        "image/png"
    };
    Some(format!("data:{mime};base64,{}", base64(&data)))
}

pub(crate) fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            s.push(if i <= c.len() {
                T[(n >> (18 - 6 * i)) as usize & 63] as char
            } else {
                '='
            });
        }
    }
    s
}

/// /proc/cpuinfo → (model name, physical cores = unique (physical id, core id), logical CPUs).
fn parse_cpuinfo(text: &str) -> (String, Option<usize>, usize) {
    let mut model = String::new();
    let (mut cores, mut threads, mut phys) = (std::collections::HashSet::new(), 0, "");
    for l in text.lines() {
        let Some((k, v)) = l.split_once(':') else {
            continue;
        };
        match k.trim() {
            "processor" => threads += 1,
            "model name" if model.is_empty() => model = clean_cpu_name(v),
            "physical id" => phys = v.trim(),
            "core id" => {
                cores.insert((phys, v.trim()));
            }
            _ => {}
        }
    }
    (model, (!cores.is_empty()).then_some(cores.len()), threads)
}

/// Drops marketing noise from a cpuinfo model name: trademark marks, the words "CPU"/"Processor" and an
/// "N-Core" right before "Processor" (AMD). Brand, family, model number and "@ x.xxGHz" stay.
fn clean_cpu_name(raw: &str) -> String {
    let mut s = raw.to_string();
    for mark in ["(R)", "(r)", "(TM)", "(tm)", "(Tm)", "®", "™"] {
        s = s.replace(mark, " ");
    }
    let words: Vec<&str> = s.split_whitespace().collect();
    let is = |w: &str, t: &str| w.eq_ignore_ascii_case(t);
    let kept: Vec<&str> = words
        .iter()
        .enumerate()
        .filter(|&(i, w)| {
            let before_processor = words.get(i + 1).is_some_and(|n| is(n, "processor"));
            !(is(w, "cpu")
                || is(w, "processor")
                || before_processor && w.to_ascii_lowercase().ends_with("-core"))
        })
        .map(|(_, w)| *w)
        .collect();
    if kept.is_empty() {
        raw.trim().to_string()
    } else {
        kept.join(" ")
    }
}

/// /proc/net/route → iface → default gateway (addresses are hex in network byte order).
fn parse_routes(text: &str) -> HashMap<String, String> {
    text.lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let flags = u32::from_str_radix(f.get(3)?, 16).ok()?;
            if *f.get(1)? != "00000000" || flags & 2 == 0 {
                return None;
            }
            let gw = u32::from_str_radix(f.get(2)?, 16).ok()?;
            Some((
                f[0].to_string(),
                std::net::Ipv4Addr::from(gw.to_ne_bytes()).to_string(),
            ))
        })
        .collect()
}

/// First IPv4 address of each interface.
fn ipv4_addrs() -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs fills a linked list we only read and then free once.
    unsafe {
        if libc::getifaddrs(&mut ifap) != 0 {
            return out;
        }
        let mut p = ifap;
        while let Some(ifa) = p.as_ref() {
            if !ifa.ifa_addr.is_null() && (*ifa.ifa_addr).sa_family as i32 == libc::AF_INET {
                let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in);
                let name = std::ffi::CStr::from_ptr(ifa.ifa_name)
                    .to_string_lossy()
                    .into_owned();
                let ip = std::net::Ipv4Addr::from(sin.sin_addr.s_addr.to_ne_bytes());
                out.entry(name).or_insert(ip.to_string());
            }
            p = ifa.ifa_next;
        }
        libc::freeifaddrs(ifap);
    }
    out
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
        free: g("MemFree"),
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

/// `/dev/nvme0n1p3`, `/dev/mapper/luks-…` → the physical disk (`nvme0n1`) under it.
fn disk_of(dev: &str) -> Option<String> {
    let real = fs::canonicalize(dev).ok()?;
    let mut name = real.file_name()?.to_str()?.to_string();
    // device-mapper / md: follow `slaves` down to the partition underneath
    for _ in 0..4 {
        let slave = fs::read_dir(format!("/sys/block/{name}/slaves"))
            .ok()
            .and_then(|mut d| d.next()?.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned());
        match slave {
            Some(s) => name = s,
            None => break,
        }
    }
    if Path::new(&format!("/sys/block/{name}/device")).exists() {
        return Some(name); // filesystem directly on the whole disk
    }
    // partition: /sys/class/block/nvme0n1p3 → …/nvme0n1/nvme0n1p3
    let link = fs::read_link(format!("/sys/class/block/{name}")).ok()?;
    Some(link.parent()?.file_name()?.to_str()?.to_string())
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
        let device = sensor_device(&chip, read(dir.join("device/model")).trim());
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
                device: device.clone(),
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

/// Display name of the device behind a hwmon chip; `model` is `<hwmon>/device/model` (disks) or empty.
fn sensor_device(chip: &str, model: &str) -> String {
    static CPU: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    match chip {
        "coretemp" | "k10temp" | "zenpower" | "cpu_thermal" => {
            let cpu = CPU.get_or_init(|| parse_cpuinfo(&read("/proc/cpuinfo")).0);
            if cpu.is_empty() { "處理器" } else { cpu }.to_string()
        }
        "nvme" | "drivetemp" if !model.is_empty() => model.replace('_', " "),
        "nvme" | "drivetemp" => "硬碟".into(),
        "acpitz" => "主機板（ACPI）".into(),
        "amdgpu" | "radeon" | "nouveau" | "i915" | "xe" => "顯示卡".into(),
        c if c.starts_with("it8") || c.starts_with("nct") => "主機板".into(),
        c => c.into(),
    }
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
                    used: None,
                    total: None,
                    mounts: Vec::new(),
                }
            })
            .filter(|d| d.size > 0 && !d.name.starts_with("sr")) // empty card readers, optical drives
            .collect();
        disks.sort_by(|a, b| a.name.cmp(&b.name));
        self.disk = ds;

        let nd = parse_net_dev(&read("/proc/net/dev"));
        let (mut ips, mut gws) = (ipv4_addrs(), parse_routes(&read("/proc/net/route")));
        let mut net: Vec<Net> = nd
            .iter()
            .filter(|(n, _)| Path::new(&format!("/sys/class/net/{n}/device")).exists())
            .map(|(n, &(rx, tx))| {
                let prev = self.net.get(n);
                Net {
                    name: n.clone(),
                    up: read(format!("/sys/class/net/{n}/operstate")).trim() == "up",
                    mac: read(format!("/sys/class/net/{n}/address"))
                        .trim()
                        .to_string(),
                    ip: ips.remove(n),
                    gateway: gws.remove(n),
                    rx_total: rx,
                    tx_total: tx,
                    rx_bps: rate(prev.map(|p| &p.0), rx),
                    tx_bps: rate(prev.map(|p| &p.1), tx),
                }
            })
            .collect();
        net.sort_by(|a, b| a.name.cmp(&b.name));
        self.net = nd;

        let mounts: Vec<Mount> = parse_mounts(&read("/proc/self/mountinfo"))
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
        for m in &mounts {
            let Some(d) = disk_of(&m.device).and_then(|n| disks.iter_mut().find(|d| d.name == n))
            else {
                continue;
            };
            d.used = Some(d.used.unwrap_or(0) + m.used);
            d.total = Some(d.total.unwrap_or(0) + m.total);
            d.mounts.push(m.mount.clone());
        }

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
    fn sensor_device_names() {
        assert_eq!(sensor_device("nvme", "Micron_2400_MTFDKBA512QFM"), "Micron 2400 MTFDKBA512QFM");
        assert_eq!(sensor_device("drivetemp", ""), "硬碟");
        assert_eq!(sensor_device("acpitz", ""), "主機板（ACPI）");
        assert_eq!(sensor_device("it8620", ""), "主機板");
        assert_eq!(sensor_device("nct6798", ""), "主機板");
        assert_eq!(sensor_device("amdgpu", ""), "顯示卡");
        assert_eq!(sensor_device("asus_ec", ""), "asus_ec");
        assert!(!sensor_device("coretemp", "").is_empty());
    }

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
                free: 102400,
                cached: 256000,
                swap_total: 512000,
                swap_used: 102400,
            }
        );
        let udev = "E:MEMORY_ARRAY_MAX_CAPACITY=34359738368\nE:MEMORY_DEVICE_0_SIZE=4294967296\nE:MEMORY_DEVICE_0_TYPE=DDR3\nE:MEMORY_DEVICE_0_SPEED_MTS=1600\nE:MEMORY_DEVICE_0_CONFIGURED_SPEED_MTS=1600\nE:MEMORY_DEVICE_1_SIZE=12884901888\nE:MEMORY_DEVICE_1_TYPE=DDR3\nE:MEMORY_DEVICE_1_SPEED_MTS=1600\nE:MEMORY_DEVICE_1_CONFIGURED_SPEED_MTS=1333\n";
        assert_eq!(parse_mem_spec(udev).as_deref(), Some("16 GB DDR3 1333"));
        assert_eq!(
            parse_mem_spec("E:MEMORY_DEVICE_0_SIZE=1610612736\nE:MEMORY_DEVICE_0_SPEED_MTS=2400\n")
                .as_deref(),
            Some("1.5 GB 2400")
        );
        assert_eq!(parse_mem_spec("E:MEMORY_ARRAY_LOCATION=x\n"), None);
        assert_eq!(
            first_version("plasmashell 6.8.2\n").as_deref(),
            Some("6.8.2")
        );
        assert_eq!(first_version("GNOME Shell 47.1\n").as_deref(), Some("47.1"));
        assert_eq!(
            first_version("xfce4-session 4.18.1 (Xfce 4.18)\n").as_deref(),
            Some("4.18.1")
        );
        assert_eq!(first_version("no version here"), None);
    }

    #[test]
    fn base64_padding() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b""), "");
    }

    #[test]
    fn cpuinfo_and_routes() {
        let c = "processor\t: 0\nmodel name\t: Xeon\nphysical id\t: 0\ncore id\t: 0\n\nprocessor\t: 1\nmodel name\t: Xeon\nphysical id\t: 0\ncore id\t: 0\n\nprocessor\t: 2\nphysical id\t: 0\ncore id\t: 1\n";
        assert_eq!(parse_cpuinfo(c), ("Xeon".into(), Some(2), 3));
        for (raw, want) in [
            (
                "Intel(R) Xeon(R) CPU E3-1231 v3 @ 3.40GHz",
                "Intel Xeon E3-1231 v3 @ 3.40GHz",
            ),
            (
                "Intel(R) Core(TM) i7-8700 CPU @ 3.20GHz",
                "Intel Core i7-8700 @ 3.20GHz",
            ),
            (
                "12th Gen Intel(R) Core(TM) i5-12400F",
                "12th Gen Intel Core i5-12400F",
            ),
            ("AMD Ryzen 7 5800X 8-Core Processor", "AMD Ryzen 7 5800X"),
            (
                "AMD Ryzen 5 PRO 4650G with Radeon Graphics",
                "AMD Ryzen 5 PRO 4650G with Radeon Graphics",
            ),
            (
                "Pentium(R) Dual-Core  CPU      E5200  @ 2.50GHz",
                "Pentium Dual-Core E5200 @ 2.50GHz",
            ),
            (
                "Intel(R) Celeron(R) N5105 @ 2.00GHz",
                "Intel Celeron N5105 @ 2.00GHz",
            ),
            ("CPU", "CPU"),
        ] {
            assert_eq!(clean_cpu_name(raw), want);
        }
        // the kernel prints the network-order address read as a native u32
        let r = format!("Iface\tDestination\tGateway\tFlags\nenp3s0\t00000000\t{:08X}\t0003\nenp3s0\t0001A8C0\t00000000\t0001\n", u32::from_ne_bytes([192, 168, 1, 1]));
        assert_eq!(parse_routes(&r)["enp3s0"], "192.168.1.1");
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
