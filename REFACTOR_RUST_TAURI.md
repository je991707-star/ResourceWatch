# ResourceWatch Rust + Tauri 重構計畫

分支：`refactor/rust-tauri`。執行規則見 `CLAUDE.md`「Rust + Tauri 重構分支規則」；
架構、檔案職責、JSON 契約與新舊資料來源對照見 `Readme_Architecture.md`。
本文件只記錄各階段核取方塊、測試證據與已知限制。

目標：以 Rust（採集、排程、歷史、設定）+ Tauri 2（桌面視窗）+ HTML/CSS/JS（顯示）取代
「Plasma QML + Python + HTML」桌面應用路徑。**目前尚未完成，舊版仍是唯一可用實作。**

標示：`[x]` 完成、`[ ]` 未完成、`NOT TESTED` 未能驗證、`BLOCKED` 受環境阻擋。

---

## Phase 0 基線與規範 ✅

- [x] 確認分支、HEAD、`git status`
  - 分支 `refactor/rust-tauri`；HEAD `314620e7dc24a5436c0d7b31ec8c5b74e1daebce`（`v0.0.5-2-g314620e`，
    feat: introduce responsive desktop layout and sidebar navigation）
  - 開始時工作樹：`AGENTS.md` 已刪除（未暫存）、`CLAUDE.md` 未追蹤，皆為使用者自行變更，Phase 0 commit 不包含
  - 既有 Tag：v0.0.1–v0.0.5；保留分支：`main`、`legacy/plasma-qml`
- [x] 保存原有測試結果 → `docs/baseline/phase0-legacy-tests.txt`
  - `tests/static_check.sh`：24 項靜態檢查 PASS、排程器契約 PASS、FormatUtils PASS、
    `gpu_backend_test` 5/5、`hardware_info_test` 5/5、`scheduler_contract_test` 3/3，exit 0
  - `application_layout_test.py`：4/4 OK
  - `qmllint`：NOT TESTED（未安裝）
  - 環境：Python 3.15.0、Node v26.13.1
- [x] 確認舊版 HTML、監控指標、JSON 欄位契約
  - HTML 快照契約、Metric 格式、設定鍵與 WebChannel 介面 → `Readme_Architecture.md`
    「`HtmlView.qml`」「`SensorManager.qml`」「`main.xml`」「`configGeneral.qml`」章節
- [x] 建立新舊來源對照表 → `Readme_Architecture.md`「Rust 後端（src-tauri/，尚未建立）」章節

### Phase 0 發現的舊版行為與限制（遷移時需處理）

- NVIDIA：`gpu_backend.py` 每個更新間隔重新啟動 Python 並初始化／關閉 NVML（500 ms 預設即每秒 2 次）。
- 硬碟速率：每個更新間隔啟動一次 `cat /proc/diskstats` 外部程序。
- Uptime：KSystemStats 感測器限速 60 s，HTML 端每秒以本地時鐘內插（符合新契約，Rust 版應保留此作法）。
- IPv4：除手動刷新外，指定介面時另訂閱 KSystemStats `ipv4address`（限速 1 天）；新契約為「啟動取得、之後只手動刷新」。
- GPU 溫度與 VRAM 在 NVIDIA 上也來自 KSystemStats，不是 NVML。
- RAM「已用」定義依 KSystemStats，Rust 版改用 `/proc/meminfo` 時需比對差異。
- `web/app.js` 的 `demo` / `empty` 物件版本字串仍為 `0.0.4`（實際顯示以快照為準）。
- 硬碟計數器重置時該次不輸出速率；初次採樣沒有速率（顯示 `--`）。

### 已知環境限制

- `BLOCKED`（Phase 1 前需處理）：本機未安裝 `cargo` / `rustc`，無法執行 `cargo check`、`cargo tauri` 等。
  Bazzite 為 immutable 系統，需選擇安裝方式（例如 `rustup` 安裝到使用者目錄、toolbox／distrobox、或 Homebrew），
  另需 Tauri 2 Linux 依賴（WebKitGTK 4.1、libsoup3 等）。

---

## Phase 1 Tauri 骨架

- [ ] 安裝 Rust toolchain 與 Tauri 2 依賴（見上方 BLOCKED）
- [ ] 初始化 Rust + Tauri 2 專案（`src-tauri/`）
- [ ] 移植 HTML 總覽與設定頁（不照搬 QtWebChannel 綁定）
- [ ] 建立前後端通訊（Commands / Events，依 JSON 契約）
- [ ] 視窗：可縮放、最小寬高、Wayland 縮放
- [ ] 使用 mock 資料，**不宣稱已完成硬體監控**

## Phase 2 Rust 硬體監控

- [ ] CPU、RAM／SWAP
- [ ] 硬碟清單與讀寫
- [ ] 網路流量與 IPv4
- [ ] NVIDIA NVML（使用率、NVENC／NVDEC、溫度、VRAM）
- [ ] AMD／Intel（記錄不支援項目）
- [ ] 啟動資訊（CPU 型號、GPU 名稱、RAM 速度）
- [ ] 與舊版或系統工具交叉比對

## Phase 3 排程、歷史與設定

- [ ] 統一排程器、歷史緩衝、快照事件
- [ ] 設定讀寫、驗證與預設值
- [ ] 手動刷新按鈕（IPv4、硬碟、硬體資訊）
- [ ] 測試 100／500／10000 ms 與睡眠恢復
- [ ] 確認沒有每次更新重複啟動 Python 的路徑

## Phase 4 功能對等與效能驗證

- [ ] 逐項核對舊版數值
- [ ] Bazzite KDE Wayland：啟動、縮放、設定持久化、關閉
- [ ] 不同更新間隔的資源占用比較
- [ ] 錯誤與裝置離線不崩潰
- [ ] 成功後才刪除不再使用的 QML／Python 桌面應用路徑

## Phase 5 Plasma Widget（後續或獨立分支）

- [ ] 評估是否需要獨立背景服務
- [ ] 定義共用資料的 IPC
- [ ] 最小 QML 外殼
- [ ] 驗證不會重複啟動高成本 GPU 查詢
