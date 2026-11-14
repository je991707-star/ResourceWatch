//! Rust → Tauri IPC integration tests on Tauri's mock runtime.
//! Exercises the real command routing, argument deserialization and error
//! serialization that the frontend uses through `invoke()`.

use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{App, WebviewWindow, WebviewWindowBuilder};

fn app() -> (
    App<tauri::test::MockRuntime>,
    WebviewWindow<tauri::test::MockRuntime>,
) {
    let app = resourcewatch_lib::configure(mock_builder())
        .build(mock_context(noop_assets()))
        .expect("build mock app");
    let window = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("build mock window");
    (app, window)
}

fn invoke(
    window: &WebviewWindow<tauri::test::MockRuntime>,
    cmd: &str,
    body: Value,
) -> Result<Value, Value> {
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
fn snapshot_follows_contract_and_is_marked_mock() {
    let (_app, w) = app();
    let snap = invoke(&w, "get_snapshot", json!({})).unwrap();
    assert_eq!(snap["dataSource"], "mock");
    for key in [
        "version",
        "cpuName",
        "cpu",
        "cpuFreqText",
        "cpuTempText",
        "uptimeText",
        "uptimeSeconds",
        "gpuIndex",
        "gpuName",
        "gpuVendor",
        "gpuSource",
        "gpu",
        "gpuTempText",
        "vram",
        "encoder",
        "decoder",
        "ram",
        "ramSubtitle",
        "ramSpeedText",
        "swap",
        "disks",
        "diskRates",
        "netHasInterface",
        "netIpAvailable",
        "netIp",
        "netUp",
        "netDown",
        "history",
    ] {
        assert!(snap.get(key).is_some(), "snapshot missing {key}");
    }
    for key in [
        "cpu", "gpu", "encode", "decode", "vram", "ram", "netUp", "netDown",
    ] {
        assert!(snap["history"][key].is_array(), "history missing {key}");
    }
}

#[test]
fn settings_roundtrip_and_validation() {
    let (_app, w) = app();
    let s = invoke(&w, "get_settings", json!({})).unwrap();
    assert_eq!(
        s,
        json!({"updateInterval": 500, "historyPoints": 30, "gpuIndex": 0, "netInterface": "all"})
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
    let (_app, w) = app();
    for kind in ["ipv4", "disks", "hardware"] {
        let snap = invoke(&w, "refresh_devices", json!({"kind": kind})).unwrap();
        assert_eq!(snap["dataSource"], "mock");
    }
    let snap = invoke(&w, "refresh_devices", json!({"kind": "disks"})).unwrap();
    // Right after a rescan there is no counter delta yet: rates are null, not 0.
    for (_, rate) in snap["diskRates"].as_object().unwrap() {
        assert!(rate["read"].is_null() && rate["write"].is_null());
    }
    assert!(invoke(&w, "refresh_devices", json!({"kind": "everything"})).is_err());
}
