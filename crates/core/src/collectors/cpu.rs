//! CPU usage (`/proc/stat`), average frequency (cpufreq sysfs) and
//! temperature (hwmon). Paths are discovered once in [`CpuCollector::new`];
//! each sample only reads small files, no process is started.

use super::{list_dir, read_i64, read_trimmed, read_u64, Roots};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTimes {
    pub total: u64,
    pub idle: u64,
}

/// Aggregate line and per-CPU `cpuN` lines (label, times) of `/proc/stat`.
/// Offline CPUs are absent, so lines are matched by label, not position.
pub fn parse_stat(text: &str) -> (Option<CpuTimes>, Vec<(String, Option<CpuTimes>)>) {
    let mut total = None;
    let mut cores = Vec::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("cpu") => total = parse_times(it),
            Some(l)
                if l.len() > 3
                    && l.starts_with("cpu")
                    && l[3..].bytes().all(|b| b.is_ascii_digit()) =>
            {
                cores.push((l.to_string(), parse_times(it)));
            }
            _ => {}
        }
    }
    (total, cores)
}

/// Fields of one `cpu`/`cpuN` line:
/// total = user+nice+system+idle+iowait+irq+softirq+steal (guest time is
/// already included in user/nice); idle = idle+iowait.
fn parse_times<'a>(it: impl Iterator<Item = &'a str>) -> Option<CpuTimes> {
    let v: Vec<u64> = it.take(8).map(|x| x.parse().ok()).collect::<Option<_>>()?;
    if v.len() < 4 {
        return None;
    }
    let get = |i: usize| v.get(i).copied().unwrap_or(0);
    // Checked arithmetic: malformed / hostile input must not overflow.
    let idle = get(3).checked_add(get(4))?;
    let total = v.iter().try_fold(0u64, |acc, x| acc.checked_add(*x))?;
    Some(CpuTimes { total, idle })
}

/// Busy percentage between two readings; `None` on counter reset or no delta.
pub fn usage_between(prev: CpuTimes, cur: CpuTimes) -> Option<f64> {
    let dt = cur.total.checked_sub(prev.total)?;
    let di = cur.idle.checked_sub(prev.idle)?;
    if dt == 0 || di > dt {
        return None;
    }
    Some((dt - di) as f64 / dt as f64 * 100.0)
}

/// How the CPU temperature is derived from one hwmon chip.
#[derive(Debug, Clone, PartialEq)]
pub enum TempSource {
    /// Average of several inputs (Intel coretemp "Core N"), like KSystemStats
    /// `cpu/all/averageTemperature`.
    Average(Vec<PathBuf>),
    /// One representative input (package, Tdie/Tctl, SoC sensor).
    Single(PathBuf),
    None,
}

/// `<kind>N_input` files of one hwmon chip with their `_label` (empty if none);
/// `kind` is `temp` or `fan`.
pub(super) fn hwmon_inputs(dir: &std::path::Path, kind: &str) -> Vec<(PathBuf, String)> {
    list_dir(dir, |n| {
        n.strip_prefix(kind)
            .and_then(|r| r.strip_suffix("_input"))
            .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
    })
    .into_iter()
    .map(|input| {
        let label_path = PathBuf::from(input.to_string_lossy().replace("_input", "_label"));
        let label = read_trimmed(&label_path).unwrap_or_default();
        (input, label)
    })
    .collect()
}

/// Finds the CPU temperature sensor among `/sys/class/hwmon/hwmon*`.
pub fn discover_temp(roots: &Roots) -> TempSource {
    let chips = list_dir(&roots.sys("class/hwmon"), |n| n.starts_with("hwmon"));
    let named: Vec<(PathBuf, String)> = chips
        .into_iter()
        .filter_map(|d| read_trimmed(&d.join("name")).map(|n| (d, n)))
        .collect();
    // Preference order: Intel, AMD, then common SoC drivers.
    for (dir, name) in &named {
        if name == "coretemp" {
            let inputs = hwmon_inputs(dir, "temp");
            let cores: Vec<PathBuf> = inputs
                .iter()
                .filter(|(_, l)| l.starts_with("Core "))
                .map(|(p, _)| p.clone())
                .collect();
            if !cores.is_empty() {
                return TempSource::Average(cores);
            }
            if let Some((p, _)) = inputs.iter().find(|(_, l)| l.starts_with("Package id")) {
                return TempSource::Single(p.clone());
            }
        }
    }
    for (dir, name) in &named {
        if name == "k10temp" || name == "zenpower" {
            let inputs = hwmon_inputs(dir, "temp");
            for want in ["Tdie", "Tctl"] {
                if let Some((p, _)) = inputs.iter().find(|(_, l)| l == want) {
                    return TempSource::Single(p.clone());
                }
            }
            if let Some((p, _)) = inputs.first() {
                return TempSource::Single(p.clone());
            }
        }
    }
    for (dir, name) in &named {
        if matches!(name.as_str(), "cpu_thermal" | "cpu-thermal" | "soc_thermal") {
            if let Some((p, _)) = hwmon_inputs(dir, "temp").first() {
                return TempSource::Single(p.clone());
            }
        }
    }
    TempSource::None
}

/// hwmon reports millidegrees Celsius. Implausible values are rejected.
pub(super) fn read_millicelsius(path: &std::path::Path) -> Option<f64> {
    let c = read_i64(path)? as f64 / 1000.0;
    (-40.0..=150.0).contains(&c).then_some(c)
}

pub fn read_temp(src: &TempSource) -> Option<f64> {
    match src {
        TempSource::Average(paths) => {
            let vals: Vec<f64> = paths.iter().filter_map(|p| read_millicelsius(p)).collect();
            (!vals.is_empty()).then(|| vals.iter().sum::<f64>() / vals.len() as f64)
        }
        TempSource::Single(p) => read_millicelsius(p),
        TempSource::None => None,
    }
}

/// `model name` (x86) or `Hardware` (some ARM) from `/proc/cpuinfo`.
pub fn cpu_model(roots: &Roots) -> Option<String> {
    let text = std::fs::read_to_string(roots.proc("cpuinfo")).ok()?;
    text.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        let key = key.trim();
        if key.eq_ignore_ascii_case("model name") || key.eq_ignore_ascii_case("Hardware") {
            let v = value.split_whitespace().collect::<Vec<_>>().join(" ");
            (!v.is_empty()).then_some(v)
        } else {
            None
        }
    })
}

/// (physical cores, logical CPUs) from `/proc/cpuinfo`: logical = number of
/// `processor` entries, physical = distinct (`physical id`, `core id`) pairs.
/// Cores are `None` when the kernel gives no core ids (many ARM systems).
pub fn parse_topology(text: &str) -> (Option<u32>, Option<u32>) {
    let mut threads = 0u32;
    let mut cores = std::collections::BTreeSet::new();
    let mut package = String::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "processor" => threads += 1,
            "physical id" => package = value.trim().to_string(),
            "core id" => {
                cores.insert((package.clone(), value.trim().to_string()));
            }
            _ => {}
        }
    }
    let n = |v: usize| u32::try_from(v).ok().filter(|v| *v > 0);
    (n(cores.len()), n(threads as usize))
}

pub fn cpu_topology(roots: &Roots) -> (Option<u32>, Option<u32>) {
    std::fs::read_to_string(roots.proc("cpuinfo")).map_or((None, None), |t| parse_topology(&t))
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CpuReading {
    pub usage: Option<f64>,
    /// Per logical CPU, in `/proc/stat` order; `None` for the first sample,
    /// a CPU that just came online or a counter reset.
    pub per_core: Vec<Option<f64>>,
    pub freq_mhz: Option<f64>,
    pub temp_c: Option<f64>,
}

pub struct CpuCollector {
    stat_path: PathBuf,
    freq_paths: Vec<PathBuf>,
    temp: TempSource,
    prev: Option<CpuTimes>,
    prev_cores: std::collections::HashMap<String, CpuTimes>,
}

impl CpuCollector {
    pub fn new(roots: &Roots) -> Self {
        let freq_paths = list_dir(&roots.sys("devices/system/cpu"), |n| {
            n.strip_prefix("cpu")
                .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
        })
        .into_iter()
        .map(|d| d.join("cpufreq/scaling_cur_freq"))
        .filter(|p| p.exists())
        .collect();
        Self {
            stat_path: roots.proc("stat"),
            freq_paths,
            temp: discover_temp(roots),
            prev: None,
            prev_cores: std::collections::HashMap::new(),
        }
    }

    /// Average of `scaling_cur_freq` (kHz) over all CPUs, in MHz.
    fn read_freq(&self) -> Option<f64> {
        let vals: Vec<u64> = self
            .freq_paths
            .iter()
            .filter_map(|p| read_u64(p))
            .filter(|v| *v > 0)
            .collect();
        (!vals.is_empty()).then(|| vals.iter().sum::<u64>() as f64 / vals.len() as f64 / 1000.0)
    }

    /// First call has no usage (needs two readings).
    pub fn sample(&mut self) -> CpuReading {
        let (cur, cores) = std::fs::read_to_string(&self.stat_path)
            .map(|t| parse_stat(&t))
            .unwrap_or_default();
        let usage = match (self.prev, cur) {
            (Some(p), Some(c)) => usage_between(p, c),
            _ => None,
        };
        self.prev = cur;
        let per_core = cores
            .iter()
            .map(|(label, c)| {
                c.zip(self.prev_cores.get(label).copied())
                    .and_then(|(c, p)| usage_between(p, c))
            })
            .collect();
        self.prev_cores = cores
            .into_iter()
            .filter_map(|(label, c)| Some((label, c?)))
            .collect();
        CpuReading {
            usage,
            per_core,
            freq_mhz: self.read_freq(),
            temp_c: read_temp(&self.temp),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topology_counts_cores_and_threads() {
        let block = |p: u32, pkg: u32, core: u32| {
            format!("processor\t: {p}\nphysical id\t: {pkg}\ncore id\t\t: {core}\n\n")
        };
        // 4 cores / 8 threads (Hyper-Threading): core ids repeat.
        let ht: String = (0..8).map(|p| block(p, 0, p % 4)).collect();
        assert_eq!(parse_topology(&ht), (Some(4), Some(8)));
        // Two sockets reuse core ids: (package, core) pairs stay distinct.
        let dual: String = (0..4).map(|p| block(p, p / 2, p % 2)).collect();
        assert_eq!(parse_topology(&dual), (Some(4), Some(4)));
        assert_eq!(
            parse_topology("processor : 0\nprocessor : 1\n"),
            (None, Some(2))
        );
        assert_eq!(parse_topology(""), (None, None));
    }
    use crate::collectors::testutil::{roots, temp_root, write};

    #[test]
    fn stat_parsing_and_usage() {
        let a = parse_stat("cpu  4619833 1989 1947850 129917758 138714 938250 394073 0 0 0")
            .0
            .unwrap();
        assert_eq!(a.idle, 129917758 + 138714);
        assert_eq!(
            a.total,
            4619833 + 1989 + 1947850 + 129917758 + 138714 + 938250 + 394073
        );
        let b = CpuTimes {
            total: a.total + 1000,
            idle: a.idle + 750,
        };
        assert_eq!(usage_between(a, b), Some(25.0));
        assert_eq!(usage_between(a, a), None, "no elapsed ticks");
        assert_eq!(usage_between(b, a), None, "counter went backwards");
        assert!(parse_stat("cpu0 1 2 3 4").0.is_none());
        assert!(parse_stat("cpu x y z").0.is_none());
        let max = u64::MAX;
        assert!(
            parse_stat(&format!("cpu {max} {max} 0 0")).0.is_none(),
            "overflow rejected"
        );
    }

    #[test]
    fn coretemp_core_average_excludes_package() {
        let r = temp_root("coretemp");
        write(&r, "sys/class/hwmon/hwmon0/name", "acpitz\n");
        write(&r, "sys/class/hwmon/hwmon0/temp1_input", "27800\n");
        let c = "sys/class/hwmon/hwmon2";
        write(&r, &format!("{c}/name"), "coretemp\n");
        for (i, (label, v)) in [
            ("Package id 0", 49000),
            ("Core 0", 45000),
            ("Core 1", 46000),
            ("Core 2", 46000),
            ("Core 3", 46500),
        ]
        .iter()
        .enumerate()
        {
            write(&r, &format!("{c}/temp{}_label", i + 1), label);
            write(&r, &format!("{c}/temp{}_input", i + 1), &v.to_string());
        }
        let src = discover_temp(&roots(&r));
        assert!(matches!(&src, TempSource::Average(v) if v.len() == 4));
        assert_eq!(read_temp(&src), Some(45.875));
    }

    #[test]
    fn k10temp_prefers_tdie_then_tctl() {
        let r = temp_root("k10");
        let c = "sys/class/hwmon/hwmon3";
        write(&r, &format!("{c}/name"), "k10temp");
        write(&r, &format!("{c}/temp1_label"), "Tctl");
        write(&r, &format!("{c}/temp1_input"), "61250");
        write(&r, &format!("{c}/temp3_label"), "Tccd1");
        write(&r, &format!("{c}/temp3_input"), "58000");
        assert_eq!(read_temp(&discover_temp(&roots(&r))), Some(61.25));
    }

    #[test]
    fn missing_sensors_are_none() {
        let r = temp_root("nosensor");
        assert_eq!(discover_temp(&roots(&r)), TempSource::None);
        let mut c = CpuCollector::new(&roots(&r));
        assert_eq!(c.sample(), CpuReading::default());
    }

    #[test]
    fn collector_reads_fake_tree() {
        let r = temp_root("cpu");
        write(
            &r,
            "proc/stat",
            "cpu  100 0 100 800 0 0 0 0 0 0\ncpu0 1 2 3 4\n",
        );
        write(
            &r,
            "proc/cpuinfo",
            "processor\t: 0\nmodel name\t: Intel(R)  Xeon(R) CPU E3-1231 v3 @ 3.40GHz\n",
        );
        write(
            &r,
            "sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq",
            "3700000",
        );
        write(
            &r,
            "sys/devices/system/cpu/cpu1/cpufreq/scaling_cur_freq",
            "3500000",
        );
        write(
            &r,
            "sys/devices/system/cpu/cpufreq/policy0/scaling_cur_freq",
            "1",
        );
        let rt = roots(&r);
        assert_eq!(
            cpu_model(&rt).as_deref(),
            Some("Intel(R) Xeon(R) CPU E3-1231 v3 @ 3.40GHz")
        );
        let mut c = CpuCollector::new(&rt);
        let first = c.sample();
        assert_eq!(first.usage, None);
        assert_eq!(first.per_core, vec![None], "first sample has no delta");
        assert_eq!(first.freq_mhz, Some(3600.0));
        // cpu0: 3 of 4 jiffies busy; cpu1 just came online (no previous value).
        write(
            &r,
            "proc/stat",
            "cpu  150 0 150 900 0 0 0 0 0 0\ncpu0 2 2 5 5\ncpu1 1 1 1 1\n",
        );
        let second = c.sample();
        assert_eq!(second.usage, Some(50.0));
        assert_eq!(second.per_core, vec![Some(75.0), None]);
    }

    #[test]
    fn stat_lines_are_matched_by_label() {
        let (total, cores) =
            parse_stat("cpu 1 1 1 1\ncpu0 1 1 1 1\ncpu2 x\ncpufoo 1 1 1 1\nintr 5\n");
        assert!(total.is_some());
        let labels: Vec<&str> = cores.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(labels, ["cpu0", "cpu2"]);
        assert!(cores[1].1.is_none(), "malformed line kept as missing");
    }
}
