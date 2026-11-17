//! Process name / desktop id → application name and icon as a data: URL (the page's CSP only allows data: images from
//! outside the app), via installed .desktop files and the hicolor/Breeze/Adwaita icon themes.

use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

struct Index {
    /// lowercase match key (desktop id, its last dotted part, StartupWMClass, Exec program) → (Icon value, Name)
    apps: HashMap<String, (String, Option<String>)>,
    /// icon file stem → candidate files (svg/png)
    files: HashMap<String, Vec<PathBuf>>,
}

#[derive(Serialize)]
pub struct App {
    /// data: URL; `None` when no icon file matches.
    icon: Option<String>,
    /// .desktop `Name` (zh_TW first).
    name: Option<String>,
}

/// Desktop entry for a process name (kernel comm, ≤ 15 chars) or desktop id; None when none matches.
pub fn lookup(name: &str) -> Option<App> {
    let (icon, name) = index().apps.get(&name.to_lowercase())?;
    Some(App {
        icon: icon_url(icon),
        name: name.clone(),
    })
}

fn icon_url(icon: &str) -> Option<String> {
    let idx = index();
    let path = if icon.starts_with('/') {
        PathBuf::from(icon)
    } else {
        // symbolic icons are dark monochrome glyphs; prefer the full-color one of the same name
        let full = icon.strip_suffix("-symbolic").unwrap_or(icon);
        best(idx.files.get(full).or_else(|| idx.files.get(icon))?)?
    };
    let data = fs::read(&path).ok().filter(|d| d.len() < 512 << 10)?;
    let mime = match path.extension()?.to_str()? {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        _ => return None,
    };
    Some(format!("data:{mime};base64,{}", crate::sys::base64(&data)))
}

/// Built once on first lookup: walks the XDG data dirs (user first, so user entries win).
fn index() -> &'static Index {
    static I: OnceLock<Index> = OnceLock::new();
    I.get_or_init(|| {
        let mut idx = Index {
            apps: HashMap::new(),
            files: HashMap::new(),
        };
        for dir in data_dirs() {
            for e in fs::read_dir(dir.join("applications"))
                .into_iter()
                .flatten()
                .flatten()
            {
                let p = e.path();
                let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                if p.extension().is_some_and(|x| x == "desktop") {
                    if let Some((keys, icon, name)) =
                        desktop_keys(stem, &fs::read_to_string(&p).unwrap_or_default())
                    {
                        for k in keys {
                            idx.apps.entry(k).or_insert_with(|| (icon.clone(), name.clone()));
                        }
                    }
                }
            }
            let mut found = Vec::new();
            for theme in ["hicolor", "breeze", "Adwaita"] {
                walk(&dir.join("icons").join(theme), 3, &mut found);
            }
            walk(&dir.join("pixmaps"), 0, &mut found);
            for p in found {
                let ok = matches!(p.extension().and_then(|x| x.to_str()), Some("svg" | "png"));
                if let (true, Some(stem)) = (ok, p.file_stem().and_then(|s| s.to_str())) {
                    idx.files.entry(stem.to_string()).or_default().push(p);
                }
            }
        }
        idx
    })
}

fn data_dirs() -> Vec<PathBuf> {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let sys =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    let mut seen = HashSet::new();
    [
        home.join(".local/share"),
        home.join(".local/share/flatpak/exports/share"),
    ]
    .into_iter()
    .chain(sys.split(':').filter(|s| !s.is_empty()).map(PathBuf::from))
    .chain([PathBuf::from("/var/lib/flatpak/exports/share")])
    .filter(|d| seen.insert(d.clone()))
    .collect()
}

fn walk(dir: &Path, depth: u8, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth > 0 {
                walk(&p, depth - 1, out);
            }
        } else {
            out.push(p);
        }
    }
}

/// SVG first; otherwise the smallest PNG of at least 32 px (sharp yet small), else the largest.
fn best(paths: &[PathBuf]) -> Option<PathBuf> {
    let size = |p: &Path| {
        p.components()
            .filter_map(|c| {
                c.as_os_str()
                    .to_str()?
                    .split(['x', '@'])
                    .next()?
                    .parse::<u32>()
                    .ok()
            })
            .next()
            .unwrap_or(0)
    };
    paths
        .iter()
        .min_by_key(|p| {
            let s = size(p);
            let svg = p.extension().is_some_and(|x| x == "svg");
            (!svg, s < 32, if s < 32 { u32::MAX - s } else { s })
        })
        .cloned()
}

/// `[Desktop Entry]` of one .desktop file → (lowercase match keys, Icon, Name). Keys are also stored
/// cut to 15 chars, because a process name is the kernel's truncated comm.
fn desktop_keys(stem: &str, text: &str) -> Option<(Vec<String>, String, Option<String>)> {
    let (mut entry, mut exec, mut icon, mut wm) = (false, None, None, None);
    let (mut name, mut zh, mut zh_tw) = (None, None, None);
    for l in text.lines().map(str::trim) {
        if l.starts_with('[') {
            entry = l == "[Desktop Entry]";
        } else if let (true, Some((k, v))) = (entry, l.split_once('=')) {
            let slot = match k.trim() {
                "Exec" => &mut exec,
                "Icon" => &mut icon,
                "StartupWMClass" => &mut wm,
                "Name" => &mut name,
                "Name[zh]" => &mut zh,
                "Name[zh_TW]" => &mut zh_tw,
                _ => continue,
            };
            slot.get_or_insert(v.trim());
        }
    }
    let icon = icon.filter(|i| !i.is_empty())?.to_string();
    let name = zh_tw.or(zh).or(name).filter(|n| !n.is_empty()).map(str::to_string);
    let mut keys = vec![stem.to_string()];
    keys.extend(stem.rsplit('.').next().map(str::to_string));
    keys.extend(wm.map(str::to_string));
    keys.extend(exec.and_then(exec_program));
    let mut out = Vec::new();
    for k in keys.into_iter().map(|k| k.to_lowercase()) {
        let cut: String = k.chars().take(15).collect();
        if cut != k {
            out.push(cut);
        }
        out.push(k);
    }
    Some((out, icon, name))
}

/// Program an Exec line runs, skipping `env` with its VAR=value/options. Launchers that start
/// something under another name give no key, or e.g. every systemctl would get Sunshine's icon.
fn exec_program(exec: &str) -> Option<String> {
    const LAUNCHERS: [&str; 14] = [
        "systemctl",
        "flatpak",
        "sh",
        "bash",
        "gio",
        "xdg-open",
        "python",
        "python3",
        "pkexec",
        "sudo",
        "kioclient",
        "kioclient5",
        "kde-open",
        "gtk-launch",
    ];
    let base = |w: &str| {
        w.trim_matches('"')
            .rsplit('/')
            .next()
            .unwrap_or(w)
            .to_string()
    };
    let mut words = exec.split_whitespace();
    let mut prog = base(words.next()?);
    if prog == "env" {
        prog = base(words.find(|w| !w.contains('=') && !w.starts_with('-'))?);
    }
    (!LAUNCHERS.contains(&prog.as_str())).then_some(prog)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entries() {
        let code = "[Desktop Entry]\nName=VS Code\nName[zh_TW]=程式碼\nExec=/usr/share/code/code --unity-launch %F\nIcon=vscode\nStartupWMClass=Code\n[Desktop Action new]\nName=New\nExec=other\n";
        let (keys, icon, name) = desktop_keys("code", code).unwrap();
        assert_eq!((icon.as_str(), name.as_deref()), ("vscode", Some("程式碼")));
        assert!(keys.contains(&"code".to_string()) && !keys.contains(&"other".to_string()));
        let sun = "[Desktop Entry]\nExec=/usr/bin/env systemctl start --u app-sunshine\nIcon=dev.lizardbyte.app.Sunshine\n";
        let (keys, _, name) = desktop_keys("dev.lizardbyte.app.Sunshine", sun).unwrap();
        assert_eq!(name, None);
        assert!(keys.contains(&"sunshine".to_string()) && !keys.contains(&"systemctl".to_string()));
        let long = "[Desktop Entry]\nExec=steamwebhelper-launcher\nIcon=steam\n";
        let (keys, _, _) = desktop_keys("x", long).unwrap();
        assert!(keys.contains(&"steamwebhelper-".to_string())); // 15-char comm
        assert_eq!(desktop_keys("x", "[Desktop Entry]\nExec=a\n"), None);
        assert_eq!(
            exec_program("env FOO=1 firefox %u").as_deref(),
            Some("firefox")
        );
        assert_eq!(exec_program("flatpak run org.x.Y"), None);
    }
}
