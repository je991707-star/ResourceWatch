//! Minimal NVML binding loaded at runtime with `libloading`.
//!
//! NVML is initialised **once** for the lifetime of the collector (the legacy
//! version started Python and re-initialised NVML every interval). Devices are
//! addressed by PCI bus id so they always match the DRM card being shown.
//! Any NVML error or `NOT_SUPPORTED` yields `None` for that metric.

use libloading::Library;
use std::ffi::{c_char, c_int, c_uint, c_ulonglong, c_void, CStr, CString};

const NVML_SUCCESS: c_int = 0;
const NVML_TEMPERATURE_GPU: c_int = 0;
const NAME_BUFFER: usize = 96;

#[repr(C)]
#[derive(Default)]
struct Utilization {
    gpu: c_uint,
    /// Required for the C layout; not used.
    _memory: c_uint,
}

#[repr(C)]
#[derive(Default)]
struct Memory {
    total: c_ulonglong,
    /// Required for the C layout; not used.
    _free: c_ulonglong,
    used: c_ulonglong,
}

type Handle = *mut c_void;
type FnVoid = unsafe extern "C" fn() -> c_int;
type FnByPci = unsafe extern "C" fn(*const c_char, *mut Handle) -> c_int;
type FnName = unsafe extern "C" fn(Handle, *mut c_char, c_uint) -> c_int;
type FnUtil = unsafe extern "C" fn(Handle, *mut Utilization) -> c_int;
type FnCodec = unsafe extern "C" fn(Handle, *mut c_uint, *mut c_uint) -> c_int;
type FnTemp = unsafe extern "C" fn(Handle, c_int, *mut c_uint) -> c_int;
type FnMem = unsafe extern "C" fn(Handle, *mut Memory) -> c_int;
type FnMemV2 = unsafe extern "C" fn(Handle, *mut MemoryV2) -> c_int;

/// nvmlMemory_v2_t: `used` excludes driver-reserved memory, matching
/// `nvidia-smi` memory.used (v1 `used` includes the reserved part).
#[repr(C)]
#[derive(Default)]
struct MemoryV2 {
    version: c_uint,
    total: c_ulonglong,
    reserved: c_ulonglong,
    /// Required for the C layout; not used.
    _free: c_ulonglong,
    used: c_ulonglong,
}

/// NVML_STRUCT_VERSION(Memory, 2) = sizeof(nvmlMemory_v2_t) | (2 << 24)
const NVML_MEMORY_V2: c_uint = std::mem::size_of::<MemoryV2>() as c_uint | (2 << 24);

/// NVML device handle. NVML is documented as thread-safe, and the handle is
/// only used while the owning [`Nvml`] (and its library) is alive.
#[derive(Clone, Copy, Debug)]
pub struct NvDevice(Handle);
// SAFETY: see type documentation.
unsafe impl Send for NvDevice {}

pub struct Nvml {
    shutdown: FnVoid,
    by_pci: FnByPci,
    name: FnName,
    util: FnUtil,
    encoder: FnCodec,
    decoder: FnCodec,
    temp: FnTemp,
    mem: FnMem,
    /// Optional: drivers older than R510 lack it; v1 is used then.
    mem_v2: Option<FnMemV2>,
    // Must outlive the function pointers above; dropped after `Drop::drop`.
    _lib: Library,
}

impl Nvml {
    /// Loads `libnvidia-ml.so.1` (or `.so`) and calls `nvmlInit_v2`.
    pub fn load() -> Result<Self, String> {
        let lib = ["libnvidia-ml.so.1", "libnvidia-ml.so"]
            .iter()
            // SAFETY: loading the vendor library runs its initialisers; this
            // is the documented way to use NVML.
            .find_map(|n| unsafe { Library::new(n) }.ok())
            .ok_or_else(|| "libnvidia-ml.so.1 not found".to_string())?;
        // SAFETY: symbol types match the NVML C API (nvml.h).
        unsafe {
            macro_rules! sym {
                ($name:literal) => {
                    *lib.get(concat!($name, "\0").as_bytes())
                        .map_err(|e| format!("{}: {e}", $name))?
                };
            }
            let init: FnVoid = sym!("nvmlInit_v2");
            let shutdown: FnVoid = sym!("nvmlShutdown");
            let by_pci: FnByPci = sym!("nvmlDeviceGetHandleByPciBusId_v2");
            let name: FnName = sym!("nvmlDeviceGetName");
            let util: FnUtil = sym!("nvmlDeviceGetUtilizationRates");
            let encoder: FnCodec = sym!("nvmlDeviceGetEncoderUtilization");
            let decoder: FnCodec = sym!("nvmlDeviceGetDecoderUtilization");
            let temp: FnTemp = sym!("nvmlDeviceGetTemperature");
            let mem: FnMem = sym!("nvmlDeviceGetMemoryInfo");
            let mem_v2: Option<FnMemV2> = lib
                .get::<FnMemV2>(b"nvmlDeviceGetMemoryInfo_v2\0")
                .ok()
                .map(|s| *s);
            // Initialise before building `Nvml`, so `Drop` (nvmlShutdown) only
            // ever runs after a successful init.
            let rc = init();
            if rc != NVML_SUCCESS {
                return Err(format!("nvmlInit_v2 failed (code {rc})"));
            }
            let nvml = Nvml {
                shutdown,
                by_pci,
                name,
                util,
                encoder,
                decoder,
                temp,
                mem,
                mem_v2,
                _lib: lib,
            };
            Ok(nvml)
        }
    }

    /// `pci` like `0000:01:00.0`.
    pub fn device_by_pci(&self, pci: &str) -> Option<NvDevice> {
        let id = CString::new(pci).ok()?;
        let mut h: Handle = std::ptr::null_mut();
        // SAFETY: valid C string and out-pointer.
        (unsafe { (self.by_pci)(id.as_ptr(), &mut h) } == NVML_SUCCESS && !h.is_null())
            .then_some(NvDevice(h))
    }

    pub fn name(&self, d: NvDevice) -> Option<String> {
        let mut buf = [0 as c_char; NAME_BUFFER];
        // SAFETY: buffer length passed to NVML; result is NUL-terminated.
        if unsafe { (self.name)(d.0, buf.as_mut_ptr(), NAME_BUFFER as c_uint) } != NVML_SUCCESS {
            return None;
        }
        let s = unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .trim()
            .to_string();
        (!s.is_empty()).then_some(s)
    }

    /// GPU-Util (%), the same value `nvidia-smi` reports as utilization.gpu.
    pub fn utilization(&self, d: NvDevice) -> Option<f64> {
        let mut u = Utilization::default();
        // SAFETY: valid out-pointer.
        let ok = unsafe { (self.util)(d.0, &mut u) } == NVML_SUCCESS;
        ok.then(|| percent(u.gpu)).flatten()
    }

    fn codec(&self, f: FnCodec, d: NvDevice) -> Option<f64> {
        let (mut value, mut period) = (0 as c_uint, 0 as c_uint);
        // SAFETY: valid out-pointers.
        let ok = unsafe { f(d.0, &mut value, &mut period) } == NVML_SUCCESS;
        ok.then(|| percent(value)).flatten()
    }

    /// NVENC utilisation; `None` on GPUs without an encoder (e.g. GT 1030).
    pub fn encoder(&self, d: NvDevice) -> Option<f64> {
        self.codec(self.encoder, d)
    }

    pub fn decoder(&self, d: NvDevice) -> Option<f64> {
        self.codec(self.decoder, d)
    }

    pub fn temperature(&self, d: NvDevice) -> Option<f64> {
        let mut t: c_uint = 0;
        // SAFETY: valid out-pointer.
        let ok = unsafe { (self.temp)(d.0, NVML_TEMPERATURE_GPU, &mut t) } == NVML_SUCCESS;
        (ok && t <= 150).then_some(f64::from(t))
    }

    /// (used, total) VRAM in bytes. Prefers nvmlMemory_v2 (used without the
    /// driver-reserved part, same as `nvidia-smi` and the legacy widget);
    /// falls back to v1 when v2 is missing or fails.
    pub fn memory(&self, d: NvDevice) -> Option<(f64, f64)> {
        if let Some(f) = self.mem_v2 {
            let mut m = MemoryV2 {
                version: NVML_MEMORY_V2,
                ..Default::default()
            };
            // SAFETY: valid out-pointer with the version field set as NVML requires.
            if unsafe { f(d.0, &mut m) } == NVML_SUCCESS
                && m.total > 0
                && m.used.saturating_add(m.reserved) <= m.total
            {
                return Some((m.used as f64, m.total as f64));
            }
        }
        let mut m = Memory::default();
        // SAFETY: valid out-pointer.
        let ok = unsafe { (self.mem)(d.0, &mut m) } == NVML_SUCCESS;
        (ok && m.total > 0 && m.used <= m.total).then_some((m.used as f64, m.total as f64))
    }
}

fn percent(v: c_uint) -> Option<f64> {
    (v <= 100).then_some(f64::from(v))
}

impl Drop for Nvml {
    fn drop(&mut self) {
        // SAFETY: init succeeded (constructor guarantees), so shutdown is valid.
        unsafe { (self.shutdown)() };
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn memory_v2_layout_matches_nvml_h() {
        // nvml.h: nvmlMemory_v2 == 0x02000028 on 64-bit Linux.
        assert_eq!(super::NVML_MEMORY_V2, 0x0200_0028);
    }

    #[test]
    fn load_does_not_crash_without_driver() {
        // Either loads (NVIDIA host) or returns an error; never panics.
        match super::Nvml::load() {
            Ok(n) => {
                assert!(n.device_by_pci("ffff:ff:1f.7").is_none());
            }
            Err(e) => assert!(!e.is_empty()),
        }
    }
}
