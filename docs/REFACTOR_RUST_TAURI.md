# ResourceWatch — Rust + Tauri 重構計畫

| 項目 | 內容 |
|---|---|
| 分支 | `refactor/rust-tauri` |
| 狀態 | Tauri 版已由 Rust collectors 讀取實際硬體（Phase 2），由統一排程器驅動、設定可持久化（Phase 3），並完成功能對等與效能驗證（Phase 4，實機睡眠恢復與執行中改縮放未驗證）；舊版 Plasma QML + Python + HTML 已從此分支移除（可由 `main`／`v0.0.5`／`legacy/plasma-qml` 取回） |
| 基礎 | ResourceWatch v0.0.5 系列的監控功能與 HTML 介面 |
| 專案路徑 | `/var/home/h97m/Projects/ResourceWatch` |
| 相關文件 | 執行規則：`CLAUDE.md`；檔案職責、JSON 契約、新舊來源對照：`Readme_Architecture.md` |

本文件記錄目標、工作清單、測試證據與已知限制，**不是已完成功能的發行公告**。
標示：`[x]` 完成、`[ ]` 未完成、`NOT TESTED` 未能驗證、`BLOCKED` 受環境阻擋。

---

## 1. 目標

將 ResourceWatch 改為獨立的 Linux 桌面監控應用程式：

| 層 | 職責 |
|---|---|
| Rust | 硬體資料採集、統一排程器、歷史資料、設定管理、對前端的 API |
| Tauri 2 | 桌面視窗、原生系統整合、Rust ↔ 前端通訊 |
| HTML / CSS / JS | 總覽、圖表、設定頁、響應式版面 |
| Plasma Widget（未來） | 必要的 QML 外殼，盡量共用 Rust 監控核心與 HTML 顯示元件 |

目標狀態：桌面應用不再依賴 `plasmawindowed`、QML UI、QtWebChannel 或定期啟動 Python 腳本。**這是目標，不代表已完成。**

## 2. 原則

1. **保留歷史**：沿用現有 Repository，從已提交的舊版基線延伸，不重建歷史。
2. **可比對、可回退**：功能對等前不刪除舊版監控程式與測試；舊版可由 `main`、既有 Tag、`legacy/plasma-qml` 查閱。
3. **先桌面應用，再桌面小工具**：第一階段不實作 Plasma Widget。
4. **單一資料來源**：只有後端取得資料，前端只呈現，不在 JS 重複查詢硬體。
5. **減少無效工作**：避免高頻外部程序、重複解析、未改變的 DOM 重繪；保留外部資料／驅動／系統呼叫的必要錯誤處理。
6. **逐步替換**：模組通過對照測試後才停用對應舊實作。
7. **不以「可編譯」代替驗收**：Wayland／Bazzite 視窗行為、感測器結果、資源占用需實機驗證。

## 3. 目標架構

```text
ResourceWatch Desktop (Tauri 2)
├─ Frontend: HTML / CSS / JavaScript
│  ├─ 總覽與歷史圖表
│  ├─ 桌面小工具管理入口（預留，後續階段）
│  └─ 設定頁
└─ Rust Backend
   ├─ collectors   CPU/GPU、RAM/SWAP、Disk I/O + Discovery、Network + IPv4（只讀資料）
   ├─ scheduler    統一排程（決定何時讀）
   ├─ state/history 最新快照與歷史緩衝
   ├─ settings     設定讀寫、驗證、持久化
   └─ Tauri Commands & Events

Future: Plasma Widget (QML shell + optional HTML view)
        └─ 透過共用 Rust Core 或本機 IPC 取得資料
```

遷移初期**不做**常駐服務或 D-Bus／本機 IPC；等 Phase 5 評估桌面應用與 Widget 是否共用監控程序時再抽出。

## 4. 監控與更新契約

| 項目 | 更新規則 |
|---|---|
| CPU 使用率／時脈／溫度、NVIDIA GPU／NVENC／NVDEC、GPU 溫度／VRAM、RAM／SWAP、硬碟讀寫、網路上下行、歷史採樣、HTML 快照推送 | 共用同一個更新間隔 |
| 系統運行時間 | 每秒更新顯示，輕量計算，不每秒啟動程序 |
| CPU／GPU 名稱、CPU 型號、RAM 速度 | 啟動時取得；可手動重新偵測 |
| 網路 IPv4、硬碟裝置清單 | 啟動時取得；之後只由設定頁按鈕手動刷新 |

- 更新間隔：預設 `500 ms`、範圍 `100–10000 ms`、步進 `50 ms`；**只有一個**可調間隔，不重新引入多組欄位。
- 排程器按到期時間觸發；系統喚醒後不補跑大量過期任務。
- 採集頻率 ≠ 硬體回報新值的頻率；感測器與驅動可能限制實際刷新速度。
- 歷史只存需要的數值；缺失明確標示，不以 `0` 代替。

## 5. 後端資料來源（待移植）

概要如下；逐項的舊版來源、Rust 候選來源與驗證方式見 `Readme_Architecture.md`「Rust 後端」章節的對照表。

| 模組 | 候選來源 | 注意事項 |
|---|---|---|
| CPU | `/proc/stat`、`/proc/cpuinfo`、cpufreq sysfs、hwmon | 使用率需前後兩次計數器差值 |
| NVIDIA GPU | NVML | 驗證 GPU／編解碼器使用率定義，維持 PCI 位址對應 |
| AMD／Intel GPU | DRM sysfs、hwmon、驅動介面 | 不同驅動指標不同，記錄不支援項目 |
| RAM／SWAP | `/proc/meminfo` | 區分容量、用量、百分比 |
| RAM 速度 | DMI／SMBIOS 或授權快取 | 權限不足顯示未知，不寫死 |
| 硬碟 I/O | `/proc/diskstats` | 計數器重置、裝置移除、採樣時間差 |
| 硬碟清單 | sysfs／udev | 與讀寫採樣分離 |
| 網路流量 | `/proc/net/dev`／Netlink | 介面選擇、計數器回捲、介面變更 |
| IPv4 | Netlink | 只由明確使用者操作觸發 |
| Uptime | Linux boot-time 時鐘 | 單調／開機時間計算，不每秒建立程序 |

上表是遷移方向，不代表已支援所有 GPU 型號、驅動與 Bazzite 配置。

## 6. 前端與視窗

- 以現有 HTML/CSS/JS 視覺風格為起點，不照搬 QtWebChannel 綁定。
- 正常桌面視窗：自由縮放、設定**最小寬高**，不鎖固定寬高。
- 左側導覽保留「總覽」、「桌面小工具」、「設定」；「桌面小工具」在 Plasma Widget 實作前僅為預留入口，不宣稱能建立或控制 Widget。
- 縮小時以響應式重排與捲動為主，不把整個畫面按固定畫布縮小。
- 只更新實際變動的文字與圖表資料。
- Tauri 指令與事件明確定義輸入、輸出、錯誤與資料格式（快照欄位沿用 `Readme_Architecture.md`「HTML 快照 JSON 契約」）；前端不得自行存取敏感系統資源。

---

## 7. 階段進度

### Phase 0 — 基線與規範 ✅

- [x] 確認分支、HEAD、`git status`，保存原有測試結果
  - 分支 `refactor/rust-tauri`；HEAD `314620e7dc24a5436c0d7b31ec8c5b74e1daebce`（`v0.0.5-2-g314620e`）
  - 開始時工作樹：`AGENTS.md` 已刪除（未暫存）、`CLAUDE.md` 未追蹤，皆為使用者自行變更，未納入 Phase 0 commit
  - Tag：v0.0.1–v0.0.5；保留分支：`main`、`legacy/plasma-qml`
  - 測試結果 → `docs/baseline/phase0-legacy-tests.txt`（Python 3.15.0、Node v26.13.1）：
    `tests/static_check.sh` 24 項靜態檢查 PASS、排程器契約 PASS、FormatUtils PASS、
    `gpu_backend_test` 5/5、`hardware_info_test` 5/5、`scheduler_contract_test` 3/3（exit 0）；
    `application_layout_test.py` 4/4；`qmllint` NOT TESTED（未安裝）
- [x] 確認舊版 HTML、監控指標與 JSON 欄位契約
  - → `Readme_Architecture.md` 的 `HtmlView.qml`（快照契約、Metric 子物件）、`SensorManager.qml`（指標與來源、排程）、
    `main.xml`（設定鍵）、`configGeneral.qml`（設定頁 WebChannel 介面）章節
- [x] 建立新舊來源對照表，避免改名造成前端讀不到值
  - → `Readme_Architecture.md`「Rust 後端：新舊資料來源對照」章節

**Phase 0 發現的舊版行為（遷移時處理）**

- NVIDIA：`gpu_backend.py` 每個更新間隔重新啟動 Python 並初始化／關閉 NVML（預設 500 ms 即每秒 2 次）。
- 硬碟速率：每個更新間隔啟動一次 `cat /proc/diskstats`。
- Uptime：KSystemStats 感測器限速 60 s，HTML 每秒以本地時鐘內插（Rust 版保留此作法）。
- IPv4：除手動刷新外，指定介面時另訂閱 KSystemStats `ipv4address`（限速 1 天）；新契約為啟動取得、之後只手動刷新。
- NVIDIA 的 GPU 溫度與 VRAM 來自 KSystemStats，不是 NVML。
- RAM「已用」依 KSystemStats 定義；改用 `/proc/meminfo` 時需比對差異。
- `web/app.js` 的 `demo`／`empty` 版本字串仍為 `0.0.4`（實際顯示以快照為準）。
- 硬碟計數器重置時該次不輸出速率；初次採樣無速率（顯示 `--`）。

**已知環境限制**

- ~~`BLOCKED`：本機未安裝 `cargo`／`rustc`~~ → 已由使用者在 distrobox `resourcewatch-dev` 建立環境（Phase 1 解除）。

### Phase 1 — Tauri 桌面應用骨架 ✅

- [x] 安裝 Rust toolchain 與 Tauri 2 依賴
  - distrobox `resourcewatch-dev`（Fedora 44）：rustc／cargo 1.100.0、tauri-cli 2.12.4、WebKitGTK 4.1 2.54.2、libsoup 3.6.6。容器內無 Node（前端不需打包）
- [x] 初始化 Rust + Tauri 2 專案 → `src-tauri/`（`Cargo.toml`、`tauri.conf.json`、`capabilities/default.json`、`src/`、`icons/`）
- [x] 移植 HTML 總覽與設定頁，建立前端與 Rust 的基本通訊
  - 前端 `frontend/`：沿用舊版卡片 DOM／樣式，移除 QtWebChannel，設定頁改為應用內第三頁，「桌面小工具」為預留說明頁
  - IPC：Commands `get_snapshot`、`get_settings`、`update_settings`、`reset_settings`、`refresh_devices`；
    Events `snapshot`、`settings-changed`（定義見 `Readme_Architecture.md`「src/lib.rs」）
- [x] 設定正常視窗尺寸、最小尺寸及 Wayland 下的縮放行為
  - 初始 1220×790、最小 640×480、可自由縮放、無最大值；尺寸為邏輯像素，由 GTK 依螢幕縮放比例換算
- [x] 暫以 mock 資料呈現；**不宣稱已完成硬體監控**
  - `src/mock.rs`，快照 `dataSource: "mock"`，名稱帶 `Mock`，畫面顯示「模擬資料 · 非實機」與提示列

**測試證據（2026-11-14，distrobox `resourcewatch-dev`）**

- `cargo fmt --check` 通過；`cargo clippy --all-targets -- -D warnings` 無警告；`cargo check` 通過
- `cargo test`：單元測試 18/18（format、settings 驗證、snapshot 缺失值與 JSON 欄位名、mock 歷史長度／刷新）、
  IPC 整合測試 3/3（`tests/ipc.rs`，Tauri mock runtime）
- `node --check frontend/app.js` 通過（主機 Node v26.13.1）
- `cargo tauri build`：release 執行檔與 `ResourceWatch_0.1.0_amd64.deb`、`ResourceWatch-0.1.0-1.x86_64.rpm` 產生成功
- 實機啟動（Bazzite KDE Wayland，從 distrobox 執行 debug 版）：視窗正常顯示，以 `spectacle` 截圖確認總覽、mock 標示、
  圖表與硬碟卡片皆有繪製並持續更新；執行 12 秒無崩潰
- 舊版測試未受影響：`tests/static_check.sh` exit 0、`application_layout_test.py` OK

**已知限制／未驗證**

- `NOT TESTED`：設定頁的實際點擊操作、縮小到最小尺寸時的響應式版面、HiDPI 縮放比例變更、視窗關閉流程（只做了截圖與 IPC 自動測試）
- 容器內缺少 NVIDIA EGL 使用者端函式庫，啟動時出現 `MESA-EGL: failed to create dri2 screen` 警告，目前仍可正常繪製（可能退回軟體繪製）；
  若之後出現白畫面，可嘗試 `WEBKIT_DISABLE_DMABUF_RENDERER=1`
- Wayland 下視窗圖示顯示為通用圖示（未安裝 `.desktop` 檔，app_id 無對應），打包安裝後再確認
- 設定只存在記憶體（持久化屬 Phase 3）；mock 迴圈是暫時實作，統一排程器屬 Phase 3
- deb／rpm 只確認可產生，未安裝測試

### Phase 2 — Rust 硬體監控 ✅

- [x] CPU、RAM／SWAP → `src-tauri/src/collectors/cpu.rs`、`memory.rs`、`system.rs`（uptime）
- [x] 硬碟 I/O、裝置清單 → `collectors/disk.rs`（`/sys/block` 掃描、`/proc/diskstats` 程序內讀取）
- [x] 網路流量、IPv4 → `collectors/network.rs`（`/proc/net/dev`、`getifaddrs`）
- [x] NVIDIA NVML，再逐步擴充 AMD／Intel → `collectors/gpu/nvml.rs`（libloading，初始化一次）、`collectors/gpu/mod.rs`（DRM sysfs／hwmon）
- [x] CPU 型號、GPU 名稱、RAM 速度等啟動資訊 → `cpu::cpu_model`、NVML 名稱／`pci.ids`、`system::ram_speed_text`；`refresh_devices("hardware")` 可重新偵測
- [x] 與舊版或系統工具交叉比對資料，記錄不支援項目（見下表）
- 組合與切換：`monitor.rs`（實際資料，預設）、`source.rs`（`RESOURCEWATCH_MOCK=1` 改用 mock）、`history.rs`（共用歷史緩衝）

**測試證據（2026-11-14）**

- `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 無警告
- `cargo test`：單元測試 45/45（含以暫存目錄假 `/proc`、`/sys` 測試的 CPU 計數差值、hwmon coretemp/k10temp、meminfo、
  SMBIOS／快取、硬碟分類與 lsblk 排序、diskstats 重置、net/dev 實體介面加總與重置、AMD/Intel DRM、pci.ids、NVML 載入不崩潰）；
  IPC 整合測試 3/3（預設 `dataSource: "live"`）
- 主機（Bazzite KDE Wayland，Xeon E3-1231 v3、Quadro P620、driver 580.196.07）以 `examples/sample.rs` 每秒取樣 6 次，與同時執行的
  `kstatsviewer`（舊版資料來源 KSystemStats）及 `nvidia-smi` 比對：

| 指標 | Rust（新） | 對照來源 | 結果 |
|---|---|---|---|
| CPU 使用率 | 7.2–11.1% | KSystemStats 5.8–10.6% | 一致（取樣時間點不同） |
| CPU 時脈 | 2.11–3.76 GHz | KSystemStats 1.50–3.69 GHz | 一致（頻率本身快速變動） |
| CPU 溫度 | 51.0–56.8 °C | KSystemStats averageTemperature 51–56.75 °C | 一致（同為 Core 平均） |
| RAM 已用 | `MemTotal − MemAvailable` | KSystemStats `memory/physical/used` | 逐位元組相同（另於啟動前以 /proc/meminfo 驗算） |
| Swap 已用 | 7.8 GiB / 7.8 GiB | KSystemStats 8341164032 B | 一致 |
| GPU 使用率 | 10–12% | `nvidia-smi` 10–12% | 一致；KSystemStats 顯示 51%（舊版已知錯誤值，正是改用 NVML 的原因） |
| NVENC / NVDEC | 40–42% / 0% | `nvidia-smi` 40–42% / 0% | 一致 |
| GPU 溫度 | 61 °C | `nvidia-smi` 61、KSystemStats 61 | 一致 |
| VRAM | 1.5–1.6 GiB / 2.0 GiB | `nvidia-smi` 1501–1546 MiB / 2048 MiB | 一致 |
| 網路上／下行 | 1.3–1.4 MiB/s ／ 1.3–1.5 KiB/s | KSystemStats 1.34 MiB/s ／ 1.37–1.70 KB/s | 一致 |
| 硬碟讀寫 | nvme0n1 寫 ~302 KB/s、sda 0 | KSystemStats `disk/*` nvme 寫 302–640 KB/s、sda 0 | 一致 |
| 硬碟清單 | sda（TOSHIBA DT01ABA2, 1.8 TiB, sata）、nvme0n1（Micron 2400, 476.9 GiB） | `lsblk` | 相同且同順序；USB 讀卡機 sdb、光碟 sr0、zram0 已排除 |
| CPU 型號／RAM 速度／IPv4／Uptime | Xeon E3-1231 v3 ／ 1600 MT/s（使用者快取）／ 192.168.50.212 ／ 2天 00:23 | 舊版輸出、KSystemStats | 一致 |

- 主機執行 release 版視窗：截圖確認顯示即時資料（「即時監控」、`NVIDIA Quadro P620`、各卡片數值）；執行期間子程序只有 WebKit 固定的
  `WebKitWebProcess`、`WebKitNetworkProcess`，三次檢查都相同 → **沒有每次更新啟動外部程序**；`libnvidia-ml.so` 只載入一次
- 舊版測試：`tests/static_check.sh`、`application_layout_test.py` 仍通過（舊版程式未修改）

**不支援／限制**

- Intel GPU 使用率：i915／xe 未在 sysfs 提供使用率（需 perf PMU），顯示 `--`；Intel iGPU 無 VRAM 指標
- AMD／Intel／nouveau：NVENC／NVDEC 不支援，顯示 `--`
- AMD／Intel GPU：`NOT TESTED`（本機無此硬體，僅以假 sysfs 單元測試）
- NVIDIA 在 NVML 無法載入時（例如在未整合 NVIDIA 的 distrobox 內執行）：改用 DRM sysfs，專有驅動沒有 `gpu_busy_percent`，使用率等顯示 `--`；
  舊版的 `nvidia-smi` 備援**未移植**（避免每次更新啟動外部程序）
- `gpuIndex` 依 DRM `cardN` 順序，舊版依 KDE `gpuN`；多 GPU 系統順序可能不同，`NOT TESTED`
- 網路 `all` 只計實體介面；舊版 KSystemStats 的 `all` 定義不同，在有 Wi-Fi／VPN／bridge 的系統上數值可能不同（本機只有 enp3s0，結果一致）
- 切換網路介面時會立即重新查詢一次 IPv4（使用者明確操作）；其餘仍只在啟動與手動刷新時查詢
- RAM 速度：SMBIOS 一般使用者無權讀取，本機讀到的是舊版既有快取；沒有快取的系統顯示 `--`
- 取樣迴圈仍是 Phase 1 的暫時實作（每次取樣讀全部 collector）；統一排程器與睡眠恢復測試屬 Phase 3
- 資源占用比較（舊版 vs 新版）屬 Phase 4，尚未量測

### Phase 3 — 排程、歷史與設定（實作完成，實機睡眠恢復待驗證）

- [x] 共用 `updateInterval` 的統一排程器 → `src-tauri/src/scheduler.rs`（單一執行緒 `rw-scheduler`、單調時鐘、固定網格、
  不重疊、略過過期週期不補跑、可動態改間隔、可停止並 join、週期 panic 不終止排程器）；取代 Phase 1／2 的暫時取樣迴圈
- [x] 歷史資料緩衝與前端快照事件 → 每個週期 `Monitor::sample(now)` → `HistoryBuffer` → `snapshot` 事件 → `latest`；
  同一週期的硬碟／網路速率共用時間戳；歷史陣列每週期只產生一次
- [x] 設定讀寫、驗證、恢復預設值 → `src-tauri/src/settings_store.rs`（`<Tauri app config dir>/config.json`、原子寫入、
  損壞檔改名保留、逐欄位回退預設）；`commit_settings` 以「驗證 → 寫檔 → 套用 Monitor → 更新記憶體 → 排程器」順序執行，寫檔失敗不套用
- [x] 手動刷新按鈕（IPv4、硬碟清單、硬體資訊）→ 沿用 `refresh_devices`，與排程週期以 source 鎖互斥，不重啟排程器；
  切換 GPU 改用偵測時快取的名稱與後端，不重新掃描
- [x] 測試 100／500／10000 ms → 單元測試（合成時鐘）、IPC 測試與主機實測（下表）
- [ ] 實機睡眠恢復 → `NOT TESTED`（未在使用者工作階段執行 suspend）；已以合成時鐘模擬一小時時間跳躍、以及睡眠後長間隔速率計算
- [x] 確認沒有每次更新重複啟動 Python 的路徑 → 實測執行期間子程序只有 WebKit 固定兩個，PID 前後相同
- 新增 IPC：`get_settings_info`（設定檔路徑、載入警告）、`get_scheduler_stats`；`reset_settings` 可能回傳 `settingsWriteFailed`
- 前端：設定頁顯示設定檔路徑與載入警告；修正欄位下方在沒有錯誤時顯示紅色 `--` 的既有 bug（`setFieldError` 不再經 `setText()`）
- 依本次指示：**未 commit、未 push、未建 tag、未改版本號**

**測試證據（2026-11-14）**

- `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo check` 通過
- `cargo test`：單元測試 62/62（新增排程器 9：100／500／10000 ms 固定網格無漂移、超時略過、耗時等於間隔、一小時時間跳躍只跑一次、
  動態改間隔、執行緒不重疊、50 次改間隔仍為同一執行緒且可乾淨停止、panic 後繼續、10 s 間隔下停止立即生效；
  設定檔 6：無檔案、存檔重載與格式、逐欄位回退與別名、損壞檔保留、目錄唯讀寫入失敗且原檔不變、8 執行緒同時寫入不產生殘缺檔；
  另新增 GPU 切換不重新掃描、網路睡眠後長間隔不產生尖峰）
- IPC 整合測試 9/9（新增：重新啟動後設定保留且排程器以儲存值啟動、非法輸入不寫檔不改排程、損壞檔回報與備份、
  40 次快速修改後磁碟／記憶體／排程器一致、唯讀目錄寫入失敗不改變任何狀態、排程器實際以 100 ms 運作並餵給 `get_snapshot`）
- 所有測試使用暫存目錄；確認使用者真實設定目錄 `~/.config/io.github.local.resourcewatch` 在測試後仍不存在
- `node --check frontend/app.js` 通過；舊版 `tests/static_check.sh`、`application_layout_test.py` 仍通過

**效能測試（主機 Bazzite KDE Wayland 原生、release 版、視窗顯示總覽、Xeon E3-1231 v3 + Quadro P620）**

每個間隔啟動一次（暫存 `XDG_CONFIG_HOME` 放入對應設定），預熱 10 秒後量測 60 秒；CPU 為「單一核心的百分比」，來自 `/proc/<pid>/stat`
utime+stime（解析度 10 ms），各只量一次、未重複取平均；RSS 含共用函式庫。

| 間隔 | 主程序 CPU | WebKit 子程序 CPU | 主程序 RSS | 子程序 RSS 合計 | 實際取樣頻率 | 略過 | 單次取樣耗時 平均／最大 | 啟動延遲 平均／最大 |
|---|---|---|---|---|---|---|---|---|
| 100 ms | 5.81% | 9.53% | 203 MiB | 367 MiB | 10.0／s（每 10 s 100–101 次） | 0 | 1.29／2.85 ms | 0.10／0.84 ms |
| 500 ms | 1.35% | 2.10% | 202 MiB | 308 MiB | 2.0／s（每 10 s 20–21 次） | 0 | 1.25／3.01 ms | 0.09／0.17 ms |
| 10000 ms | 0.88% | 0.96% | 203 MiB | 356 MiB | 0.1／s | 0 | 1.41／2.33 ms | 0.09／0.14 ms |

主程序內部依執行緒拆分（另一次 30 秒量測）：

| 間隔 | `rw-scheduler`（Rust 取樣＋序列化＋送出事件） | 主執行緒（GTK／WebKit UI） | 其他執行緒 |
|---|---|---|---|
| 100 ms | 1.23% | 3.73% | 0.90% |
| 500 ms | 0.17% | 1.20% | 0.40% |
| 10000 ms | < 0.03%（低於量測解析度） | 0.40% | 0.13% |

- Rust 後端成本與間隔成正比且很小；主要成本是 WebKit 繪製（子程序）與 GTK 主執行緒。10000 ms 時仍有約 1–2% 的基本成本，
  主要來自 UI（含前端每秒更新一次運作時間文字），不是取樣
- 子程序：三種間隔皆只有 `WebKitWebProcess`、`WebKitNetworkProcess`，量測開始與結束 PID 相同 → 沒有意外的外部程序

**圖形介面**

- 原生 Wayland：啟動、即時資料與圖表更新正常（截圖）
- XWayland（`GDK_BACKEND=x11`，因原生 Wayland 不允許外部工具操作視窗）以 `xdotool`／`wmctrl` 自動化：
  要求 500×400 時視窗維持 640×480（最小尺寸生效）；640×480 下側欄收合為圖示、內容可捲動；鍵盤切到設定頁；
  輸入 99 → 欄位旁顯示錯誤、未寫檔；輸入 1250 → `config.json` 寫入 1250；關閉視窗 → 輸出 `scheduler stopped`、程序結束；
  重新啟動 → 設定頁顯示 1250、排程器統計 `interval_ms=1250`
- 原生 Wayland 下的關閉流程：`NOT TESTED`（無法以外部工具關閉原生 Wayland 視窗；XWayland 下已驗證同一段 `RunEvent::Exit` 程式）

**已知限制**

- XWayland＋NVIDIA 下 WebKitGTK 出現 `Failed to create GBM buffer`，畫面空白；設定 `WEBKIT_DISABLE_DMABUF_RENDERER=1` 後正常。
  原生 Wayland 不受影響；程式目前**沒有**自動設定此變數（Phase 4 評估）
- 實機 suspend／resume `NOT TESTED`
- 效能數據各只量一次，未計算變異；未與舊版 Plasma 實作比較（屬 Phase 4）
- 設定目錄無法解析時只保存在記憶體（設定頁會顯示），此情況未在實機發生過
- 設定檔格式損壞時會改名保留後使用預設值；若改名本身失敗（目錄不可寫），之後的儲存也會失敗，不會覆蓋原檔

### Phase 4 — 功能對等與效能驗證 ✅（實機睡眠恢復等項目仍 NOT TESTED）

- [x] 核對舊版每項監控數值與新後端輸出（同時執行舊版 `gpu_backend.py`、`hardware_info.py`、KSystemStats、`lsblk`、`ip` 與新版 `examples/sample`）

| 指標 | 舊版 | 新版 | 結果 |
|---|---|---|---|
| CPU 使用率 | KSystemStats 同 6 秒平均 6.96% | 6.99% | 一致（單次比較的差異來自取樣時間點） |
| CPU 時脈／溫度 | KSystemStats 範圍內 | 範圍內 | 一致 |
| CPU 型號 | `hardware_info.py`：Xeon E3-1231 v3 @ 3.40GHz | 相同 | 一致 |
| RAM／Swap 已用 | KSystemStats | 多筆數值逐位元組相同 | 一致 |
| RAM 速度 | `hardware_info.py`：1600 MT/s | 1600 MT/s | 一致 |
| GPU 使用率／NVENC／NVDEC | `gpu_backend.py`：10／41／0 | 10／41／0 | 一致 |
| GPU 溫度 | 60 °C | 60 °C | 一致 |
| VRAM 已用 | KSystemStats ≈1502 MiB、`nvidia-smi` 1512 MiB | 修正前 ≈1562 MiB → **修正後 1511 MiB** | 已修正（NVML v1 含 59 MiB 保留記憶體，改用 v2） |
| 硬碟型號 | `lsblk`：`TOSHIBA DT01ABA200V` | 修正前 `TOSHIBA DT01ABA2` → **修正後相同** | 已修正（改讀 udev `ID_MODEL_ENC`） |
| 硬碟清單／容量／順序 | sda、nvme0n1 | 相同 | 一致 |
| IPv4 | `ip`：192.168.50.212 | 相同 | 一致 |
| 網路、uptime | KSystemStats | 範圍內 | 一致 |

- [x] 驗證 Bazzite KDE Wayland 實際啟動、縮放、設定持久化與關閉流程
  - 原生 Wayland（螢幕 2560×1440、**分數縮放 1.3**）：正常啟動與更新；以 KWin 腳本要求縮成 300×200，內容區域維持最小 640×480，
    640×480 下側欄收合、內容可捲動；以預先寫入 750 ms 的設定檔啟動，排程器 `interval_ms=750`（載入成功）；
    以 KWin `closeWindow()`（等同按 ✕）關閉 → `scheduler stopped`、程序結束、沒有殘留的 WebKit 程序
  - 設定頁輸入與存檔：原生 Wayland 下無法模擬鍵盤（ydotool 未啟用），已在 XWayland 下以實際鍵盤操作驗證（Phase 3）
  - XWayland＋NVIDIA 空白畫面：已修正，程式自動設定 `WEBKIT_DISABLE_DMABUF_RENDERER=1`（截圖確認，原生 Wayland 不設定）
  - `NOT TESTED`：執行中改變螢幕縮放比例（會改動使用者螢幕設定）
- [x] 比較閒置與不同更新間隔下的 CPU、RAM、程序數、事件頻率
  - 新版 100／500／10000 ms：見 Phase 3 效能表
  - 新舊版比較（500 ms，舊版預設值；視窗顯示中；各量 60 秒；以程序樹計算，含已結束子程序的 CPU 時間）：

| | 舊版（plasmawindowed＋QtWebEngine＋Python） | 新版（Tauri） |
|---|---|---|
| 程序樹 CPU（單核 %） | 17.52% | 3.38% |
| 60 秒內新建程序（全系統 fork 數，基準 304–307） | 668（約 +361） | 314（約 +7，誤差內） |
| 程序樹 RSS | 734 MiB | 512 MiB |
| 出現過的程序 | plasmawindowed、QtWebEngineProcess、python3 | resourcewatch、WebKitWebProcess、WebKitNetworkProcess |
| 快照推送頻率 | 2 次／秒 | 2 次／秒 |

  - 限制：各只量一次；舊版的 KSystemStats 常駐服務（`ksystemstats`）不在其程序樹內，舊版成本實際上被低估；
    只比較了 500 ms（改動舊版間隔需修改使用者的 Plasma 設定）；兩者的網頁引擎不同（QtWebEngine 與 WebKitGTK）
- [x] 確認外部資料錯誤、裝置離線及權限不足時不會導致程式崩潰
  - `src-tauri/src/robustness.rs`：隨機垃圾輸入（3000 組）、極大計數器、執行中裝置消失與恢復、權限不足、SMBIOS 無權限 → 皆無 panic，數值為缺失
  - 修正：`/proc/stat` 加總改用檢查過的整數運算（異常極大值原本會溢位）
  - `NOT TESTED`：實體熱插拔（拔除 USB 網卡／硬碟）；NVML 執行期回傳錯誤只能以「載入失敗」路徑驗證（容器內執行 IPC 測試即走此路徑）
- [x] 刪除不再使用的 QML／Python 桌面應用路徑 → 經使用者確認刪除整個舊版：`package/`（QML、Python、舊 HTML）、
  只測試舊版的 `tests/`（7 個檔案）、`install.sh`（Plasmoid 安裝）、`packaging/io.github.local.resourcewatch.desktop`（`plasmawindowed` 啟動器）；
  主 `README.md` 改寫為 Tauri 版說明。舊版可由 `main`、`v0.0.5` Tag、`legacy/plasma-qml` 分支取回；`docs/baseline/` 保留 Phase 0 測試紀錄
  - 使用者電腦上已安裝的舊版 Plasmoid（`~/.local/share/plasma/plasmoids/io.github.local.resourcewatch`）與啟動器
    （`~/.local/share/applications/io.github.local.resourcewatch.desktop`）不屬於 repo，未變動
  - 刪除時 Phase 3 的實機睡眠恢復仍為 `NOT TESTED`（使用者知情並選擇刪除）
  - Plasma 桌面小工具在 Phase 5 完成前不可用

**測試證據（2026-11-14）**：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo check` 通過；
`cargo test` 單元測試 70/70、IPC 9/9；`node --check frontend/app.js` 通過；舊版測試仍通過。

### Phase 5 — Plasma Widget（本分支後續或獨立分支）

- [ ] 評估 Rust Core 是否需要獨立背景服務
- [ ] 定義桌面應用與 Widget 共用資料的 IPC 介面
- [ ] 最小 QML Plasma 外殼與緊湊 HTML 介面
- [ ] 驗證兩種介面不會重複啟動高成本 GPU 查詢

---

## 8. 測試與品質標準

| 類別 | 內容 |
|---|---|
| 靜態檢查 | `cargo fmt --check`、`cargo clippy`、`cargo check`；JS 語法與前端建置 |
| 單元測試 | 計數器差值、溢位／重置、更新排程、設定驗證、資料格式化 |
| 整合測試 | Rust → Tauri IPC → HTML、設定保存、手動刷新、程序退出 |
| 回歸測試 | 監控數據、圖表與 v0.0.5 行為相比不得無說明地退化（基線：`docs/baseline/`） |
| 效能測試 | 相同間隔、相同硬體、可比情境下量測；不以少幾個 `if` 推定 CPU 明顯降低 |
| 穩定性 | 硬體與外部資料在邊界驗證，不因刪除防護讓錯誤數值進入 UI |

## 9. Git 工作流程

```bash
cd /var/home/h97m/Projects/ResourceWatch
git branch --show-current   # 預期 refactor/rust-tauri
git status --short
git diff --check
git add <本次里程碑的檔案>    # 不要把使用者未要求的變更一起加入
git diff --cached --stat
git commit -m "build: initialize Rust and Tauri 2 desktop application"   # 範例，完成對應工作後才提交
```

- 每個獨立里程碑提交一次，只提交已實際完成的內容；標題用 `build:`、`feat:`、`refactor:`、`test:`、`docs:`、`perf:`。
- 新版完成實機驗證與功能對等前不合併回 `main`；不改寫 v0.0.5 Tag，不以重構中途結果覆蓋穩定版。
- 不自動 push，不在測試或安裝腳本中推送。

## 10. 本文件維護方式

- 每完成一個 Phase：更新核取方塊、實作位置、測試證據、已知限制。
- 架構、檔案職責、資料流寫入 `Readme_Architecture.md` 對應章節，不寫在這裡。
- 實際方案（Rust／Tauri、監控 API、Plasma Widget）與規劃不同時，先更新本文件設計說明，再改程式與測試。
- 桌面應用穩定後，再更新主 `README.md` 對外說明新架構；保留本文件作為開發歷程。
