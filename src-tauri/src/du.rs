//! Folder size analyzer. `scan` walks a tree once (allocated size like `du`, no
//! symlinks, never crossing into another filesystem) and keeps every directory's
//! total; `children` then lists one level from that cache without touching the disk tree again.

use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::{DirEntryExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::{Condvar, Mutex};

/// Items walked so far by the running scan.
static ITEMS: AtomicU64 = AtomicU64::new(0);
/// Set by `cancel`; workers stop taking folders and `scan` returns an error.
static CANCEL: AtomicBool = AtomicBool::new(false);
/// The root's entries with the bytes found under each so far, for drawing the first level while scanning.
static TOP: Mutex<Vec<Entry>> = Mutex::new(Vec::new());

#[derive(Default)]
pub struct Cache(HashMap<PathBuf, u64>);

#[derive(Serialize)]
pub struct Scan {
    path: String,
    size: u64,
    items: u64,
}

#[derive(Serialize, Clone)]
pub struct Entry {
    name: String,
    path: String,
    size: u64,
    dir: bool,
}

#[derive(Serialize)]
pub struct Progress {
    items: u64,
    /// Root entries, largest first; folder sizes still growing.
    top: Vec<Entry>,
}

pub fn progress() -> Progress {
    let mut top = lock(&TOP).clone();
    sort(&mut top);
    Progress {
        items: ITEMS.load(Relaxed),
        top,
    }
}

pub fn cancel() {
    CANCEL.store(true, Relaxed);
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn sort(v: &mut [Entry]) {
    v.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
}

/// Stats one folder's entries in inode order (on a hard disk that reads the inode table in sequence instead of
/// seeking, as coreutils du does). Returns (file bytes, subfolders on the same filesystem, all entries).
fn read_level(dir: &Path, dev: u64) -> (u64, Vec<PathBuf>, Vec<(fs::DirEntry, fs::Metadata)>) {
    // Unreadable folders count as empty instead of failing the whole scan.
    let mut entries: Vec<_> = fs::read_dir(dir).into_iter().flatten().flatten().collect();
    entries.sort_unstable_by_key(|e| e.ino());
    let (mut bytes, mut subdirs, mut all) = (0, Vec::new(), Vec::new());
    for e in entries {
        let Ok(m) = e.metadata() else { continue }; // does not follow symlinks
        ITEMS.fetch_add(1, Relaxed);
        if !m.is_dir() {
            bytes += m.blocks() * 512;
        } else if m.dev() == dev {
            subdirs.push(e.path()); // folders on other filesystems count as 0
        }
        all.push((e, m));
    }
    (bytes, subdirs, all)
}

/// Validates `path` (absolute, existing directory) and scans it with `threads` workers sharing one stack of
/// folders. Slow on big trees: run off the UI thread.
pub fn scan(path: &str, threads: usize) -> Result<(Cache, Scan), String> {
    let root = Path::new(path);
    if !root.is_absolute() {
        return Err("請輸入絕對路徑".into());
    }
    let meta = fs::metadata(root).map_err(|e| format!("{path}：{e}"))?;
    if !meta.is_dir() {
        return Err(format!("{path} 不是資料夾"));
    }
    let dev = meta.dev();
    ITEMS.store(0, Relaxed);
    CANCEL.store(false, Relaxed);
    lock(&TOP).clear(); // a poll before the root is read must not see the previous scan

    let (root_bytes, subdirs, all) = read_level(root, dev);
    let top: Vec<Entry> = all
        .iter()
        .map(|(e, m)| Entry {
            name: e.file_name().to_string_lossy().into_owned(),
            path: e.path().to_string_lossy().into_owned(),
            size: if m.is_dir() { 0 } else { m.blocks() * 512 },
            dir: m.is_dir(),
        })
        .collect();
    let top_index: HashMap<PathBuf, usize> = all
        .iter()
        .enumerate()
        .map(|(i, (e, _))| (e.path(), i))
        .collect();
    *lock(&TOP) = top;

    // (folders waiting, workers busy): the walk is over when both are empty.
    let queue = Mutex::new((subdirs, 0usize));
    let wake = Condvar::new();
    let worker = || {
        let mut own = Vec::new(); // (folder, bytes of its own files)
        loop {
            let dir = {
                let mut q = lock(&queue);
                loop {
                    if let Some(d) = q.0.pop() {
                        q.1 += 1;
                        break d;
                    }
                    if q.1 == 0 {
                        return own;
                    }
                    q = wake.wait(q).unwrap_or_else(|e| e.into_inner());
                }
            };
            let subdirs = if CANCEL.load(Relaxed) {
                Vec::new() // drain the stack without reading
            } else {
                let (bytes, subdirs, _) = read_level(&dir, dev);
                // stack paths are always under root, so the first component names the root entry they belong to
                if let Some(first) = dir
                    .strip_prefix(root)
                    .ok()
                    .and_then(|r| r.components().next())
                {
                    if let Some(&i) = top_index.get(&root.join(first)) {
                        lock(&TOP)[i].size += bytes;
                    }
                }
                own.push((dir, bytes));
                subdirs
            };
            let mut q = lock(&queue);
            q.0.extend(subdirs);
            q.1 -= 1;
            drop(q);
            wake.notify_all();
        }
    };
    let mut own: Vec<(PathBuf, u64)> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads.max(1)).map(|_| s.spawn(worker)).collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    });
    if CANCEL.load(Relaxed) {
        return Err("已取消掃描".into());
    }

    // Totals bottom-up: deepest folders first, each adds its total into its parent.
    own.push((root.to_path_buf(), root_bytes));
    own.sort_unstable_by_key(|(p, _)| std::cmp::Reverse(p.components().count()));
    let mut sizes: HashMap<PathBuf, u64> = own.iter().cloned().collect();
    for (p, _) in &own {
        if p.as_path() == root {
            continue;
        }
        let t = sizes[p];
        if let Some(parent) = p.parent().and_then(|par| sizes.get_mut(par)) {
            *parent += t;
        }
    }
    let size = sizes[root];
    Ok((
        Cache(sizes),
        Scan {
            path: path.to_string(),
            size,
            items: ITEMS.load(Relaxed),
        },
    ))
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
        sort(&mut out);
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

        let (cache, s) = scan(root.to_str().unwrap(), 4).unwrap();
        let kids = cache.children(root.to_str().unwrap());
        let (cache1, s1) = scan(root.to_str().unwrap(), 1).unwrap();
        let big = root.join("big");
        let top = progress().top;
        fs::remove_dir_all(&root).unwrap();

        assert_eq!((s1.size, s1.items), (s.size, s.items)); // threads don't change the result
        assert_eq!(cache1.0[&big], cache.0[&big]);
        assert_eq!(top[0].size, kids[0].size); // live first level ends up at the final totals

        assert_eq!(s.items, 5); // big, inner, a, small, link
        assert_eq!(kids[0].name, "big");
        assert!(kids[0].dir && kids[0].size >= 64 * 1024);
        assert!(kids.iter().any(|k| k.name == "link" && !k.dir)); // symlink not followed
        assert!(scan("relative", 1).is_err());
        assert!(scan("/proc/self/stat", 1).is_err());
    }
}
