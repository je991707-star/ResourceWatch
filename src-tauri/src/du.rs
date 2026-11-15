//! Folder size analyzer. `scan` walks a tree once (allocated size like `du`, no
//! symlinks, never crossing into another filesystem) and keeps every directory's
//! total; `children` then lists one level from that cache without touching the disk tree again.

use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Cache(HashMap<PathBuf, u64>);

#[derive(Serialize)]
pub struct Scan {
    path: String,
    size: u64,
    items: u64,
}

#[derive(Serialize)]
pub struct Entry {
    name: String,
    path: String,
    size: u64,
    dir: bool,
}

/// Validates `path` (absolute, existing directory) and scans it. Slow on big trees: run off the UI thread.
pub fn scan(path: &str) -> Result<(Cache, Scan), String> {
    let root = Path::new(path);
    if !root.is_absolute() {
        return Err("請輸入絕對路徑".into());
    }
    let meta = fs::metadata(root).map_err(|e| format!("{path}：{e}"))?;
    if !meta.is_dir() {
        return Err(format!("{path} 不是資料夾"));
    }
    let mut cache = Cache::default();
    let mut items = 0;
    let size = walk(root, meta.dev(), &mut cache.0, &mut items);
    Ok((
        cache,
        Scan {
            path: path.to_string(),
            size,
            items,
        },
    ))
}

fn walk(dir: &Path, dev: u64, sizes: &mut HashMap<PathBuf, u64>, items: &mut u64) -> u64 {
    let mut total = 0;
    // Unreadable folders count as empty instead of failing the whole scan.
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let Ok(m) = e.metadata() else { continue }; // does not follow symlinks
        *items += 1;
        total += if m.is_dir() {
            if m.dev() == dev {
                walk(&e.path(), dev, sizes, items)
            } else {
                0
            }
        } else {
            m.blocks() * 512
        };
    }
    sizes.insert(dir.to_path_buf(), total);
    total
}

impl Cache {
    /// Entries of `dir`, largest first. Folders not in the cache (other filesystem) are 0.
    pub fn children(&self, dir: &str) -> Vec<Entry> {
        let mut out: Vec<Entry> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                let path = e.path();
                Some(Entry {
                    name: e.file_name().to_string_lossy().into_owned(),
                    size: if m.is_dir() {
                        self.0.get(&path).copied().unwrap_or(0)
                    } else {
                        m.blocks() * 512
                    },
                    dir: m.is_dir(),
                    path: path.to_string_lossy().into_owned(),
                })
            })
            .collect();
        out.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_and_children() {
        let root = std::env::temp_dir().join(format!("rw-du-{}", std::process::id()));
        fs::create_dir_all(root.join("big/inner")).unwrap();
        fs::write(root.join("big/inner/a"), vec![1u8; 64 * 1024]).unwrap();
        fs::write(root.join("small"), b"x").unwrap();
        std::os::unix::fs::symlink("/usr", root.join("link")).unwrap();

        let (cache, s) = scan(root.to_str().unwrap()).unwrap();
        let kids = cache.children(root.to_str().unwrap());
        fs::remove_dir_all(&root).unwrap();

        assert_eq!(s.items, 5); // big, inner, a, small, link
        assert_eq!(kids[0].name, "big");
        assert!(kids[0].dir && kids[0].size >= 64 * 1024);
        assert!(kids.iter().any(|k| k.name == "link" && !k.dir)); // symlink not followed
        assert!(scan("relative").is_err());
        assert!(scan("/proc/self/stat").is_err());
    }
}
