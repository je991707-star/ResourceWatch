//! GPUs: NVIDIA through NVML (libnvidia-ml.so.1, loaded at runtime so the app
//! still runs without the driver); AMD/Intel and NVIDIA-without-NVML through DRM sysfs.

use libloading::Library;
use serde::Serialize;
use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_uint, c_ulonglong, c_void, CStr};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    pub(crate) name: String,
    usage: Option<f64>,
    mem_used: Option<u64>,
    mem_total: Option<u64>,
    pub(crate) temp: Option<f64>,
    /// NVENC / NVDEC utilization (%); `null` for non-NVIDIA GPUs.
    encoder: Option<f64>,
    decoder: Option<f64>,
    /// Read through NVML (its temperature is not in hwmon, so sys.rs adds it to the sensors).
    #[serde(skip)]
    pub(crate) nvml: bool,
    /// Fan speed (%) and board power (W); NVML only, for the sensors list.
    #[serde(skip)]
    pub(crate) fan: Option<f64>,
    #[serde(skip)]
    pub(crate) power: Option<f64>,
}

type Handle = *mut c_void;

#[repr(C)]
#[derive(Default)]
struct Util {
    gpu: c_uint,
    _memory: c_uint,
}

#[repr(C)]
#[derive(Default)]
struct Memory {
    total: c_ulonglong,
    _free: c_ulonglong,
    used: c_ulonglong,
}

/// nvmlProcessUtilizationSample_t
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct ProcSample {
    pid: c_uint,
    /// CPU timestamp, µs
    ts: c_ulonglong,
    sm: c_uint,
    _mem: c_uint,
    enc: c_uint,
    dec: c_uint,
}

/// pid → (GPU %, "GPU <index> - <engine>") for the busiest engine.
pub type ProcGpu = HashMap<u32, (f64, String)>;

/// Keep the busier entry per pid.
fn put(out: &mut ProcGpu, pid: u32, pct: f64, engine: String) {
    if out.get(&pid).is_none_or(|v| pct > v.0) {
        out.insert(pid, (pct, engine));
    }
}

struct Nvml {
    count: unsafe extern "C" fn(*mut c_uint) -> c_int,
    handle: unsafe extern "C" fn(c_uint, *mut Handle) -> c_int,
    name: unsafe extern "C" fn(Handle, *mut c_char, c_uint) -> c_int,
    util: unsafe extern "C" fn(Handle, *mut Util) -> c_int,
    mem: unsafe extern "C" fn(Handle, *mut Memory) -> c_int,
    temp: unsafe extern "C" fn(Handle, c_int, *mut c_uint) -> c_int,
    // (handle, utilization %, sampling period µs); optional so an old driver still loads.
    encoder: Option<unsafe extern "C" fn(Handle, *mut c_uint, *mut c_uint) -> c_int>,
    decoder: Option<unsafe extern "C" fn(Handle, *mut c_uint, *mut c_uint) -> c_int>,
    // (handle, fan %) and (handle, milliwatts); fail with NOT_SUPPORTED on many laptops.
    fan: Option<unsafe extern "C" fn(Handle, *mut c_uint) -> c_int>,
    power: Option<unsafe extern "C" fn(Handle, *mut c_uint) -> c_int>,
    // (handle, samples or null to size, count in/out, last seen timestamp µs)
    proc_util:
        Option<unsafe extern "C" fn(Handle, *mut ProcSample, *mut c_uint, c_ulonglong) -> c_int>,
    // Keeps the function pointers above valid.
    _lib: Library,
}

impl Nvml {
    fn load() -> Option<Self> {
        // SAFETY: symbol types match the NVML C API; the library outlives the pointers (`_lib`).
        unsafe {
            let lib = Library::new("libnvidia-ml.so.1").ok()?;
            let init: unsafe extern "C" fn() -> c_int = *lib.get(b"nvmlInit_v2\0").ok()?;
            if init() != 0 {
                return None;
            }
            Some(Nvml {
                count: *lib.get(b"nvmlDeviceGetCount_v2\0").ok()?,
                handle: *lib.get(b"nvmlDeviceGetHandleByIndex_v2\0").ok()?,
                name: *lib.get(b"nvmlDeviceGetName\0").ok()?,
                util: *lib.get(b"nvmlDeviceGetUtilizationRates\0").ok()?,
                mem: *lib.get(b"nvmlDeviceGetMemoryInfo\0").ok()?,
                temp: *lib.get(b"nvmlDeviceGetTemperature\0").ok()?,
                encoder: lib
                    .get(b"nvmlDeviceGetEncoderUtilization\0")
                    .ok()
                    .map(|f| *f),
                decoder: lib
                    .get(b"nvmlDeviceGetDecoderUtilization\0")
                    .ok()
                    .map(|f| *f),
                fan: lib.get(b"nvmlDeviceGetFanSpeed\0").ok().map(|f| *f),
                power: lib.get(b"nvmlDeviceGetPowerUsage\0").ok().map(|f| *f),
                proc_util: lib
                    .get(b"nvmlDeviceGetProcessUtilization\0")
                    .ok()
                    .map(|f| *f),
                _lib: lib,
            })
        }
    }

    fn sample(&self) -> Vec<Gpu> {
        let mut n = 0;
        // SAFETY: out-pointers are valid locals; NVML is thread-safe and initialised in `load`.
        unsafe {
            if (self.count)(&mut n) != 0 {
                return Vec::new();
            }
            (0..n)
                .filter_map(|i| {
                    let mut h: Handle = std::ptr::null_mut();
                    if (self.handle)(i, &mut h) != 0 {
                        return None;
                    }
                    let mut buf = [0 as c_char; 96];
                    let name = if (self.name)(h, buf.as_mut_ptr(), buf.len() as c_uint) == 0 {
                        CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
                    } else {
                        "NVIDIA GPU".into()
                    };
                    let (mut u, mut m, mut t) = (Util::default(), Memory::default(), 0);
                    let mem_ok = (self.mem)(h, &mut m) == 0;
                    let codec = |f: Option<
                        unsafe extern "C" fn(Handle, *mut c_uint, *mut c_uint) -> c_int,
                    >| {
                        let (mut u, mut period) = (0, 0);
                        (f?(h, &mut u, &mut period) == 0).then_some(u as f64)
                    };
                    let one = |f: Option<unsafe extern "C" fn(Handle, *mut c_uint) -> c_int>| {
                        let mut v = 0;
                        (f?(h, &mut v) == 0).then_some(v as f64)
                    };
                    Some(Gpu {
                        name,
                        usage: ((self.util)(h, &mut u) == 0).then_some(u.gpu as f64),
                        mem_used: mem_ok.then_some(m.used),
                        mem_total: mem_ok.then_some(m.total),
                        temp: ((self.temp)(h, 0, &mut t) == 0).then_some(t as f64),
                        encoder: codec(self.encoder),
                        decoder: codec(self.decoder),
                        nvml: true,
                        fan: one(self.fan),
                        power: one(self.power).map(|mw| mw / 1000.0),
                    })
                })
                .collect()
        }
    }
}

impl Nvml {
    /// Per-process SM / NVENC / NVDEC use since `since` (µs), averaged over the samples NVML
    /// returns for each process. Returns the newest sample timestamp.
    fn proc_util(&self, since: u64, out: &mut ProcGpu) -> u64 {
        let (Some(f), mut n, mut seen) = (self.proc_util, 0, since) else {
            return since;
        };
        // SAFETY: out-pointers are valid; the buffer holds `cnt` samples as NVML was told.
        unsafe {
            if (self.count)(&mut n) != 0 {
                return since;
            }
            for i in 0..n {
                let mut h: Handle = std::ptr::null_mut();
                let mut cnt = 0;
                if (self.handle)(i, &mut h) != 0 {
                    continue;
                }
                f(h, std::ptr::null_mut(), &mut cnt, since); // sizes the buffer
                let mut buf = vec![ProcSample::default(); cnt as usize];
                if cnt == 0 || f(h, buf.as_mut_ptr(), &mut cnt, since) != 0 {
                    continue;
                }
                let mut sum: HashMap<u32, [f64; 4]> = HashMap::new();
                for s in &buf[..(cnt as usize).min(buf.len())] {
                    seen = seen.max(s.ts);
                    let a = sum.entry(s.pid).or_default();
                    a[0] += s.sm as f64;
                    a[1] += s.enc as f64;
                    a[2] += s.dec as f64;
                    a[3] += 1.0;
                }
                for (pid, [sm, enc, dec, k]) in sum {
                    let (pct, e) = [(sm, "3D"), (enc, "Video Encode"), (dec, "Video Decode")]
                        .into_iter()
                        .fold((0.0, "3D"), |m, v| if v.0 > m.0 { v } else { m });
                    put(out, pid, pct / k, format!("GPU {i} - {e}"));
                }
            }
        }
        seen
    }
}

/// One DRM client's fdinfo → (client id, PCI address, [(engine, busy ns)]); None when it has no
/// `drm-engine-*` ns counters (NVIDIA's proprietary driver, xe's cycle counters).
fn parse_fdinfo(text: &str) -> Option<(u64, String, Vec<(String, u64)>)> {
    let (mut id, mut pdev, mut eng) = (None, None, Vec::new());
    for l in text.lines() {
        let Some((k, v)) = l.split_once(':') else {
            continue;
        };
        let v = v.trim();
        match k {
            "drm-client-id" => id = v.parse().ok(),
            "drm-pdev" => pdev = Some(v.to_string()),
            _ => {
                if let (Some(e), Some(ns)) = (k.strip_prefix("drm-engine-"), v.strip_suffix(" ns"))
                {
                    if let (false, Ok(ns)) = (e.starts_with("capacity-"), ns.parse()) {
                        eng.push((e.to_string(), ns));
                    }
                }
            }
        }
    }
    (!eng.is_empty()).then_some((id?, pdev?, eng))
}

/// amdgpu / i915 engine name → Task Manager style label.
fn engine_label(e: &str) -> &str {
    match e {
        "gfx" | "render" => "3D",
        "compute" => "Compute",
        "dec" | "video" | "jpeg" => "Video Decode",
        "video-enhance" => "Video Processing",
        "dma" | "copy" => "Copy",
        _ if e.starts_with("enc") => "Video Encode",
        _ => e,
    }
}

pub struct Gpus {
    nvml: Option<Nvml>,
    /// /sys/class/drm/cardN/device paths not covered by NVML, with their PCI address.
    drm: Vec<(String, PathBuf, String)>,
    /// Newest NVML process sample seen (µs).
    nvml_seen: u64,
    /// (PCI address, client id, engine) → busy ns at the last call.
    drm_prev: HashMap<(String, u64, String), u64>,
}

impl Gpus {
    pub fn new() -> Self {
        let nvml = Nvml::load();
        let mut drm: Vec<_> = fs::read_dir("/sys/class/drm")
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with("card") && !n.contains('-'))
            })
            .filter_map(|e| {
                let dev = e.path().join("device");
                let name = match fs::read_to_string(dev.join("vendor")).ok()?.trim() {
                    "0x1002" => "AMD GPU",
                    "0x8086" => "Intel GPU",
                    "0x10de" if nvml.is_none() => "NVIDIA GPU",
                    _ => return None,
                };
                let pci = fs::canonicalize(&dev)
                    .ok()?
                    .file_name()?
                    .to_str()?
                    .to_string();
                Some((name.to_string(), dev, pci))
            })
            .collect();
        drm.sort_by(|a, b| a.1.cmp(&b.1));
        Gpus {
            nvml,
            drm,
            nvml_seen: 0,
            drm_prev: HashMap::new(),
        }
    }

    /// Per-process GPU use. `own` lists our own pids (DRM fdinfo is only readable for them);
    /// `secs` is the time since the last call. The flag is true when NVML covers every process,
    /// so a missing pid means 0 % rather than unknown.
    pub fn proc_util(&mut self, own: &[u32], secs: Option<f64>) -> (ProcGpu, bool) {
        let mut out = ProcGpu::new();
        let all = self.nvml.as_ref().is_some_and(|n| n.proc_util.is_some());
        if let Some(n) = &self.nvml {
            let since = if self.nvml_seen == 0 {
                // first call: only the last few seconds, not the driver's whole buffer
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_micros() as u64)
                    .saturating_sub(3_000_000)
            } else {
                self.nvml_seen
            };
            self.nvml_seen = n.proc_util(since, &mut out);
        }
        if self.drm.is_empty() {
            return (out, all);
        }
        let base = self.nvml.as_ref().map_or(0, |n| {
            let mut c = 0;
            // SAFETY: valid out-pointer.
            unsafe { (n.count)(&mut c) };
            c as usize
        });
        let mut prev = HashMap::new();
        for &pid in own {
            let fds = fs::read_dir(format!("/proc/{pid}/fd"))
                .into_iter()
                .flatten()
                .flatten();
            for fd in fds {
                if !fs::read_link(fd.path()).is_ok_and(|l| l.starts_with("/dev/dri/")) {
                    continue;
                }
                let info = fs::read_to_string(format!(
                    "/proc/{pid}/fdinfo/{}",
                    fd.file_name().to_string_lossy()
                ));
                let Some((id, pdev, eng)) = info.ok().as_deref().and_then(parse_fdinfo) else {
                    continue;
                };
                let Some(gi) = self.drm.iter().position(|d| d.2 == pdev) else {
                    continue;
                };
                for (e, ns) in eng {
                    let key = (pdev.clone(), id, e);
                    // the same client shows up on every fd/process sharing it: count it once
                    if prev.contains_key(&key) {
                        continue;
                    }
                    let pct = self.drm_prev.get(&key).zip(secs).map(|(&p, s)| {
                        (ns.saturating_sub(p) as f64 / (s * 1e9) * 100.0).min(100.0)
                    });
                    put(
                        &mut out,
                        pid,
                        pct.unwrap_or(0.0),
                        format!("GPU {} - {}", base + gi, engine_label(&key.2)),
                    );
                    prev.insert(key, ns);
                }
            }
        }
        self.drm_prev = prev;
        (out, all)
    }

    pub fn sample(&self) -> Vec<Gpu> {
        let num = |p: std::path::PathBuf| fs::read_to_string(p).ok()?.trim().parse::<f64>().ok();
        let mut out = self.nvml.as_ref().map(Nvml::sample).unwrap_or_default();
        out.extend(self.drm.iter().map(|(name, dev, _)| {
            let temp = fs::read_dir(dev.join("hwmon"))
                .into_iter()
                .flatten()
                .flatten()
                .find_map(|e| num(e.path().join("temp1_input")))
                .map(|t| t / 1000.0);
            Gpu {
                name: name.clone(),
                usage: num(dev.join("gpu_busy_percent")),
                mem_used: num(dev.join("mem_info_vram_used")).map(|v| v as u64),
                mem_total: num(dev.join("mem_info_vram_total")).map(|v| v as u64),
                temp,
                encoder: None,
                decoder: None,
                nvml: false,
                fan: None,
                power: None,
            }
        }));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fdinfo() {
        let t = "pos:\t0\ndrm-driver:\ti915\ndrm-pdev:\t0000:00:02.0\ndrm-client-id:\t7\n\
                 drm-engine-render:\t1500 ns\ndrm-engine-capacity-video:\t2\ndrm-engine-video:\t30 ns\n";
        let (id, pdev, eng) = parse_fdinfo(t).unwrap();
        assert_eq!((id, pdev.as_str()), (7, "0000:00:02.0"));
        assert_eq!(eng, vec![("render".into(), 1500), ("video".into(), 30)]);
        // NVIDIA proprietary fdinfo has no engine counters
        assert_eq!(parse_fdinfo("pos:\t0\nflags:\t02100002\n"), None);
        assert_eq!(engine_label("gfx"), "3D");
        assert_eq!(engine_label("enc_1"), "Video Encode");
    }
}
