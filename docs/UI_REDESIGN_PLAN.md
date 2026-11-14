# ResourceWatch — 介面改版計畫（iStatistica 風格）

| 項目 | 內容 |
|---|---|
| 分支 | `refactor/rust-tauri` |
| 參考 | iStatistica 儀表板：左側毛玻璃導覽、右側卡片顯示區 |
| 相關文件 | 架構與 JSON 契約：`Readme_Architecture.md`；重構進度：`REFACTOR_RUST_TAURI.md`；執行規則：`CLAUDE.md` |

標示：`[x]` 完成、`[ ]` 未完成、`NOT TESTED` 未能驗證。每個 Phase 完成後：更新 `Readme_Architecture.md` 對應章節 → 編譯／測試 → commit。

## 共同原則
- 前端維持無打包工具、無 npm 依賴（HTML／CSS／JS＋手寫 SVG）；CSP `style-src 'self'`，動態樣式一律用 CSSOM。
- 新快照欄位：同步更新 `snapshot.rs`、`mock.rs`、`tests/ipc.rs` 契約測試、`robustness.rs` 解析器容錯、Readme 欄位表。
- 缺失值一律 `null`／`"--"`，不轉成 0；硬體不存在（例如無電池、無風扇）時卡片或列隱藏。
- 不啟動外部程序、不連外網（因此不做公網 IP）。
- 驗證：distrobox `resourcewatch-dev` 內 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`；
  `RESOURCEWATCH_MOCK=1` 與實機各跑一次截圖；新數值與 `free`、`lsblk`、`df`、`sensors`、`ip route` 比對。

---

## Phase 1：純前端儀表板 `[x]`
- [x] 色彩 token＋淺色／深色主題切換（`localStorage`）
- [x] 側欄：儀表板／儲存裝置／網路／感測器／桌面小工具／設定
- [x] 頂部主機資訊列（CPU、運作時間、GPU、RAM）
- [x] 卡片：摘要（三層環形圖）、圖形處理器、處理器、記憶體、磁碟、網路雙線圖、感測器
- [x] 儲存裝置／網路／感測器詳細頁骨架
- [ ] NOT TESTED：980 px 以下窄版排版只寫了 CSS，未實際縮放視窗檢查

---

## Phase 2：視窗外觀——左側毛玻璃、右側顯示區

目標：側欄半透明並模糊桌面背景；右側顯示區（頂部資訊列＋卡片）維持不透明底色，確保數字可讀。

### 2.1 透明視窗（`tauri.conf.json`、`styles.css`、`application.css`）
- [ ] 視窗 `main` 設 `"transparent": true`（保留系統標題列）。
- [ ] `html, body` 背景改透明；`.app-main` 使用不透明 `--bg`；`.app-sidebar` 使用半透明 token
      `--sidebar-glass`（深色約 `rgba(58,42,54,.55)`、淺色約 `rgba(240,230,236,.55)`），加細邊線分隔。
- [ ] 確認 NVIDIA＋X11 時既有的 `WEBKIT_DISABLE_DMABUF_RENDERER` 解法下透明仍正常（`lib.rs` `prepare_environment()`）。

### 2.2 KWin 模糊（新檔 `src-tauri/src/blur.rs`，在 `lib.rs` 的 `setup` 呼叫）
- [ ] X11／XWayland：對視窗設定 `_KDE_NET_WM_BLUR_BEHIND_REGION`（CARDINAL 陣列，空值＝整個視窗；
      或只給側欄矩形，視窗縮放時更新）。取得 X window：Tauri `WebviewWindow::gtk_window()` → GDK X11 window XID。
- [ ] Wayland 原生：`org_kde_kwin_blur_manager` 協定（`wayland-client`＋`wayland-protocols-plasma`），
      對 GTK 視窗的 `wl_surface` 建立 blur 並 `commit`。
- [ ] 偵測：非 KDE（`XDG_CURRENT_DESKTOP` 不含 KDE）或協定不存在 → 不設定模糊，前端收到 `glass: false`，
      側欄改用不透明 `--sidebar`（以 `<html data-glass>` 切換）。
- [ ] 設定頁新增「側欄毛玻璃」開關（`Settings.sidebarGlass`，預設開；關閉時同上退回不透明）。
- 新依賴只在確定需要時加入（Wayland 部分），並在 Readme 依賴清單說明原因。

### 2.3 頂部資訊列對齊參考圖
- [ ] 左：電腦圖示＋主機名稱＋運作時間；右：機型（DMI `product_name`）＋CPU 型號（需 Phase 3.1 資料）。
- [ ] 側欄頂部加收合按鈕（只剩圖示，偏好存 `localStorage`）。

---

## Phase 3：後端擴充資料（每項一個 commit）

| # | 項目 | 來源 | 快照欄位（暫定） | 用在 |
|---|---|---|---|---|
| 3.1 | 主機名稱、機型 | `/proc/sys/kernel/hostname`；`/sys/class/dmi/id/product_name`（一般使用者可讀） | `hostname`, `machineModel` | 頂部資訊列 |
| 3.2 | CPU 核心／執行緒數 | `/proc/cpuinfo`（`physical id`＋`core id` 去重）、邏輯 CPU 數 | `cpuCores`, `cpuThreads` | 摘要「處理器」 |
| 3.3 | 每核心使用率 | `/proc/stat` `cpuN` 行，重用 `collectors/cpu.rs` `parse_stat_line`／`usage_between` | `cpuPerCore: (number\|null)[]` | 處理器卡片左側小長條 |
| 3.4 | 記憶體細分＋壓力 | `/proc/meminfo`：應用程式＝Total−Free−Buffers−Cached−SReclaimable、快取＝Buffers+Cached+SReclaimable、可用＝MemFree；`/proc/pressure/memory` `some avg10` | `ramBreakdown {app, cache, free}`, `history.memPressure` | 記憶體卡片多段堆疊條＋Pressure 圖、摘要記憶體環 |
| 3.5 | 磁碟 I/O 歷史 | 現有 `/proc/diskstats` 速率加總 | `history.diskRead`, `history.diskWrite` | 新「磁碟 I/O」卡片雙線圖 |
| 3.6 | 硬碟容量 | `/proc/self/mounts` 對應分割區所屬硬碟 → `statvfs`（`libc`）；啟動、手動刷新與每 10 秒讀一次 | `disks[].used/total/percent` | 磁碟卡片容量條、摘要卡片硬碟切換（‹ ›） |
| 3.7 | 感測器清單＋風扇 | 掃描所有 hwmon `temp*_input`／`fan*_input`＋`*_label`（擴充 `collectors/cpu.rs` 既有掃描） | `sensors: {chip, label, kind: "temp"\|"fan", value}[]` | 感測器卡片（前 10 筆＋顯示更多）、°C／°F 切換（前端） |
| 3.8 | 網路細節＋介面清單 | `/sys/class/net/<if>/address`（MAC）、`/proc/net/route`（預設閘道）、`getifaddrs`（本機 IP）、實體介面清單 | `netInterfaceName`, `netMac`, `netGateway`, `netInterfaces[]` | 網路卡片與網路頁；介面下拉選單寫入既有 `netInterface` 設定 |
| 3.9 | GPU 清單 | `collectors/gpu` 既有 `discover` 結果 | `gpus: {index, name, vendor}[]` | 圖形處理器卡片下拉選單寫入既有 `gpuIndex` 設定 |
| 3.10 | 電池 | `/sys/class/power_supply/BAT*`：`capacity`、`status`、`cycle_count`、`energy_*`／`charge_*`（full、full_design、now）、剩餘時間估算 | `battery: {...} \| null` | 電池卡片（無電池時隱藏；本機為桌機，以假 `/sys` 測試） |

不做：公網 IP（需連外）、藍牙（需 BlueZ D-Bus）、螢幕解析度（需合成器 API）。

---

## Phase 4：詳細頁充實
- [ ] 儲存裝置頁：每顆硬碟容量條、讀寫速率、磁碟 I/O 大圖。
- [ ] 網路頁：介面、本機 IP、MAC、閘道、上下行大圖。
- [ ] 感測器頁：全部溫度與風扇，依晶片分組，°C／°F 切換。
- [ ] 儀表板卡片「顯示更多…」全部連到對應頁。

---

## Phase 5：程序清單（應用程式頁）
- [ ] Rust `collectors/process.rs`：讀 `/proc/[pid]/stat`（utime+stime 差值算 CPU%）、`/proc/[pid]/status`（`VmRSS`）、`comm`／`cmdline` 名稱；
      程序消失、權限不足時略過。
- [ ] 只在處理器／記憶體卡片或應用程式頁可見時取樣，間隔約 2 秒（獨立於 `updateInterval`，避免每週期掃描所有 PID）；
      新 Command `set_process_watch(enabled)` 或併入 `snapshot` 的可選欄位 `processes`，以實作時成本較低者為準。
- [ ] 處理器卡片顯示 CPU 前 5 名、記憶體卡片顯示記憶體前 5 名（「顯示應用程式…」展開）；側欄新增「應用程式」頁，可依 CPU／記憶體排序。
- [ ] 效能驗證：程序數 500+ 時單次取樣耗時與 `RW_STATS` 排程延遲。
