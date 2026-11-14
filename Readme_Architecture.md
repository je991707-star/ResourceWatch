# ResourceWatch 架構說明

本文件依「程式檔／目錄」分章，描述每個檔案的職責、資料流與對外契約。
修改任何程式時，請把變更寫入對應章節。重構目標與階段進度見 `REFACTOR_RUST_TAURI.md`。

> 目前狀態：Tauri 桌面應用（`src-tauri/` + `frontend/`）由 Rust collectors 讀取實際硬體資料（`dataSource: "live"`），
> 以單一統一排程器驅動，設定儲存於使用者設定目錄。舊版「Plasma QML + Python + HTML」仍完整保留，功能對等驗證尚未完成。

```text
舊版（現行）
KDE Plasma 6 / plasmawindowed
└── package/contents/ui/main.qml
    ├── models/SensorManager.qml ── KSystemStats、gpu_backend.py、hardware_info.py、lsblk、/proc/diskstats、ip
    ├── HtmlView.qml ── QtWebEngine + QtWebChannel ── web/index.html + app.js
    └── CompactView.qml
KDE 設定視窗
└── config/config.qml → ui/configGeneral.qml ── QtWebChannel ── web/settings/*

Tauri 桌面應用
src-tauri/src/lib.rs ── AppState{settings(+SettingsStore → config.json), source, latest, scheduler}
        │   scheduler.rs: rw-scheduler 執行緒（唯一）每個 updateInterval 一個週期：
        │     source.sample(now) → emit "snapshot" → latest
        │   source: Source::Live(Monitor) ─ collectors/{cpu,memory,system,disk,network,gpu(+nvml)}
        │           Source::Mock(MockSource)（RESOURCEWATCH_MOCK=1）
        ▲ commands: get_snapshot / get_settings / get_settings_info / update_settings / reset_settings /
        │           refresh_devices / get_scheduler_stats
frontend/index.html + app.js（window.__TAURI__ invoke / listen）

目標（尚未實作）
Frontend (HTML/CSS/JS) ◀── Tauri Commands & Events ──▶ Rust: collectors / scheduler / state+history / settings
```

---

## package/metadata.json

Plasma Applet 中繼資料。`KPlugin.Id = io.github.local.resourcewatch`、`Version = 0.0.5`、
`X-Plasma-API-Minimum-Version = 6.0`。Tauri 版的應用 ID 與版本號另行定義，不覆寫此檔。

## package/contents/config/main.xml

KConfig 設定規格（舊版設定契約）：

| 鍵 | 型別 | 預設 | 範圍／說明 |
|---|---|---|---|
| `updateInterval` | Int | 500 | 100–10000 ms；步進 50 由 `settings.js` / `configGeneral.qml` 驗證。唯一的更新間隔 |
| `historyPoints` | Int | 30 | 10–120，設定頁步進 10；歷史緩衝長度（不是更新間隔） |
| `gpuIndex` | Int | 0 | 0–15；KSystemStats `gpu/gpu<N>` 與 DRM 清單的選擇索引 |
| `netInterface` | String | `all` | `all` = `network/all` 彙總；其他值為介面名稱，不拼入 shell |
| `ipRefreshSerial` | Int | 0 | 一次性動作計數器：遞增即觸發 IPv4 刷新 |
| `diskRefreshSerial` | Int | 0 | 一次性動作計數器：遞增即觸發硬碟清單重新偵測 |

Rust `settings` 模組需保留前四項的語意、預設值與範圍；兩個 `*RefreshSerial` 在 Tauri 版改為明確的 Command，不再存成設定值。

## package/contents/config/config.qml、package/contents/ui/configGeneral.qml

KDE 設定頁容器。`configGeneral.qml` 以 `cfg_updateInterval`、`cfg_historyPoints`、`cfg_gpuIndex`、
`cfg_netInterface` 承接 KConfig，並透過 QtWebChannel 物件 `resourceSettings` 對 `web/settings/` 公開：

- `settingsJson`（屬性，JSON 字串：`{updateInterval, historyPoints, gpuIndex, netInterface}`）
- `setSetting(key, value)`：驗證後寫入 `cfg_*`
- `refresh()`：重新發布 `settingsJson`
- `refreshDevice(kind)`：`kind = "ipv4" | "disks"`，遞增對應的 `*RefreshSerial`，回傳是否接受

## package/contents/ui/main.qml

`PlasmoidItem` 容器，建立唯一的 `SensorManager` 並把 `Plasmoid.configuration` 綁定進去。
以 `Application.name` 判斷是否為 `plasmawindowed`：是則為應用模式（初始 900–1220 × 580–790，最小 900×580），
否則為 Widget 模式（依螢幕高度縮放，最小 360×410）。只設最小值，不綁最大值以免鎖住縮放。
`fullRepresentation` 為 `HtmlView`（`viewMode = application | widget`），`compactRepresentation` 為 `CompactView`。

## package/contents/ui/CompactView.qml

面板緊湊表示，只讀 `cpu/gpu/ram.displayValue`；點擊展開完整視圖。Tauri 桌面版不需要此檔。

## package/contents/ui/models/SensorManager.qml

舊版唯一的感測器訂閱、正規化、排程與歷史緩衝層。

### Metric 物件格式

- 百分比型（`cpu`、`gpu`、`encoder`、`decoder`）：`{id, label, status, rawValue, displayValue, percent}`
- 成對型（`ram`、`vram`、`swap`）：另加 `usedText`、`totalText`；總量缺失或 0 時 `percent = null`
- 速率型（`netUp`、`netDown`、`disk*`）：`percent = null`，`rawValue` 單位為 B/s
- `status`：`ready` / `loading`（啟動後 5 秒寬限期內）/ `unavailable`
- 缺失值一律為 `null` 與 `"--"`，不寫成 0

### 統一排程

單一 one-shot `Timer`，按到期時間執行四個工作：`gpu`（啟動 `gpu_backend.py`）、`disk`（啟動 `cat /proc/diskstats`）、
`history`（`_sample()`）、`html`（`snapshotDue`）。每次喚醒每項工作最多執行一次，睡眠恢復不補跑。
`updateInterval` 變更時重新安排全部到期時間。KSystemStats 感測器以 `updateRateLimit: updateInterval` 推播。
同一時間每種外部查詢只允許一個在執行（`connectedSources.length === 0` 才啟動）。

### 指標與舊版資料來源

| 指標 | 舊版來源 | 更新方式 |
|---|---|---|
| CPU 使用率 | KSystemStats `cpu/all/usage` | 更新間隔 |
| CPU 時脈 | `cpu/all/averageFrequency` | 更新間隔 |
| CPU 溫度 | `cpu/all/averageTemperature`，缺失時 `maximumTemperature` | 更新間隔 |
| CPU 型號 | `hardware_info.py`（`/proc/cpuinfo`），回退 `cpu/cpu0/name`、`cpu/all/name`（過濾「all/全部」等泛用名） | 啟動一次 |
| Uptime | `os/system/uptime`（限速 60 s），HTML 端以本地單調時鐘每秒內插 | 每秒顯示 |
| GPU 使用率 | NVIDIA：`gpu_backend.py` NVML GPU-Util（失敗回退 `nvidia-smi`）；其他：`gpu/gpu<N>/usage`，回退 `gpu/all/usage` | 更新間隔（NVIDIA 每次啟動 Python） |
| NVENC / NVDEC | `gpu_backend.py` NVML encoder/decoder utilization（僅 NVIDIA） | 更新間隔 |
| GPU 溫度 | `gpu/gpu<N>/temperature` | 更新間隔 |
| VRAM | `gpu/gpu<N>/usedVram`、`totalVram` | 更新間隔 |
| GPU 名稱／廠牌 | `gpu/gpu<N>/name` + DRM PCI vendor（`gpu_backend.py`）；以名稱比對 KDE gpuN 與 DRM 裝置 | 名稱限速 1 天 |
| RAM / Swap | `memory/physical/used|total`、`memory/swap/used|total` | 更新間隔 |
| RAM 速度 | `hardware_info.py`：SMBIOS Type 17（`/sys/firmware/dmi/entries`）→ 使用者快取 `${XDG_STATE_HOME:-~/.local/state}/resourcewatch/ram_speed.json` | 啟動一次 |
| 硬碟清單 | `lsblk -J -b -d -o NAME,SIZE,MODEL,TRAN,TYPE`；保留 `TYPE=disk`、size>0、TRAN ∈ sata/ata/pata/ide/sas/scsi/nvme 或名稱符合 `nvmeXnY` | 啟動與手動 |
| 硬碟讀寫 | `cat /proc/diskstats` 第 6／10 欄 × 512 bytes 的相鄰差值；負差值（重置）略過 | 更新間隔（每次啟動外部程序） |
| 網路上下行 | `network/<if|all>/download|upload` | 更新間隔 |
| IPv4 | 手動：`ip -j -4 address show`（第一個 `scope global` inet）；指定介面時另訂閱 `network/<if>/ipv4address`（限速 1 天） | 手動／啟動 |
| 歷史 | `cpu, gpu, encode, decode, vram, ram`（百分比）與 `netUp, netDown`（B/s），長度 `historyPoints` | 更新間隔 |

## package/contents/ui/models/FormatUtils.js

`isValidNumber`、`isValidRate`、`percentOf`、`clampPercent`、`formatPercent`、`displayText`、`pushHistory` 等純函式。
單元測試：`tests/format_utils_test.js`。Rust 版需提供等價的格式化／驗證並移植測試案例。

## package/contents/ui/HtmlView.qml

QtWebEngine 容器與 QtWebChannel 橋接（物件 `resourceWatch`），只公開 `snapshotJson`、`snapshotReady(json)`、`openSettings()`；
不公開命令執行、檔案或網路存取。拒絕導覽離開本機頁面。快照只在 `SensorManager.snapshotDue` 時發布。

### HTML 快照 JSON 契約（v0.0.5）

Tauri 版的快照（`src-tauri/src/snapshot.rs`）沿用此表，並額外加入 `dataSource`（`"live"` 實際量測／`"mock"` 模擬，前端據 `"mock"` 顯示「模擬資料 · 非實機」）；
Tauri 版 `gpuSource` 值為 `"nvml"`、`"drm"`、`"unavailable"`、`"mock"`。欄位改名或刪除須同步更新本表、`snapshot.rs`、`frontend/app.js` 與 `package/contents/web/app.js`。

| 欄位 | 型別 | 說明 |
|---|---|---|
| `version` | string | 應用版本 |
| `cpuName` | string | 缺失為 `"--"` |
| `cpu` | Metric | 見下方 Metric 子物件 |
| `cpuFreqText`, `cpuTempText`, `uptimeText` | string | 已格式化文字，缺失 `"--"` |
| `uptimeSeconds` | number \| null | 供 HTML 本地每秒內插 |
| `gpuIndex` | number | 選擇的 GPU 索引 |
| `gpuName` | string | |
| `gpuVendor` | `"nvidia"\|"amd"\|"intel"\|"other"` | |
| `gpuSource` | `"nvml"\|"nvidia-smi"\|"ksystemstats"\|"unavailable"` | |
| `gpu`, `vram`, `encoder`, `decoder` | Metric | `vram` 含 used/total |
| `gpuTempText` | string | |
| `ram`, `swap` | Metric | 含 used/total |
| `ramSubtitle` | string | 目前等於 `ram.totalText` |
| `ramSpeedText` | string | 例如 `"1600 MT/s"`，缺失 `"--"` |
| `disks` | `{name, model, sizeText, transport}[]` | 只在清單變動時重建 DOM |
| `diskRates` | `{[name]: {read: number\|null, write: number\|null}}` | B/s |
| `netHasInterface` | bool | `netInterface !== "all"` |
| `netIpAvailable` | bool | 已手動刷新且取得 IP |
| `netIp` | string | |
| `netUp`, `netDown` | Metric | `rawValue` 為 B/s |
| `history` | `{cpu, gpu, encode, decode, vram, ram, netUp, netDown}: (number\|null)[]` | 最多 120 點，缺失保留 `null` |

Metric 子物件：`{displayValue: string, percent: number|null, rawValue: number|null, status: string, usedText: string, totalText: string}`。

## package/contents/scripts/gpu_backend.py

一次性、非特權的 GPU 取樣器。掃描 `/sys/class/drm/card*`（PCI class 0x0300/0x0302/0x0380）取得 `card`、`pci`、`vendor`；
對 NVIDIA 以 ctypes 載入 `libnvidia-ml.so.1`，用 PCI 位址取得 handle，讀取名稱、GPU-Util、encoder/decoder utilization；
NVML 失敗時以 `nvidia-smi -q -x -d UTILIZATION` 備援。百分比超出 0–100 或無效時輸出 `null`。
輸出：`{"devices": [{card, pci, vendor, name, usage, encoder, decoder, source}]}`。測試：`tests/gpu_backend_test.py`。

## package/contents/scripts/hardware_info.py

一次性取得 `cpuModel`（`/proc/cpuinfo` 的 `model name` / `Hardware`）與 `ramSpeedText`（`/sys/firmware/dmi/entries/17-*/raw` 的 SMBIOS Type 17 configured／rated speed，讀不到時用使用者快取；單一速度輸出 `N MT/s`，多種速度輸出 `最小–最大 MT/s`，皆無則 `null`）。
`--cache-ram-from-stdin` 由使用者自行以 `sudo dmidecode --type 17 |` 餵入以寫入快取。測試：`tests/hardware_info_test.py`。

## package/contents/web/index.html、styles.css、application.css、app.js

主監控 HTML。`app.js` 透過 QtWebChannel 接收快照並呼叫 `window.ResourceWatchUI.update(snapshot)`；
`setText()` 只在文字改變時寫入 DOM，SVG 圖表 path 相同時不重設，硬碟卡片只在清單變動時重建。
`?mode=application` 時顯示左側導覽（總覽／桌面小工具／設定）與響應式總覽；Widget 模式以 720 px 設計寬度等比縮放。
瀏覽器直接開啟時顯示標示「示範資料 · 非實機」的 demo；嵌入時不顯示示範數值。
已知：`demo` / `empty` 物件內的版本字串仍為 `0.0.4`（實際顯示以快照 `version` 為準）。

## package/contents/web/settings/

設定頁 HTML。`settings.js` 驗證 `updateInterval`（100–10000、步進 50）、`historyPoints`（10–120、步進 10）、
`gpuIndex`（0–15）、`netInterface`（空值視為 `all`），經 `resourceSettings.setSetting` 寫入；
「刷新 IPv4」「刷新硬碟」按鈕呼叫 `refreshDevice("ipv4"|"disks")`。瀏覽器模式不保存任何值。

## tests/

舊版測試（Python unittest、Node、bash 靜態檢查）。`tests/static_check.sh` 匯總執行；`application_layout_test.py` 需單獨執行。
新系統達到功能對等前不刪除。基線結果保存於 `docs/baseline/phase0-legacy-tests.txt`。

## install.sh、packaging/

舊版 Plasma 套件安裝腳本與 `plasmawindowed` 的 `.desktop` 入口。Tauri 版由 `cargo tauri build` 產生 deb／rpm（見 `src-tauri/tauri.conf.json`）。

## frontend/

Tauri 桌面應用的前端（靜態檔，無打包工具、無 npm 依賴；`tauri.conf.json` 的 `build.frontendDist` 指向此目錄）。只負責顯示，不存取硬體、檔案或網路。

- `index.html`：左側導覽「總覽」「桌面小工具」「設定」三頁。總覽沿用舊版卡片 DOM 與元素 ID；
  「桌面小工具」只是預留說明頁，不建立或控制 Plasma Widget；設定頁為應用內頁面（取代 KDE 設定視窗）。
  資料來源為 mock 時顯示黃色提示列。
- `styles.css`、`application.css`：自 `package/contents/web/` 複製的卡片與應用外框樣式（移除 Plasma 瀏覽器預覽用的 popover 規則）。
  響應式：寬度 ≤1200 px 單欄、≤980 px 側欄變窄、≤790 px 側欄只剩圖示。
- `settings.css`：設定頁、mock 提示列、欄位錯誤訊息樣式。
- `app.js`：以 `window.__TAURI__.core.invoke` / `event.listen` 連接後端（`withGlobalTauri`）。
  啟動時先 `listen("snapshot")`、`listen("settings-changed")`，再呼叫 `get_settings`、`get_settings_info`、`get_snapshot`。
  `setText()` 只在文字改變時寫入 DOM；SVG path 相同時不重設；硬碟卡片只在清單變動時重建（以 DOM API 建立，不用 innerHTML）；
  總覽隱藏時只保存快照，切回總覽才繪製。Uptime 以快照 `uptimeSeconds` 為錨點、`performance.now()` 每秒更新一個文字節點，不呼叫 IPC。
  設定欄位 `change` 時先做前端範圍檢查，再呼叫 `update_settings`；錯誤依 `CommandError.field` 顯示在對應欄位下，
  寫檔失敗（`settingsWriteFailed`，`field` 為 `null`）顯示在剛修改的欄位下。欄位錯誤文字直接寫入（不經 `setText()`，以免空值顯示成 `--`）。
  設定頁底部顯示設定檔路徑；啟動時讀取設定檔有問題（`loadWarning`）時顯示紅色提示。
  在 Tauri 以外（例如瀏覽器）開啟時不顯示任何數值，只標示「未連線到應用程式後端」。

## src-tauri/

Rust + Tauri 2 桌面應用。建置：`cargo tauri build`（release + deb／rpm）、`cargo build`；檢查：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`。
開發環境為 distrobox `resourcewatch-dev`（Fedora 44）。

### Cargo.toml、build.rs、tauri.conf.json、capabilities/default.json、icons/

- 套件 `resourcewatch` 0.1.0（Tauri 版自己的版號，與 Plasma 套件 0.0.5 分開；rpm 版號不允許 `-`）；lib 名稱 `resourcewatch_lib`。
  依賴：`tauri`、`serde`、`serde_json`、`libloading`（執行期載入 NVML，不在連結時依賴 NVIDIA）、`libc`（`getifaddrs`）；
  dev 依賴啟用 `tauri` 的 `test` feature 供 IPC 測試。`rust-version = 1.85`。
- `tauri.conf.json`：identifier `io.github.local.resourcewatch`；視窗 `main` 初始 1220×790、最小 640×480、可自由縮放、不設最大值；
  CSP 僅允許本機資源與 Tauri IPC；`withGlobalTauri: true`；打包目標 deb、rpm。
- `capabilities/default.json`：`main` 視窗只有 `core:default`（含事件監聽）；未安裝 fs／shell／http 等外掛。
- `icons/`：由 `icons/source/app-icon.png` 以 `cargo tauri icon` 產生，只保留 Linux 需要的 PNG。

### src/lib.rs

應用組裝與 IPC。`configure_with(builder, config_dir)` 註冊所有 Command，並在 Tauri `setup` 中：解析設定目錄
（`None` → `app.path().app_config_dir()`，即 `$XDG_CONFIG_HOME/io.github.local.resourcewatch`，通常為 `~/.config/io.github.local.resourcewatch`；
測試傳入暫存目錄）→ 載入設定 → 建立 `AppState` → 啟動唯一的排程器。`configure(builder)` = `configure_with(builder, None)`。
`run()` 在 `RunEvent::Exit`（關閉最後一個視窗）時呼叫 `AppState::shutdown()` 停止並 join 排程器執行緒。
`sample_live(count, interval)` 不開視窗、直接用 `Monitor` 取樣並回傳 JSON，供 `examples/sample.rs` 與系統工具交叉比對。

| Command | 輸入 | 輸出 | 錯誤 |
|---|---|---|---|
| `get_snapshot` | — | 排程器最近一次產生的 `Snapshot`（不觸發硬體讀取；排程器尚未產生時才用上次讀值組成） | — |
| `get_settings` | — | `Settings` | — |
| `get_settings_info` | — | `{path: string \| null, loadWarning: string \| null}`：設定檔路徑與啟動載入時的問題 | — |
| `update_settings` | `{patch: SettingsPatch}`（部分欄位，未知鍵拒絕） | 驗證並**已寫入磁碟**的 `Settings`，並發出 `settings-changed` | `invalidSetting`（含 `field`）；`settingsWriteFailed`（`field: null`，記憶體／排程器／磁碟皆不變）；型別錯誤為 Tauri 反序列化錯誤字串 |
| `reset_settings` | — | 預設 `Settings`（已寫入磁碟），並發出 `settings-changed` | `settingsWriteFailed` |
| `get_scheduler_stats` | — | `SchedulerStats`（間隔、週期數、略過數、panic 數、耗時與延遲） | — |
| `refresh_devices` | `{kind: "ipv4" \| "disks" \| "hardware"}`：IPv4／硬碟清單／CPU 型號＋RAM 速度＋GPU 清單與名稱 | 刷新後的 `Snapshot`，並立即發出 `snapshot` | 未知 `kind` 為反序列化錯誤 |

| Event | Payload | 時機 |
|---|---|---|
| `snapshot` | `Snapshot` | 每個 `updateInterval` 一次；手動刷新後立即一次 |
| `settings-changed` | `Settings` | 設定更新或恢復預設後 |

`AppState` 分成獨立的鎖（中毒時復原而非崩潰）：`settings`（目前設定 `Arc<Settings>`、`SettingsStore`、載入警告）、`source`（Monitor／Mock）、
`latest`（最新快照）與 `scheduler`（`OnceLock`，只能設定一次 → 不會有第二個排程器）。鎖順序固定為 settings → source；
排程週期只先短暫取 settings（複製 `Arc`）再取 source，不會巢狀，因此不會死結。命令不再等待整個取樣持有的大鎖。

排程週期（`start_scheduler`）：`source.sample(now, &settings)`（只持有 source 鎖）→ `emit("snapshot")`（以參考序列化，不複製快照）→
存入 `latest`。`RESOURCEWATCH_DEBUG_STATS=1` 時每 10 秒與結束時在 stderr 輸出 `RW_STATS …`。執行期間不啟動任何外部程序。

設定變更（`commit_settings`，整段持有 settings 鎖，因此連續或同時的修改會依序處理）：驗證 → 與目前值相同則直接回傳 →
寫入 `config.json`（失敗即回傳錯誤，不套用任何變更）→ `Source::apply_settings`（切換 GPU／網路介面）→ 更新記憶體 →
間隔有變才 `Scheduler::set_interval` → 發出 `settings-changed`。因此磁碟、記憶體與排程器不會長期不一致。
手動刷新在呼叫端執行緒進行，以 source 鎖與排程週期互斥，不重啟排程器。

### src/settings.rs

`Settings {updateInterval, historyPoints, gpuIndex, netInterface}`，預設與範圍同 `main.xml`：
`updateInterval` 100–10000／步進 50、`historyPoints` 10–120／步進 10、`gpuIndex` 0–15、`netInterface` 為 `all`（空值亦視為 `all`）
或 1–15 個 `[A-Za-z0-9-_.:@]` 字元（Linux IFNAMSIZ），值不會傳給 shell。`Settings::apply(&SettingsPatch)` 全部驗證通過才回傳新值，失敗時原值不變。
持久化見 `src/settings_store.rs`。

### src/scheduler.rs

正式統一排程器。單一執行緒 `rw-scheduler` 同步執行每個監控週期，因此**同一時間最多一個週期、沒有工作佇列**。
- `Schedule`（純邏輯，可用合成 `Instant` 測試）：第一個週期立即執行；之後依固定網格（due + interval）前進，不因取樣耗時而漂移；
  若週期超時或執行緒停頓（含時間大幅跳躍），`advance()` 直接跳到完成時間之後的第一個網格點，並回傳略過的次數——**不補跑**。
  `set_interval()` 讓新間隔從「現在」起算。間隔最小夾在 1 ms（設定本身已限制 100–10000 ms）。
- `Scheduler`：`start(interval, tick)` 建立執行緒；`set_interval()` 只更新共享狀態並喚醒同一執行緒（不建立新執行緒或計時器）；
  `stop()`（也在 `Drop` 時呼叫）設停止旗標、喚醒並 join，可重複呼叫；`stats()` 回傳 `SchedulerStats`。
  等待使用 `Condvar::wait_timeout`（Linux 上為 CLOCK_MONOTONIC）。週期內 panic 會被 `catch_unwind` 攔截並計數，排程器繼續運作。
- 未使用 Tokio：程式沒有其他非同步工作，單一阻塞執行緒加 `Condvar` 是滿足以上條件最簡單的機制。
- 睡眠：CLOCK_MONOTONIC 在系統睡眠時不前進，喚醒後只會執行一個到期的週期；collectors 的速率以相同單調時間計算，
  睡眠期間計數器也幾乎不增加，因此不會產生尖峰；計數器在喚醒後被重置時，該次為 `None`。

### src/settings_store.rs

設定檔 `<設定目錄>/config.json`，格式即 `Settings` 的 JSON（`{"updateInterval":500,"historyPoints":30,"gpuIndex":0,"netInterface":"all"}`）。
- 載入：檔案不存在 → 預設值、無警告（首次啟動）；無法讀取 → 預設值＋警告，不動原檔；不是合法 JSON 或不是物件 →
  原檔改名為 `config.json.corrupt-<unix 秒>`（同秒重複時加序號）保留，使用預設值並警告；個別欄位無效或型別錯誤 → 該欄位用預設值、
  其餘有效欄位保留，並警告；未知欄位忽略；接受 `networkInterface` 作為 `netInterface` 的別名。
- 寫入：建立目錄 → 寫入同目錄的唯一暫存檔（`.config.json.tmp-<pid>-<序號>`）→ `fsync` → 原子 `rename` 覆蓋 → `fsync` 目錄；
  失敗時刪除暫存檔並回傳錯誤，原檔不變。呼叫端（`commit_settings`）以 settings 鎖序列化所有寫入。

### src/snapshot.rs

快照型別，serde 以 camelCase 輸出，欄位對應上方「HTML 快照 JSON 契約」。`Metric::percent / pair / rate` 建構子負責把缺失、非有限值、
負值與總量 0 轉成 `null` / `"--"` / `status: "unavailable"`，不轉成 0。`format_uptime()` 對應舊版 `uptimeFromSeconds`。

### src/format.rs

自 `FormatUtils.js` 與舊版 QML 格式化函式移植：`valid`、`valid_rate`、`clamp_percent`、`percent_of`、`format_percent`（一位小數、去掉 `.0`）、
`format_bytes`（B…PiB）、`format_rate`（B/s…GiB/s）。

### src/source.rs

`Source::Live(Box<Monitor>)`（預設）或 `Source::Mock(Box<MockSource>)`（環境變數 `RESOURCEWATCH_MOCK=1`，僅供 UI 開發）。
統一提供 `sample`、`peek`、`refresh(RefreshKind)`、`apply_settings`。`RefreshKind` = `ipv4` | `disks` | `hardware`。

### src/monitor.rs

實際資料來源。持有各 collector、啟動時取得的靜態資訊（CPU 型號、RAM 速度、硬碟清單、IPv4、GPU 選擇）與歷史緩衝。
- `sample(now, settings)`：一個監控週期。讀取 CPU、記憶體、uptime、GPU、硬碟 I/O、網路各一次；硬碟與網路速率使用排程器提供的
  同一個週期時間戳 `now`；組成快照、寫入歷史，歷史陣列每週期只產生一次。
- `peek()`：只用上次讀值組成快照，不讀硬體、不寫歷史。
- 溫度／時脈文字不再重複做有限值檢查：collectors 只會輸出有限值（hwmon／NVML 整數已檢查範圍、cpufreq 為正整數平均），`None` 是唯一缺失情況。
- `refresh(kind)`：IPv4 重新查詢；硬碟清單重新掃描並重設 I/O 計數（下一個樣本無速率）；硬體資訊重新讀 CPU 型號、RAM 速度並重掃 GPU。
- `apply_settings()`：`gpuIndex` 改變時切換 GPU；`netInterface` 改變時切換統計介面，並因為是使用者明確操作而重新查詢一次 IPv4。
- 啟動後 5 秒內缺失的指標標為 `status: "loading"`（速率需兩次取樣），之後為 `"unavailable"`。
- 文字格式：CPU 時脈 `x.xx GHz`、CPU 溫度一位小數、GPU 溫度整數、`°C`。

### src/history.rs

`HistoryBuffer`：八條序列（cpu、gpu、encode、decode、vram、ram、netUp、netDown），每條最多 `historyPoints`（至少 2）點，
`historyPoints` 縮小時捨棄最舊的點；非有限值存成 `None`。Monitor 與 MockSource 共用。

### src/collectors/mod.rs

collector 共用工具。`Roots {proc_dir, sys_dir}`（預設 `/proc`、`/sys`，測試時指向假目錄樹）、`read_trimmed`／`read_u64`／`read_i64`／`read_first_line`、
`list_dir`（自然排序，`card10` 在 `card9` 後）、`counter_rate`（計數器倒退或時間差 ≤ 0 回傳 `None`）。collector 只讀資料，不決定何時讀、不格式化文字。

### src/collectors/cpu.rs

- 使用率：只讀 `/proc/stat` 第一行；total = user+nice+system+idle+iowait+irq+softirq+steal，idle = idle+iowait，取前後兩次差值；第一次、計數器倒退時為 `None`。
- 時脈：啟動時找出各 `cpuN/cpufreq/scaling_cur_freq`，每次取平均（kHz → MHz）。
- 溫度：啟動時掃描 hwmon。`coretemp` 取所有 `Core N` 平均（與 KSystemStats `averageTemperature` 相同，不含 Package），沒有則用 `Package id`；
  `k10temp`/`zenpower` 取 `Tdie`，其次 `Tctl`；`cpu_thermal` 等 SoC 取第一個輸入。超出 −40～150 °C 視為無效。
- 型號：`/proc/cpuinfo` 的 `model name`（或 `Hardware`），啟動與手動重新偵測時讀取。

### src/collectors/memory.rs

`/proc/meminfo`：RAM used = `MemTotal − MemAvailable`、Swap used = `SwapTotal − SwapFree`（與 KSystemStats 舊版數值逐位元組相同）。
`SwapTotal` 為 0 時保留 0（UI 顯示「0 B / --」且無百分比），欄位缺失或矛盾（Available > Total）時為 `None`。

### src/collectors/system.rs

- Uptime：`/proc/uptime` 第一欄（含睡眠時間，等同 CLOCK_BOOTTIME），每次取樣讀取；前端以此為錨點每秒內插。
- RAM 速度：先讀 `/sys/firmware/dmi/entries/17-*/raw`（SMBIOS Type 17，configured 優先、其次 rated；一般使用者通常無權限），
  再讀舊版相同的快取 `${XDG_STATE_HOME:-~/.local/state}/resourcewatch/ram_speed.json`（`{"speedsMTs":[…]}`）；
  輸出 `N MT/s` 或 `最小–最大 MT/s`，都沒有則未知。不要求 root。

### src/collectors/disk.rs

- 清單（啟動與手動刷新）：掃描 `/sys/block`，依 `dev` 主次號排序（與 `lsblk` 相同，硬碟編號與舊版一致）。
  排除 `/virtual/`（loop、zram、dm、md）、USB、SCSI type ≠ 0（光碟）、容量 0（空讀卡機）、`vd*`／`mmcblk*` 等；
  transport：`nvmeXnY` → `nvme`，路徑含 `/ata` → `sata`，含 `end_device-`/`sas_` → `sas`，其餘 `sd*` → `scsi`。型號取 `device/model`。
- 讀寫速率（每次取樣）：直接讀 `/proc/diskstats` 第 6／10 欄（512-byte sectors），先算 sectors/s 再乘 512（避免溢位）；
  第一次、裝置不在 diskstats、計數器倒退時為 `None`。`reset()` 用於重新掃描後。

### src/collectors/network.rs

- 流量：`/proc/net/dev` 的 rx/tx bytes 差值。`all` 只加總**實體介面**（有 `/sys/class/net/<if>/device`；結果快取），排除 lo、bridge、veth、tun/VPN，避免轉送或封裝流量重複計算。
  指定介面時只看該介面；介面不存在、剛出現／消失或計數器重置的那一次略過，全部略過則為 `None`。切換介面會清除前次計數。
- IPv4：`getifaddrs(3)`，取選定介面（`all` 則任一非 loopback 介面）第一個非 loopback、非 169.254 的位址。只在啟動、手動刷新、切換介面時查詢；不啟動 `ip`、不經 shell。

### src/collectors/gpu/mod.rs

- 清單：`/sys/class/drm/cardN` 中 PCI class `0x03xxxx` 的裝置，依 `cardN` 排序；`gpuIndex` 即此清單的索引（舊版是 KDE 的 `gpuN`，多 GPU 時順序可能不同）。
  記錄 PCI 位址（`0000:01:00.0`）、vendor／device ID、驅動。
- NVIDIA 且 NVML 可用：以 PCI 位址取得 NVML handle，讀使用率、NVENC、NVDEC、溫度、VRAM；名稱用 NVML。
- 其他（AMD、Intel、NVML 不可用的 NVIDIA）：DRM sysfs。`gpu_busy_percent`（AMD）、`mem_info_vram_used/total`、裝置自身 hwmon 溫度（優先 `edge`/`pkg` 標籤）；
  NVENC/NVDEC 一律 `None`；名稱查 `pci.ids`（`/usr/share/hwdata/pci.ids` 等，僅啟動／手動時讀取）。
- `source()`：`nvml` | `drm` | `unavailable`（索引超出範圍）。NVML 只在偵測到 NVIDIA 卡時嘗試載入一次。
- 偵測（啟動與 `rediscover()` 手動重新偵測）時為每張卡一次解析好名稱與讀取後端（NVML handle 或 DRM 路徑），`pci.ids` 每次偵測最多讀一次；
  `select(index)` 切換 GPU 只改索引，不重新掃描、不讀 `pci.ids`、不查 NVML。

### src/collectors/gpu/nvml.rs

以 `libloading` 在執行期載入 `libnvidia-ml.so.1`（或 `.so`），只綁定 9 個函式（init、shutdown、依 PCI 取 handle、名稱、使用率、
encoder／decoder 使用率、溫度、memory info v1）。`nvmlInit_v2` 成功後才建立物件，`Drop` 時 `nvmlShutdown`。整個程式生命週期只初始化一次。
回傳碼非成功（含 `NOT_SUPPORTED`，例如無 NVENC 的 GT 1030）或數值不合理時為 `None`。

### src/mock.rs

mock 資料來源（`RESOURCEWATCH_MOCK=1`）：以正弦波產生數值，名稱一律帶 `Mock`，IPv4 使用文件保留位址 `192.0.2.10`，RAM 速度刻意為 `--` 以測試缺失顯示；
不讀硬體、不啟動程序。`sample()` 推進時間並寫入歷史（共用 `HistoryBuffer`），`peek()` 不推進。
`refresh(Disks)` 後第一個樣本的硬碟速率為 `null`（模擬舊版計數器差值行為）。

### examples/sample.rs

`cargo run --example sample -- [次數] [間隔ms]`：以 `sample_live()` 輸出每次快照的 JSON 行，供與 `kstatsviewer`、`nvidia-smi`、`free` 等比對。不隨應用打包。

### tests/ipc.rs

以 Tauri mock runtime 走真實的 invoke 路由；每個測試使用自己的暫存設定目錄，並以 `run_iteration` 執行一次 `setup`（載入設定、啟動排程器）。
涵蓋：快照欄位契約與 `dataSource: "live"`、設定讀寫／驗證錯誤格式／未知鍵／失敗不改值／恢復預設、三種 `refresh_devices` 與未知 `kind`、
設定存檔後重新啟動仍保留（排程器以儲存的間隔啟動）、非法輸入不寫檔不改排程、損壞設定檔回報並保留備份、
快速連續修改後磁碟／記憶體／排程器一致且無暫存檔殘留、目錄唯讀時寫入失敗不改變任何狀態、排程器實際執行並餵給 `get_snapshot`。各模組的單元測試寫在該檔案的 `#[cfg(test)]` 內，collector 測試使用暫存目錄建立的假 `/proc`、`/sys`。

## Rust 後端：新舊資料來源對照

`collectors` 已實作；統一排程器（`scheduler`）與設定持久化尚未實作。交叉比對結果記錄在 `REFACTOR_RUST_TAURI.md` Phase 2。

| 指標 | 舊版來源 | Rust 實作來源 | 觸發 | 對照驗證方式 |
|---|---|---|---|---|
| CPU 使用率 | KSystemStats `cpu/all/usage` | `/proc/stat` 前後兩次計數差值 | 更新間隔 | `kstatsviewer` |
| CPU 時脈 | `cpu/all/averageFrequency` | cpufreq `scaling_cur_freq` 平均 | 更新間隔 | `kstatsviewer` |
| CPU 溫度 | `cpu/all/averageTemperature`（Core 平均） | hwmon coretemp Core 平均／k10temp Tdie、Tctl | 更新間隔 | `kstatsviewer` |
| CPU 型號 | `hardware_info.py` `/proc/cpuinfo` | `/proc/cpuinfo` `model name` | 啟動／手動 | 舊版輸出 |
| Uptime | `os/system/uptime` + HTML 內插 | `/proc/uptime` + 前端單調時鐘 | 每秒顯示、不啟動程序 | `kstatsviewer` |
| GPU 使用率（NVIDIA） | `gpu_backend.py` NVML（每間隔一次 Python） | NVML（只初始化一次，以 PCI 位址對應） | 更新間隔 | `nvidia-smi` |
| NVENC / NVDEC | `gpu_backend.py` NVML | NVML；不支援為 `None` | 更新間隔 | `nvidia-smi` |
| GPU 溫度 | KSystemStats `gpu/gpuN/temperature` | NVIDIA：NVML；其他：DRM 裝置 hwmon | 更新間隔 | `nvidia-smi`、`kstatsviewer` |
| VRAM | KSystemStats `usedVram/totalVram` | NVIDIA：NVML memory v1；其他：`mem_info_vram_*`（若有） | 更新間隔 | `nvidia-smi` |
| GPU 使用率（AMD/Intel） | KSystemStats `gpu/gpuN/usage` | AMD：`gpu_busy_percent`；Intel：不支援（sysfs 無此指標） | 更新間隔 | NOT TESTED（無硬體） |
| GPU 名稱／廠牌 | KSystemStats name + DRM PCI vendor | NVML 名稱／`pci.ids` + DRM PCI vendor | 啟動／手動 | 舊版輸出 |
| RAM / Swap | KSystemStats `memory/*` | `/proc/meminfo`（MemTotal − MemAvailable；SwapTotal − SwapFree） | 更新間隔 | `kstatsviewer`（逐位元組相同） |
| RAM 速度 | `hardware_info.py` SMBIOS Type 17 / 使用者快取 | 相同來源與快取檔，權限不足顯示未知 | 啟動／手動 | 舊版輸出 |
| 硬碟清單 | `lsblk` JSON + 篩選 | `/sys/block/*`，依主次號排序，不啟動 `lsblk` | 啟動／手動按鈕 | `lsblk` |
| 硬碟讀寫 | 每間隔 `cat /proc/diskstats` | 程序內直接讀 `/proc/diskstats` | 更新間隔 | `kstatsviewer disk/*` |
| 網路上下行 | KSystemStats `network/*` | `/proc/net/dev` 差值（`all` 只計實體介面） | 更新間隔 | `kstatsviewer` |
| IPv4 | `ip -j -4 address show` + KSystemStats `ipv4address` | `getifaddrs(3)` | 啟動／手動按鈕／切換介面 | `kstatsviewer` |
| 歷史 | QML 陣列 `historyPoints` 長度 | `HistoryBuffer`，缺失保留 `None` | 更新間隔 | 單元測試 |
| HTML 快照 | QtWebChannel `snapshotReady` | Tauri Event `snapshot` | 更新間隔 | IPC 整合測試 |
| 設定 | KConfig `main.xml` | Rust `Settings` + `config.json`（Tauri 設定目錄，原子寫入） | 使用者操作 | 單元／IPC 測試、GUI 重新啟動測試 |
| 排程 | `SensorManager.qml` 單一 one-shot Timer | `scheduler.rs` 單一執行緒、固定網格、略過過期週期 | `updateInterval` | 單元／IPC 測試、實機統計 |
