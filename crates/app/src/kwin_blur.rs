//! Parameters for the ResourceWatch KWin effect (`kwin-effect/`), which blurs
//! behind this app's windows only. The effect watches the file written here.

use std::path::PathBuf;
use std::process::Command;

/// `(ini key, default, min, max)`, in the order the settings page shows them.
pub const FIELDS: [(&str, i32, i32, i32); 5] = [
    ("strength", 15, 1, 15),
    ("noise", 0, 0, 100),
    ("saturation", 100, 0, 300),
    ("contrast", 100, 0, 300),
    ("brightness", 100, 0, 300),
];

pub type Params = [i32; FIELDS.len()];

/// `~/.config/resourcewatch/kwin-blur.ini`; must match `RwBlurEffect::loadConfig`.
fn path() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|d| d.join("resourcewatch/kwin-blur.ini"))
}

/// True when the running KWin has the effect loaded (asked over D-Bus).
pub fn effect_loaded() -> bool {
    Command::new("busctl")
        .args([
            "--user",
            "call",
            "org.kde.KWin",
            "/Effects",
            "org.kde.kwin.Effects",
        ])
        .args(["isEffectLoaded", "s", "resourcewatch_blur"])
        .output()
        .is_ok_and(|o| o.stdout.starts_with(b"b true"))
}

fn parse(text: &str) -> Params {
    FIELDS.map(|(key, default, min, max)| {
        text.lines()
            .filter_map(|l| l.split_once('='))
            .find(|(k, _)| k.trim() == key)
            .and_then(|(_, v)| v.trim().parse::<i32>().ok())
            .map_or(default, |v| v.clamp(min, max))
    })
}

fn format(enabled: bool, params: &Params, radius: i32) -> String {
    let mut out = format!("[General]\nenabled={enabled}\nradius={radius}\n");
    for ((key, _, min, max), v) in FIELDS.iter().zip(params) {
        out += &format!("{key}={}\n", v.clamp(min, max));
    }
    out
}

/// Stored parameters, defaults for anything missing or unreadable.
pub fn load() -> Params {
    parse(
        &path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default(),
    )
}

/// Replaces the file by rename: the effect watches the directory, and never sees half a file.
pub fn save(enabled: bool, params: &Params, radius: i32) -> std::io::Result<()> {
    let path = path().ok_or(std::io::ErrorKind::NotFound)?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("ini.tmp");
    std::fs::write(&tmp, format(enabled, params, radius))?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_clamps() {
        assert_eq!(
            parse(&format(true, &[7, 3, 120, 90, 110], 16)),
            [7, 3, 120, 90, 110]
        );
        assert_eq!(
            parse("strength=99\nnoise=-1\nsaturation=abc\n"),
            [15, 0, 100, 100, 100]
        );
        assert!(format(false, &[0; 5], 16).contains("enabled=false\nradius=16\nstrength=1\n"));
    }
}
