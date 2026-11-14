//! Physical disk discovery (`/sys/block`) and read/write rates
//! (`/proc/diskstats`). Discovery runs only at startup and on manual refresh;
//! rate sampling reads one in-kernel file per interval (no `cat`/`lsblk`).

use super::{counter_rate, list_dir, read_trimmed, read_u64, Roots};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
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
            let model = read_trimmed(&dir.join("device/model"))
                .map(|m| m.split_whitespace().collect::<Vec<_>>().join(" "));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::testutil::{roots, temp_root, write};
    use std::os::unix::fs::symlink;
    use std::time::Duration;

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
        assert_eq!(disks[0].model.as_deref(), Some("TOSHIBA DT01ABA2"));
        assert_eq!(disks[0].size_bytes, 3907029168 * 512);
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
