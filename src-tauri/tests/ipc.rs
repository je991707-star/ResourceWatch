//! Rust → Tauri IPC integration tests on Tauri's mock runtime.
//! Exercises the real command routing, argument deserialization, error
//! serialization, settings persistence and the running scheduler, exactly as
//! the frontend uses them through `invoke()`. Every test uses its own
//! temporary config directory; the user's real settings are never touched.

use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{App, WebviewWindow, WebviewWindowBuilder};

type Mock = tauri::test::MockRuntime;

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rw-ipc-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Builds the app with `dir` as config directory and runs `setup` once
/// (loads settings, starts the scheduler), like the real `App::run`.
fn app_in(dir: &Path) -> (App<Mock>, WebviewWindow<Mock>) {
    let mut app = resourcewatch_lib::configure_with(mock_builder(), Some(dir.to_path_buf()))
        .build(mock_context(noop_assets()))
        .expect("build mock app");
    #[allow(deprecated)] // the only public way to run `setup` on the mock runtime
    app.run_iteration(|_, _| {});
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("build mock window");
    (app, window)
}

fn app(tag: &str) -> (App<Mock>, WebviewWindow<Mock>) {
    app_in(&temp_dir(tag))
}

fn read_config(dir: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("config.json")).unwrap()).unwrap()
}

fn invoke(window: &WebviewWindow<Mock>, cmd: &str, body: Value) -> Result<Value, Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|r| r.deserialize::<Value>().expect("json response"))
}

#[test]
fn snapshot_follows_contract_and_is_live_by_default() {
    let (_app, w) = app("contract");
    let snap = invoke(&w, "get_snapshot", json!({})).unwrap();
    // Without RESOURCEWATCH_MOCK=1 the live monitor is used.
    assert_eq!(snap["dataSource"], "live");
    for key in [
        "version",
        "hostname",
        "machineModel",
        "cpuName",
        "cpuCores",
        "cpuThreads",
        "cpuPerCore",
        "cpu",
        "cpuFreqText",
        "cpuTempText",
        "uptimeText",
        "uptimeSeconds",
        "gpuIndex",
        "gpuName",
        "gpuVendor",
        "gpuSource",
        "gpus",
        "gpu",
        "gpuTempText",
        "gpuClockText",
        "vram",
        "encoder",
        "decoder",
        "ram",
        "ramSubtitle",
        "ramSpeedText",
        "ramBreakdown",
        "memPressure",
        "swap",
        "disks",
        "diskRates",
        "diskUsage",
        "sensors",
        "battery",
        "netHasInterface",
        "netIpAvailable",
        "netIp",
        "netInterfaceName",
        "netMac",
        "netGateway",
        "netGatewayMac",
        "netInterfaces",
        "netUp",
        "netDown",
        "history",
    ] {
        assert!(snap.get(key).is_some(), "snapshot missing {key}");
    }
    for key in [
        "cpu",
        "gpu",
        "encode",
        "decode",
        "vram",
        "ram",
        "netUp",
        "netDown",
        "memPressure",
        "diskRead",
        "diskWrite",
    ] {
        assert!(snap["history"][key].is_array(), "history missing {key}");
    }
}

#[test]
fn settings_roundtrip_and_validation() {
    let (_app, w) = app("roundtrip");
    let s = invoke(&w, "get_settings", json!({})).unwrap();
    assert_eq!(
        s,
        json!({"updateInterval": 500, "historyPoints": 30, "gpuIndex": 0, "netInterface": "all", "sidebarGlass": true})
    );

    let s = invoke(
        &w,
        "update_settings",
        json!({"patch": {"updateInterval": 100}}),
    )
    .unwrap();
    assert_eq!(s["updateInterval"], 100);
    let s = invoke(
        &w,
        "update_settings",
        json!({"patch": {"updateInterval": 10000, "netInterface": " enp3s0 "}}),
    )
    .unwrap();
    assert_eq!(
        (s["updateInterval"].as_u64(), s["netInterface"].as_str()),
        (Some(10000), Some("enp3s0"))
    );

    let err = invoke(
        &w,
        "update_settings",
        json!({"patch": {"updateInterval": 125}}),
    )
    .unwrap_err();
    assert_eq!(err["code"], "invalidSetting");
    assert_eq!(err["field"], "updateInterval");
    let err = invoke(
        &w,
        "update_settings",
        json!({"patch": {"netInterface": "eth0; id"}}),
    )
    .unwrap_err();
    assert_eq!(err["field"], "netInterface");
    // Unknown keys are rejected at deserialization.
    assert!(invoke(
        &w,
        "update_settings",
        json!({"patch": {"cpuInterval": 500}})
    )
    .is_err());
    // Failed updates do not change stored settings.
    assert_eq!(
        invoke(&w, "get_settings", json!({})).unwrap()["updateInterval"],
        10000
    );

    let s = invoke(&w, "reset_settings", json!({})).unwrap();
    assert_eq!(s["updateInterval"], 500);
    assert_eq!(s["netInterface"], "all");
}

#[test]
fn refresh_devices_kinds() {
    let (_app, w) = app("refresh");
    for kind in ["ipv4", "disks", "hardware"] {
        let snap = invoke(&w, "refresh_devices", json!({"kind": kind})).unwrap();
        assert_eq!(snap["dataSource"], "live");
    }
    let snap = invoke(&w, "refresh_devices", json!({"kind": "disks"})).unwrap();
    // Right after a rescan there is no counter delta yet: rates are null, not 0.
    for (_, rate) in snap["diskRates"].as_object().unwrap() {
        assert!(rate["read"].is_null() && rate["write"].is_null());
    }
    assert!(invoke(&w, "refresh_devices", json!({"kind": "everything"})).is_err());
}

#[test]
fn processes_list_this_test_with_cpu_after_second_call() {
    let (_app, w) = app("processes");
    let me = std::process::id();
    let first = invoke(&w, "get_processes", json!({})).unwrap();
    let mine = |list: &serde_json::Value| {
        list.as_array()
            .unwrap()
            .iter()
            .find(|p| p["pid"] == me)
            .cloned()
            .unwrap()
    };
    let p = mine(&first);
    assert!(p["name"].is_string() && p["memory"].as_u64().unwrap() > 0);
    assert!(p["cpu"].is_null(), "no CPU share before a second scan");
    std::thread::sleep(std::time::Duration::from_millis(50));
    let second = invoke(&w, "get_processes", json!({})).unwrap();
    assert!(mine(&second)["cpu"].is_number());
}

#[test]
fn window_info_reports_no_glass_without_compositor_blur() {
    // The mock runtime has no Wayland surface, so blur is never requested and
    // the frontend must keep the sidebar opaque.
    let (_app, w) = app("window-info");
    assert_eq!(
        invoke(&w, "get_window_info", json!({})).unwrap(),
        json!({"glass": false})
    );
}

#[test]
fn settings_persist_across_restart_and_report_path() {
    let dir = temp_dir("persist");
    {
        let (_app, w) = app_in(&dir);
        let info = invoke(&w, "get_settings_info", json!({})).unwrap();
        assert_eq!(info["path"], dir.join("config.json").display().to_string());
        assert!(info["loadWarning"].is_null(), "first start is not an error");
        assert!(
            !dir.join("config.json").exists(),
            "nothing written before a change"
        );
        invoke(
            &w,
            "update_settings",
            json!({"patch": {"updateInterval": 1500, "historyPoints": 90, "netInterface": "lo"}}),
        )
        .unwrap();
        assert_eq!(
            read_config(&dir),
            json!({"updateInterval": 1500, "historyPoints": 90, "gpuIndex": 0, "netInterface": "lo", "sidebarGlass": true})
        );
    }
    // "Restart": a new app instance on the same directory loads the file.
    let (_app, w) = app_in(&dir);
    let s = invoke(&w, "get_settings", json!({})).unwrap();
    assert_eq!(
        s,
        json!({"updateInterval": 1500, "historyPoints": 90, "gpuIndex": 0, "netInterface": "lo", "sidebarGlass": true})
    );
    std::thread::sleep(Duration::from_millis(50));
    let stats = invoke(&w, "get_scheduler_stats", json!({})).unwrap();
    assert_eq!(
        stats["intervalMs"], 1500,
        "scheduler started with the stored interval"
    );
    // Reset is persisted too.
    invoke(&w, "reset_settings", json!({})).unwrap();
    assert_eq!(read_config(&dir)["updateInterval"], 500);
}

#[test]
fn invalid_input_is_not_saved_or_applied() {
    let dir = temp_dir("invalid");
    let (_app, w) = app_in(&dir);
    for patch in [
        json!({"updateInterval": 99}),
        json!({"updateInterval": 10_050}),
        json!({"updateInterval": 333}),
        json!({"gpuIndex": 16}),
    ] {
        assert!(invoke(&w, "update_settings", json!({"patch": patch})).is_err());
    }
    assert!(!dir.join("config.json").exists());
    assert_eq!(
        invoke(&w, "get_scheduler_stats", json!({})).unwrap()["intervalMs"],
        500
    );
}

#[test]
fn corrupt_config_is_reported_and_preserved() {
    let dir = temp_dir("corrupt");
    std::fs::write(dir.join("config.json"), "{ not json").unwrap();
    let (_app, w) = app_in(&dir);
    let info = invoke(&w, "get_settings_info", json!({})).unwrap();
    assert!(info["loadWarning"].as_str().unwrap().contains("格式損壞"));
    assert_eq!(
        invoke(&w, "get_settings", json!({})).unwrap()["updateInterval"],
        500
    );
    let backups = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("config.json.corrupt-")
        })
        .count();
    assert_eq!(backups, 1);
}

#[test]
fn rapid_changes_keep_disk_memory_and_scheduler_consistent() {
    let dir = temp_dir("rapid");
    let (_app, w) = app_in(&dir);
    let mut last = 0;
    for i in 0..40u32 {
        last = 100 + 50 * ((i * 37) % 199);
        invoke(
            &w,
            "update_settings",
            json!({"patch": {"updateInterval": last}}),
        )
        .unwrap();
    }
    assert_eq!(read_config(&dir)["updateInterval"], last);
    assert_eq!(
        invoke(&w, "get_settings", json!({})).unwrap()["updateInterval"],
        last
    );
    assert_eq!(
        invoke(&w, "get_scheduler_stats", json!({})).unwrap()["intervalMs"],
        last
    );
    let entries = std::fs::read_dir(&dir).unwrap().count();
    assert_eq!(entries, 1, "no temporary files left behind");
}

#[test]
fn write_failure_changes_nothing() {
    if unsafe_euid_is_root() {
        return; // permission bits are not enforced for root
    }
    let dir = temp_dir("readonly");
    let (_app, w) = app_in(&dir);
    invoke(
        &w,
        "update_settings",
        json!({"patch": {"updateInterval": 800}}),
    )
    .unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let err = invoke(
        &w,
        "update_settings",
        json!({"patch": {"updateInterval": 2000}}),
    );
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let err = err.unwrap_err();
    assert_eq!(err["code"], "settingsWriteFailed");
    assert!(err["field"].is_null());
    assert_eq!(
        invoke(&w, "get_settings", json!({})).unwrap()["updateInterval"],
        800,
        "memory unchanged"
    );
    assert_eq!(
        invoke(&w, "get_scheduler_stats", json!({})).unwrap()["intervalMs"],
        800,
        "scheduler unchanged"
    );
    assert_eq!(read_config(&dir)["updateInterval"], 800, "disk unchanged");
}

#[test]
fn scheduler_runs_and_feeds_get_snapshot() {
    let (_app, w) = app("sched");
    invoke(
        &w,
        "update_settings",
        json!({"patch": {"updateInterval": 100}}),
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(450));
    let stats = invoke(&w, "get_scheduler_stats", json!({})).unwrap();
    let ticks = stats["ticks"].as_u64().unwrap();
    assert!(
        (3..=7).contains(&ticks),
        "≈ 1 + 4 cycles at 100 ms: {stats}"
    );
    assert_eq!(stats["panics"], 0);
    let snap = invoke(&w, "get_snapshot", json!({})).unwrap();
    assert_eq!(snap["dataSource"], "live");
    assert!(
        snap["history"]["cpu"].as_array().unwrap().len() >= 2,
        "history fed by the scheduler"
    );
}

fn unsafe_euid_is_root() -> bool {
    std::fs::metadata("/proc/self")
        .map(|m| std::os::unix::fs::MetadataExt::uid(&m) == 0)
        .unwrap_or(false)
}
