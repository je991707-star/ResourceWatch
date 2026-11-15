# ResourceWatch — v0.2.0 改版計畫（macOS 26 霧面玻璃風格，改用 Slint）

| 項目 | 內容 |
|---|---|
| 目標版本 | `0.2.0`（全部 Phase 完成後改版本號並打 tag `v0.2.0`） |
| 介面技術 | Slint（Rust＋`.slint` 標記語言），取代 Tauri／WebKit＋HTML |
| 設計方向 | Apple macOS 26 分層玻璃材質；參考 iStat Menus、iStatistica Pro 的資訊密度與分類方式，但版面、圖示、配色、元件自訂，不照抄 |
| 範圍 | 除「桌面小工具」不做之外，介面全部重構；後端資料收集沿用現有 Rust |
| 相關文件 | 架構：`Readme_Architecture.md`；上一輪介面改版：`UI_REDESIGN_PLAN.md`；執行規則：`CLAUDE.md` |

標示：`[x]` 完成、`[ ]` 未完成、`NOT TESTED` 未能驗證。改版可分次進行；每個 Phase 完成後：更新 `Readme_Architecture.md` 對應章節 → 編譯／測試 → commit。

## 為什麼改
- WebKitGTK 在 NVIDIA＋Wayland 上，透明視窗必須走 DMA-BUF 繪圖。結果是開毛玻璃時，縮放會卡（內容跟不上邊框），拖動時內容有時會消失。
- WebKit 本身佔用高：3 個程序，約 150–250 MB。
- Slint 是單一程序的原生繪圖，沒有網頁引擎，可以同時做到真透明毛玻璃和輕量。

---

## 設計原則

### 視覺語言（macOS 26 分層玻璃）

**三層材質**

| 層 | 材質 |
|---|---|
| 1 視窗底層 | 透出並模糊桌面的玻璃，標題列區域也包含在內 |
| 2 側欄 | 浮起的圓角玻璃面板，與視窗邊緣留間距 |
| 3 內容卡片 | 比底層亮一階的半透明霧面，數字需維持清楚可讀 |

**細節**
- 圓角：視窗 16 px、側欄 14 px、卡片 14 px、膠囊元件全圓角。
- 1 px 高光內邊：上緣較亮、下緣較暗。
- 柔和的大範圍陰影。
- 文字用高對比的「vibrancy」前景色，次要文字半透明。

**其他**
- 深色／淺色：預設跟隨系統，可手動切換。兩種模式都要定義完整的 token。
- 指標色沿用目前的色相（CPU 青、GPU 紫、RAM 藍、上傳綠、下載藍、讀取紅、寫入藍），再依玻璃底色微調亮度與飽和度。
- 動態：只在數值變動時做短暫過渡；更新間隔 < 1 秒時不做動畫，沿用目前 `data-motion` 的規則。

### 參考與區隔
- **參考 iStat 系列**：高資訊密度、小型即時圖、儀表板＋分類詳細頁的結構。
- **刻意不同**：
  - 卡片版型以 SUMMARY 為範本。
  - 圖示用 Font Awesome Free（CC BY 4.0，保留授權註記）。
  - 配色、圓角、圖表樣式（2 px 長條＋1 px 間隔、上下鏡像）都是自訂。
  - 不複製任何截圖中的排版或素材。

### 輕量目標（驗收門檻）

| 項目 | 目標 |
|---|---|
| 記憶體（RSS） | ≤ 60 MB，單一程序 |
| 閒置 CPU | ≤ 1%（單核 %）：預設 500 ms 更新、儀表板顯示中 |
| 最小化／隱藏 | 不更新介面資料，只保留後端取樣以延續歷史 |
| 拖動、縮放 | 不卡頓，不出現空白或內容消失 |

### 沿用的規則
- 缺失值顯示 `--`，不轉成 0；硬體不存在時隱藏對應卡片或列。
- 不啟動外部程序、不連外網。
- `RESOURCEWATCH_MOCK=1` 模擬模式。
- 新快照欄位要同步更新：`snapshot.rs`、`mock.rs`、契約測試、`robustness.rs`、Readme 欄位表。

---

## 技術架構

### Cargo workspace
- **`crates/core`**（函式庫）：從 `src-tauri/src` 抽出不依賴 Tauri 的模組，包括：
  - `collectors/`、`monitor`、`history`、`settings`、`settings_store`
  - `scheduler`、`snapshot`、`format`、`mock`、`source`，以及 `robustness` 測試

  目前只有 `lib.rs`、`blur.rs`、`tests/ipc.rs` 依賴 Tauri。
- **`crates/app`**（執行檔）：Slint 介面＋視窗管理。
  - 執行檔名稱維持 `resourcewatch`（Phase 2–6 與 Tauri 版並存期間暫名 `resourcewatch-slint`），Wayland app_id 也是 `resourcewatch`，`install.sh` 的桌面檔不用改名。

### 資料流
- 沿用 `scheduler.rs` 的 `Scheduler::start(interval, tick)`。
- tick 在背景執行緒取樣，再用 `slint::invoke_from_event_loop` 把 Snapshot 寫進 Slint 的 global 屬性與 model。
- 只在數值變動時設定屬性。Slint 只重畫變動的區域，這是達成閒置 CPU 目標的關鍵。
- 程序清單只在「應用程式」頁或儀表板 Top processes 顯示時才取樣，沿用目前的 `watchProcesses` 規則。

### 視窗與玻璃
- **視窗**：用 winit 後端，`no-frame`＋透明背景。
- **模糊**：沿用現有的 `blur.rs`。
  - 優先用 `ext_background_effect_manager_v1`，退回 `org_kde_kwin_blur_manager`。
  - 輸入從 GTK 改成 raw-window-handle 取得的 `wl_display`／`wl_surface`。
- **圓角品質（軟體渲染器）**：不用 `Rectangle` 的 `border-width` 畫圓角邊框（1 px 邊框在圓弧上會重複疊色出現亮點）；玻璃面板＝無邊框圓角 `Rectangle` 底色＋1 px 直線 `Rectangle`＋四角小 `Path` 弧線。`Path` 每次重繪都重新點陣化，不可做成整面大小。
- **模糊區域**：只模糊視窗的圓角矩形（wl_region 以每列 1 px 的長條逼近圓角），每次縮放、最大化／還原時重設；最大化時無圓角。
- **軟體渲染器的透明**：softbuffer 0.4 在 Wayland 固定用 `Xrgb8888`（丟 alpha），需用 `[patch.crates-io]` 套修補版改成 `Argb8888`，否則視窗不透明、看不到模糊。
- **退回**：以下情況自動改成不透明底色，介面不壞：
  - X11 或 XWayland。
  - 合成器沒有模糊協定。
  - 使用者關閉「毛玻璃」。
- **即時切換**：不再需要 `WEBKIT_DISABLE_DMABUF_RENDERER`，所以「毛玻璃」開關立即生效，不必重新啟動。
- **自訂標題列**：
  - 和側欄、內容同一片玻璃，與工具列合一。
  - 右上角保留最小化／最大化／關閉，順序和圖示語意與 KDE 一致；圖示取自系統圖示主題（`window-minimize` 等，依 `kdeglobals` 與 `Inherits=`），染成前景色，找不到時用內建符號。
  - 拖動用 Slint 內建 `WindowMoveArea`（內部呼叫 winit `drag_window()`）；邊緣與角落縮放用 Window 的 `resize-border-width`（內部呼叫 `drag_resize_window()`）；雙擊切換最大化。
  - `slint::set_xdg_app_id("resourcewatch")` 必須在 `BackendSelector::select()` 之後、建立視窗之前呼叫。

### 繪圖
- 環形圖、堆疊條、長條圖都用 Slint `Path`。
- 長條路徑在 Rust 端產生，沿用現有規則：
  - 儀表板：固定 2 px 長條＋1 px 間隔，靠右排列。
  - 詳細頁：取最後 `historyPoints` 點，平均分布。
  - 成對序列：以中線上下鏡像、共用縱軸。
- 後端保留的歷史長度沿用 `history::KEPT_POINTS`（240）。

### 渲染器
- 預設 Slint 軟體渲染器（`renderer-software`）：Phase 0 實測 femtovg 在 NVIDIA 上會載入 NVIDIA GL 驅動與 Mesa／LLVM，RSS 167 MB，超過 60 MB 門檻；軟體渲染器 24 MB。
- 保留 `SLINT_RENDERER` 環境變數可切換 femtovg 比較；Skia 未評估（軟體渲染器已達標）。

### 設定
- 原本存在 `localStorage` 的顯示偏好，改進 Rust `Settings`，存在同一個 `config.json`：
  - `theme`：`system`／`dark`／`light`
  - `tempUnit`：`c`／`f`
  - `sidebarCollapsed`
- 舊設定檔缺這些欄位時用預設值，沿用 `settings_store` 的容錯規則。

### 授權
- Slint 採 Royalty-free 2.0 授權，需在「關於」頁放 Slint 標示。
- 專案本身維持 MIT，Font Awesome 註記保留。

---

## Phase 0：技術驗證 `[x]`
目標：先證明 Slint 在這台機器（KWin Wayland＋NVIDIA 580）上做得到。任何一項不達標就停下來回報，再決定替代方案。
驗證程式：`spikes/slint-poc/`（Slint 1.18.1，說明見 `Readme_Architecture.md`）。

- [x] 做一個最小 Slint 視窗（透明、無邊框、有一張卡片與一張每 500 ms 更新的長條圖），確認：
  - [x] 透明＋背景模糊有效。軟體渲染器需搭配修補過的 softbuffer（Wayland 改用 `Argb8888`，原版 `Xrgb8888` 會讓視窗不透明）；修補後全螢幕截圖確認視窗後方內容被模糊。模糊區域需設成與視窗相同的圓角矩形（`blur.rs` 的 `set_shape`，每次縮放重設），否則圓角外也被模糊，看起來像鋸齒；修正後截圖確認圓角外不再有模糊雜訊。
  - [x] 縮放、拖動時沒有空白，內容不會消失（使用者確認）。圖表原本只在 500 ms 取樣時重算，縮放時跟不上；改成圖表尺寸一變就重算，KWin 腳本放大視窗後 0.3 秒截圖確認長條已貼齊新右緣。
  - [x] 圓角無鋸齒：視窗與卡片改用 `GlassPanel`（見「視窗與玻璃」），快照與實機截圖確認圓弧平滑、無亮點；閒置 CPU 0.45%、RSS 24 MB。
  - [x] 自訂標題列可以拖動、邊緣可以縮放，右上角三顆按鈕正常（使用者確認）。
  - [x] Wayland app_id 為 `resourcewatch`，KWin 顯示正確的圖示（KWin 腳本讀到 `desktopFileName=resourcewatch`，對應 `~/.local/share/applications/resourcewatch.desktop`）。
- [x] 量測 RSS 與閒置 CPU（`/proc/<pid>/status`、`/proc/<pid>/stat`），和目前的 Tauri 版比較，數據記錄在本文件。
- [x] 確認可以用 Slint 軟體渲染器把畫面輸出成 PNG：`slint-poc --snapshot out.png`（無視窗 `MinimalSoftwareWindow`＋`take_snapshot()`）。

### Phase 0 量測（2026-11-15，KWin Wayland、NVIDIA 580.196、release 建置）
視窗 520×340、500 ms 更新、開毛玻璃；啟動 3 秒後取 RSS，CPU 為 20 秒內 utime+stime 換算的單核 %。

| 版本 | 程序數 | RSS（合計） | 閒置 CPU |
|---|---|---|---|
| Tauri 0.1.0（`~/.local/bin/resourcewatch`，儀表板） | 3（主程式＋WebKitNetwork＋WebKitWeb） | 506 MB | 8.7% |
| Slint PoC，femtovg | 1 | 167 MB（RssAnon 70／RssFile 102；NVIDIA EGL＋Mesa LLVM） | 0.4% |
| Slint PoC，software（採用） | 1 | 24 MB（修補 softbuffer、開透明後 23 MB；`GlassPanel` 後 24 MB） | 0.6%（修補後 0.53%；`GlassPanel` 後 0.45%） |

注意：PoC 只有一張卡片，完整儀表板的 RSS／CPU 會更高，Phase 7 依「輕量目標」重測。

## Phase 1：抽出 `crates/core` `[x]`
- [x] 建立 workspace，把不依賴 Tauri 的模組與測試搬到 `crates/core`。
  - 根目錄 `Cargo.toml`（成員 `crates/core`、`src-tauri`，`exclude = ["spikes"]`）；release profile、`Cargo.lock`、`target/` 移到根目錄。
  - `git mv` 搬移 `collectors/`、`format`、`history`、`mock`、`monitor`、`robustness`、`scheduler`、`settings`、`settings_store`、`snapshot`、`source` 與 `examples/sample.rs`；`sample_live` 也移到 core。
- [x] Tauri 版改成依賴 core，行為不變，現有測試全部通過：搬移前後皆 96 個測試（core 83、Tauri lib 2、`tests/ipc.rs` 11），fmt／clippy `-D warnings` 通過；安裝後實際啟動正常，`sample` 範例輸出快照正常。
- [x] `install.sh` 暫時照舊建置 Tauri 版，確保改版期間仍可安裝使用：只改執行檔來源為根目錄 `target/release/resourcewatch`，實際執行 `./install.sh` 成功。

## Phase 2：Slint 外殼 `[x]`
實作：`crates/app`（說明見 `Readme_Architecture.md`）。執行檔暫名 `resourcewatch-slint`（Tauri 版仍叫 `resourcewatch`、共用 `target/`），Phase 7 改回 `resourcewatch`。
- [x] 視窗、三層玻璃、自訂標題列。第 1 層視窗 `GlassPanel`＋合成器模糊（圓角模糊區域、隨縮放重設）、第 2 層浮起側欄、第 3 層內容卡片；1 px 高光上亮下暗；標題列可拖動、雙擊最大化，邊緣縮放沿用 `resize-border-width`。
  最小化／最大化直接用 Window 內建的 `minimized`／`maximized`（與實際視窗雙向同步）。實機截圖確認模糊與三層效果；KWin 讀到 `desktopFileName=resourcewatch`。
- [x] 右上角按鈕圖示跟隨系統風格（Phase 0 延後項目）：KWin 視窗裝飾主題不是圖示主題，無法讀取；改以 1 px `Path` 自繪 KDE 預設 Breeze 樣式（細橫線、方框、重疊方框、叉），顏色跟隨深淺色 token，取代 PoC 讀 Papirus 圖示的做法。
- [x] 側欄導覽：儀表板、儲存裝置、網路、感測器、應用程式、設定；可收合（56／208 px，160 ms 過渡，狀態存入 `sidebarCollapsed`）。各頁目前為佔位內容，標示實作的 Phase。
- [x] 深淺色 token（Slint `global Theme`），跟隨系統或手動切換：`system` 取 `Palette.color-scheme`（XDG portal；實機啟動時正確判定深色，系統執行中切換未實測）；側欄主題鈕循環 跟隨系統 → 深色 → 淺色，存入 `theme`。截圖確認深色與淺色；實際點擊切換與存檔待使用者操作確認（存檔邏輯有單元測試）。
- [x] 頂部主機資訊列（主機名稱、運作時間、機型、CPU 型號）、模擬模式提示：與 `hostname`、`uptime`、DMI、`/proc/cpuinfo` 比對一致；`RESOURCEWATCH_MOCK=1` 時顯示「模擬資料 · 非實機」。
- [x] 設定讀寫接上 core 的 `settings_store`：與 Tauri 版共用 `~/.config/io.github.local.resourcewatch/config.json`；core `Settings` 新增 `theme`／`tempUnit`／`sidebarCollapsed`（含驗證、容錯、單元測試，IPC 測試同步更新）；沒有變更時不寫檔。

### Phase 2 量測（2026-11-15，KWin Wayland、縮放 1.3、release 建置、預設 500 ms、儀表板佔位頁）
| 項目 | 結果 |
|---|---|
| RSS | 56 MB（RssAnon 18／RssFile 27／RssShmem 13；Shmem 為 1586×1027 的 Wayland 畫面緩衝區，隨視窗尺寸增加） |
| 閒置 CPU | 0.55% |

注意：RSS 已接近 60 MB 門檻，Phase 4 儀表板完成後須重測。

## Phase 3：元件庫 `[ ]`
- [ ] GlassCard（標題列可帶膠囊選單或分段控制）
- [ ] RingGauge（環內可放三行文字）
- [ ] BarChart：單向與上下鏡像，固定像素長條
- [ ] StackBar、Legend（置中直排）、KV 數值格
- [ ] Pill 下拉選單（NIC、GPU）、SegmentedControl（°C／°F、主題）
- [ ] 可排序表格（程序清單）

## Phase 4：儀表板 `[ ]`
- [ ] 卡片：SUMMARY、Network、Sensors、Graphics、Processor、Disk I/O、Memory、Disks、Battery。
  - 內容與目前一致，版型以 SUMMARY 為範本。
  - 沒有電池時隱藏 Battery。
- [ ] 1–3 欄自適應，瀑布流排版：卡片依固定順序分配到目前最短的一欄。
  - 欄數依可用寬度與卡片最小寬度決定。
  - 與舊版的差異：Slint 量不到文字是否被截斷，所以改用最小寬度門檻，不再依 GRAPHICS 文字截斷判斷。
- [ ] 卡片內的「Show more…」連到對應詳細頁。

## Phase 5：詳細頁 `[ ]`
- [ ] 儲存裝置：硬碟清單（容量條、讀寫速率）＋磁碟 I/O 大圖。
- [ ] 網路：介面選單、IPv4、MAC、閘道、上傳／下載速率＋大圖。
- [ ] 感測器：CPU／GPU 溫度、全部 hwmon 感測器（依晶片分組）、°C／°F。
- [ ] 應用程式：程序表（名稱、PID、CPU、記憶體），可排序、可搜尋。
- [ ] 不做「桌面小工具」頁。

## Phase 6：設定頁 `[ ]`
- [ ] 更新間隔、歷史點數、GPU、NIC（沿用現有範圍與驗證）。
- [ ] 外觀：毛玻璃開關（即時生效）、主題、溫度單位。
- [ ] 關於：版本、Slint 標示、Font Awesome 授權。

## Phase 7：收尾與 0.2.0 `[ ]`
- [ ] 移除 Tauri 相關程式（`src-tauri` 的 Tauri 部分、`tauri.conf.json`、`blur.rs` 的 GTK 部分）與 `frontend/`。
- [ ] `install.sh` 改成建置 `crates/app`；執行檔、桌面檔、圖示的安裝路徑不變。
- [ ] 依「輕量目標」逐項實測，數據記錄在本文件。
- [ ] `Readme_Architecture.md`、`README.md` 全面改寫為 Slint 架構。
- [ ] 版本：各 `Cargo.toml` 的 `version` 改成 `0.2.0` → commit → 打本地 tag `v0.2.0`（沿用 `v0.1.0` 格式；不 push，除非另外要求）。
- [ ] 回頭檢查各 Phase 是否確實完成（打 ✅ 前先重新讀取本文件，確認有無新增項目）。

---

## 驗證方式（每個 Phase）
- 在 distrobox `resourcewatch-dev` 內執行：
  - `cargo fmt --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test`
- 用軟體渲染器輸出截圖：模擬模式與實機各一張，深色和淺色都要。
- 新數值和 `free`、`lsblk`、`df`、`sensors`、`ip route` 比對。
- Phase 0 與 Phase 7 量 RSS、CPU，並請使用者實際拖動、縮放視窗確認。
