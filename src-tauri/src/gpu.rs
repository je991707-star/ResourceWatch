//! GPUs: NVIDIA through NVML (libnvidia-ml.so.1, loaded at runtime so the app
//! still runs without the driver); AMD/Intel and NVIDIA-without-NVML through DRM sysfs.

use libloading::Library;
use serde::Serialize;
use std::ffi::{c_char, c_int, c_uint, c_ulonglong, c_void, CStr};
use std::fs;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    name: String,
    usage: Option<f64>,
    mem_used: Option<u64>,
    mem_total: Option<u64>,
    temp: Option<f64>,
    /// NVENC / NVDEC utilization (%); `null` for non-NVIDIA GPUs.
    encoder: Option<f64>,
    decoder: Option<f64>,
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
                    Some(Gpu {
                        name,
                        usage: ((self.util)(h, &mut u) == 0).then_some(u.gpu as f64),
                        mem_used: mem_ok.then_some(m.used),
                        mem_total: mem_ok.then_some(m.total),
                        temp: ((self.temp)(h, 0, &mut t) == 0).then_some(t as f64),
                        encoder: codec(self.encoder),
                        decoder: codec(self.decoder),
                    })
                })
                .collect()
        }
    }
}

pub struct Gpus {
    nvml: Option<Nvml>,
    /// /sys/class/drm/cardN/device paths not covered by NVML.
    drm: Vec<(String, std::path::PathBuf)>,
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
                Some((name.to_string(), dev))
            })
            .collect();
        drm.sort_by(|a, b| a.1.cmp(&b.1));
        Gpus { nvml, drm }
    }

    pub fn sample(&self) -> Vec<Gpu> {
        let num = |p: std::path::PathBuf| fs::read_to_string(p).ok()?.trim().parse::<f64>().ok();
        let mut out = self.nvml.as_ref().map(Nvml::sample).unwrap_or_default();
        out.extend(self.drm.iter().map(|(name, dev)| {
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
            }
        }));
        out
    }
}
