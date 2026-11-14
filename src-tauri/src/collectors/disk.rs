//! Physical disk discovery (`/sys/block`) and read/write rates
//! (`/proc/diskstats`). Discovery runs only at startup and on manual refresh;
//! rate sampling reads one in-kernel file per interval (no `cat`/`lsblk`).

use super::{counter_rate, list_dir, read_trimmed, read_u64, Roots};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// `/proc/diskstats` and `/sys/block/*/size` always count 512-byte sectors.
const SECTOR: u64 = 512;

/// name → cumulative (first, second) counters.
type Counters = HashMap<String, (u64, u64)>;

#[derive(Debug, Clone, PartialEq)]
pub struct DiskDevice {
    pub name: String,
    pub model: Option<String>,
    pub size_bytes: u64,
    /// `sata` | `sas` | `scsi` | `nvme`
    pub transport: String,
}

fn is_nvme_namespace(name: &str) -> bool {
    // nvme0n1 (not nvme0c0n1 multipath paths, not partitions)
    let Some(rest) = name.strip_prefix("nvme") else {
        return false;
    };
    let Some((ctrl, ns)) = rest.split_once('n') else {
        return false;
    };
    !ctrl.is_empty()
        && !ns.is_empty()
        && ctrl.bytes().all(|b| b.is_ascii_digit())
        && ns.bytes().all(|b| b.is_ascii_digit())
}

/// Same filter as the legacy `lsblk` code: TYPE=disk, size > 0,
/// transport sata/ata/pata/ide/sas/scsi/nvme. USB, optical, virtual
/// (loop/zram/dm/md), virtio and MMC devices are excluded.
pub fn classify(name: &str, real_path: &str) -> Option<&'static str> {
    if real_path.contains("/virtual/") || real_path.contains("/usb") {
        return None;
    }
    if is_nvme_namespace(name) {
        return Some("nvme");
    }
    if !name.starts_with("sd") && !name.starts_with("hd") {
        return None; // sr*, vd*, mmcblk*, xvd*, ...
    }
    if real_path.contains("/ata") || name.starts_with("hd") {
        Some("sata")
    } else if real_path.contains("/end_device-") || real_path.contains("/sas_") {
        Some("sas")
    } else {
        Some("scsi")
    }
}

/// `MAJ:MIN` from `/sys/block/<dev>/dev`, used for `lsblk`-compatible order.
fn dev_number(dir: &Path) -> (u32, u32) {
    read_trimmed(&dir.join("dev"))
        .and_then(|s| {
            let (a, b) = s.split_once(':')?;
            Some((a.parse().ok()?, b.parse().ok()?))
        })
        .unwrap_or((u32::MAX, u32::MAX))
}

/// Decodes udev's `\xHH` escapes (e.g. `ID_MODEL_ENC=TOSHIBA\x20DT01ABA200V`).
pub fn udev_unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 4 <= b.len() && b[i + 1] == b'x' {
            if let Some(v) = std::str::from_utf8(&b[i + 2..i + 4])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(v);
                i += 4;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Full model name as `lsblk` shows it: udev `ID_MODEL_ENC` (the SCSI sysfs
/// `device/model` is truncated to 16 characters), else sysfs `device/model`.
/// Reads the udev database file directly; no `udevadm` process.
fn disk_model(roots: &Roots, dir: &Path, (major, minor): (u32, u32)) -> Option<String> {
    let udev = std::fs::read_to_string(roots.run(&format!("udev/data/b{major}:{minor}")))
        .ok()
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("E:ID_MODEL_ENC=").map(udev_unescape))
        });
    udev.or_else(|| read_trimmed(&dir.join("device/model")))
        .map(|m| m.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|m| !m.is_empty())
}

/// Physical disks in `lsblk` order (by device number), so "硬碟1" is the
/// same device as in the legacy widget.
pub fn discover(roots: &Roots) -> Vec<DiskDevice> {
    let mut dirs = list_dir(&roots.sys("block"), |_| true);
    dirs.sort_by_key(|d| dev_number(d));
    dirs.into_iter()
        .filter_map(|dir| {
            let name = dir.file_name()?.to_str()?.to_string();
            // SCSI peripheral type 0 = disk (5 = CD/DVD); NVMe has no `type`.
            if let Some(t) = read_trimmed(&dir.join("device/type")) {
                if t != "0" {
                    return None;
                }
            }
            let real = std::fs::canonicalize(&dir).ok()?;
            let transport = classify(&name, &real.to_string_lossy())?;
            let size_bytes = read_u64(&dir.join("size"))?.checked_mul(SECTOR)?;
            if size_bytes == 0 {
                return None; // empty card reader slot etc.
            }
            let model = disk_model(roots, &dir, dev_number(&dir));
            Some(DiskDevice {
                name,
                model,
                size_bytes,
                transport: transport.to_string(),
            })
        })
        .collect()
}

/// name → (sectors read, sectors written)
pub fn parse_diskstats(text: &str) -> HashMap<String, (u64, u64)> {
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 10 {
                return None;
            }
            Some((f[2].to_string(), (f[5].parse().ok()?, f[9].parse().ok()?)))
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiskRateReading {
    pub read: Option<f64>,
    pub write: Option<f64>,
}

pub struct DiskCollector {
    path: std::path::PathBuf,
    prev: Option<(Instant, Counters)>,
}

impl DiskCollector {
    pub fn new(roots: &Roots) -> Self {
        Self {
            path: roots.proc("diskstats"),
            prev: None,
        }
    }

    /// Forget previous counters (after a disk rescan): next sample has no rate.
    pub fn reset(&mut self) {
        self.prev = None;
    }

    fn read(path: &Path) -> Option<HashMap<String, (u64, u64)>> {
        std::fs::read_to_string(path)
            .ok()
            .map(|t| parse_diskstats(&t))
    }

    /// Rates (B/s) for each given disk, using the cycle timestamp `now`.
    /// Missing device, first sample or a counter reset yields `None`.
    pub fn sample_at(
        &mut self,
        disks: &[DiskDevice],
        now: Instant,
    ) -> BTreeMap<String, DiskRateReading> {
        let cur = Self::read(&self.path);
        let mut out = BTreeMap::new();
        for d in disks {
            let rate = match (&self.prev, &cur) {
                (Some((t0, prev)), Some(cur)) => {
                    let secs = now.saturating_duration_since(*t0).as_secs_f64();
                    match (prev.get(&d.name), cur.get(&d.name)) {
                        (Some(&(r0, w0)), Some(&(r1, w1))) => DiskRateReading {
                            // Rate in sectors first, so huge counters cannot overflow.
                            read: counter_rate(r0, r1, secs).map(|v| v * SECTOR as f64),
                            write: counter_rate(w0, w1, secs).map(|v| v * SECTOR as f64),
                        },
                        _ => DiskRateReading {
                            read: None,
                            write: None,
                        },
                    }
                }
                _ => DiskRateReading {
                    read: None,
                    write: None,
                },
            };
            out.insert(d.name.clone(), rate);
        }
        self.prev = cur.map(|c| (now, c));
        out
    }
}

/// One line of `/proc/self/mountinfo`: device number, mount point and
/// mount source (after the ` - fstype` separator).
#[derive(Debug, Clone, PartialEq)]
pub struct Mount {
    pub dev: (u32, u32),
    pub mount_point: PathBuf,
    pub source: String,
}

/// Mountinfo escapes space, tab, newline and backslash as `\ooo` octal.
fn unescape_mount(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c))
        {
            out.push((b[i + 1] - b'0') * 64 + (b[i + 2] - b'0') * 8 + (b[i + 3] - b'0'));
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn parse_mountinfo(text: &str) -> Vec<Mount> {
    text.lines()
        .filter_map(|line| {
            let (left, right) = line.split_once(" - ")?;
            let f: Vec<&str> = left.split_whitespace().collect();
            let (maj, min) = f.get(2)?.split_once(':')?;
            let source = right.split_whitespace().nth(1)?;
            Some(Mount {
                dev: (maj.parse().ok()?, min.parse().ok()?),
                mount_point: PathBuf::from(unescape_mount(f.get(4)?)),
                source: unescape_mount(source),
            })
        })
        .collect()
}

/// Kernel block device name behind a mount. The source path is tried first
/// because btrfs reports an anonymous device number (`0:N`); `/dev/mapper/*`
/// symlinks resolve to `dm-N`.
fn block_name(roots: &Roots, m: &Mount) -> Option<String> {
    if let Some(rest) = m.source.strip_prefix("/dev/") {
        let real = std::fs::canonicalize(&m.source).ok();
        let name = real
            .as_deref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| rest.rsplit('/').next().unwrap_or(rest));
        return Some(name.to_string());
    }
    if m.dev.0 == 0 {
        return None; // virtual filesystem
    }
    let link = std::fs::read_link(roots.sys(&format!("dev/block/{}:{}", m.dev.0, m.dev.1))).ok()?;
    Some(link.file_name()?.to_str()?.to_string())
}

/// Physical disk (from `disks`) that holds block device `name`: the disk
/// itself, one of its partitions, or — for device-mapper (LUKS, LVM) — the
/// disk under its first `slaves` entry.
// shortcut: an LVM volume spanning several disks is counted on its first disk only; split by extent if that matters.
fn owning_disk(roots: &Roots, disks: &[DiskDevice], name: &str, depth: u8) -> Option<String> {
    if disks.iter().any(|d| d.name == name) {
        return Some(name.to_string());
    }
    if let Some(d) = disks
        .iter()
        .find(|d| roots.sys(&format!("block/{}/{name}", d.name)).exists())
    {
        return Some(d.name.clone());
    }
    if depth == 0 {
        return None;
    }
    list_dir(&roots.sys(&format!("block/{name}/slaves")), |_| true)
        .iter()
        .filter_map(|p| p.file_name()?.to_str())
        .find_map(|slave| owning_disk(roots, disks, slave, depth - 1))
}

/// (used, total) bytes of a mounted filesystem, as `df` computes them.
fn statvfs(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: zeroed POD struct filled by statvfs(3) on success.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let frsize = st.f_frsize as u64;
    let total = (st.f_blocks as u64).checked_mul(frsize)?;
    let used = (st.f_blocks as u64)
        .checked_sub(st.f_bfree as u64)?
        .checked_mul(frsize)?;
    (total > 0).then_some((used, total))
}

/// Used / total bytes per disk over every mounted filesystem on it. Each
/// block device is counted once even when mounted many times (btrfs
/// subvolumes, bind mounts). Disks with nothing mounted are absent.
pub fn usage(roots: &Roots, disks: &[DiskDevice]) -> BTreeMap<String, (u64, u64)> {
    let mounts = std::fs::read_to_string(roots.proc("self/mountinfo"))
        .map(|t| parse_mountinfo(&t))
        .unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    let mut out: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for m in &mounts {
        let Some(dev) = block_name(roots, m) else {
            continue;
        };
        let Some(disk) = owning_disk(roots, disks, &dev, 4) else {
            continue;
        };
        if !seen.insert(dev) {
            continue;
        }
        if let Some((used, total)) = statvfs(&m.mount_point) {
            let e = out.entry(disk).or_default();
            e.0 = e.0.saturating_add(used);
            e.1 = e.1.saturating_add(total);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::testutil::{roots, temp_root, write};
    use std::os::unix::fs::symlink;
    use std::time::Duration;

    #[test]
    fn mountinfo_parsing_and_unescape() {
        let text = "29 1 0:33 /root / rw,relatime shared:1 - btrfs /dev/nvme0n1p3 rw\n\
                    40 29 8:1 / /run/media/a\\040b rw - ext4 /dev/sda1 rw\n\
                    41 29 0:21 / /dev/mqueue rw - mqueue mqueue rw\n\
                    garbage line\n";
        let m = parse_mountinfo(text);
        assert_eq!(m.len(), 3);
        assert_eq!(m[0].dev, (0, 33));
        assert_eq!(m[0].source, "/dev/nvme0n1p3");
        assert_eq!(m[1].mount_point, PathBuf::from("/run/media/a b"));
    }

    #[test]
    fn usage_counts_each_device_once_per_disk() {
        let r = temp_root("disk-usage");
        let disk = |name: &str| DiskDevice {
            name: name.to_string(),
            model: None,
            size_bytes: 1,
            transport: "sata".into(),
        };
        let disks = [disk("fakea"), disk("fakeb")];
        write(&r, "sys/block/fakea/fakea1/partition", "1");
        write(&r, "sys/block/fakea/fakea2/partition", "2");
        write(&r, "sys/block/dm-0/slaves/fakeb1", "");
        write(&r, "sys/block/fakeb/fakeb1/partition", "1");
        let mnt = r.join("mnt");
        std::fs::create_dir_all(&mnt).unwrap();
        let p = mnt.display();
        // fakea1 mounted twice (btrfs subvolumes) is counted once; fakea2 adds
        // to the same disk; dm-0 maps through slaves to fakeb; tmpfs is ignored.
        write(
            &r,
            "proc/self/mountinfo",
            &format!(
                "1 0 0:33 / {p} rw - btrfs /dev/fakea1 rw\n\
                 2 0 0:33 /home {p} rw - btrfs /dev/fakea1 rw\n\
                 3 0 8:2 / {p} rw - ext4 /dev/fakea2 rw\n\
                 4 0 253:0 / {p} rw - ext4 /dev/dm-0 rw\n\
                 5 0 0:40 / {p} rw - tmpfs tmpfs rw\n"
            ),
        );
        let u = usage(&roots(&r), &disks);
        let (used, total) = statvfs(&mnt).unwrap();
        // Totals are exact; "used" may move a little because other tests write
        // to the same temporary filesystem between the statvfs calls.
        assert_eq!((u["fakea"].1, u["fakeb"].1), (total * 2, total));
        let near = |a: u64, b: u64| a.abs_diff(b) <= b / 100 + (64 << 20);
        assert!(near(u["fakea"].0, used * 2) && near(u["fakeb"].0, used));
        assert_eq!(u.len(), 2);
        assert!(usage(&roots(&temp_root("no-mounts")), &disks).is_empty());
    }

    #[test]
    fn classification_matches_legacy_filter() {
        let ata = "/sys/devices/pci0000:00/0000:00:1f.2/ata3/host2/target2:0:0/2:0:0:0/block/sda";
        let usb = "/sys/devices/pci0000:00/0000:00:14.0/usb3/3-5/3-5:1.1/host6/target6:0:0/6:0:0:0/block/sdb";
        let nvme = "/sys/devices/pci0000:00/0000:00:1c.4/0000:06:00.0/nvme/nvme0/nvme0n1";
        assert_eq!(classify("sda", ata), Some("sata"));
        assert_eq!(classify("sdb", usb), None);
        assert_eq!(classify("nvme0n1", nvme), Some("nvme"));
        assert_eq!(classify("nvme0c0n1", nvme), None);
        assert_eq!(classify("zram0", "/sys/devices/virtual/block/zram0"), None);
        assert_eq!(classify("sr0", ata), None);
        assert_eq!(
            classify(
                "vda",
                "/sys/devices/pci0000:00/0000:00:04.0/virtio1/block/vda"
            ),
            None
        );
        assert_eq!(
            classify(
                "sdc",
                "/sys/devices/.../end_device-0:0/target0:0:0/0:0:0:0/block/sdc"
            ),
            Some("sas")
        );
    }

    #[test]
    fn discovery_on_fake_sysfs() {
        let r = temp_root("disk");
        let dev = r.join("sys/devices/pci0000:00");
        let mk = |rel: &str, size: &str, model: &str, typ: Option<&str>| {
            let d = dev.join(rel);
            std::fs::create_dir_all(d.join("device")).unwrap();
            std::fs::write(d.join("size"), size).unwrap();
            std::fs::write(d.join("device/model"), model).unwrap();
            if let Some(t) = typ {
                std::fs::write(d.join("device/type"), t).unwrap();
            }
            d
        };
        let sda = mk(
            "ata3/host2/block/sda",
            "3907029168",
            "TOSHIBA DT01ABA2   ",
            Some("0"),
        );
        let sdb = mk("usb3/3-5/host6/block/sdb", "62521344", "Flash", Some("0"));
        let sr0 = mk("ata4/host3/block/sr0", "2097151", "DVD", Some("5"));
        let nvme = mk(
            "0000:06:00.0/nvme/nvme0/nvme0n1",
            "1000215216",
            "Micron_2400_MTFDKBA512QFM",
            None,
        );
        let empty = mk("ata5/host4/block/sdc", "0", "Card", Some("0"));
        std::fs::write(sda.join("dev"), "8:0\n").unwrap();
        // Full model from the udev database overrides the truncated sysfs one.
        write(
            &r,
            "run/udev/data/b8:0",
            "S:disk/by-id/x\nE:ID_MODEL=TOSHIBA_DT01ABA200V\nE:ID_MODEL_ENC=TOSHIBA\\x20DT01ABA200V\\x20\\x20\n",
        );
        std::fs::write(nvme.join("dev"), "259:0\n").unwrap();
        std::fs::create_dir_all(r.join("sys/block")).unwrap();
        for (n, d) in [
            ("sda", &sda),
            ("sdb", &sdb),
            ("sr0", &sr0),
            ("nvme0n1", &nvme),
            ("sdc", &empty),
        ] {
            symlink(d, r.join("sys/block").join(n)).unwrap();
        }
        let disks = discover(&roots(&r));
        let names: Vec<_> = disks
            .iter()
            .map(|d| (d.name.as_str(), d.transport.as_str()))
            .collect();
        assert_eq!(names, [("sda", "sata"), ("nvme0n1", "nvme")], "lsblk order");
        assert_eq!(disks[0].model.as_deref(), Some("TOSHIBA DT01ABA200V"));
        assert_eq!(
            disks[1].model.as_deref(),
            Some("Micron_2400_MTFDKBA512QFM"),
            "no udev entry: sysfs model, like lsblk"
        );
        assert_eq!(disks[0].size_bytes, 3907029168 * 512);
    }

    #[test]
    fn udev_escapes() {
        assert_eq!(
            udev_unescape("TOSHIBA\\x20DT01ABA200V\\x20"),
            "TOSHIBA DT01ABA200V "
        );
        assert_eq!(udev_unescape("a\\x2"), "a\\x2", "truncated escape kept");
        assert_eq!(udev_unescape("a\\xZZb"), "a\\xZZb", "invalid hex kept");
        assert_eq!(udev_unescape("plain"), "plain");
    }

    #[test]
    fn rates_from_diskstats() {
        let r = temp_root("diskstats");
        let line = |rs: u64, ws: u64| {
            format!(" 8 0 sda 1 2 {rs} 4 5 6 {ws} 8 0 9 10\n 8 1 sda1 1 2 3 4 5 6 7 8 0 9 10\n")
        };
        write(&r, "proc/diskstats", &line(1000, 2000));
        let disks = vec![
            DiskDevice {
                name: "sda".into(),
                model: None,
                size_bytes: 1,
                transport: "sata".into(),
            },
            DiskDevice {
                name: "gone".into(),
                model: None,
                size_bytes: 1,
                transport: "sata".into(),
            },
        ];
        let mut c = DiskCollector::new(&roots(&r));
        let t0 = Instant::now();
        let first = c.sample_at(&disks, t0);
        assert_eq!(first["sda"].read, None, "first sample has no rate");
        write(&r, "proc/diskstats", &line(1000 + 2048, 2000));
        let second = c.sample_at(&disks, t0 + Duration::from_secs(2));
        assert_eq!(second["sda"].read, Some(2048.0 * 512.0 / 2.0));
        assert_eq!(second["sda"].write, Some(0.0));
        assert_eq!(second["gone"].read, None, "device not in diskstats");
        write(&r, "proc/diskstats", &line(10, 10));
        let reset = c.sample_at(&disks, t0 + Duration::from_secs(3));
        assert_eq!(
            (reset["sda"].read, reset["sda"].write),
            (None, None),
            "counter reset"
        );
        c.reset();
        assert_eq!(
            c.sample_at(&disks, t0 + Duration::from_secs(4))["sda"].read,
            None
        );
    }
}
