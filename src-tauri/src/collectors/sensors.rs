//! Every hwmon temperature and fan input (`/sys/class/hwmon/hwmon*`), for the
//! sensor list. Inputs are discovered at startup and on hardware refresh; each
//! sample reads only the small `*_input` files.

use super::cpu::{hwmon_inputs, read_millicelsius};
use super::{list_dir, read_trimmed, read_u64, Roots};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SensorKind {
    /// °C
    Temp,
    /// RPM
    Fan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SensorInput {
    /// hwmon `name` (e.g. `coretemp`, `nvme`, `it8686`).
    pub chip: String,
    /// `*_label`, or the input name (`temp1`, `fan2`) when there is none.
    pub label: String,
    pub kind: SensorKind,
    path: PathBuf,
}

pub fn discover(roots: &Roots) -> Vec<SensorInput> {
    let mut out = Vec::new();
    for dir in list_dir(&roots.sys("class/hwmon"), |n| n.starts_with("hwmon")) {
        let Some(chip) = read_trimmed(&dir.join("name")) else {
            continue;
        };
        for (kind, prefix) in [(SensorKind::Temp, "temp"), (SensorKind::Fan, "fan")] {
            for (path, label) in hwmon_inputs(&dir, prefix) {
                let label = if label.is_empty() {
                    let file = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    file.trim_end_matches("_input").to_string()
                } else {
                    label
                };
                out.push(SensorInput {
                    chip: chip.clone(),
                    label,
                    kind,
                    path,
                });
            }
        }
    }
    out
}

/// °C for temperatures (−40…150, like the CPU sensor), RPM for fans
/// (0…100 000); anything else is missing.
pub fn read(input: &SensorInput) -> Option<f64> {
    match input.kind {
        SensorKind::Temp => read_millicelsius(&input.path),
        SensorKind::Fan => read_u64(&input.path)
            .filter(|rpm| *rpm <= 100_000)
            .map(|rpm| rpm as f64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::testutil::{roots, temp_root, write};

    #[test]
    fn lists_temps_and_fans_with_labels() {
        let r = temp_root("sensors");
        write(&r, "sys/class/hwmon/hwmon0/name", "acpitz\n");
        write(&r, "sys/class/hwmon/hwmon0/temp1_input", "27800\n");
        write(&r, "sys/class/hwmon/hwmon2/name", "it8686\n");
        write(&r, "sys/class/hwmon/hwmon2/fan1_input", "1700\n");
        write(&r, "sys/class/hwmon/hwmon2/fan1_label", "CPU Fan\n");
        write(&r, "sys/class/hwmon/hwmon2/fan2_input", "999999\n");
        write(&r, "sys/class/hwmon/hwmon2/temp1_input", "200000\n");
        write(&r, "sys/class/hwmon/hwmon2/temp1_max", "90000\n");
        write(&r, "sys/class/hwmon/hwmon3/temp1_input", "40000\n"); // no name: skipped
        let found = discover(&roots(&r));
        let view: Vec<(&str, &str, SensorKind, Option<f64>)> = found
            .iter()
            .map(|s| (s.chip.as_str(), s.label.as_str(), s.kind, read(s)))
            .collect();
        assert_eq!(
            view,
            [
                ("acpitz", "temp1", SensorKind::Temp, Some(27.8)),
                ("it8686", "temp1", SensorKind::Temp, None), // 200 °C rejected
                ("it8686", "CPU Fan", SensorKind::Fan, Some(1700.0)),
                ("it8686", "fan2", SensorKind::Fan, None), // implausible RPM
            ]
        );
        assert!(discover(&roots(&temp_root("no-hwmon"))).is_empty());
    }
}
