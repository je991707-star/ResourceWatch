mod gpu;
mod sys;

use std::sync::Mutex;
use tauri::State;

type Mon<'a> = State<'a, Mutex<sys::Monitor>>;

// The frontend polls these; counters for rates live in `Monitor` between calls.
#[tauri::command]
fn snapshot(m: Mon) -> sys::Snapshot {
    m.lock().unwrap_or_else(|e| e.into_inner()).sample()
}

#[tauri::command]
fn processes(m: Mon) -> Vec<sys::Proc> {
    m.lock().unwrap_or_else(|e| e.into_inner()).processes()
}

#[tauri::command]
fn system_info() -> sys::SystemInfo {
    sys::system_info()
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
        .manage(Mutex::new(sys::Monitor::new()))
        .invoke_handler(tauri::generate_handler![snapshot, processes, system_info])
        .run(tauri::generate_context!())
        .expect("failed to start ResourceWatch");
}
