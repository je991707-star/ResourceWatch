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
    /// Nameservers from systemd-resolved's upstream list, else /etc/resolv.conf.
    dns: Vec<String>,
    sensors: Vec<Sensor>,
    /// A RAPL zone exists but its `energy_uj` is root-only (the frontend offers `elevate("rapl")`).
    rapl_locked: bool,
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
    /// Active time % (io_ticks delta / elapsed), like Windows' disk %.
    busy: Option<f64>,
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
    prefix: Option<u32>,
    ipv6: Vec<String>,
    gateway: Option<String>,
    model: Option<String>,
    driver: Option<String>,
    driver_open: Option<bool>,
    speed: Option<u32>, // Mb/s
    duplex: Option<String>,
    mtu: Option<u32>,
    rx_total: u64,
    tx_total: u64,
    rx_bps: Option<f64>,
    tx_bps: Option<f64>,
    rx_packets: u64,
    tx_packets: u64,
    rx_errors: u64,
    tx_errors: u64,
    rx_dropped: u64,
    tx_dropped: u64,
    wifi: Option<Wifi>,
}

#[derive(Serialize, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Wifi {
    ssid: String,
    bssid: String,
    freq: Option<f64>,    // MHz
    signal: Option<i32>,  // dBm
    rx_rate: Option<f64>, // Mb/s
    tx_rate: Option<f64>,
    generation: Option<u8>, // Wi-Fi 4/5/6/7 from the HT/VHT/HE/EHT rate info
    width: Option<u32>,     // channel width, MHz
}

#[derive(Serialize)]
struct Sensor {
    chip: String,
    device: String,
    label: String,
    kind: &'static str,
    value: f64,
    /// Critical temperature (°C) from `tempN_crit`, else `tempN_max`; `null` when absent or implausible.
    crit: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proc {
    pid: u32,
    name: String,
    cpu: Option<f64>,
    mem: u64,
    /// `stat` state letter (R, S, D, T, Z, …).
    state: char,
    nice: i32,
    /// "app" (user app scope), "bg" (other user units), "sys" (system, other users, kernel threads).
    kind: &'static str,
    /// cgroup unit (last path component); "kernel" for kernel threads.
    unit: Option<String>,
    /// Desktop id parsed from an app unit, for the .desktop name and icon.
    app: Option<String>,
    /// read_bytes + write_bytes per second from /proc/[pid]/io; `None` when unreadable or first call.
    disk: Option<f64>,
    /// GPU % of the busiest engine; `None` when unknown (other users' processes without NVML).
    gpu: Option<f64>,
    /// "GPU <index> - <engine>" while the process uses a GPU.
    gpu_engine: Option<String>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Conn {
    pid: u32,
    name: String,
    proto: &'static str, // "tcp" | "udp"
    local: String,
    /// None for listening / unconnected sockets.
    remote: Option<String>,
    /// TCP state name; "listen" for unconnected UDP.
    state: &'static str,
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

#[derive(Default)]
struct Addrs {
    v4: Option<(String, u32)>,
    v6: Vec<String>,
}

/// Udev's PCI/USB database names → vendor + chip, e.g. "Realtek RTL8111" from
/// "RTL8111/8168/8211/8411 PCI Express Gigabit Ethernet Controller"; the chip is the first
/// plain word with both letters and digits ("AX200", "I219-LM"), else the whole name.
/// Kernel module taint flag `P` marks a proprietary license; drivers built into the kernel have
/// no module link and are open source by definition. None when the interface has no driver.
fn driver_open(n: &str) -> Option<bool> {
    let drv = format!("/sys/class/net/{n}/device/driver");
    std::fs::metadata(&drv).ok()?;
    Some(!read(format!("{drv}/module/taint")).contains('P'))
}

fn parse_nic_model(udev: &str) -> Option<String> {
    let get = |k: &str| {
        udev.lines()
            .find_map(|l| l.strip_prefix(&format!("E:{k}=")))
            .map(str::trim)
    };
    let model = get("ID_MODEL_FROM_DATABASE")?;
    let chip = model.split_whitespace().find(|w| {
        w.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-/*".contains(c))
            && w.chars().any(|c| c.is_ascii_alphabetic())
            && w.chars().any(|c| c.is_ascii_digit())
    });
    let model = match chip {
        Some(c) => c.split('/').next().unwrap_or(c).trim_end_matches('*'),
        None => model.split(" (").next().unwrap_or(model),
    };
    Some(
        match get("ID_VENDOR_FROM_DATABASE").and_then(|v| v.split_whitespace().next()) {
            Some(v) => format!("{v} {model}"),
            None => model.to_string(),
        },
    )
}

/// `iw dev <n> link` → None when "Not connected.".
fn parse_iw_link(text: &str) -> Option<Wifi> {
    let bssid = text
        .strip_prefix("Connected to ")?
        .split_whitespace()
        .next()?;
    let mut w = Wifi {
        bssid: bssid.to_string(),
        ..Default::default()
    };
    let num = |v: &str| v.split_whitespace().next()?.parse::<f64>().ok();
    for l in text.lines().map(str::trim) {
        let Some((k, v)) = l.split_once(": ") else {
            continue;
        };
        match k {
            "SSID" => w.ssid = v.to_string(),
            "freq" => w.freq = num(v),
            "signal" => w.signal = num(v).map(|d| d as i32),
            "tx bitrate" => w.tx_rate = num(v),
            "rx bitrate" => {
                w.rx_rate = num(v);
                for t in v.split_whitespace() {
                    let gen = match t.split('-').next() {
                        Some("EHT") => 7,
                        Some("HE") => 6,
                        Some("VHT") => 5,
                        Some("MCS") => 4, // HT rates print as "MCS 7"
                        _ => 0,
                    };
                    if gen > 0 {
                        w.generation.get_or_insert(gen);
                    }
                    if let Some(mhz) = t.strip_suffix("MHz") {
                        w.width = mhz.parse().ok();
                    }
                }
            }
            _ => {}
        }
    }
    Some(w)
}

/// /proc/net/{tcp,udp}{,6} address "0100007F:0035" → "127.0.0.1:53"; the IP is
/// 32-bit words printed in host order, the port is big-endian hex.
fn proc_addr(hex: &str) -> Option<(String, bool)> {
    let (ip, port) = hex.split_once(':')?;
    let port = u16::from_str_radix(port, 16).ok()?;
    let words: Vec<[u8; 4]> = (0..ip.len() / 8)
        .map(|i| {
            Some(
                u32::from_str_radix(ip.get(i * 8..i * 8 + 8)?, 16)
                    .ok()?
                    .to_ne_bytes(),
            )
        })
        .collect::<Option<_>>()?;
    let ip: std::net::IpAddr = match words.len() {
        1 => std::net::Ipv4Addr::from(words[0]).into(),
        4 => {
            let v6 = std::net::Ipv6Addr::from(<[u8; 16]>::try_from(words.concat()).ok()?);
            // dual-stack sockets show IPv4 peers as ::ffff:a.b.c.d
            v6.to_ipv4_mapped().map_or(v6.into(), Into::into)
        }
        _ => return None,
    };
    let unspecified = ip.is_unspecified() && port == 0;
    Some((std::net::SocketAddr::new(ip, port).to_string(), unspecified))
}

/// /proc/net/<proto> → (inode, local, remote, state).
fn parse_proc_net(text: &str, udp: bool) -> Vec<(u64, String, Option<String>, &'static str)> {
    const TCP: [&str; 12] = [
        "",
        "established",
        "syn_sent",
        "syn_recv",
        "fin_wait1",
        "fin_wait2",
        "time_wait",
        "close",
        "close_wait",
        "last_ack",
        "listen",
        "closing",
    ];
    text.lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let (local, _) = proc_addr(f.get(1)?)?;
            let (remote, none) = proc_addr(f.get(2)?)?;
            let st = usize::from_str_radix(f.get(3)?, 16).ok()?;
            let state = if udp {
                if st == 1 {
                    "established"
                } else {
                    "listen"
                }
            } else {
                TCP.get(st).copied().unwrap_or("")
            };
            Some((
                f.get(9)?.parse().ok()?,
                local,
                (!none).then_some(remote),
                state,
            ))
        })
        .collect()
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Conns {
    list: Vec<Conn>,
    /// System-wide TCP socket count per state (every socket in the netns, not only ours).
    tcp_states: HashMap<&'static str, u32>,
    /// Cumulative /proc/net/snmp counters; the frontend turns their deltas into a retransmit rate.
    out_segs: Option<u64>,
    retrans_segs: Option<u64>,
}

/// /proc/net/snmp → the `Tcp:` header/value line pair as name → value.
fn parse_snmp_tcp(text: &str) -> HashMap<String, u64> {
    let mut rows = text.lines().filter_map(|l| l.strip_prefix("Tcp:"));
    let (Some(keys), Some(vals)) = (rows.next(), rows.next()) else {
        return HashMap::new();
    };
    keys.split_whitespace()
        .zip(vals.split_whitespace())
        .filter_map(|(k, v)| Some((k.to_string(), v.parse().ok()?)))
        .collect()
}

/// Sockets owned by processes we can read (/proc/<pid>/fd is owner/root only).
pub fn connections() -> Conns {
    let mut owner: HashMap<u64, u32> = HashMap::new();
    for e in fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|p| p.parse::<u32>().ok()) else {
            continue;
        };
        for fd in fs::read_dir(e.path().join("fd"))
            .into_iter()
            .flatten()
            .flatten()
        {
            let link = fs::read_link(fd.path()).unwrap_or_default();
            if let Some(ino) = link
                .to_str()
                .and_then(|l| l.strip_prefix("socket:["))
                .and_then(|l| l.strip_suffix(']'))
                .and_then(|l| l.parse().ok())
            {
                owner.entry(ino).or_insert(pid);
            }
        }
    }
    let mut out = Vec::new();
    let mut tcp_states = HashMap::new();
    for (file, proto) in [
        ("tcp", "tcp"),
        ("tcp6", "tcp"),
        ("udp", "udp"),
        ("udp6", "udp"),
    ] {
        for (ino, local, remote, state) in
            parse_proc_net(&read(format!("/proc/net/{file}")), proto == "udp")
        {
            if proto == "tcp" {
                *tcp_states.entry(state).or_insert(0) += 1;
            }
            // inode 0: TIME_WAIT and other sockets no longer held by a process
            let Some(&pid) = owner.get(&ino).filter(|_| ino != 0) else {
                continue;
            };
            out.push(Conn {
                pid,
                name: read(format!("/proc/{pid}/comm")).trim().to_string(),
                proto,
                local,
                remote,
                state,
            });
        }
    }
    let snmp = parse_snmp_tcp(&read("/proc/net/snmp"));
    Conns {
        list: out,
        tcp_states,
        out_segs: snmp.get("OutSegs").copied(),
        retrans_segs: snmp.get("RetransSegs").copied(),
    }
}

fn parse_nameservers(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.strip_prefix("nameserver"))
        .map(|v| v.trim().to_string())
        .collect()
}

/// First IPv4 (with prefix length) and every IPv6 address of each interface.
fn if_addrs() -> HashMap<String, Addrs> {
    let mut out: HashMap<String, Addrs> = HashMap::new();
    let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs fills a linked list we only read and then free once.
    unsafe {
        if libc::getifaddrs(&mut ifap) != 0 {
            return out;
        }
        let mut p = ifap;
        while let Some(ifa) = p.as_ref() {
            if !ifa.ifa_addr.is_null() {
                let name = std::ffi::CStr::from_ptr(ifa.ifa_name)
                    .to_string_lossy()
                    .into_owned();
                let mask = ifa.ifa_netmask;
                match (*ifa.ifa_addr).sa_family as i32 {
                    libc::AF_INET => {
                        let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in);
                        let ip = std::net::Ipv4Addr::from(sin.sin_addr.s_addr.to_ne_bytes());
                        let bits = if mask.is_null() {
                            0
                        } else {
                            (*(mask as *const libc::sockaddr_in))
                                .sin_addr
                                .s_addr
                                .count_ones()
                        };
                        out.entry(name)
                            .or_default()
                            .v4
                            .get_or_insert((ip.to_string(), bits));
                    }
                    libc::AF_INET6 => {
                        let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in6);
                        let ip = std::net::Ipv6Addr::from(sin.sin6_addr.s6_addr);
                        let bits: u32 = if mask.is_null() {
                            0
                        } else {
                            let m = &*(mask as *const libc::sockaddr_in6);
                            m.sin6_addr.s6_addr.iter().map(|b| b.count_ones()).sum()
                        };
                        out.entry(name).or_default().v6.push(format!("{ip}/{bits}"));
                    }
                    _ => {}
                }
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

/// /proc/diskstats → name → (sectors read, sectors written, io_ticks ms).
fn parse_diskstats(text: &str) -> HashMap<String, (u64, u64, u64)> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            Some((
                f.get(2)?.to_string(),
                (
                    f.get(5)?.parse().ok()?,
                    f.get(9)?.parse().ok()?,
                    f.get(12)?.parse().ok()?,
                ),
            ))
        })
        .collect()
}

/// /proc/net/dev → name → the 16 counters (rx bytes, packets, errs, drop, …; tx starts at 8).
fn parse_net_dev(text: &str) -> HashMap<String, Vec<u64>> {
    text.lines()
        .skip(2)
        .filter_map(|l| {
            let (name, rest) = l.split_once(':')?;
            let f: Vec<u64> = rest
                .split_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect();
            (f.len() >= 12).then(|| (name.trim().to_string(), f))
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

/// Whether `path` lives on a spinning disk: deepest mount containing it → its physical disk's `queue/rotational`.
pub fn rotational(path: &Path) -> bool {
    let info = read("/proc/self/mountinfo");
    let dev = info
        .lines()
        .filter_map(|l| {
            let (left, right) = l.split_once(" - ")?;
            let mnt = left.split_whitespace().nth(4)?.replace("\\040", " ");
            let dev = right.split_whitespace().nth(1)?;
            path.starts_with(&mnt).then(|| (mnt.len(), dev.to_string()))
        })
        .max_by_key(|m| m.0) // last wins on ties: a later mount stacks on top
        .map(|m| m.1);
    dev.and_then(|d| disk_of(&d))
        .is_some_and(|d| read(format!("/sys/block/{d}/queue/rotational")).trim() == "1")
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

#[derive(Debug, PartialEq)]
struct ProcStat {
    name: String,
    state: char,
    ppid: u32,
    /// utime + stime
    ticks: u64,
    nice: i32,
    rss: u64,
}

/// /proc/[pid]/stat; comm may contain spaces and ')'.
fn parse_proc_stat(text: &str) -> Option<ProcStat> {
    let name = text.get(text.find('(')? + 1..text.rfind(')')?)?.to_string();
    let f: Vec<&str> = text[text.rfind(')')? + 1..].split_whitespace().collect();
    Some(ProcStat {
        name,
        state: f.first()?.chars().next()?,
        ppid: f.get(1)?.parse().ok()?,
        ticks: f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?,
        nice: f.get(16)?.parse().ok()?,
        rss: f.get(21)?.parse().ok()?,
    })
}

/// /proc/[pid]/io → read_bytes + write_bytes (only readable for our own processes).
fn parse_proc_io(text: &str) -> Option<u64> {
    let get = |k: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(k))
            .and_then(|v| v.trim().parse::<u64>().ok())
    };
    Some(get("read_bytes:")? + get("write_bytes:")?)
}

/// /proc/[pid]/cgroup (v2 line `0::<path>`) → (kind, unit); `uid` is ours (other users are "sys").
fn classify(cgroup: &str, uid: u32) -> (&'static str, Option<String>) {
    let Some(path) = cgroup.lines().find_map(|l| l.strip_prefix("0::")) else {
        return ("sys", None);
    };
    let mut parts = path.rsplit('/').filter(|p| !p.is_empty());
    let unit = parts.next().map(str::to_string);
    let parent = parts.next().unwrap_or("");
    let kind = if !path.contains(&format!("/user-{uid}.slice/")) {
        "sys"
    } else if parent == "app.slice" && unit.as_deref().is_some_and(|u| u.starts_with("app-")) {
        "app"
    } else {
        "bg"
    };
    (kind, unit)
}

/// systemd app unit → desktop id: `app[-<launcher>]-<id>-<n>.scope` / `app[-<launcher>]-<id>[@<x>].service`.
fn app_id(unit: &str) -> Option<String> {
    let rest = unit.strip_prefix("app-")?;
    let id = if let Some(r) = rest.strip_suffix(".scope") {
        r.rsplit_once('-')?.0
    } else {
        let r = rest.strip_suffix(".service")?;
        r.split('@').next()?
    };
    let id = id.replace("\\x2d", "-");
    // Launcher prefix only when what follows is a reverse-DNS id.
    let id = ["flatpak-", "gnome-", "kde-"]
        .iter()
        .find_map(|l| id.strip_prefix(l).filter(|r| r.contains('.')))
        .map_or(id.clone(), str::to_string);
    (!id.is_empty()).then_some(id)
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
        let ite = chip.starts_with("it8");
        let files: Vec<String> = fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        let mut found: Vec<_> = files
            .iter()
            .filter_map(|f| hwmon_kind(f).map(|(p, k, s)| (f, p, k, s)))
            // amdgpu has power1_average without power1_input; prefer _input when both exist
            .filter(|(f, p, _, _)| {
                !f.ends_with("_average") || !files.contains(&format!("{p}_input"))
            })
            .collect();
        found.sort_by_key(|&(_, p, k, _)| {
            (
                ["temp", "fan", "volt", "power"]
                    .iter()
                    .position(|&x| x == k),
                p.len(),
                p,
            )
        });
        for (f, prefix, kind, scale) in found {
            let Some(raw) = read_num(dir.join(f)) else {
                continue;
            };
            // it87 reports -128 °C for a header with no thermistor attached
            if kind == "temp" && raw / scale <= -100.0 {
                continue;
            }
            let label = read(dir.join(format!("{prefix}_label"))).trim().to_string();
            let limit = |s: &str| read_num(dir.join(format!("{prefix}_{s}"))).map(|v| v / 1000.0);
            // shortcut: crit outside 0–150 °C is treated as bogus (nvme reports 65261 °C), widen if a real chip needs it
            let crit = (kind == "temp")
                .then(|| limit("crit").or_else(|| limit("max")))
                .flatten()
                .filter(|c| *c > 0.0 && *c <= 150.0);
            let named = if !label.is_empty() {
                Some((label, 1.0))
            } else if ite {
                let t = read_num(dir.join(format!("{prefix}_type"))).map(|t| t as u32);
                ite_label(gigabyte(), prefix, t).map(|(l, m)| (l.to_string(), m))
            } else {
                None
            };
            let (label, mult) = named.unwrap_or_else(|| (prefix.to_string(), 1.0));
            out.push(Sensor {
                chip: chip.clone(),
                device: device.clone(),
                label,
                kind,
                value: raw / scale * mult,
                crit,
            });
        }
    }
    out
}

/// hwmon file name → (prefix, kind, divisor to °C / RPM / V / W); `None` for files we don't show.
fn hwmon_kind(file: &str) -> Option<(&str, &'static str, f64)> {
    let (prefix, suffix) = file.rsplit_once('_')?;
    let kind = prefix.trim_end_matches(|c: char| c.is_ascii_digit());
    if kind.len() == prefix.len() {
        return None;
    }
    let (k, scale) = match (kind, suffix) {
        ("temp", "input") => ("temp", 1000.0),
        ("fan", "input") => ("fan", 1.0),
        ("in", "input") => ("volt", 1000.0),
        ("power", "input" | "average") => ("power", 1_000_000.0),
        _ => return None,
    };
    Some((prefix, k, scale))
}

fn gigabyte() -> bool {
    static G: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *G.get_or_init(|| {
        read("/sys/class/dmi/id/board_vendor")
            .to_lowercase()
            .contains("gigabyte")
    })
}

/// Name and divider multiplier for an unlabeled ITE (it87) input. Temperatures go by `tempN_type`
/// (any board); voltages and fan1 follow Gigabyte's wiring, whose in1–in3 rails read exactly
/// 3.3 / 12 / 5 V after the dividers.
/// shortcut: in4+ and fan2+ differ per Gigabyte board generation and stay unnamed; add a per-board table if needed
fn ite_label(gigabyte: bool, prefix: &str, temp_type: Option<u32>) -> Option<(&'static str, f64)> {
    if prefix.starts_with("temp") {
        return match temp_type? {
            6 => Some(("CPU（PECI）", 1.0)),
            5 => Some(("CPU（AMD TSI）", 1.0)),
            4 => Some(("主機板", 1.0)),
            3 => Some(("主機板（二極體）", 1.0)),
            _ => None,
        };
    }
    if !gigabyte {
        return None;
    }
    match prefix {
        "in0" => Some(("CPU Vcore", 1.0)),
        "in1" => Some(("+3.3V", 1.65)),
        "in2" => Some(("+12V", 6.0)),
        "in3" => Some(("+5V", 2.5)),
        "fan1" => Some(("CPU 風扇", 1.0)),
        _ => None,
    }
}

/// CPU model as `system_info` gives it (`clean_cpu_name`), for `resourcewatch --widget`.
pub fn cpu_model() -> String {
    parse_cpuinfo(&read("/proc/cpuinfo")).0
}

/// Display name of the device behind a hwmon chip; `model` is `<hwmon>/device/model` (disks) or empty.
fn sensor_device(chip: &str, model: &str) -> String {
    static CPU: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    match chip {
        "coretemp" | "k10temp" | "zenpower" | "cpu_thermal" | "intel-rapl" => {
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

/// NVIDIA temperature, fan % and power from NVML (not exposed through hwmon); chip "nvidia".
fn nvml_sensors(gpus: &[Gpu]) -> Vec<Sensor> {
    gpus.iter()
        .filter(|g| g.nvml)
        .flat_map(|g| {
            [
                ("核心", "temp", g.temp),
                ("風扇", "percent", g.fan),
                ("功耗", "power", g.power),
            ]
            .into_iter()
            .filter_map(|(label, kind, v)| {
                Some(Sensor {
                    chip: "nvidia".into(),
                    device: g.name.clone(),
                    label: label.into(),
                    kind,
                    value: v?,
                    crit: None,
                })
            })
        })
        .collect()
}

pub struct Monitor {
    at: Option<Instant>,
    cpu: Vec<(u64, u64)>,
    disk: HashMap<String, (u64, u64, u64)>,
    net: HashMap<String, Vec<u64>>,
    procs_at: Option<Instant>,
    /// pid → (cpu ticks, disk bytes)
    procs: HashMap<u32, (u64, Option<u64>)>,
    gpus: Gpus,
    /// RAPL zone dir name → last `energy_uj`.
    rapl: HashMap<String, u64>,
}

/// RAPL energy counter delta; the counter wraps to 0 after `range` µJ.
fn energy_delta(prev: u64, cur: u64, range: u64) -> u64 {
    if cur >= prev {
        cur - prev
    } else {
        (cur + range).saturating_sub(prev)
    }
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
            rapl: HashMap::new(),
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
        let mut sensors = sensors();
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
        let gpus = self.gpus.sample();
        sensors.extend(nvml_sensors(&gpus));

        // CPU power (W) from RAPL energy counters; root-only unless `elevate("rapl")` opened them this boot
        let mut zones: Vec<_> = fs::read_dir("/sys/class/powercap")
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.starts_with("intel-rapl:"))
            .collect();
        zones.sort();
        let mut rapl = HashMap::new();
        let mut rapl_locked = false;
        for z in zones {
            let dir = Path::new("/sys/class/powercap").join(&z);
            let Ok(raw) = fs::read_to_string(dir.join("energy_uj")) else {
                rapl_locked = true;
                continue;
            };
            let Ok(e) = raw.trim().parse::<u64>() else {
                continue;
            };
            if let (Some(&p), Some(s)) = (self.rapl.get(&z), secs) {
                let range = read_num(dir.join("max_energy_range_uj")).unwrap_or(0.0) as u64;
                sensors.push(Sensor {
                    chip: "intel-rapl".into(),
                    device: sensor_device("intel-rapl", ""),
                    label: read(dir.join("name")).trim().to_string(),
                    kind: "power",
                    value: energy_delta(p, e, range) as f64 / 1e6 / s,
                    crit: None,
                });
            }
            rapl.insert(z, e);
        }
        self.rapl = rapl;

        let ds = parse_diskstats(&read("/proc/diskstats"));
        let mut disks: Vec<Disk> = ds
            .iter()
            .filter(|(n, _)| Path::new(&format!("/sys/block/{n}/device")).exists())
            .map(|(n, &(r, w, t))| {
                let prev = self.disk.get(n);
                Disk {
                    name: n.clone(),
                    model: read(format!("/sys/block/{n}/device/model"))
                        .trim()
                        .to_string(),
                    size: read_num(format!("/sys/block/{n}/size")).unwrap_or(0.0) as u64 * 512,
                    read_bps: rate(prev.map(|p| &p.0), r).map(|v| v * 512.0),
                    write_bps: rate(prev.map(|p| &p.1), w).map(|v| v * 512.0),
                    busy: rate(prev.map(|p| &p.2), t).map(|v| (v / 10.0).min(100.0)),
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
        let (mut addrs, mut gws) = (if_addrs(), parse_routes(&read("/proc/net/route")));
        let mut net: Vec<Net> = nd
            .iter()
            .filter(|(n, _)| Path::new(&format!("/sys/class/net/{n}/device")).exists())
            .map(|(n, f)| {
                let prev = self.net.get(n);
                let a = addrs.remove(n).unwrap_or_default();
                let sys = |k: &str| read(format!("/sys/class/net/{n}/{k}")).trim().to_string();
                let ifindex = sys("ifindex");
                Net {
                    name: n.clone(),
                    up: read(format!("/sys/class/net/{n}/operstate")).trim() == "up",
                    mac: read(format!("/sys/class/net/{n}/address"))
                        .trim()
                        .to_string(),
                    prefix: a.v4.as_ref().map(|v| v.1),
                    ip: a.v4.map(|v| v.0),
                    ipv6: a.v6,
                    gateway: gws.remove(n),
                    model: parse_nic_model(&read(format!("/run/udev/data/n{ifindex}"))),
                    driver: std::fs::read_link(format!("/sys/class/net/{n}/device/driver"))
                        .ok()
                        .and_then(|p| Some(p.file_name()?.to_string_lossy().into_owned())),
                    driver_open: driver_open(n),
                    // -1 (unknown) when down or on many Wi-Fi drivers
                    speed: sys("speed")
                        .parse::<i64>()
                        .ok()
                        .filter(|&v| v > 0)
                        .map(|v| v as u32),
                    duplex: Some(sys("duplex")).filter(|d| d == "full" || d == "half"),
                    mtu: sys("mtu").parse().ok(),
                    rx_total: f[0],
                    tx_total: f[8],
                    rx_bps: rate(prev.map(|p| &p[0]), f[0]),
                    tx_bps: rate(prev.map(|p| &p[8]), f[8]),
                    rx_packets: f[1],
                    tx_packets: f[9],
                    rx_errors: f[2],
                    tx_errors: f[10],
                    rx_dropped: f[3],
                    tx_dropped: f[11],
                    // shortcut: spawns `iw` per tick, upgrade to nl80211 netlink if it shows up in profiles
                    wifi: Path::new(&format!("/sys/class/net/{n}/wireless"))
                        .exists()
                        .then(|| {
                            std::process::Command::new("iw")
                                .args(["dev", n, "link"])
                                .output()
                                .ok()
                        })
                        .flatten()
                        .and_then(|o| parse_iw_link(&String::from_utf8_lossy(&o.stdout))),
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
            gpus,
            disks,
            mounts,
            net,
            // stub 127.0.0.53 in /etc/resolv.conf hides the real servers when resolved runs
            dns: Some(parse_nameservers(&read("/run/systemd/resolve/resolv.conf")))
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| parse_nameservers(&read("/etc/resolv.conf"))),
            sensors,
            rapl_locked,
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
        // SAFETY: getuid has no preconditions.
        let uid = unsafe { libc::getuid() };
        let mut seen = HashMap::new();
        let mut list: Vec<Proc> = fs::read_dir("/proc")
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let pid: u32 = e.file_name().to_str()?.parse().ok()?;
                let st = parse_proc_stat(&read(e.path().join("stat")))?;
                let io = parse_proc_io(&read(e.path().join("io")));
                let prev = self.procs.get(&pid);
                seen.insert(pid, (st.ticks, io));
                let cpu = prev.zip(secs).map(|(p, s)| {
                    (st.ticks.saturating_sub(p.0) as f64 / tck / s / threads * 100.0).min(100.0)
                });
                let disk = io
                    .zip(prev.and_then(|p| p.1))
                    .zip(secs)
                    .map(|((c, p), s)| c.saturating_sub(p) as f64 / s);
                let (kind, unit) = if pid == 2 || st.ppid == 2 {
                    ("sys", Some("kernel".to_string()))
                } else {
                    classify(&read(e.path().join("cgroup")), uid)
                };
                let app = (kind == "app")
                    .then(|| unit.as_deref().and_then(app_id))
                    .flatten();
                Some(Proc {
                    pid,
                    name: st.name,
                    cpu,
                    mem: st.rss * page,
                    state: st.state,
                    nice: st.nice,
                    kind,
                    unit,
                    app,
                    disk,
                    gpu: None,
                    gpu_engine: None,
                })
            })
            .collect();
        self.procs = seen;
        // /proc/[pid]/io is readable exactly for the processes whose fdinfo we can read too
        let own: Vec<u32> = (self.procs.iter())
            .filter(|(_, v)| v.1.is_some())
            .map(|(&p, _)| p)
            .collect();
        let (gpu, all) = self.gpus.proc_util(&own, secs);
        for p in &mut list {
            match gpu.get(&p.pid) {
                Some((v, e)) => (p.gpu, p.gpu_engine) = (Some(*v), (*v > 0.0).then(|| e.clone())),
                None if all || self.procs.get(&p.pid).is_some_and(|v| v.1.is_some()) => {
                    p.gpu = Some(0.0)
                }
                None => {}
            }
        }
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ite_labels() {
        assert_eq!(ite_label(true, "in2", None), Some(("+12V", 6.0)));
        assert_eq!(ite_label(true, "in1", None), Some(("+3.3V", 1.65)));
        assert_eq!(ite_label(false, "in2", None), None); // dividers are Gigabyte-only
        assert_eq!(ite_label(true, "in5", None), None);
        assert_eq!(
            ite_label(false, "temp3", Some(6)),
            Some(("CPU（PECI）", 1.0))
        );
        assert_eq!(ite_label(false, "temp1", Some(4)), Some(("主機板", 1.0)));
        assert_eq!(ite_label(true, "temp4", None), None);
        assert_eq!(ite_label(true, "fan1", None), Some(("CPU 風扇", 1.0)));
    }

    #[test]
    fn energy_wrap() {
        assert_eq!(energy_delta(100, 250, 1000), 150);
        assert_eq!(energy_delta(900, 50, 1000), 150);
        assert_eq!(energy_delta(900, 50, 0), 0); // unknown range: no negative / overflow
    }

    #[test]
    fn hwmon_kinds() {
        assert_eq!(hwmon_kind("temp1_input"), Some(("temp1", "temp", 1000.0)));
        assert_eq!(hwmon_kind("fan2_input"), Some(("fan2", "fan", 1.0)));
        assert_eq!(hwmon_kind("in0_input"), Some(("in0", "volt", 1000.0)));
        assert_eq!(
            hwmon_kind("power1_average"),
            Some(("power1", "power", 1_000_000.0))
        );
        assert_eq!(hwmon_kind("temp1_crit"), None);
        assert_eq!(hwmon_kind("temp_input"), None);
        assert_eq!(hwmon_kind("name"), None);
        assert_eq!(hwmon_kind("curr1_input"), None);
    }

    #[test]
    fn sensor_device_names() {
        assert_eq!(
            sensor_device("nvme", "Micron_2400_MTFDKBA512QFM"),
            "Micron 2400 MTFDKBA512QFM"
        );
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
        let d = parse_diskstats(" 259 0 nvme0n1 10 0 2048 5 20 0 4096 9 0 77 0\n");
        assert_eq!(d["nvme0n1"], (2048, 4096, 77));
        let n = parse_net_dev("h1\nh2\n  eth0: 100 1 0 0 0 0 0 0 200 2 0 0 0 0 0 0\n");
        assert_eq!((n["eth0"][0], n["eth0"][1], n["eth0"][8]), (100, 1, 200));
        let iw = "Connected to aa:bb:cc:dd:ee:ff (on wlan0)\n\tSSID: Home Net\n\tfreq: 5180.0\n\tsignal: -52 dBm\n\trx bitrate: 1200.9 MBit/s 80MHz HE-MCS 11 HE-NSS 2 HE-GI 0\n\ttx bitrate: 864.8 MBit/s 80MHz HE-MCS 8\n";
        assert_eq!(
            parse_iw_link(iw),
            Some(Wifi {
                ssid: "Home Net".into(),
                bssid: "aa:bb:cc:dd:ee:ff".into(),
                freq: Some(5180.0),
                signal: Some(-52),
                rx_rate: Some(1200.9),
                tx_rate: Some(864.8),
                generation: Some(6),
                width: Some(80),
            })
        );
        assert_eq!(parse_iw_link("Not connected.\n"), None);
        let snmp = "Ip: Forwarding\nIp: 1\nTcp: ActiveOpens OutSegs RetransSegs\nTcp: 5 1000 20\nUdp: InDatagrams\nUdp: 9\n";
        let t = parse_snmp_tcp(snmp);
        assert_eq!((t["OutSegs"], t["RetransSegs"]), (1000, 20));
        let tcp = "  sl  local_address rem_address   st\n   0: 0100007F:0035 00000000:0000 0A 00000000:00000000 00:00000000 00000000   991        0 22983 1\n   1: D432A8C0:A1B2 22D8B85D:01BB 01 00000000:00000000 00:00000000 00000000  1000        0 555 1\n";
        assert_eq!(
            parse_proc_net(tcp, false),
            [
                (22983, "127.0.0.1:53".into(), None, "listen"),
                (
                    555,
                    "192.168.50.212:41394".into(),
                    Some("93.184.216.34:443".into()),
                    "established"
                ),
            ]
        );
        let tcp6 = "h\n 0: 00000000000000000000000001000000:0277 0000000000000000FFFF00000100007F:1F90 01 0 0 0 0 0 7 1\n";
        assert_eq!(
            parse_proc_net(tcp6, false)[0],
            (
                7,
                "[::1]:631".into(),
                Some("127.0.0.1:8080".into()),
                "established"
            )
        );
        assert_eq!(parse_nic_model("E:ID_VENDOR_FROM_DATABASE=Realtek Semiconductor Co., Ltd.\nE:ID_MODEL_FROM_DATABASE=RTL8111/8168 PCI Express Gigabit Ethernet Controller (Onboard Ethernet)\n").as_deref(), Some("Realtek RTL8111"));
        let m = |v: &str, d: &str| {
            parse_nic_model(&format!(
                "E:ID_VENDOR_FROM_DATABASE={v}\nE:ID_MODEL_FROM_DATABASE={d}\n"
            ))
        };
        assert_eq!(
            m(
                "Intel Corporation",
                "Wi-Fi 6E(802.11ax) AX210/AX1675* 2x2 [Typhoon Peak]"
            )
            .as_deref(),
            Some("Intel AX210")
        );
        assert_eq!(
            m("Intel Corporation", "Ethernet Connection (17) I219-LM").as_deref(),
            Some("Intel I219-LM")
        );
        assert_eq!(
            m("Intel Corporation", "Ethernet Controller").as_deref(),
            Some("Intel Ethernet Controller")
        );
        assert_eq!(
            parse_nameservers("# x\nnameserver 1.1.1.1\nsearch .\nnameserver ::1\n"),
            ["1.1.1.1", "::1"]
        );
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
        let st = parse_proc_stat(line).unwrap();
        assert_eq!(
            (
                st.name.as_str(),
                st.state,
                st.ppid,
                st.ticks,
                st.nice,
                st.rss
            ),
            ("a (b) c", 'S', 1, 10, 0, 25)
        );
        assert_eq!(parse_proc_stat("garbage"), None);
    }

    #[test]
    fn proc_io() {
        let t =
            "rchar: 9\nwchar: 9\nread_bytes: 4096\nwrite_bytes: 8192\ncancelled_write_bytes: 0\n";
        assert_eq!(parse_proc_io(t), Some(12288));
        assert_eq!(parse_proc_io(""), None);
    }

    #[test]
    fn cgroup_kind() {
        let u = "0::/user.slice/user-1000.slice/user@1000.service/";
        let c = |s: &str| classify(&format!("{u}{s}\n"), 1000);
        assert_eq!(
            c("app.slice/app-code-4440.scope"),
            ("app", Some("app-code-4440.scope".into()))
        );
        assert_eq!(c("app.slice/ntfs-nag.service").0, "bg");
        assert_eq!(c("session.slice/plasma-kwin_wayland.service").0, "bg");
        assert_eq!(
            classify("0::/user.slice/user-1000.slice/session-3.scope", 1000).0,
            "bg"
        );
        assert_eq!(classify("0::/system.slice/sshd.service", 1000).0, "sys");
        assert_eq!(
            classify(&format!("{u}app.slice/app-x-1.scope"), 1001).0,
            "sys"
        );
        assert_eq!(classify("0::/", 1000), ("sys", None));
        assert_eq!(classify("", 1000), ("sys", None));
    }

    #[test]
    fn app_ids() {
        assert_eq!(app_id("app-code-4440.scope").as_deref(), Some("code"));
        assert_eq!(
            app_id("app-steam@autostart.service").as_deref(),
            Some("steam")
        );
        assert_eq!(app_id("app-code@e9d30105.service").as_deref(), Some("code"));
        assert_eq!(
            app_id("app-flatpak-org.fcitx.Fcitx5-3782739276.scope").as_deref(),
            Some("org.fcitx.Fcitx5")
        );
        assert_eq!(
            app_id("app-gnome-org.gnome.Terminal-123.scope").as_deref(),
            Some("org.gnome.Terminal")
        );
        assert_eq!(
            app_id("app-google\\x2dchrome-55.scope").as_deref(),
            Some("google-chrome")
        );
        assert_eq!(app_id("ntfs-nag.service"), None);
    }
}
