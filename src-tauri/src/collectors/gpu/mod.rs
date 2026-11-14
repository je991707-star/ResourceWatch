//! GPU discovery (DRM sysfs) and sampling.
//!
//! | Vendor | Usage | NVENC/NVDEC | Temperature | VRAM |
//! |---|---|---|---|---|
//! | NVIDIA (proprietary driver) | NVML | NVML | NVML | NVML |
//! | AMD (`amdgpu`) | `gpu_busy_percent` | not available → `None` | hwmon | `mem_info_vram_*` |
//! | Intel (`i915`/`xe`) | not exposed in sysfs → `None` | `None` | hwmon if present | `mem_info_vram_*` if present |
//! | Other / NVIDIA without NVML | `gpu_busy_percent` if present | `None` | hwmon if present | `mem_info_vram_*` if present |
//!
//! `gpuIndex` selects from DRM cards in `cardN` order (display controllers only).

pub mod nvml;

use super::{list_dir, read_i64, read_trimmed, read_u64, Roots};
use nvml::{NvDevice, Nvml};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

impl Vendor {
    pub fn from_pci(id: u16) -> Self {
        match id {
            0x10de => Vendor::Nvidia,
            0x1002 | 0x1022 => Vendor::Amd,
            0x8086 => Vendor::Intel,
            _ => Vendor::Other,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Vendor::Nvidia => "nvidia",
            Vendor::Amd => "amd",
            Vendor::Intel => "intel",
            Vendor::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GpuDevice {
    pub card: String,
    /// `0000:01:00.0`; `None` for non-PCI (e.g. SoC) devices.
    pub pci: Option<String>,
    pub vendor: Vendor,
    pub vendor_id: u16,
    pub device_id: u16,
    pub driver: Option<String>,
    pub device_dir: PathBuf,
}

fn hex_u16(path: &Path) -> Option<u16> {
    let s = read_trimmed(path)?;
    u16::from_str_radix(s.trim_start_matches("0x"), 16).ok()
}

/// Normalised PCI address `dddd:bb:dd.f` if `name` looks like one.
fn pci_address(name: &str) -> Option<String> {
    let (domain, rest) = name.split_once(':')?;
    let (bus, rest) = rest.split_once(':')?;
    let (dev, func) = rest.split_once('.')?;
    let ok = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit());
    if !(ok(bus, 2) && ok(dev, 2) && ok(func, 1)) || domain.len() < 4 {
        return None;
    }
    let domain = u32::from_str_radix(domain, 16).ok()?;
    Some(format!("{domain:04x}:{bus}:{dev}.{func}").to_lowercase())
}

/// Display controllers (PCI class 0x03xxxx) under `/sys/class/drm/cardN`.
pub fn discover(roots: &Roots) -> Vec<GpuDevice> {
    list_dir(&roots.sys("class/drm"), |n| {
        n.strip_prefix("card")
            .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
    })
    .into_iter()
    .filter_map(|card| {
        let dev = card.join("device");
        let class = read_trimmed(&dev.join("class"))?;
        if !class.trim_start_matches("0x").starts_with("03") {
            return None;
        }
        let vendor_id = hex_u16(&dev.join("vendor"))?;
        let device_id = hex_u16(&dev.join("device")).unwrap_or(0);
        let real = std::fs::canonicalize(&dev).ok()?;
        let pci = real
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(pci_address);
        let driver = std::fs::canonicalize(dev.join("driver"))
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
        Some(GpuDevice {
            card: card.file_name()?.to_string_lossy().into_owned(),
            pci,
            vendor: Vendor::from_pci(vendor_id),
            vendor_id,
            device_id,
            driver,
            device_dir: dev,
        })
    })
    .collect()
}

pub const PCI_IDS_PATHS: [&str; 3] = [
    "/usr/share/hwdata/pci.ids",
    "/usr/share/misc/pci.ids",
    "/usr/share/pci.ids",
];

/// Device name from a `pci.ids` database text.
pub fn pci_ids_lookup(text: &str, vendor: u16, device: u16) -> Option<String> {
    let vendor_prefix = format!("{vendor:04x}  ");
    let device_prefix = format!("\t{device:04x}  ");
    let mut in_vendor = false;
    for line in text.lines() {
        if in_vendor {
            if let Some(name) = line.strip_prefix(&device_prefix) {
                return Some(name.trim().to_string());
            }
            if !line.starts_with('\t') && !line.starts_with('#') && !line.is_empty() {
                return None; // next vendor
            }
        } else if line.starts_with(&vendor_prefix) {
            in_vendor = true;
        }
    }
    None
}

fn read_pci_ids() -> Option<String> {
    PCI_IDS_PATHS
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GpuReading {
    pub usage: Option<f64>,
    pub encoder: Option<f64>,
    pub decoder: Option<f64>,
    pub temp_c: Option<f64>,
    pub vram_used: Option<f64>,
    pub vram_total: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DrmPaths {
    busy: Option<PathBuf>,
    vram_used: Option<PathBuf>,
    vram_total: Option<PathBuf>,
    temp: Option<PathBuf>,
}

fn existing(p: PathBuf) -> Option<PathBuf> {
    p.exists().then_some(p)
}

/// First temperature input of the card's own hwmon (amdgpu: `edge`).
fn card_temp_input(device_dir: &Path) -> Option<PathBuf> {
    let chip = list_dir(&device_dir.join("hwmon"), |n| n.starts_with("hwmon"))
        .into_iter()
        .next()?;
    let inputs = list_dir(&chip, |n| n.starts_with("temp") && n.ends_with("_input"));
    inputs
        .iter()
        .find(|p| {
            let label = PathBuf::from(p.to_string_lossy().replace("_input", "_label"));
            matches!(read_trimmed(&label).as_deref(), Some("edge") | Some("pkg"))
        })
        .or(inputs.first())
        .cloned()
}

pub fn drm_paths(dev: &GpuDevice) -> DrmPaths {
    let d = &dev.device_dir;
    DrmPaths {
        busy: existing(d.join("gpu_busy_percent")),
        vram_used: existing(d.join("mem_info_vram_used")),
        vram_total: existing(d.join("mem_info_vram_total")),
        temp: card_temp_input(d),
    }
}

pub fn read_drm(p: &DrmPaths) -> GpuReading {
    let usage = p
        .busy
        .as_deref()
        .and_then(read_u64)
        .filter(|v| *v <= 100)
        .map(|v| v as f64);
    let total = p
        .vram_total
        .as_deref()
        .and_then(read_u64)
        .filter(|t| *t > 0);
    let used = p
        .vram_used
        .as_deref()
        .and_then(read_u64)
        .filter(|u| total.is_none_or(|t| *u <= t));
    let temp_c = p
        .temp
        .as_deref()
        .and_then(read_i64)
        .map(|m| m as f64 / 1000.0)
        .filter(|c| (-40.0..=150.0).contains(c));
    GpuReading {
        usage,
        encoder: None,
        decoder: None,
        temp_c,
        vram_used: used.map(|v| v as f64),
        vram_total: total.map(|v| v as f64),
    }
}

enum Backend {
    Nvml(NvDevice),
    Drm(DrmPaths),
}

/// One GPU with everything resolved at discovery time (name, backend), so
/// switching `gpuIndex` needs no rescan, no `pci.ids` read, no NVML lookup.
struct Entry {
    device: GpuDevice,
    name: Option<String>,
    backend: Backend,
}

pub struct GpuCollector {
    roots: Roots,
    entries: Vec<Entry>,
    nvml: Option<Nvml>,
    nvml_tried: bool,
    index: u32,
}

impl GpuCollector {
    pub fn new(roots: &Roots, index: u32) -> Self {
        let mut c = Self {
            roots: roots.clone(),
            entries: Vec::new(),
            nvml: None,
            nvml_tried: false,
            index,
        };
        c.rediscover();
        c
    }

    /// Startup / manual hardware re-detection: rescans DRM cards and resolves
    /// each card's backend and name once.
    pub fn rediscover(&mut self) {
        let devices = discover(&self.roots);
        if !self.nvml_tried && devices.iter().any(|d| d.vendor == Vendor::Nvidia) {
            self.nvml_tried = true;
            match Nvml::load() {
                Ok(n) => self.nvml = Some(n),
                Err(e) => eprintln!("ResourceWatch: NVML unavailable, NVIDIA metrics limited: {e}"),
            }
        }
        let mut pci_db: Option<Option<String>> = None; // read lazily, at most once
        let nvml = self.nvml.as_ref();
        self.entries = devices
            .into_iter()
            .map(|device| {
                let handle = (device.vendor == Vendor::Nvidia)
                    .then(|| nvml.zip(device.pci.as_deref()))
                    .flatten()
                    .and_then(|(n, pci)| n.device_by_pci(pci));
                let mut name = handle.and_then(|h| nvml.and_then(|n| n.name(h)));
                if name.is_none() {
                    name = pci_db
                        .get_or_insert_with(read_pci_ids)
                        .as_deref()
                        .and_then(|db| pci_ids_lookup(db, device.vendor_id, device.device_id));
                }
                let backend = match handle {
                    Some(h) => Backend::Nvml(h),
                    None => Backend::Drm(drm_paths(&device)),
                };
                Entry {
                    device,
                    name,
                    backend,
                }
            })
            .collect();
    }

    /// (name, vendor) of every discovered GPU, in `gpuIndex` order.
    pub fn list(&self) -> Vec<(Option<&str>, Vendor)> {
        self.entries
            .iter()
            .map(|e| (e.name.as_deref(), e.device.vendor))
            .collect()
    }

    /// Switches to another already-discovered GPU (no hardware scan).
    pub fn select(&mut self, index: u32) {
        self.index = index;
    }

    fn selected(&self) -> Option<&Entry> {
        self.entries.get(self.index as usize)
    }

    pub fn index(&self) -> u32 {
        self.index
    }
    pub fn name(&self) -> Option<&str> {
        self.selected()?.name.as_deref()
    }
    pub fn vendor(&self) -> Option<Vendor> {
        self.selected().map(|s| s.device.vendor)
    }
    /// `nvml` | `drm` | `unavailable`
    pub fn source(&self) -> &'static str {
        match self.selected().map(|s| &s.backend) {
            Some(Backend::Nvml(_)) => "nvml",
            Some(Backend::Drm(_)) => "drm",
            None => "unavailable",
        }
    }

    pub fn sample(&self) -> GpuReading {
        match self.selected().map(|s| &s.backend) {
            Some(Backend::Nvml(h)) => {
                let Some(n) = self.nvml.as_ref() else {
                    return GpuReading::default();
                };
                let mem = n.memory(*h);
                GpuReading {
                    usage: n.utilization(*h),
                    encoder: n.encoder(*h),
                    decoder: n.decoder(*h),
                    temp_c: n.temperature(*h),
                    vram_used: mem.map(|m| m.0),
                    vram_total: mem.map(|m| m.1),
                }
            }
            Some(Backend::Drm(p)) => read_drm(p),
            None => GpuReading::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::testutil::{roots, temp_root, write};
    use std::os::unix::fs::symlink;

    fn fake_card(
        r: &Path,
        card: &str,
        pci: &str,
        vendor: &str,
        device: &str,
        class: &str,
    ) -> PathBuf {
        let dev = r.join("sys/devices/pci0000:00").join(pci);
        write(&dev, "vendor", vendor);
        write(&dev, "device", device);
        write(&dev, "class", class);
        let cdir = r.join("sys/class/drm").join(card);
        std::fs::create_dir_all(&cdir).unwrap();
        symlink(&dev, cdir.join("device")).unwrap();
        dev
    }

    #[test]
    fn discovery_and_amd_sysfs() {
        let r = temp_root("gpu");
        let amd = fake_card(
            &r,
            "card0",
            "0000:03:00.0",
            "0x1002\n",
            "0x73bf\n",
            "0x030000\n",
        );
        fake_card(
            &r,
            "card1",
            "0000:00:1f.3",
            "0x8086\n",
            "0xa348\n",
            "0x040300\n",
        ); // audio, not GPU
        write(&amd, "gpu_busy_percent", "37\n");
        write(&amd, "mem_info_vram_used", "1073741824\n");
        write(&amd, "mem_info_vram_total", "17163091968\n");
        write(&amd, "hwmon/hwmon5/temp1_label", "edge\n");
        write(&amd, "hwmon/hwmon5/temp1_input", "52000\n");
        write(&amd, "hwmon/hwmon5/temp2_label", "junction\n");
        write(&amd, "hwmon/hwmon5/temp2_input", "60000\n");
        std::fs::create_dir_all(r.join("sys/class/drm/card0-DP-1")).unwrap();

        let devs = discover(&roots(&r));
        assert_eq!(devs.len(), 1);
        assert_eq!(devs[0].vendor, Vendor::Amd);
        assert_eq!(devs[0].pci.as_deref(), Some("0000:03:00.0"));
        let reading = read_drm(&drm_paths(&devs[0]));
        assert_eq!(reading.usage, Some(37.0));
        assert_eq!(reading.temp_c, Some(52.0));
        assert_eq!(reading.vram_used, Some(1073741824.0));
        assert_eq!(reading.encoder, None, "no codec metric via sysfs");

        let c = GpuCollector::new(&roots(&r), 0);
        assert_eq!(c.source(), "drm");
        let none = GpuCollector::new(&roots(&r), 3);
        assert_eq!(none.source(), "unavailable");
        assert_eq!(none.sample(), GpuReading::default());
    }

    #[test]
    fn intel_without_busy_metric_is_none() {
        let r = temp_root("intel");
        fake_card(&r, "card0", "0000:00:02.0", "0x8086", "0x3e92", "0x030000");
        let c = GpuCollector::new(&roots(&r), 0);
        assert_eq!(c.vendor(), Some(Vendor::Intel));
        assert_eq!(c.sample(), GpuReading::default());
    }

    #[test]
    fn switching_gpu_uses_cached_discovery() {
        let r = temp_root("gpu-switch");
        fake_card(&r, "card0", "0000:03:00.0", "0x1002", "0x73bf", "0x030000");
        fake_card(&r, "card1", "0000:00:02.0", "0x8086", "0x3e92", "0x030000");
        let mut c = GpuCollector::new(&roots(&r), 0);
        assert_eq!(c.vendor(), Some(Vendor::Amd));
        std::fs::remove_dir_all(r.join("sys")).unwrap(); // no rescan possible now
        c.select(1);
        assert_eq!((c.vendor(), c.source()), (Some(Vendor::Intel), "drm"));
        c.select(0);
        assert_eq!(c.vendor(), Some(Vendor::Amd));
        c.select(7);
        assert_eq!(c.source(), "unavailable");
    }

    #[test]
    fn pci_ids() {
        let db = "# comment\n10de  NVIDIA Corporation\n\t1cb6  GP107GL [Quadro P620]\n\t1cb6 bad\n\
                  1002  Advanced Micro Devices, Inc. [AMD/ATI]\n\t73bf  Navi 21 [Radeon RX 6800/6800 XT / 6900 XT]\n\
                  \t\t1002 0e3a  Radeon RX 6900 XT\n1003  ULSI\n\t0201  GUI Engine\n";
        assert_eq!(
            pci_ids_lookup(db, 0x10de, 0x1cb6).as_deref(),
            Some("GP107GL [Quadro P620]")
        );
        assert_eq!(
            pci_ids_lookup(db, 0x1002, 0x73bf).as_deref(),
            Some("Navi 21 [Radeon RX 6800/6800 XT / 6900 XT]")
        );
        assert_eq!(
            pci_ids_lookup(db, 0x1002, 0x0201),
            None,
            "belongs to next vendor"
        );
        assert_eq!(pci_ids_lookup(db, 0x9999, 0x0001), None);
    }

    #[test]
    fn pci_address_normalisation() {
        assert_eq!(pci_address("0000:01:00.0").as_deref(), Some("0000:01:00.0"));
        assert_eq!(pci_address("0000:0A:00.1").as_deref(), Some("0000:0a:00.1"));
        assert_eq!(pci_address("platform-gpu"), None);
    }
}
