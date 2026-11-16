mod du;
mod gpu;
mod sys;

use std::sync::Mutex;
use tauri::{Manager, State};

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

#[tauri::command]
fn open_devtools(window: tauri::WebviewWindow) -> Result<(), String> {
    window
        .with_webview(|wv| {
            use webkit2gtk::{WebInspectorExt, WebViewExt};
            if let Some(i) = wv.inner().inspector() {
                i.show();
            }
        })
        .map_err(|e| e.to_string())
}

/// Hosts the inspector's view in a separate top-level window; closing either side closes the other.
fn inspector_window(i: &webkit2gtk::WebInspector) {
    use gtk::prelude::*;
    use webkit2gtk::WebInspectorExt;
    let Some(view) = i.web_view() else { return };
    if let Some(win) = view
        .toplevel()
        .and_then(|t| t.downcast::<gtk::Window>().ok())
    {
        if view.parent().is_some() {
            win.present(); // already hosted
            return;
        }
    }
    let win = gtk::Window::new(gtk::WindowType::Toplevel);
    win.set_title("ResourceWatch 檢查器");
    win.set_default_size(1100, 720);
    win.add(&view);
    let insp = i.clone();
    win.connect_delete_event(move |w, _| {
        insp.close();
        w.hide();
        gtk::glib::Propagation::Stop
    });
    let w = win.clone();
    i.connect_closed(move |_| unsafe { w.destroy() });
    win.show_all();
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
        .setup(|app| {
            // WebKit docks the inspector inside the app; take over "attach" and host it in our own window.
            if let Some(w) = app.get_webview_window("main") {
                w.with_webview(|wv| {
                    use webkit2gtk::{WebInspectorExt, WebViewExt};
                    if let Some(i) = wv.inner().inspector() {
                        i.connect_attach(|i| {
                            inspector_window(i);
                            true
                        });
                    }
                })?;
            }
            Ok(())
        })
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
