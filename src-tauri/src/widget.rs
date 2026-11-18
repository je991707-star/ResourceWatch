//! Desktop widget support: the config file the settings page writes, and `resourcewatch --widget`,
//! which the Plasma widget runs on a timer to get `{ config, snapshot }` as JSON.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const ITEMS: [&str; 6] = ["cpu", "mem", "gpu", "net", "disk", "temp"];
/// What the panel can show (no temperature there), and its default order.
const PANEL_ITEMS: [&str; 5] = ["cpu", "gpu", "mem", "net", "disk"];

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(default)]
pub struct Config {
    /// Cards in display order; values from `ITEMS`.
    pub items: Vec<String>,
    /// Seconds between widget refreshes (each run itself takes about a second).
    pub interval: u32,
    /// Interface for the network card; None = the one with the default gateway.
    pub nic: Option<String>,
    /// The app icon/name card above the CPU card.
    pub app: bool,
    /// Items shown when the widget sits in a panel, in order, independent of the desktop cards; values from `PANEL_ITEMS`.
    pub panel: Vec<String>,
    /// Card background opacity in percent, 60–100 (the settings slider covers 0–40% transparency; text and charts stay opaque).
    pub opacity: u32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            // left column cpu/net/temp, right column gpu/disk/mem (item i goes to column i % 2)
            items: ["cpu", "gpu", "net", "disk", "temp", "mem"].map(String::from).to_vec(),
            interval: 2,
            nic: None,
            app: true,
            opacity: 100,
            panel: PANEL_ITEMS.map(String::from).to_vec(),
        }
    }
}

impl Config {
    /// Drops unknown and repeated items and keeps the interval at least 2 s.
    fn clean(mut self) -> Self {
        let mut seen = Vec::new();
        self.items
            .retain(|i| ITEMS.contains(&i.as_str()) && !seen.contains(i) && { seen.push(i.clone()); true });
        let mut seen = Vec::new();
        self.panel
            .retain(|i| PANEL_ITEMS.contains(&i.as_str()) && !seen.contains(i) && { seen.push(i.clone()); true });
        self.interval = self.interval.clamp(2, 60);
        self.opacity = self.opacity.clamp(60, 100);
        self
    }
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("resourcewatch/widget.json"))
}

/// Missing fields take their defaults; unreadable JSON gives the whole default.
fn parse(s: &str) -> Config {
    serde_json::from_str::<Config>(s).unwrap_or_default().clean()
}

pub fn load() -> Config {
    path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map_or_else(Config::default, |s| parse(&s))
}

pub fn save(cfg: Config) -> Result<Config, String> {
    let cfg = cfg.clean();
    let p = path().ok_or("找不到設定資料夾")?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
    std::fs::write(&p, json).map_err(|e| e.to_string())?;
    Ok(cfg)
}

/// `resourcewatch --widget-order cpu,mem,…`: the widget saves a new card order after a drag.
/// The list goes through `clean` like any other save, so unknown names are dropped.
pub fn set_order(list: &str) -> Result<(), String> {
    let mut cfg = load();
    cfg.items = list.split(',').map(str::to_string).collect();
    save(cfg).map(|_| ())
}

/// `resourcewatch --widget-app on|off`: the widget's right-click menu closes the app card.
pub fn set_app(on: &str) -> Result<(), String> {
    let mut cfg = load();
    cfg.app = match on {
        "on" => true,
        "off" => false,
        _ => return Err("用法：--widget-app on|off".into()),
    };
    save(cfg).map(|_| ())
}

/// Samples twice a second apart (rates need two readings), prints `{ config, cpuModel, snapshot }`. No GUI is started.
pub fn run() {
    let mut m = crate::sys::Monitor::new();
    m.sample();
    std::thread::sleep(std::time::Duration::from_secs(1));
    let out = serde_json::json!({ "config": load(), "cpuModel": crate::sys::cpu_model(), "snapshot": m.sample() });
    println!("{out}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config() {
        assert_eq!(parse("not json"), Config::default());
        assert_eq!(parse("{}"), Config::default());
        let c = parse(r#"{"items":["temp","bogus","cpu","temp"],"interval":0}"#);
        assert_eq!(c.items, ["temp", "cpu"]);
        assert_eq!(c.interval, 2);
        assert_eq!(c.nic, None);
        assert_eq!(parse(r#"{"nic":"eth0","interval":5}"#).nic.as_deref(), Some("eth0"));
        assert_eq!(parse(r#"{"interval":5}"#).interval, 5);
        assert!(parse("{}").app);
        assert!(!parse(r#"{"app":false}"#).app);
        assert_eq!(parse("{}").opacity, 100);
        assert_eq!(parse(r#"{"opacity":250}"#).opacity, 100);
        assert_eq!(parse(r#"{"opacity":10}"#).opacity, 60);
        assert_eq!(parse("{}").panel, PANEL_ITEMS);
        assert_eq!(parse(r#"{"panel":["net","temp","cpu","net"]}"#).panel, ["net", "cpu"]);
    }
}
