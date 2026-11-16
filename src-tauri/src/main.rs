mod du;
mod gpu;
mod sys;

use std::sync::Mutex;
use tauri::State;

type Mon<'a> = State<'a, Mutex<sys::Monitor>>;
type DirCache<'a> = State<'a, Mutex<du::Cache>>;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// The frontend polls these; counters for rates live in `Monitor` between calls.
#[tauri::command]
fn snapshot(m: Mon) -> sys::Snapshot {
    lock(&m).sample()
}

#[tauri::command]
fn processes(m: Mon) -> Vec<sys::Proc> {
    lock(&m).processes()
}

#[tauri::command]
fn system_info() -> sys::SystemInfo {
    sys::system_info()
}

/// Walks the whole tree on a blocking thread so the window stays responsive.
#[tauri::command]
async fn scan_dir(path: String, cache: DirCache<'_>) -> Result<du::Scan, String> {
    let (c, scan) = tauri::async_runtime::spawn_blocking(move || du::scan(&path))
        .await
        .map_err(|e| e.to_string())??;
    *lock(&cache) = c;
    Ok(scan)
}

#[tauri::command]
fn dir_children(path: String, cache: DirCache) -> Vec<du::Entry> {
    lock(&cache).children(&path)
}

/// Developer mode: WebKit only offers "Inspect Element" and the inspector while developer extras are on.
#[tauri::command]
fn set_dev_mode(window: tauri::WebviewWindow, on: bool) -> Result<(), String> {
    window
        .with_webview(move |wv| {
            use webkit2gtk::{SettingsExt, WebViewExt};
            if let Some(s) = WebViewExt::settings(&wv.inner()) {
                s.set_enable_developer_extras(on);
            }
        })
        .map_err(|e| e.to_string())
}

/// Opens the inspector in its own window instead of docked inside the app.
#[tauri::command]
fn open_devtools(window: tauri::WebviewWindow) -> Result<(), String> {
    window
        .with_webview(|wv| {
            use webkit2gtk::{WebInspectorExt, WebViewExt};
            if let Some(i) = wv.inner().inspector() {
                i.show();
                i.detach();
            }
        })
        .map_err(|e| e.to_string())
}

fn main() {
    // NVIDIA proprietary driver: WebKitGTK's DMA-BUF renderer leaves the window blank on X11
    // and lags behind resizes on Wayland. Set before any thread starts.
    if std::path::Path::new("/proc/driver/nvidia").exists()
        && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none()
    {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(sys::Monitor::new()))
        .manage(Mutex::new(du::Cache::default()))
        .invoke_handler(tauri::generate_handler![
            snapshot,
            processes,
            system_info,
            scan_dir,
            dir_children,
            open_devtools,
            set_dev_mode
        ])
        .run(tauri::generate_context!())
        .expect("failed to start ResourceWatch");
}
