//! Fault-injection tests: malformed external data, devices disappearing at
//! run time, permission errors and extreme counter values must never panic
//! and must yield missing values (`None` / `"--"`) instead of fake numbers.

use crate::collectors::testutil::{roots, temp_root, write};
use crate::collectors::{cpu, disk, gpu, memory, network, process, system};
use crate::monitor::Monitor;
use crate::settings::Settings;
use crate::snapshot::MetricStatus;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

/// Deterministic xorshift PRNG (no external crate, reproducible failures).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn bytes(&mut self, max: usize) -> Vec<u8> {
        let n = (self.next() as usize) % max;
        (0..n).map(|_| self.next() as u8).collect()
    }
    /// Text built from tokens that look like the real files.
    fn tokens(&mut self) -> String {
        const T: &[&str] = &[
            "cpu",
            "cpu0",
            " ",
            "  ",
            "\n",
            "\t",
            ":",
            "kB",
            "MemTotal",
            "MemAvailable",
            "SwapTotal",
            "SwapFree",
            "0",
            "1",
            "-1",
            "18446744073709551615",
            "99999999999999999999",
            "NaN",
            "inf",
            "sda",
            "nvme0n1",
            "enp3s0",
            "lo",
            "|",
            "x",
            "\\x20",
            "\\x",
            "\\xZZ",
            "E:ID_MODEL_ENC=",
        ];
        let n = (self.next() as usize) % 60;
        (0..n)
            .map(|_| T[(self.next() as usize) % T.len()])
            .collect()
    }
}

#[test]
fn parsers_never_panic_on_garbage() {
    let mut r = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..3000 {
        let raw = String::from_utf8_lossy(&r.bytes(400)).into_owned();
        let tok = r.tokens();
        for text in [&raw, &tok] {
            let _ = cpu::parse_stat(text);
            let _ = cpu::parse_topology(text);
            let _ = memory::parse_meminfo(text);
            let _ = memory::parse_pressure(text);
            let _ = process::parse_stat(text);
            let _ = disk::parse_diskstats(text);
            let _ = disk::parse_mountinfo(text);
            let _ = disk::udev_unescape(text);
            let _ = network::parse_net_dev(text);
            let _ = network::parse_default_route(text);
            let _ = network::parse_arp(text, "192.168.0.1", "eth0");
            let _ = gpu::pci_ids_lookup(text, 0x10de, 0x1cb6);
            let _ = gpu::parse_dpm_clock(text);
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
                let _ = crate::settings_store::parse_stored(&v);
            }
        }
        let _ = system::parse_smbios_type17(&r.bytes(64));
        let _ = system::format_speeds(&[(r.next() % 20_000) as u32, (r.next() % 20_000) as u32]);
    }
}

#[test]
fn extreme_counter_values() {
    let max = u64::MAX;
    // CPU: sums that would overflow are rejected.
    assert!(cpu::parse_stat(&format!("cpu {max} 1 1 1 1 1 1 1"))
        .0
        .is_none());
    // Memory: kB values that overflow when converted to bytes are rejected.
    assert_eq!(
        memory::parse_meminfo(&format!("MemTotal: {max} kB\n")).ram_total,
        None
    );
    // Disk / network: maximum counters parse; a wrap to small values is a reset.
    let d = disk::parse_diskstats(&format!(" 8 0 sda 1 2 {max} 4 5 6 {max} 8 0 9 10\n"));
    assert_eq!(d["sda"], (max, max));
    assert_eq!(crate::collectors::counter_rate(max, 5, 1.0), None);
    let n = network::parse_net_dev(&format!(
        "h\nh\neth0: {max} 0 0 0 0 0 0 0 {max} 0 0 0 0 0 0 0\n"
    ));
    assert_eq!(n["eth0"], (max, max));
    // Uptime formatting saturates instead of overflowing.
    assert!(!crate::snapshot::format_uptime(Some(1e300)).is_empty());
}

fn base_system(r: &std::path::Path) {
    write(r, "proc/stat", "cpu  100 0 100 800 0 0 0 0 0 0\n");
    write(
        r,
        "proc/meminfo",
        "MemTotal: 1000 kB\nMemAvailable: 250 kB\nSwapTotal: 100 kB\nSwapFree: 50 kB\n",
    );
    write(r, "proc/uptime", "100.0 1.0\n");
    write(r, "proc/cpuinfo", "model name : Test CPU\n");
    write(
        r,
        "proc/diskstats",
        " 8 0 sda 1 2 1000 4 5 6 2000 8 0 9 10\n",
    );
    write(
        r,
        "proc/net/dev",
        "h\nh\nenp3s0: 1000 1 0 0 0 0 0 0 2000 1 0 0 0 0 0 0\n",
    );
    std::fs::create_dir_all(r.join("sys/class/net/enp3s0/device")).unwrap();
    write(r, "sys/class/hwmon/hwmon0/name", "coretemp\n");
    write(r, "sys/class/hwmon/hwmon0/temp2_label", "Core 0\n");
    write(r, "sys/class/hwmon/hwmon0/temp2_input", "45000\n");
    let dev = r.join("sys/devices/pci0000:00/ata1/host0/block/sda");
    write(&dev, "size", "1000\n");
    write(&dev, "dev", "8:0\n");
    write(&dev, "device/type", "0\n");
    write(&dev, "device/model", "Disk\n");
    std::fs::create_dir_all(r.join("sys/block")).unwrap();
    std::os::unix::fs::symlink(&dev, r.join("sys/block/sda")).unwrap();
}

#[test]
fn devices_disappearing_and_returning_at_runtime() {
    let r = temp_root("robust-remove");
    base_system(&r);
    let s = Settings::default();
    let mut m = Monitor::with_roots(roots(&r), &s);
    let t0 = Instant::now();
    m.sample(t0, &s);
    write(&r, "proc/stat", "cpu  200 0 100 900 0 0 0 0 0 0\n");
    let ok = m.sample(t0 + Duration::from_secs(1), &s);
    assert_eq!(ok.cpu.display_value, "50%");
    assert_eq!(ok.cpu_temp_text, "45.0 °C");
    assert!(ok.disk_rates["sda"].read.is_some());
    assert!(ok.net_down.raw_value.is_some());

    // Disk vanishes from diskstats, NIC removed, hwmon chip gone, meminfo
    // and /proc/stat unreadable/garbage.
    write(&r, "proc/diskstats", "");
    write(&r, "proc/net/dev", "h\nh\n");
    std::fs::remove_dir_all(r.join("sys/class/hwmon/hwmon0")).unwrap();
    write(&r, "proc/meminfo", "garbage\n");
    write(&r, "proc/stat", "not a stat file\n");
    std::fs::remove_file(r.join("proc/uptime")).unwrap();
    let gone = m.sample(t0 + Duration::from_secs(2), &s);
    assert_eq!(gone.cpu.percent, None);
    assert_eq!(gone.cpu_temp_text, "--");
    assert_eq!(gone.ram.display_value, "--");
    assert_eq!(
        gone.disk_rates["sda"].read, None,
        "missing device is None, not 0"
    );
    assert_eq!(gone.net_down.raw_value, None);
    assert_eq!(gone.uptime_seconds, None);
    assert_eq!(
        gone.history.cpu.last(),
        Some(&None),
        "history keeps the gap"
    );

    // Devices come back: values recover on the following samples.
    base_system_restore(&r);
    m.sample(t0 + Duration::from_secs(3), &s);
    write(&r, "proc/stat", "cpu  300 0 100 1000 0 0 0 0 0 0\n");
    write(
        &r,
        "proc/diskstats",
        " 8 0 sda 1 2 3000 4 5 6 2000 8 0 9 10\n",
    );
    let back = m.sample(t0 + Duration::from_secs(4), &s);
    assert_eq!(back.cpu.display_value, "50%");
    assert_eq!(back.ram.display_value, "75%");
    assert!(back.disk_rates["sda"].read.is_some());
}

fn base_system_restore(r: &std::path::Path) {
    write(
        r,
        "proc/meminfo",
        "MemTotal: 1000 kB\nMemAvailable: 250 kB\nSwapTotal: 100 kB\nSwapFree: 50 kB\n",
    );
    write(r, "proc/uptime", "103.0 1.0\n");
    write(r, "proc/stat", "cpu  200 0 100 900 0 0 0 0 0 0\n");
    write(
        r,
        "proc/diskstats",
        " 8 0 sda 1 2 1000 4 5 6 2000 8 0 9 10\n",
    );
    write(
        r,
        "proc/net/dev",
        "h\nh\nenp3s0: 1000 1 0 0 0 0 0 0 2000 1 0 0 0 0 0 0\n",
    );
}

#[test]
fn permission_denied_yields_missing_values() {
    if unsafe { libc::geteuid() } == 0 {
        return; // root ignores permission bits
    }
    let r = temp_root("robust-perm");
    base_system(&r);
    for f in [
        "proc/stat",
        "proc/meminfo",
        "proc/uptime",
        "proc/diskstats",
        "proc/net/dev",
        "proc/cpuinfo",
    ] {
        std::fs::set_permissions(r.join(f), std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    std::fs::set_permissions(
        r.join("sys/class/hwmon/hwmon0/temp2_input"),
        std::fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    let s = Settings::default();
    let mut m = Monitor::with_roots(roots(&r), &s);
    let t0 = Instant::now();
    m.sample(t0, &s);
    let snap = m.sample(t0 + Duration::from_secs(1), &s);
    assert_eq!(snap.cpu.percent, None);
    assert_eq!(snap.ram.percent, None);
    assert_eq!(snap.cpu_temp_text, "--");
    assert_eq!(snap.cpu_name, "--");
    assert_eq!(snap.uptime_seconds, None);
    assert_eq!(snap.disk_rates["sda"].read, None);
    assert_eq!(snap.net_up.raw_value, None);
    assert!(matches!(
        snap.cpu.status,
        MetricStatus::Loading | MetricStatus::Unavailable
    ));
    // Restore so the temp dir can be cleaned up.
    for f in [
        "proc/stat",
        "proc/meminfo",
        "proc/uptime",
        "proc/diskstats",
        "proc/net/dev",
        "proc/cpuinfo",
    ] {
        std::fs::set_permissions(r.join(f), std::fs::Permissions::from_mode(0o644)).unwrap();
    }
}

#[test]
fn ram_speed_without_permission_or_cache_is_unknown() {
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let r = temp_root("robust-smbios");
    write(&r, "sys/firmware/dmi/entries/17-0/raw", "\u{11}");
    std::fs::set_permissions(
        r.join("sys/firmware/dmi/entries/17-0/raw"),
        std::fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    assert_eq!(
        system::ram_speed_text(&roots(&r), Some(&r.join("missing.json"))),
        None
    );
}
