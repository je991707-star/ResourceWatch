//! Settings persistence: `<app config dir>/config.json`.
//!
//! - Missing file: normal first start → defaults, no warning.
//! - Unreadable file: defaults + warning; the file is left untouched.
//! - Not valid JSON / not an object: the file is renamed to
//!   `config.json.corrupt-<unix seconds>` (preserved, never overwritten),
//!   defaults are used and a warning is reported.
//! - Individual invalid or missing fields fall back to their default value
//!   (other valid fields are kept) with a warning.
//! - Saving writes a unique temporary file in the same directory, fsyncs it
//!   and atomically renames it over `config.json`, so a crash never leaves a
//!   half-written file. Callers serialise saves (see `AppState`).

use crate::settings::{Settings, SettingsPatch};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const FILE_NAME: &str = "config.json";

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub settings: Settings,
    /// Human-readable problem found while loading (shown in the settings page).
    pub warning: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

/// Field-by-field validation of a stored JSON object. Unknown keys are
/// ignored (forward compatibility); `networkInterface` is accepted as an
/// alias of `netInterface`.
pub fn parse_stored(value: &Value) -> Result<(Settings, Vec<String>), String> {
    let obj = value.as_object().ok_or("設定檔內容不是 JSON 物件")?;
    let mut settings = Settings::default();
    let mut problems = Vec::new();
    let mut apply = |key: &str, patch: Option<SettingsPatch>| match patch {
        Some(p) => match settings.apply(&p) {
            Ok(next) => settings = next,
            Err(e) => problems.push(format!("{key}：{}，已改用預設值", e.message)),
        },
        None => problems.push(format!("{key}：型別錯誤，已改用預設值")),
    };
    let int = |v: &Value| v.as_u64().and_then(|n| u32::try_from(n).ok());
    if let Some(v) = obj.get("updateInterval") {
        apply(
            "updateInterval",
            int(v).map(|n| SettingsPatch {
                update_interval: Some(n),
                ..Default::default()
            }),
        );
    }
    if let Some(v) = obj.get("historyPoints") {
        apply(
            "historyPoints",
            int(v).map(|n| SettingsPatch {
                history_points: Some(n),
                ..Default::default()
            }),
        );
    }
    if let Some(v) = obj.get("gpuIndex") {
        apply(
            "gpuIndex",
            int(v).map(|n| SettingsPatch {
                gpu_index: Some(n),
                ..Default::default()
            }),
        );
    }
    if let Some(v) = obj
        .get("netInterface")
        .or_else(|| obj.get("networkInterface"))
    {
        apply(
            "netInterface",
            v.as_str().map(|s| SettingsPatch {
                net_interface: Some(s.to_string()),
                ..Default::default()
            }),
        );
    }
    if let Some(v) = obj.get("sidebarGlass") {
        apply(
            "sidebarGlass",
            v.as_bool().map(|b| SettingsPatch {
                sidebar_glass: Some(b),
                ..Default::default()
            }),
        );
    }
    Ok((settings, problems))
}

impl SettingsStore {
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join(FILE_NAME),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Loaded {
        let text = match fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Loaded {
                    settings: Settings::default(),
                    warning: None,
                };
            }
            Err(e) => {
                return Loaded {
                    settings: Settings::default(),
                    warning: Some(format!(
                        "無法讀取設定檔 {}（{e}），本次使用預設值",
                        self.path.display()
                    )),
                };
            }
        };
        let parsed = serde_json::from_str::<Value>(&text)
            .map_err(|e| e.to_string())
            .and_then(|v| parse_stored(&v));
        match parsed {
            Ok((settings, problems)) => Loaded {
                settings,
                warning: (!problems.is_empty())
                    .then(|| format!("設定檔部分欄位無效：{}", problems.join("；"))),
            },
            Err(reason) => {
                let backup = self.backup_corrupt();
                let kept = match &backup {
                    Ok(p) => format!("原檔已保留為 {}", p.display()),
                    // Renaming within the same directory fails mainly when the
                    // directory is not writable; saving then fails too, so the
                    // original file cannot be overwritten silently.
                    Err(e) => format!("且無法備份原檔（{e}）"),
                };
                Loaded {
                    settings: Settings::default(),
                    warning: Some(format!("設定檔格式損壞（{reason}），已改用預設值；{kept}")),
                }
            }
        }
    }

    fn backup_corrupt(&self) -> std::io::Result<PathBuf> {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut target = self
            .path
            .with_file_name(format!("{FILE_NAME}.corrupt-{secs}"));
        let mut n = 1;
        while target.exists() {
            target = self
                .path
                .with_file_name(format!("{FILE_NAME}.corrupt-{secs}-{n}"));
            n += 1;
        }
        fs::rename(&self.path, &target)?;
        Ok(target)
    }

    /// Atomic replace of `config.json`. On error nothing is changed on disk
    /// (the temporary file is removed).
    pub fn save(&self, settings: &Settings) -> std::io::Result<()> {
        let dir = self
            .path
            .parent()
            .ok_or_else(|| std::io::Error::other("設定檔路徑沒有上層目錄"))?;
        fs::create_dir_all(dir)?;
        let tmp = dir.join(format!(
            ".{FILE_NAME}.tmp-{}-{}",
            std::process::id(),
            TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| {
            let mut json = serde_json::to_vec_pretty(settings).map_err(std::io::Error::other)?;
            json.push(b'\n');
            let mut f = fs::File::create(&tmp)?;
            f.write_all(&json)?;
            f.sync_all()?;
            fs::rename(&tmp, &self.path)?;
            // Persist the rename itself; failure here does not undo the write.
            if let Ok(d) = fs::File::open(dir) {
                let _ = d.sync_all();
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::testutil::temp_root;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn first_start_without_file() {
        let dir = temp_root("cfg-none");
        let l = SettingsStore::new(&dir.join("io.github.local.resourcewatch")).load();
        assert_eq!(
            l,
            Loaded {
                settings: Settings::default(),
                warning: None
            }
        );
    }

    #[test]
    fn save_then_reload_and_format() {
        let dir = temp_root("cfg-roundtrip").join("nested/app");
        let store = SettingsStore::new(&dir);
        let s = Settings {
            update_interval: 1250,
            history_points: 60,
            gpu_index: 1,
            net_interface: "enp3s0".into(),
            sidebar_glass: false,
        };
        store.save(&s).unwrap();
        assert_eq!(
            store.load(),
            Loaded {
                settings: s,
                warning: None
            }
        );
        let v: Value = serde_json::from_str(&fs::read_to_string(store.path()).unwrap()).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"updateInterval": 1250, "historyPoints": 60, "gpuIndex": 1, "netInterface": "enp3s0", "sidebarGlass": false})
        );
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name())
            .collect();
        assert_eq!(leftovers.len(), 1, "no temporary file left: {leftovers:?}");
    }

    #[test]
    fn invalid_fields_fall_back_individually() {
        let dir = temp_root("cfg-invalid");
        fs::write(
            dir.join(FILE_NAME),
            r#"{"updateInterval": 125, "historyPoints": 60, "gpuIndex": 99, "networkInterface": "wlp2s0", "sidebarGlass": "yes", "future": true}"#,
        )
        .unwrap();
        let l = SettingsStore::new(&dir).load();
        assert_eq!(l.settings.update_interval, 500, "invalid → default");
        assert_eq!(l.settings.history_points, 60, "valid kept");
        assert_eq!(l.settings.gpu_index, 0);
        assert_eq!(l.settings.net_interface, "wlp2s0", "alias accepted");
        assert!(l.settings.sidebar_glass, "wrong type → default");
        let w = l.warning.unwrap();
        assert!(
            w.contains("updateInterval") && w.contains("gpuIndex") && w.contains("sidebarGlass"),
            "{w}"
        );

        fs::write(
            dir.join(FILE_NAME),
            r#"{"updateInterval": "fast", "gpuIndex": -1}"#,
        )
        .unwrap();
        let l = SettingsStore::new(&dir).load();
        assert_eq!(l.settings, Settings::default());
        assert!(l.warning.unwrap().contains("型別錯誤"));
    }

    #[test]
    fn corrupt_json_is_preserved_not_overwritten() {
        let dir = temp_root("cfg-corrupt");
        let store = SettingsStore::new(&dir);
        fs::write(store.path(), "{\"updateInterval\": 5").unwrap();
        let l = store.load();
        assert_eq!(l.settings, Settings::default());
        assert!(l.warning.as_deref().unwrap().contains("格式損壞"));
        let backups: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("config.json.corrupt-")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(
            fs::read_to_string(backups[0].path()).unwrap(),
            "{\"updateInterval\": 5",
            "content kept"
        );
        assert!(!store.path().exists());
        // A second corrupt file in the same second gets a distinct backup name.
        fs::write(store.path(), "[1,2]").unwrap();
        assert!(store.load().warning.unwrap().contains("不是 JSON 物件"));
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
    }

    #[test]
    fn unwritable_directory_reports_error_and_keeps_old_file() {
        // Root ignores permission bits; the check is meaningless there.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let dir = temp_root("cfg-ro");
        let store = SettingsStore::new(&dir);
        store.save(&Settings::default()).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
        let s = Settings {
            update_interval: 2000,
            ..Settings::default()
        };
        let err = store.save(&s);
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(err.is_err());
        assert_eq!(
            store.load().settings,
            Settings::default(),
            "old file intact"
        );
    }

    #[test]
    fn concurrent_saves_never_leave_partial_file() {
        let dir = temp_root("cfg-race");
        let store = SettingsStore::new(&dir);
        let handles: Vec<_> = (0..8u32)
            .map(|i| {
                let st = store.clone();
                std::thread::spawn(move || {
                    for j in 0..25u32 {
                        let s = Settings {
                            update_interval: 100 + 50 * ((i * 25 + j) % 199),
                            ..Settings::default()
                        };
                        st.save(&s).unwrap();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let l = store.load();
        assert_eq!(l.warning, None, "file is always complete, valid JSON");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "no temp files left");
    }
}
