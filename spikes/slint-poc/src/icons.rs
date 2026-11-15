//! Window-control icons from the user's freedesktop icon theme (KDE: `kdeglobals`
//! `[Icons] Theme=`), so the title bar follows the system style.
//! shortcut: fixed size/context subdirectories instead of parsing index.theme
//! `Directories=`; read once at startup (theme change needs a restart); upgrade if
//! a theme lays out action icons elsewhere.

use std::path::{Path, PathBuf};

const SUBDIRS: &[&str] = &[
    "16x16/actions",
    "actions/16",
    "22x22/actions",
    "actions/22",
    "24x24/actions",
    "actions/24",
    "symbolic/actions",
    "scalable/actions",
];

/// Icon base directories in freedesktop lookup order.
fn roots() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|h| h.join(".local/share")));
    let data_dirs =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    let mut roots: Vec<PathBuf> = home.iter().map(|h| h.join(".icons")).collect();
    roots.extend(data_home.map(|d| d.join("icons")));
    roots.extend(
        data_dirs
            .split(':')
            .filter(|d| !d.is_empty())
            .map(|d| Path::new(d).join("icons")),
    );
    roots
}

/// `[Icons] Theme=` from kdeglobals, default `breeze` (KDE's own default).
fn current_theme() -> String {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    config
        .and_then(|c| std::fs::read_to_string(c.join("kdeglobals")).ok())
        .and_then(|s| ini_value(&s, "Icons", "Theme"))
        .unwrap_or_else(|| "breeze".into())
}

fn ini_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_section = line == format!("[{section}]");
        } else if in_section {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim() == key {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

/// Theme plus its `Inherits=` chain (breadth-first), ending with hicolor.
fn theme_chain(roots: &[PathBuf], theme: &str) -> Vec<String> {
    let mut chain = vec![theme.to_string()];
    let mut i = 0;
    while i < chain.len() {
        let inherits = roots
            .iter()
            .find_map(|r| std::fs::read_to_string(r.join(&chain[i]).join("index.theme")).ok())
            .and_then(|s| ini_value(&s, "Icon Theme", "Inherits"));
        for parent in inherits.iter().flat_map(|s| s.split(',')).map(str::trim) {
            if !parent.is_empty() && !chain.iter().any(|t| t == parent) {
                chain.push(parent.to_string());
            }
        }
        i += 1;
    }
    if !chain.iter().any(|t| t == "hicolor") {
        chain.push("hicolor".into());
    }
    chain
}

fn find_icon(roots: &[PathBuf], chain: &[String], name: &str) -> Option<PathBuf> {
    let symbolic = format!("{name}-symbolic");
    chain.iter().find_map(|theme| {
        roots.iter().find_map(|root| {
            SUBDIRS.iter().find_map(|sub| {
                [name, symbolic.as_str()].iter().find_map(|n| {
                    let p = root.join(theme).join(sub).join(format!("{n}.svg"));
                    p.is_file().then_some(p)
                })
            })
        })
    })
}

/// Loads `name` from the current theme; an empty image (width 0) when missing.
pub fn load(names: &[&str]) -> Vec<slint::Image> {
    let roots = roots();
    let chain = theme_chain(&roots, &current_theme());
    names
        .iter()
        .map(|n| {
            find_icon(&roots, &chain, n)
                .and_then(|p| slint::Image::load_from_path(&p).ok())
                .unwrap_or_default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_inherits_and_prefers_small_sizes() {
        let root = std::env::temp_dir().join(format!("rw-icons-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let write = |p: &str, s: &str| {
            let p = root.join(p);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, s).unwrap();
        };
        write(
            "Child/index.theme",
            "[Icon Theme]\nName=Child\nInherits=Parent,hicolor\n",
        );
        write("Child/22x22/actions/window-close.svg", "");
        write("Child/16x16/actions/window-close.svg", "");
        write("Parent/actions/16/window-minimize-symbolic.svg", "");
        let roots = vec![root.clone()];
        let chain = theme_chain(&roots, "Child");
        assert_eq!(chain, ["Child", "Parent", "hicolor"]);
        assert!(find_icon(&roots, &chain, "window-close")
            .unwrap()
            .ends_with("16x16/actions/window-close.svg"));
        assert!(find_icon(&roots, &chain, "window-minimize")
            .unwrap()
            .starts_with(root.join("Parent")));
        assert!(find_icon(&roots, &chain, "window-restore").is_none());
        assert_eq!(
            ini_value(
                "[KDE]\nTheme=x\n[Icons]\nTheme = Papirus\n",
                "Icons",
                "Theme"
            )
            .as_deref(),
            Some("Papirus")
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
