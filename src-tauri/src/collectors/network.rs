//! Network throughput (`/proc/net/dev`) and IPv4 address (`getifaddrs`).
//!
//! `"all"` sums **physical** interfaces only (those with
//! `/sys/class/net/<if>/device`): loopback, bridges, veth, tun/VPN and other
//! virtual devices are excluded so forwarded or encapsulated traffic is not
//! counted twice. IPv4 is looked up only at startup and on manual refresh.

use super::{counter_rate, Roots};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

/// interface → (rx bytes, tx bytes)
pub fn parse_net_dev(text: &str) -> HashMap<String, (u64, u64)> {
    text.lines()
        .skip(2)
        .filter_map(|line| {
            let (name, rest) = line.split_once(':')?;
            let f: Vec<u64> = rest
                .split_whitespace()
                .map(|x| x.parse().ok())
                .collect::<Option<_>>()?;
            (f.len() >= 16).then(|| (name.trim().to_string(), (f[0], f[8])))
        })
        .collect()
}

/// name → cumulative (first, second) counters.
type Counters = HashMap<String, (u64, u64)>;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NetReading {
    pub down: Option<f64>,
    pub up: Option<f64>,
}

pub struct NetCollector {
    dev_path: PathBuf,
    class_net: PathBuf,
    /// `None` = all physical interfaces.
    interface: Option<String>,
    physical: HashMap<String, bool>,
    prev: Option<(Instant, Counters)>,
}

impl NetCollector {
    pub fn new(roots: &Roots, selection: &str) -> Self {
        let mut c = Self {
            dev_path: roots.proc("net/dev"),
            class_net: roots.sys("class/net"),
            interface: None,
            physical: HashMap::new(),
            prev: None,
        };
        c.select(selection);
        c
    }

    /// `"all"` or an interface name (already validated by settings).
    pub fn select(&mut self, selection: &str) {
        let next = (selection != "all").then(|| selection.to_string());
        if next != self.interface {
            self.interface = next;
            self.prev = None;
        }
    }

    fn is_physical(&mut self, name: &str) -> bool {
        if let Some(v) = self.physical.get(name) {
            return *v;
        }
        let v = self.class_net.join(name).join("device").exists();
        self.physical.insert(name.to_string(), v);
        v
    }

    /// Throughput since the previous call, using the cycle timestamp `now`.
    pub fn sample_at(&mut self, now: Instant) -> NetReading {
        let cur = std::fs::read_to_string(&self.dev_path)
            .ok()
            .map(|t| parse_net_dev(&t));
        let reading = match (self.prev.take(), &cur) {
            (Some((t0, prev)), Some(cur)) => {
                let secs = now.saturating_duration_since(t0).as_secs_f64();
                let names: Vec<String> = match &self.interface {
                    Some(n) => vec![n.clone()],
                    None => cur.keys().cloned().collect(),
                };
                let (mut down, mut up, mut any) = (0.0, 0.0, false);
                for name in names {
                    if self.interface.is_none() && !self.is_physical(&name) {
                        continue;
                    }
                    // Interfaces that appeared, disappeared or reset their
                    // counters are skipped for this one sample.
                    if let (Some(&(rx0, tx0)), Some(&(rx1, tx1))) =
                        (prev.get(&name), cur.get(&name))
                    {
                        if let (Some(d), Some(u)) =
                            (counter_rate(rx0, rx1, secs), counter_rate(tx0, tx1, secs))
                        {
                            down += d;
                            up += u;
                            any = true;
                        }
                    }
                }
                if any {
                    NetReading {
                        down: Some(down),
                        up: Some(up),
                    }
                } else {
                    NetReading::default()
                }
            }
            _ => NetReading::default(),
        };
        self.prev = cur.map(|c| (now, c));
        reading
    }
}

fn usable_ipv4(ip: std::net::Ipv4Addr) -> bool {
    !ip.is_loopback() && !ip.is_link_local() && !ip.is_unspecified()
}

/// First usable IPv4 of the selected interface (`"all"`: any non-loopback
/// interface). Uses `getifaddrs(3)`; no external command, no shell.
pub fn ipv4_address(selection: &str) -> Option<String> {
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs allocates a linked list we free with freeifaddrs.
    if unsafe { libc::getifaddrs(&mut list) } != 0 {
        return None;
    }
    let mut found = None;
    let mut cur = list;
    while !cur.is_null() {
        // SAFETY: `cur` is a node of the list returned above.
        let ifa = unsafe { &*cur };
        cur = ifa.ifa_next;
        if ifa.ifa_addr.is_null() || ifa.ifa_name.is_null() {
            continue;
        }
        // SAFETY: non-null addr/name pointers from getifaddrs are valid.
        let family = unsafe { (*ifa.ifa_addr).sa_family };
        if i32::from(family) != libc::AF_INET || ifa.ifa_flags & (libc::IFF_LOOPBACK as u32) != 0 {
            continue;
        }
        let name = unsafe { std::ffi::CStr::from_ptr(ifa.ifa_name) }.to_string_lossy();
        if selection != "all" && name != selection {
            continue;
        }
        // SAFETY: family is AF_INET, so the address is a sockaddr_in.
        let sin = unsafe { &*(ifa.ifa_addr as *const libc::sockaddr_in) };
        let ip = std::net::Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
        if usable_ipv4(ip) {
            found = Some(ip.to_string());
            break;
        }
    }
    // SAFETY: list came from getifaddrs and is freed exactly once.
    unsafe { libc::freeifaddrs(list) };
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::testutil::{roots, temp_root, write};
    use std::time::Duration;

    const HEADER: &str = "Inter-|   Receive    |  Transmit\n face |bytes packets errs drop fifo frame compressed multicast|bytes packets errs drop fifo colls carrier compressed\n";

    fn dev(lo: u64, eth: (u64, u64), br: u64) -> String {
        format!(
            "{HEADER}    lo: {lo} 1 0 0 0 0 0 0 {lo} 1 0 0 0 0 0 0\n\
             enp3s0: {} 1 0 0 0 0 0 0 {} 1 0 0 0 0 0 0\n\
             virbr0: {br} 1 0 0 0 0 0 0 {br} 1 0 0 0 0 0 0\n",
            eth.0, eth.1
        )
    }

    #[test]
    fn parse() {
        let m = parse_net_dev(&dev(5, (100, 200), 7));
        assert_eq!(m["enp3s0"], (100, 200));
        assert_eq!(m["lo"], (5, 5));
        assert!(parse_net_dev("garbage").is_empty());
    }

    #[test]
    fn all_counts_physical_only_and_handles_reset() {
        let r = temp_root("net");
        std::fs::create_dir_all(r.join("sys/class/net/enp3s0/device")).unwrap();
        std::fs::create_dir_all(r.join("sys/class/net/virbr0")).unwrap();
        std::fs::create_dir_all(r.join("sys/class/net/lo")).unwrap();
        write(&r, "proc/net/dev", &dev(0, (1000, 2000), 0));
        let mut c = NetCollector::new(&roots(&r), "all");
        let t0 = Instant::now();
        assert_eq!(c.sample_at(t0), NetReading::default());
        write(&r, "proc/net/dev", &dev(9_000_000, (3000, 2500), 9_000_000));
        let s = c.sample_at(t0 + Duration::from_secs(2));
        assert_eq!(
            s,
            NetReading {
                down: Some(1000.0),
                up: Some(250.0)
            }
        );
        write(&r, "proc/net/dev", &dev(0, (10, 10), 0));
        assert_eq!(
            c.sample_at(t0 + Duration::from_secs(3)),
            NetReading::default(),
            "reset"
        );
    }

    #[test]
    fn named_interface_and_missing_interface() {
        let r = temp_root("net2");
        write(&r, "proc/net/dev", &dev(0, (1000, 2000), 50));
        let mut c = NetCollector::new(&roots(&r), "virbr0");
        let t0 = Instant::now();
        c.sample_at(t0);
        write(&r, "proc/net/dev", &dev(0, (1000, 2000), 150));
        assert_eq!(c.sample_at(t0 + Duration::from_secs(1)).down, Some(100.0));
        c.select("wlan9");
        c.sample_at(t0 + Duration::from_secs(2));
        assert_eq!(
            c.sample_at(t0 + Duration::from_secs(3)),
            NetReading::default()
        );
    }

    #[test]
    fn long_gap_after_resume_gives_average_not_spike() {
        let r = temp_root("net-resume");
        write(&r, "proc/net/dev", &dev(0, (1_000, 1_000), 0));
        let mut c = NetCollector::new(&roots(&r), "enp3s0");
        let t0 = Instant::now();
        c.sample_at(t0);
        write(&r, "proc/net/dev", &dev(0, (1_000 + 3_600, 1_000), 0));
        let s = c.sample_at(t0 + Duration::from_secs(3600));
        assert_eq!(
            s,
            NetReading {
                down: Some(1.0),
                up: Some(0.0)
            }
        );
    }

    #[test]
    fn ipv4_filter() {
        assert!(!usable_ipv4("127.0.0.1".parse().unwrap()));
        assert!(!usable_ipv4("169.254.1.2".parse().unwrap()));
        assert!(usable_ipv4("192.168.50.212".parse().unwrap()));
        // Real call must not crash; a non-existent interface yields None.
        assert_eq!(ipv4_address("rw-no-such-if"), None);
    }
}
