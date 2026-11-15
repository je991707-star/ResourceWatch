//! ResourceWatch core: hardware collectors, snapshot model, scheduler and
//! settings. No UI toolkit dependency; the Tauri app (`src-tauri`) and the
//! Slint app build on top of it.
//!
//! Snapshots come from the live `monitor::Monitor` (Rust collectors reading
//! `/proc`, sysfs, hwmon and NVML), or from `mock::MockSource` when
//! `RESOURCEWATCH_MOCK=1` is set for UI development.

pub mod collectors;
pub mod format;
pub mod history;
pub mod mock;
pub mod monitor;
#[cfg(test)]
mod robustness;
pub mod scheduler;
pub mod settings;
pub mod settings_store;
pub mod snapshot;
pub mod source;

use std::time::{Duration, Instant};

/// Headless sampling for cross-checking against system tools
/// (`cargo run -p resourcewatch-core --example sample`). Returns `count` live
/// snapshots taken `interval` apart; no window, no settings file.
pub fn sample_live(count: usize, interval: Duration) -> Vec<serde_json::Value> {
    let settings = settings::Settings::default();
    let mut m = monitor::Monitor::new(&settings);
    (0..count)
        .map(|i| {
            if i > 0 {
                std::thread::sleep(interval);
            }
            serde_json::to_value(m.sample(Instant::now(), &settings)).unwrap_or_default()
        })
        .collect()
}
