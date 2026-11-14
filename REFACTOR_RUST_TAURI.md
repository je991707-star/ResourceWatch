# ResourceWatch — Rust + Tauri 重構計畫

| 項目 | 內容 |
|---|---|
| 分支 | `refactor/rust-tauri` |
| 狀態 | 架構遷移規劃／逐步實作。**舊版 Plasma QML + Python + HTML 仍是唯一可用實作** |
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
  - → `Readme_Architecture.md`「Rust 後端（src-tauri/，尚未建立）」章節

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

- `BLOCKED`：本機未安裝 `cargo`／`rustc`。Bazzite 為 immutable 系統，需選擇安裝方式
  （`rustup` 裝在使用者目錄、toolbox／distrobox、或 Homebrew），另需 Tauri 2 Linux 依賴（WebKitGTK 4.1、libsoup3 等）。

### Phase 1 — Tauri 桌面應用骨架

- [ ] 安裝 Rust toolchain 與 Tauri 2 依賴（見 Phase 0 BLOCKED）
- [ ] 初始化 Rust + Tauri 2 專案
- [ ] 移植 HTML 總覽與設定頁，建立前端與 Rust 的基本通訊
- [ ] 設定正常視窗尺寸、最小尺寸及 Wayland 下的縮放行為
- [ ] 暫以 mock 資料呈現；**不宣稱已完成硬體監控**

### Phase 2 — Rust 硬體監控

- [ ] CPU、RAM／SWAP
- [ ] 硬碟 I/O、裝置清單
- [ ] 網路流量、IPv4
- [ ] NVIDIA NVML，再逐步擴充 AMD／Intel
- [ ] CPU 型號、GPU 名稱、RAM 速度等啟動資訊
- [ ] 與舊版或系統工具交叉比對，記錄不支援項目

### Phase 3 — 排程、歷史與設定

- [ ] 共用 `updateInterval` 的統一排程器
- [ ] 歷史資料緩衝與前端快照事件
- [ ] 設定讀寫、驗證、恢復預設值及手動刷新按鈕
- [ ] 測試 100／500／10000 ms 更新頻率與睡眠恢復行為
- [ ] 確認沒有每次更新重複啟動 Python 程序的路徑

### Phase 4 — 功能對等與效能驗證

- [ ] 核對舊版每項監控數值與新後端輸出
- [ ] Bazzite KDE Wayland：實際啟動、縮放、設定持久化、關閉流程
- [ ] 比較閒置與不同更新間隔下的 CPU、RAM、程序數、事件頻率
- [ ] 外部資料錯誤、裝置離線、權限不足時不崩潰
- [ ] 成功後才刪除不再使用的 QML／Python 桌面應用路徑

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
