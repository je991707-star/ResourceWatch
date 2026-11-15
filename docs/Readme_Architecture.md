# ResourceWatch 架構說明

本文件依「程式檔／目錄」分章，描述每個檔案的職責、資料流與對外契約。
修改任何程式時，請把變更寫入對應章節。重構目標與階段進度見 `REFACTOR_RUST_TAURI.md`。介面改版計畫見 `UI_REDESIGN_PLAN.md`；v0.2.0 改版計畫（改用 Slint）見 `REDESIGN_V0.2.md`。所有文件（除根目錄 `README.md`、`CLAUDE.md`）集中在 `docs/`；安裝見根目錄 `install.sh`。

> 目前狀態：桌面應用為 Rust + Tauri 2（`src-tauri/` + `frontend/`），根目錄是 Cargo workspace：不依賴介面工具組的資料收集、排程、設定在 `crates/core`，
> Tauri 版與改版中的 Slint 版（`crates/app`，v0.2.0）都依賴它。由 Rust collectors 讀取實際硬體資料、單一統一排程器驅動，
> 設定儲存於使用者設定目錄。舊版「Plasma QML + Python + HTML」（v0.0.5）已從此分支移除，可由 `main`、`v0.0.5` Tag 或
> `legacy/plasma-qml` 分支取回。Plasma 桌面小工具屬 Phase 5，尚未實作。

```text
src-tauri/src/lib.rs ── AppState{settings(+SettingsStore → config.json), source, latest, scheduler}
        │   （以下 scheduler／source／collectors／settings_store 皆在 crates/core）
        │   scheduler.rs: rw-scheduler 執行緒（唯一）每個 updateInterval 一個週期：
        │     source.sample(now) → emit "snapshot" → latest
        │   source: Source::Live(Monitor) ─ collectors/{cpu,memory,system,disk,network,sensors,battery,process,gpu(+nvml)}
        │           Source::Mock(MockSource)（RESOURCEWATCH_MOCK=1）
        ▲ commands: get_snapshot / get_settings / get_settings_info / update_settings / reset_settings /
        │           refresh_devices / get_scheduler_stats / get_window_info / get_processes（不經排程器，前端輪詢）
        │   blur.rs: 主視窗於 setup 建立（Wayland 才透明）→ 合成器模糊（側欄毛玻璃）
frontend/index.html + app.js（window.__TAURI__ invoke / listen）
```

---

## install.sh

使用者空間安裝／更新／解除安裝，不需要 sudo、不分層、不重開機（適用 Bazzite 等不可變系統）。
- `install`（預設）：能進入 distrobox 容器 `resourcewatch-dev`（可用 `RESOURCEWATCH_BOX` 改名；以 `distrobox enter … -- true` 探測，`distrobox list` 偶爾漏列）時在容器內執行 `cargo tauri build --no-bundle`，
  否則用主機的 cargo-tauri；先 `pkill -x resourcewatch` 再把 workspace 根目錄的 `target/release/resourcewatch` 安裝到 `~/.local/bin/resourcewatch`、圖示到
  `~/.local/share/icons/hicolor/128x128/apps/io.github.local.resourcewatch.png`、啟動器到
  `~/.local/share/applications/resourcewatch.desktop`（檔名必須等於視窗的 Wayland app_id＝執行檔名 `resourcewatch`，KWin 才會在標題列與工作列顯示應用圖示；同時刪除舊版的 `io.github.local.resourcewatch.desktop` 啟動器）。
  偵測到舊版 Plasma Widget 仍安裝時只印出移除指令，不自動移除。
- `uninstall`：刪除上述三個檔案，保留 `~/.config/io.github.local.resourcewatch` 設定。

## frontend/

Tauri 桌面應用的前端（靜態檔，無打包工具、無 npm 依賴；`tauri.conf.json` 的 `build.frontendDist` 指向此目錄）。只負責顯示，不存取硬體、檔案或網路。

- `index.html`：側欄品牌列為應用圖示 `app-icon.png`＋名稱；左側導覽「儀表板」「儲存裝置」「網路」「感測器」「應用程式」「桌面小工具」，頂部收合按鈕，底部「設定」與淺色／深色主題切換按鈕。
  頂部主機資訊列：電腦圖示＋主機名稱＋運作時間；機型＋CPU 型號（窄視窗時隱藏）；即時／模擬狀態。
  儀表板卡片文字為英文、標題大寫（其餘頁面仍為中文；與詳細頁共用、由 JS 產生的文字如硬碟列「未掛載」仍為中文）。
  SUMMARY（第一張）：CPU／GPU／RAM 三個環形圖（`data-color` 為 `sum-cpu`／`sum-gpu`／`sum-ram`，環內為溫度＋時脈、溫度＋GPU 時脈 `gpuClock`、使用率＋總量）；
  Memory 四段堆疊條 Used／Cached／Free／Other（`ramApp`、`ramCache`、`ramFree`、`ramOther`）；Storage：‹ › 切換硬碟，名稱為「大寫裝置名 · 型號」，Total／Used／Free。
  NETWORK（第二張）：標題列 NIC 選單（`.title-pill`，`data-all-label="all"`）；本機 IP＋MAC 與閘道 IP＋MAC（`netGatewayMac`），圖示為內嵌的 Font Awesome Free SVG（desktop、network-wired，翻頁鈕為 caret-right／水平翻轉，CC BY 4.0 註記保留在標記中）；
  上傳（上）／下載（下）鏡像長條圖與 Mbps 數值（`netDownMbps`、`netUpMbps`）。其餘儀表板卡片沿用 SUMMARY 的版面元素：粗體小節標題 `.panel-sub`（右側可帶灰字數值 `.panel-val`）、分隔線小節 `.panel-sub--rule`（Details、Top processes；下方清單為空時以 `:has(+ :empty)` 隱藏）、置中圖例 `.mem-legend`（磁碟 I/O 用兩欄 `.mem-legend--2`）、`kv` 數值格（感測器 CPU／GPU、記憶體 Total／Speed／Pressure）。其餘卡片（感測器；圖形處理器：GPU 環形圖＋VRAM 進度條＋溫度／編碼／解碼；
  處理器卡片：各核心使用率小長條（`data-core-bars`，CPU 數變動才重建）＋整體使用率歷史圖；磁碟 I/O 卡片：讀取（上）／寫入（下）鏡像長條圖（共用縱軸）＋目前加總速率；記憶體卡片：Used／Cached／Free／Other 四段堆疊條＋總量、速度、記憶體壓力歷史圖（縱軸上限為近期最大值，至少 10%）；磁碟清單：讀寫速率＋容量條與「已用 / 總量」（未掛載顯示「未掛載」）；圖形處理器卡片標題列有 GPU 選單（與 NIC 相同的 `.title-pill`，選項只顯示 `GPU0`、`GPU1`…，完整名稱顯示在卡片內）），感測器、磁碟、磁碟 I/O 卡片的「Show more…」（`data-goto`）切換到對應詳細頁（磁碟 I/O → 儲存裝置頁）。詳細頁：儲存裝置頁＝硬碟清單（容量條、讀寫速率）＋磁碟 I/O 大圖；網路頁＝介面選單、使用中介面、IPv4、MAC、閘道、上傳／下載速率＋大圖；感測器頁＝CPU／GPU 溫度、全部 hwmon 感測器、CPU 平均時脈。同一 `data-bind`／`data-chart` 可在多頁重複出現，全部一起更新。
  電池卡片（`#batteryCard`，`battery` 為 `null` 時隱藏）：百分比、狀態、剩餘／充滿時間、進度條、健康度、循環次數與容量。
  感測器卡片：CPU／GPU 溫度＋前 8 筆 hwmon 感測器（`data-sensor-list="8"`），感測器頁列出全部（`"all"`，相鄰的同一晶片感測器歸在晶片名稱標題 `.sensor-group` 下，列名只顯示標籤；卡片則以「晶片 · 標籤」顯示）；兩處都有 °C／°F 切換（`data-temp-unit`）。
  儲存裝置／網路詳細頁目前只顯示現有快照欄位。「桌面小工具」只是預留說明頁，不建立或控制 Plasma Widget；設定頁為應用內頁面（取代 KDE 設定視窗），含「外觀」卡片的「側欄毛玻璃」開關。
  資料來源為 mock 時顯示黃色提示列。標記以屬性綁定資料，同一數值可出現在多頁：`data-bind`（文字）、`data-ring`（環形圖百分比）、
  `data-bar`（進度條）、`data-chart`（歷史長條圖，`<g>` 內含一個 `.area` path）、`data-disk-list`（硬碟列容器）、`data-color`（指標色）。
- `styles.css`：色彩 token（`:root` 為深色：近黑深藍底 `--bg` #0d1117、卡片 `--card` #161b22 比底色亮一階、淡框線；`:root[data-theme="light"]` 覆寫淺色：冷灰白底＋白卡片，指標色同色相但加深以維持對比；指標色偏霓虹：`--cpu` 青、`--gpu` 紫、`--ram` 藍（記憶體快取沿用 `--gpu`），`--disk-read` 紅、`--disk-write` 藍；歷史圖為實心長條、無格線與圖表底色；側欄為近黑的中性燻黑 `--sidebar`，`--sidebar-glass` 為毛玻璃側欄的半透明色；`--net-down` 藍、`--net-up` 綠，記憶體 `--mem-free` 綠、`--mem-other` 灰，SUMMARY 環形圖 `--sum-cpu` 青、`--sum-gpu` 紫、`--sum-ram` 藍）。
  環形圖、長條、核心長條的過渡動畫（0.4 s）只在 `<html data-motion="on">` 時套用：這些屬性（stroke-dasharray、width、height）無法合成，每幀都要重排／重繪，更新間隔 500 ms 時動畫會一直跑（實測閒置 WebKit 約 70%、主程序約 50% 單核，關閉後皆約 5%）。
  視窗在 Wayland 為透明：`html, body` 背景透明，顯示區 `.app-main` 不透明（`--bg`），側欄只有 `<html data-glass="on">` 時才半透明，其餘情況為不透明 `--sidebar`。
  其餘樣式：卡片、環形圖（`pathLength=100` 的 circle，以 `stroke-dasharray` 顯示百分比）、
  進度條、長條圖與硬碟列樣式。
- `application.css`：側欄、頂部資訊列、頁面外框；儀表板以 CSS `column-count: 3` 排成由上而下的多欄卡片，欄數由 `app.js` `fitColumns()` 依 GRAPHICS 卡片文字是否被截斷調整（不使用像素門檻；儀表板卡片內距、標題、圖例、SUMMARY／NETWORK 字級已縮小，預設 1220 px 視窗可排三欄），卡片間距（欄距與上下）與外距皆 30 px（卡片為 `inline-block`，避免下緣間距跑到下一欄頂端）；儀表板卡片標題大寫。
  側欄收合：`.app-shell.is-collapsed` 只剩圖示（58 px），由使用者按鈕或寬度 ≤760 px 觸發（≤760 px 時隱藏收合按鈕與頂部 GPU 資訊）。
  響應式：欄數依寬度自動減少、≤980 px 側欄變窄、≤420 px 摘要卡片改為上下排列。
- `settings.css`：設定頁、mock 提示列、欄位錯誤訊息樣式，顏色全部使用 `styles.css` 的 token。
- `app.js`：以 `window.__TAURI__.core.invoke` / `event.listen` 連接後端（`withGlobalTauri`）。
  啟動時先 `listen("snapshot")`、`listen("settings-changed")`，再呼叫 `get_settings`、`get_settings_info`、`get_snapshot`。
  歷史圖（`chartSeries` → `barPath`）：每個取樣一根實心長條，缺值留空，0 保留短基線；SVG 的 viewBox 寬度設為圖表實際像素寬（只拉伸縱向），長條寬度為真正的像素值，最新的在右。
  儀表板：固定 2 px 長條、1 px 間隔，畫得下的最近取樣；其他頁面：只取最後 `historyPoints` 點，3 px 長條平均分布在 `historyPoints` 格（成對序列與壓力圖的縱軸上限取自整段保留的歷史）。成對序列（網路上傳／下載、磁碟讀／寫）為以中線鏡像的上／下兩半，共用縱軸；
  隱藏頁面不繪製（需要實際寬度），切到監控頁時立即重畫。
  `formatBytes`（與後端 `format_bytes` 同規則）、`formatMbps`（B/s × 8 ÷ 10⁶，< 100 兩位小數）；SUMMARY 的 Free＝已用 ÷ 使用率 − 已用（`diskUsage` 為 used/total pair）。
  儀表板欄數（`fitColumns()`，ResizeObserver 與每次 render 後執行）：從 3 欄開始，GRAPHICS 卡片的 `.device-name`、`.bar-label span`、`.kv dd` 任一出現截斷（`scrollWidth > clientWidth`）就減一欄並記下當時寬度；
  儀表板比該欄數上次截斷時更寬才試著加回一欄（截斷就再退回），GPU 名稱改變時清除記錄；以 `style.columnCount` 設定。
  `fillSettingSelect` 對有 `data-all-label` 的選單把 `all` 選項改用該標籤。電池卡片狀態與時間為英文（後端 `format_duration` 輸出 `2 h 17 min`）。
  `displaySettings()` 依 `updateInterval >= 1000` 設 `<html data-motion="on|off">`（未收到設定前為無動畫）。
  程序清單：只在儀表板或應用程式頁顯示且視窗可見（`document.hidden` 為 false）時輪詢 `get_processes`，第一次後 0.5 秒補一次（第一次沒有 CPU 比例），之後每 2 秒；
  切頁與 `visibilitychange` 時重新判斷，同時只會有一個輪詢迴圈。處理器卡片列 CPU 前 5 名、記憶體卡片列記憶體前 5 名（`data-proc-list="cpu|memory"`，「顯示應用程式…」→ 應用程式頁）；
  應用程式頁（`data-proc-table`）列出全部程序（名稱、PID、CPU、記憶體），按 CPU／記憶體欄標題切換由大到小排序（`aria-sort`），只在該頁顯示時重建表格。
  卡片上的設定選單（`<select data-setting-select="netInterface|gpuIndex">`）：選項來自快照（`netInterfaces`、`gpus`），值來自目前設定（`settings-changed`／`get_settings`），
  選擇後呼叫 `update_settings`，被拒絕時回到已儲存的值；已儲存但不在清單中的值（例如拔除的網卡）會保留為選項。
  主題：`localStorage` 的 `resourcewatch-theme`（`light`／`dark`，無法存取時改用系統 `prefers-color-scheme`），寫入 `<html data-theme>`；屬於顯示偏好，不放進 Rust `Settings`。
  側欄毛玻璃：開關即後端設定 `sidebarGlass`（`update_settings`，失敗時回到已儲存值）；啟動時呼叫 `get_window_info`，本次視窗已套用模糊（`glass`）且設定為開時設 `<html data-glass="on">`
  （關閉後側欄立即改為不透明；視窗本身要重新啟動才改為不透明）。提示文字依「設定值、啟動時的設定值、本次視窗是否有模糊」說明：重新啟動後生效／不透明較省 CPU 且 NVIDIA 上縮放較順／開啟時 NVIDIA 上縮放內容會落後／桌面環境不支援。側欄收合偏好存於 `resourcewatch-sidebar-collapsed`，寬度 ≤760 px（`matchMedia`）時一律收合。
  `setText(key)` 更新所有 `data-bind=key` 元素且只在文字改變時寫入 DOM；環形圖與進度條以 CSSOM（`el.style`）設定，因 CSP `style-src 'self'` 不允許行內 style 屬性，
  百分比為 0 或缺失時隱藏環形弧線；SVG path 相同時不重設；硬碟列只在清單變動時重建（以 DOM API 建立，不用 innerHTML），每個 `data-disk-list` 各建一份；
  網路上傳／下載兩條線共用同一個縱軸上限。目前頁面不是監控頁（`PAGES[].live`，即設定與桌面小工具）時只保存快照，切回監控頁才繪製。
  溫度單位：`localStorage` `resourcewatch-temp-unit`（`c`／`f`）；後端一律送 °C，`tempText()` 只在選 °F 時換算 `"48.6 °C"` 這類字串，感測器列以 `formatTemp()` 格式化，風扇顯示 `N RPM`；
  感測器列只在清單（chip／label／kind）變動時重建。
  Uptime 以快照 `uptimeSeconds` 為錨點、`performance.now()` 每秒更新文字，不呼叫 IPC。
  設定欄位 `change` 時先做前端範圍檢查，再呼叫 `update_settings`；錯誤依 `CommandError.field` 顯示在對應欄位下，
  寫檔失敗（`settingsWriteFailed`，`field` 為 `null`）顯示在剛修改的欄位下。欄位錯誤文字直接寫入（不經 `setText()`，以免空值顯示成 `--`）。
  設定頁底部顯示設定檔路徑；啟動時讀取設定檔有問題（`loadWarning`）時顯示紅色提示。
  在 Tauri 以外（例如瀏覽器）開啟時不顯示任何數值，只標示「未連線到應用程式後端」。

## spikes/slint-poc/

v0.2.0 Phase 0 的 Slint 技術驗證程式（獨立 crate，自帶空 `[workspace]`，不屬於任何 workspace；不被 `install.sh` 建置，Phase 7 移除或併入 `crates/app`）。量測數據與結論寫在 `REDESIGN_V0.2.md` 的 Phase 0。
- `ui/poc.slint`：`Poc` 視窗 `no-frame`＋`background: transparent`＋`resize-border-width: 6px`（Slint 內建的無邊框邊緣縮放，winit 負責游標與 `drag_resize_window`）；
  自訂標題列用內建 `WindowMoveArea`（拖過門檻才 `drag_window()`，單擊不影響子元件），其中的 `TouchArea` 雙擊切換最大化；右上角依 KDE 順序放最小化／最大化／關閉（回呼名 `close-window`，避開內建 `close`）。
  一層視窗玻璃（`glass` 決定半透明或不透明）＋一張卡片，兩者都是 `GlassPanel`；長條圖是 `Path`，`viewbox` 等於自身像素尺寸，`commands` 由 Rust 產生；`chart-width`／`chart-height` 輸出給 Rust 算座標，兩者一變（`changed`）就呼叫 `chart-resized`，Rust 立刻重算長條，不等下一次取樣。
  `GlassPanel`：底色用無邊框的圓角 `Rectangle`，1 px 邊緣另外畫（四條 1 px `Rectangle`＋四個 r×r 的 `EdgeArc` 四分之一圓弧 `Path`）。
  原因：軟體渲染器畫「圓角＋邊框」的 `Rectangle` 時，1 px 邊框的外緣與內緣反鋸齒落在同一像素被疊兩次，圓弧出現亮點（看起來像鋸齒）；`Path` 每次重繪都會依完整尺寸重新點陣化（無快取），所以只用在小小的四個角，整面視窗大小的 `Path` 會讓閒置 CPU 從約 0.5% 升到約 1.5%。
  `Path` 會把 viewbox 對應到「尺寸減描邊寬」並位移半個描邊，因此 `EdgeArc` 的 viewbox 是 `radius - 1`（1:1 像素），弧線中心線半徑 `radius - 0.5`。
- `src/main.rs`：
  - 視窗模式：`BackendSelector` 選 winit＋渲染器（`SLINT_RENDERER`，預設 `software`）→ 之後才能呼叫 `slint::set_xdg_app_id("resourcewatch")`（未選平台時呼叫會回錯「No default Slint platform」）。
    `spawn_local` 等 `winit_window()` 取得 raw-window-handle 的 `wl_display`／`wl_surface` 交給 `blur.rs`；失敗（X11、無協定、合成器關閉模糊）改成不透明。`POC_NO_GLASS=1` 直接不透明。
  - 500 ms `slint::Timer`：讀 `/proc/stat` 算 CPU 忙碌比例，存最多 240 點；`bars_path` 產生 2 px 長條＋1 px 間隔、最新在右緣、超出左緣即停（有單元測試）。每次 tick 也同步 `maximized-state`（合成器端最大化時圓角改 0）。
  - `--snapshot <png>`：自訂 `Platform` 回傳 `MinimalSoftwareWindow`，無視窗地以軟體渲染器繪製並 `take_snapshot()`，用 `png` crate 寫出 RGBA PNG（之後視覺驗證用，避開 KWin 防搶焦點）。
- `src/icons.rs`：標題列三顆按鈕的圖示取自系統圖示主題（`kdeglobals` 的 `[Icons] Theme=`，預設 `breeze`），沿 `index.theme` 的 `Inherits=` 往上找，最後找 hicolor；
  在 `~/.icons`、`$XDG_DATA_HOME/icons`、`$XDG_DATA_DIRS/*/icons` 的固定子目錄（`16x16/actions`、`actions/16`…`scalable/actions`）找 `window-minimize`／`window-maximize`／`window-restore`／`window-close`（含 `-symbolic`）。
  找不到時回傳空圖片，`TitleButton` 改顯示文字符號；圖示以 `colorize` 染成標題列前景色（主題圖示是單色）。只在啟動時讀一次，換主題要重開（有單元測試）。
- softbuffer 修補：`Cargo.toml` 的 `[patch.crates-io]` 指向 workspace 根目錄的 `patches/softbuffer`（與 `crates/app` 共用同一份，見該章節）。
- `src/blur.rs`：沿用 `src-tauri/src/blur.rs` 的 `wayland` 模組（`ext_background_effect_manager_v1` → `org_kde_kwin_blur_manager`），GTK 取得指標的部分換成 raw-window-handle。
  `blur()` 回傳 `Blur`（保留 compositor、queue handle 與 ext／kde 模糊物件），`set_shape(w, h, radius)` 把模糊區域設成圓角矩形；區域由 `rounded_rect_strips` 產生（四角每列 1 px 的長條＋中段一塊，每列起點取弧線在列中心最近的像素，有單元測試）。
  若模糊整個矩形，圓角外的透明區也會被模糊（KWin 還加雜訊），在花俏背景上看起來像方角鋸齒。
  `main.rs` 在取得 `Blur` 後立即套用，並以 `on_winit_window_event` 在每次 `Resized`（含最大化／還原）依實際邏輯尺寸重設；最大化時半徑 0。`WINDOW_RADIUS`（16）必須與 `poc.slint` 外層 `border-radius` 一致。
- 建置：`distrobox enter resourcewatch-dev -- bash -lc 'cd spikes/slint-poc && cargo build --release'`；Slint 鎖在 `=1.18.1`（`unstable-winit-030` 的 API 可能在小版本變動）。

## Cargo.toml（workspace 根目錄）

Cargo workspace：成員 `crates/core`、`crates/app`、`src-tauri`；`exclude = ["spikes"]`（`spikes/*` 各自帶空的 `[workspace]`，獨立建置）。
`[profile.release]`（`lto`、`codegen-units = 1`、`strip`）只能寫在 workspace 根目錄；`Cargo.lock` 與建置輸出 `target/` 也在根目錄（`.gitignore` 忽略 `/target/`）。
`[patch.crates-io]` 把 `softbuffer`、`i-slint-core`、`i-slint-renderer-software` 換成 `patches/` 下的修補版（只影響 `crates/app` 的 Slint 依賴樹）。
檢查一律在根目錄執行：`cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`。

## patches/softbuffer/

softbuffer 0.4.8 原樣複製（移除 benches），只把 Wayland 的 `wl_shm::Format::Xrgb8888` 改成 `Argb8888`（`src/backends/wayland/buffer.rs` 兩處，註記 `ResourceWatch patch`）。
原版丟棄 alpha，Slint 軟體渲染器畫出的視窗在 Wayland 上是不透明的，模糊雖然請求成功卻看不到；Slint 寫入的就是預乘 ARGB，與 `Argb8888` 相符。局部重繪時髒區先以視窗背景色覆寫（非疊加），透明不會累積。
Slint 或 softbuffer 升級時須重新比對這份修補。

## patches/i-slint-core/、patches/i-slint-renderer-software/

Slint 1.18.1 的兩個內部 crate 原樣複製（移除 benches 與 Cargo.lock），只改重畫區域的矩形數上限，修改處註記 `ResourceWatch patch`：
- `i-slint-core/partial_renderer.rs`：`DirtyRegion::MAX_COUNT` 3 → 32（陣列長度 32 是 `Default` 的上限）。上限 3 時，儀表板每週期分散在各卡片的小變動會被合併成幾乎涵蓋整個內容區的方框，軟體渲染器等於每週期重畫整個儀表板。
- `i-slint-renderer-software/lib.rs`：移除 `PHYSICAL_REGION_MAX_SIZE == 3`／`MAX_COUNT == 3` 斷言，改為兩者相等。該值只為 C++（cbindgen）ABI 固定；Rust 端的區域拆線（`region_line_ranges`）適用任意矩形數，`region_iter` 單元測試在修補後通過。
Slint 升級時須重新比對這兩處；`slint`／`slint-build` 版本鎖在 `=1.18.1`。

## crates/app/

v0.2.0 的 Slint 介面（套件 `resourcewatch-app`），單一程序、軟體渲染器、winit 後端（Wayland／X11）。資料、設定、排程全部來自 `crates/core`。
建置：`cargo build --release -p resourcewatch-app`；執行檔暫名 `target/release/resourcewatch-slint`（Tauri 版也叫 `resourcewatch`，兩者共用 workspace 的 `target/`，Phase 7 移除 Tauri 後改回 `resourcewatch`）。尚未納入 `install.sh`。
Wayland app_id 為 `resourcewatch`，與安裝的 `resourcewatch.desktop` 相符，KWin 顯示應用圖示。

### Cargo.toml、build.rs

依賴：`resourcewatch-core`、`slint`（鎖 `=1.18.1`，`unstable-winit-030` 的 API 可能在小版本變動；features 只開 `std`、`compat-1-2`、winit Wayland／X11、`renderer-software`）、`raw-window-handle`、`png`（截圖模式）；
Linux：`wayland-client`／`wayland-backend`（`client_system`，接手 winit 的 Wayland 連線）、`wayland-protocols`（`staging`）、`wayland-protocols-plasma`，只供 `blur.rs`。`build.rs` 以 `slint-build` 編譯 `ui/app.slint`。

### src/main.rs

- 視窗模式：`BackendSelector` 選 winit＋`software` → `slint::set_xdg_app_id("resourcewatch")`（必須在選平台之後、建立視窗之前）。
- 設定：`config_dir()` 為 `$XDG_CONFIG_HOME`（或 `~/.config`）`/io.github.local.resourcewatch`，與 Tauri 版共用同一個 `config.json`；`Prefs` 以 core `SettingsStore` 載入（容錯規則相同，警告印到 stderr）。
  `apply_prefs` 把 `theme` 設到 `Theme.mode`、`sidebarGlass` 設到 `Theme.glass`、`sidebarCollapsed` 設到側欄。UI 改變主題或側欄時呼叫 `prefs-changed`，`Prefs::update` 以 `Settings::apply` 驗證，值有變才寫檔；驗證或寫檔失敗保留原值（有單元測試）。
- 狀態：`Shared`（`settings: Mutex<Arc<Settings>>`、`source: Mutex<Source>`，鎖序 settings → source）與排程器執行緒共用；UI 執行緒的 `State` 另存 `Prefs`、排程器、最新快照與 `dashboard::View`，
  放在 `thread_local! STATE`，讓 `invoke_from_event_loop` 的閉包（必須 `Send`，不能帶 `Rc`）回到 UI 執行緒後取用。
- `State::commit(patch)`：比照 Tauri `commit_settings`——`Prefs::update` 驗證並存檔（失敗則什麼都不變）→ `source.apply_settings` → 換掉共用的 settings → 間隔改變時 `scheduler.set_interval`。
  儀表板的 NIC／GPU 膠囊、°C／°F、主題、側欄收合都經過這裡。
- 資料流：core `Scheduler::start(updateInterval, tick)` 在 `rw-scheduler` 執行緒取樣（每週期讀當下的共用 settings），快照以 `invoke_from_event_loop` 送回 UI 執行緒存成 `latest`；
  視窗最小化時只保存快照、不更新介面（歷史照常累積）。`State::render` 更新標題列 `Host`，目前在儀表板頁時再呼叫 `dashboard::apply`；切回儀表板（`page-changed`）時立即重畫一次。
- 程序清單：比照 Tauri 前端 `watchProcesses`，只在儀表板顯示且未最小化時於 UI 執行緒每 2 秒 `source.processes()`（啟動時先掃一次、500 ms 後補一次以取得 CPU 比例），交給 `dashboard::apply_processes`。
- 側欄寬度動畫在視窗顯示 300 ms 後才開啟（`animated`），儲存的收合狀態啟動時直接套用、不從展開寬度動畫縮回。
- 毛玻璃：`sidebarGlass` 為 `true` 時 `start_blur`：`spawn_local` 等 `winit_window()`，以 raw-window-handle 的 `wl_display`／`wl_surface` 呼叫 `blur.rs`；
  成功後依視窗實際邏輯尺寸設定圓角模糊區域，並以 `on_winit_window_event` 在每次 `Resized`（含最大化／還原）重設，最大化時半徑 0；失敗（X11、無協定、合成器關閉模糊）設 `Theme.glass = false`，改用不透明底色。`WINDOW_RADIUS`（16）須與 `app.slint` 視窗圓角一致。
- 最小化／最大化不經 Rust：Window 內建的 `minimized`／`maximized` 屬性與實際視窗狀態雙向同步（合成器端最大化也會寫回），`app.slint` 直接設定。
- `--snapshot out.png [--theme dark|light] [--collapsed] [--page N] [--gallery] [--size WxH]`（預設 1220×790；儀表板先取樣 5 次、掃描程序以取得速率與 CPU 比例）：自訂 `Platform` 回傳 `MinimalSoftwareWindow`，以 1220×790 無視窗繪製一次（主機資料取自實機或 `RESOURCEWATCH_MOCK=1`；`--gallery` 改畫元件展示頁），`write_png` 以 `take_snapshot()` 寫出 RGBA PNG。視覺驗證都用這個方式，避開 KWin 防搶焦點。
- `--gallery [--theme dark|light]`：開啟元件展示頁視窗（`Gallery`，不透明、不請求模糊），可實際操作下拉選單、分段控制與表格排序。

### src/charts.rs

Slint `Path` 用的圖表幾何，單位為像素（`BarChart` 的 viewbox 等於自身尺寸，描邊 0，長條保持固定像素寬）。規則與 Tauri 前端 `barPath` 相同：
- `bar_path(values, max, dir, layout, width, height)`：每個樣本一根；缺值（`None`、非有限值）留空；0 或負值保留最小高度（該半邊的 2%）的短樁；超過 `max` 夾到滿格；最新樣本在右緣。
  `Layout::Fixed`（儀表板）：2 px 長條＋1 px 間隔，只畫放得下的最近樣本；`Layout::Slots(n)`（詳細頁，n＝`historyPoints`）：最後 n 點平均分布，長條寬 `min(3, 寬/n)` 置中於格內。
  `Dir::Full` 從底部算滿高；`Dir::Up`／`Dir::Down` 為成對序列的上下半邊（上傳／讀取向上、下載／寫入向下）。
- `pair_max(a, b)`：成對序列共用的縱軸上限＝兩者最大值 ×1.12（至少 1）。
- `stack(parts, total)`：堆疊條各段的起點與寬度（0–1），缺值或負值為 0，總和超過時截到滿格。

### src/dashboard.rs

快照 → `Dash` global。格式規則沿用 Tauri 前端 `render()`：缺值維持 `--`；後端已格式化的文字直接沿用；`temp_text` 把 `"48.6 °C"` 在選 °F 時換算（`format_temp` 用於感測器原始值）；`format_mbps`（網路卡片 bps）；
程序記憶體 `proc_memory`（≥ 1 GiB 用 GiB 一位小數，否則整數 MiB）；GPU 名稱缺品牌時補上（`labeled_gpu_name`）；儲存空間 Free＝used×(100/percent−1)。
- 列表用 `sync(current, set, new)` 就地更新：同長度只換有變的列（`set_row_data`），長度變了才 `set_vec`，第一次才建立 `VecModel`——每週期建新 model 會讓 Slint 重建所有重複元件。
- 圖表：`canvas_path` 以 `ChartCanvas.width`（720＝240 點×3 px）×100 的畫布產生 `Layout::Fixed` 路徑（CPU、記憶體壓力、網路上下、磁碟讀寫）；成對序列用 `pair_max` 共用縱軸，Disk I/O 標示的最大值不含 12% 留白；記憶體壓力上限取近期峰值、至少 10%。
- `View`：快照沒有的 UI 狀態——SUMMARY 儲存翻頁的硬碟索引、NIC／GPU 膠囊選項對應的設定值（缺席的已存介面仍保留在選單）。
- `apply_processes`：CPU 前 5（缺值排最後）與記憶體前 5。
- 單元測試涵蓋溫度換算、Mbps、程序記憶體／CPU 文字、GPU 品牌、Free 計算。

### src/processes.rs

`sort(list, column, ascending)`：依欄位（`COL_NAME` 名稱不分大小寫、`COL_PID`、`COL_CPU`、3＝記憶體）排序 `ProcessInfo`；CPU 缺值（第一次掃描）不論升降冪都排最後；同值以 PID 排序（有單元測試）。欄位索引與 `ui/table.slint` 的 `ProcessTable` 一致。

### src/gallery.rs

元件展示頁的資料：合成的歷史序列（固定週期留空、一段 0 值，驗證圖表規則）、記憶體組成（`charts::stack`）、圖例、KV、溫度（°C／°F 切換時重建）；
程序表取 `Source::from_env` 的實際程序清單（掃描兩次以取得 CPU 比例），`sort-changed` 時以 `processes::sort` 重排後 `VecModel::set_vec`。圖表在 `charts-resized`（任一 `BarChart` 尺寸改變）時依新尺寸重算。

### src/blur.rs（Linux）

Wayland 合成器模糊：`ext_background_effect_manager_v1`（KWin 6.8+，`capabilities` 回報模糊是否開啟），否則 `org_kde_kwin_blur_manager`。`blur()` 接手 winit 的 `wl_display`／`wl_surface`，回傳 `Blur`；
`set_shape(w, h, radius)` 以 `rounded_rect_strips`（四角每列 1 px 長條＋中段一塊，每列起點取弧線在列中心最近的像素，有單元測試）設定圓角模糊區域——模糊整個矩形時圓角外的透明區也會被模糊（KWin 還加雜訊），看起來像方角鋸齒。
event queue 刻意洩漏（每視窗一次），避免之後的事件打到已釋放的 queue。

### ui/theme.slint

`global Theme`：`mode`（`system`／`dark`／`light`，來自設定）、`glass`；`dark` 在 `system` 時取 std-widgets 的 `Palette.color-scheme`（Slint 在 Linux 透過 XDG portal 監聽系統深淺色，切換系統主題即時生效）。
深淺兩套完整 token：三層材質（`window-glass`，`glass` 為 false 時改不透明色；`sidebar-glass`；`card-glass` 比底層亮一階）、1 px 高光 `edge-top`（較亮）／`edge-bottom`（較暗）、`shadow`、
前景 `text`／`text-secondary`／`text-tertiary`（vibrancy）、`hover`、`selected`、`danger`、`warning-*`（模擬模式標籤），以及指標色（CPU 青、GPU 紫、RAM 藍、上傳綠、下載藍、讀取紅、寫入藍，依深淺調整亮度）。

### ui/glass.slint

`GlassPanel`：無邊框的圓角 `Rectangle` 底色＋另外畫的 1 px 高光（上緣 `edge-top`、下緣 `edge-bottom`、左右兩側為上到下的漸層；四角為 r×r 的 `EdgeArc` 四分之一圓弧 `Path`，上兩角用 `edge-top`、下兩角用 `edge-bottom`）。
原因：軟體渲染器畫「圓角＋邊框」的 `Rectangle` 時，1 px 邊框外緣與內緣的反鋸齒落在同一像素被疊兩次，圓弧出現亮點（看起來像鋸齒）；`Path` 每次重繪都依完整尺寸重新點陣化（無快取），整面大小的 `Path` 會讓閒置 CPU 明顯上升，所以只用在四個角。
`Path` 把 viewbox 對應到「尺寸減描邊寬」並位移半個描邊，因此 `EdgeArc` 的 viewbox 是 `radius - 1`（1:1 像素），弧線中心線半徑 `radius - 0.5`。

### ui/icons.slint

`global Icons`：側欄線條圖示（24×24 viewbox，與 Tauri 前端側欄相同的圖形；圓、圓角矩形改寫成 path 指令，齒輪的 `transform` 已換算成絕對座標）。`LineIcon`：18 px、1.5 px 描邊的 `Path`。

### ui/titlebar.slint

- `WindowButtons`：依 KDE 順序（最小化、最大化／還原、關閉），`WindowGlyph` 以 1 px `Path` 畫 Breeze 樣式（細橫線、方框、兩個重疊方框、叉）；KWin 視窗裝飾主題不是圖示主題，讀不到，因此自繪預設 Breeze 外觀，顏色跟隨 `Theme`。
  viewbox 為尺寸減 1，整數座標落在像素中心。滑過時顯示 `hover` 底；關閉鈕滑過為紅底白字。每顆按鈕有 `accessible-role`／`accessible-label`。
- `HostBar`：一行「主機名稱 · 運作 時間 · 機型 · CPU 型號」，視窗窄時以 `elide` 截斷；`mock` 為 true 時右側顯示「模擬資料 · 非實機」膠囊。

### ui/sidebar.slint

`Sidebar`（第 2 層，`GlassPanel` 圓角 14、陰影）：頂部 App 圖示＋名稱（`@image-url` 讀 `src-tauri/icons/64x64.png`）與收合鈕；中段主頁面（儀表板、儲存裝置、網路、感測器、應用程式，`pages` 由 `app.slint` 傳入）；
底部「設定」（頁索引 `pages.length`）、主題切換鈕（跟隨系統 → 深色 → 淺色循環）、版本。收合時寬 56 px 只顯示圖示，展開 208 px；寬度變化 160 ms 過渡（`animated` 為 false 時 0 ms，啟動時不播放）。

### ui/card.slint

- `GlassCard`：第 3 層內容卡片（`GlassPanel` 圓角 14＋陰影），SUMMARY 版型：標題列（13 px 粗體、字距 0.8 px）可選擇放 `PillSelect`（`pill-options` 非空時）或 `SegmentedControl`（`segments` 非空時），其下為子元件垂直排列。
- `SectionHeading`：卡片內的小標題（如 Memory、Storage）。

### ui/gauges.slint

- `RingGauge`：環形圖，`percent` 0–100（< 0 為缺值，只畫軌道），圓頭弧線；環內最多三行文字（`label`／`value`／`detail`）。弧線以 `sin`／`cos` 算終點，0% 時隱藏（圓頭在長度 0 時仍會畫出一點），上限 99.99% 避免起訖點重合。
  `Path` 會把 viewbox 對應到「尺寸減描邊寬」，因此 viewbox 取 `2r`（r＝(寬−粗細)/2），描邊中心線正好落在半徑 r。屬性名用 `arc-color`（`Rectangle` 內建的 `color` 不能覆寫）。
- `StackBar`：圓角軌道上的堆疊條，各段 `{start, width, color}` 來自 `charts::stack`。軟體渲染器的 `clip` 只裁成直角矩形，所以由第一段／最後一段自己設左／右圓角（個別角半徑）。
- `Legend`：圓點＋標籤＋數值，垂直排列、整塊置中。
- `KvGrid`：鍵值格，`columns` 欄（預設 3），每格 40 px 高，數值過長以 `elide` 截斷。
- `BarChart`：顯示 Rust 產生的 `up`／`down` 路徑（`mirrored` 時畫中線與下半部）。`fixed-width` > 0（儀表板）時，Rust 在 `fixed-width`×100 的畫布上靠右畫出全部樣本，
  viewbox 只取最右邊「自身寬度」像素（`fit: fill`、`clip: true`，橫向 1:1、縱向拉伸），縮放不必重算；否則 viewbox 等於自身像素尺寸，`plot-width`／`plot-height` 輸出給 Rust，尺寸改變時呼叫 `geometry-changed` 由擁有者重算。
- `Value`：會頻繁變動的單行文字。Text 放在固定行高（`font-metrics` 的 ascent−descent）的容器內、不參與版面，寬度由呼叫端指定或伸展。
  Slint 的屬性是「先標記髒、晚點重算」：版面內的 Text 每次改字都會讓所在版面（以至儀表板瀑布流）失效，所有讀取幾何的 Text／Path 都被重繪，還會多算一幀；改用 `Value` 後只重畫文字本身。
  環形圖內文字、KV、圖例數值與標籤、名稱／數值列、硬碟列、標題列主機資訊都用它。列表的某一列以 `set_row_data` 更新時，該列「所有」欄位的相依都會失效，所以重複元件內連不變的標籤也要用 `Value`。
- `ProgressBar`（VRAM、電池、硬碟使用量）、`Rows`（名稱／數值列，名稱截斷）、`CoreBars`（各邏輯 CPU 直條，absolute 排列）、`MoreLink`（卡片底部「Show more…」）。
- 環形圖弧線、進度條、核心直條的顯示用 `visible`，不用 `if`（條件依賴每週期變動的值時，`if` 會讓 Slint 重新檢查條件元件）。

### ui/controls.slint

- `PillSelect`：卡片標題列的膠囊下拉選單（「NIC: all ▾」），點擊開 `PopupWindow` 列出選項，選定後更新 `current` 並呼叫 `selected(i)`；名稱最長 180 px 後截斷。有 `combobox` 無障礙角色。
- `SegmentedControl`：分段控制（°C／°F、主題），選中段為 `selected` 底色＋小陰影；`tab-list`／`tab` 無障礙角色。

### ui/table.slint

`ProcessTable`：可排序程序表（名稱、PID、CPU、記憶體）。點目前排序欄切換升降冪；換欄時名稱／PID 預設升冪、CPU／記憶體預設降冪，並呼叫 `sort-changed(column, ascending)` 由 Rust 排序；
排序欄標題顯示 ▲／▼。列表用 std-widgets 的 `ListView`（只建立可見列），奇數列淡底色。資料列 `ProcessRow` 為已格式化的字串。

### ui/data.slint

`global Dash`：儀表板（之後詳細頁也用）顯示的所有值，由 `dashboard.rs` 設定——預先格式化的文字、比例（< 0 為缺值）、列表 model（Rust 就地更新；在 Slint 用變動的值組陣列字面值會每週期重建重複元件），
以及 NIC／GPU／°C°F 選擇與 SUMMARY 儲存翻頁的回呼。`DiskRow`：硬碟列（名稱、「型號 · 容量 · 介面」、讀寫速率、使用比例與文字，未掛載為 −1／「未掛載」）。
`global FaIcons`：Network 卡片的電腦與閘道圖示（Font Awesome Free 7.3.1，CC BY 4.0，註記保留在檔案內）。

### ui/dashboard.slint

`Dashboard`：九張 `GlassCard`，內容與 Tauri 版一致——SUMMARY（CPU／GPU／RAM 環形圖、記憶體組成、儲存翻頁與 Total／Used／Free）、NETWORK（NIC 膠囊、本機與閘道位址／MAC、上傳下載鏡像圖、Mbps）、
SENSORS（°C／°F、CPU／GPU 溫度、前 8 個感測器）、GRAPHICS（GPU 膠囊、使用率環、名稱、VRAM、Temp／Encode／Decode）、PROCESSOR（各核心直條、使用率圖、CPU 前 5 程序）、
DISK I/O（讀寫鏡像圖、目前速率）、MEMORY（組成、Total／Speed／Pressure、壓力歷史、記憶體前 5 程序）、DISKS（每顆硬碟的速率、型號、使用量）、BATTERY（沒有電池時 `visible: false`、不佔空間）。
「Show more…」／「Show applications…」呼叫 `goto(頁)`：Sensors → 感測器、Disk I/O／Disks → 儲存裝置、Processor／Memory → 應用程式。
- 瀑布流：欄數＝`floor((可用寬 + 12) / (300 + 12))` 夾在 1–3，欄寬平分；卡片依固定順序放到目前最短的一欄。Slint 沒有迴圈，所以 9 個步驟展開為屬性鏈（`h/col/y/inc` 與各欄底部 `a/b/c`），
  `pick()` 取最短欄（同高取左邊）；高度取卡片 `preferred-height`（隱藏卡片為 0、不加間距）。內容放在 std-widgets 的 `ScrollView`，內容高度＝最高欄。
- `ChartCanvas.width`（720）：儀表板圖表畫布寬度，Rust 也讀它產生路徑。

### ui/gallery.slint

`Gallery`（由 `app.slint` 匯出）：以兩欄 `GlassCard` 展示所有元件——SUMMARY（環形圖含 0% 與缺值、堆疊條、圖例、KV）、PROCESSES（可排序表）、NETWORK（NIC 膠囊＋鏡像長條）、PROCESSOR（GPU 膠囊＋固定 2 px 長條）、DETAIL（`historyPoints` 平均分布）、SENSORS（°C／°F 分段）。不在 App 導覽內，只供驗證與手動操作。

### ui/app.slint

`App`（匯出 `Theme`、`Gallery`、`Dash`、`ChartCanvas` 給 Rust）：`no-frame`、透明背景、`resize-border-width: 6px`（無邊框邊緣縮放）、初始 1220×790、最小 640×480。
第 1 層為整個視窗的 `GlassPanel`（圓角 16，最大化時 0），內含側欄與右欄；右欄上方為標題列（`WindowMoveArea` 拖動、雙擊切換內建 `maximized`、頁面標題＋`HostBar`＋`WindowButtons`），下方為第 3 層內容卡片。
第 1 頁（儀表板）為 `Dashboard`，其餘頁目前只顯示頁名與預計實作的 Phase（佔位）；`page` 改變時呼叫 `page-changed`。`animated` 由 Rust 在啟動後開啟，傳給側欄。主題切換與側欄收合在 UI 端改值後呼叫 `prefs-changed` 存檔；回呼命名 `close-window`，避開 Window 內建的 `close`。


## crates/core/

套件 `resourcewatch-core`（lib 名稱 `resourcewatch_core`），不依賴任何介面工具組：硬體 collectors、快照模型、排程器、設定與設定檔、模擬資料。
依賴：`serde`、`serde_json`、`libloading`（執行期載入 NVML，不在連結時依賴 NVIDIA）、`libc`（`getifaddrs`）。`rust-version = 1.85`。
模組之間以 `crate::` 互相引用，不引用任何 UI 層程式；介面層（`src-tauri`）透過 `use resourcewatch_core::{…}` 取用，模組皆為 `pub mod`。

### src/lib.rs

宣告所有模組（`robustness` 僅 `#[cfg(test)]`）。`sample_live(count, interval)` 不開視窗、直接用 `Monitor` 取樣並回傳 JSON，供 `examples/sample.rs` 與系統工具交叉比對。

### src/settings.rs

`Settings {updateInterval, historyPoints, gpuIndex, netInterface, sidebarGlass, theme, tempUnit, sidebarCollapsed}`，前五項預設與範圍沿用舊版 v0.0.5 的 KConfig `main.xml`：
`updateInterval` 100–10000／步進 50、`historyPoints` 10–120／步進 10、`gpuIndex` 0–15、`netInterface` 為 `all`（空值亦視為 `all`）
或 1–15 個 `[A-Za-z0-9-_.:@]` 字元（Linux IFNAMSIZ），值不會傳給 shell；`sidebarGlass` 布林、預設 `true`，只在建立視窗時讀取（`src-tauri/src/lib.rs` `create_main_window`），改變後下次啟動生效。
Slint 版的顯示偏好：`theme` 為 `system`（預設）／`dark`／`light`、`tempUnit` 為 `c`（預設）／`f`（大小寫須完全相符，其他值回 `SettingsError`）、`sidebarCollapsed` 布林（預設 `false`）；Tauri 版讀寫設定時原樣保留。
`Settings::apply(&SettingsPatch)` 全部驗證通過才回傳新值，失敗時原值不變。
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

設定檔 `<設定目錄>/config.json`，格式即 `Settings` 的 JSON（`{"updateInterval":500,"historyPoints":30,"gpuIndex":0,"netInterface":"all","sidebarGlass":true,"theme":"system","tempUnit":"c","sidebarCollapsed":false}`；舊檔沒有 `sidebarGlass`、`theme`、`tempUnit`、`sidebarCollapsed` 時用預設值）。
- 載入：檔案不存在 → 預設值、無警告（首次啟動）；無法讀取 → 預設值＋警告，不動原檔；不是合法 JSON 或不是物件 →
  原檔改名為 `config.json.corrupt-<unix 秒>`（同秒重複時加序號）保留，使用預設值並警告；個別欄位無效或型別錯誤 → 該欄位用預設值、
  其餘有效欄位保留，並警告；未知欄位忽略；接受 `networkInterface` 作為 `netInterface` 的別名。
- `peek_sidebar_glass(dir)`：只讀出 `sidebarGlass`（缺檔、損壞、型別錯誤 → 預設 `true`），不備份、不警告，供 `prepare_environment` 在 WebKit 啟動前使用；正式載入與問題回報仍由 `load()` 處理。
- 寫入：建立目錄 → 寫入同目錄的唯一暫存檔（`.config.json.tmp-<pid>-<序號>`）→ `fsync` → 原子 `rename` 覆蓋 → `fsync` 目錄；
  失敗時刪除暫存檔並回傳錯誤，原檔不變。呼叫端（`commit_settings`）以 settings 鎖序列化所有寫入。

### src/snapshot.rs

快照型別，serde 以 camelCase 輸出，即 `snapshot` 事件與 `get_snapshot` 的資料格式。欄位沿用舊版 v0.0.5 的 HTML 快照契約，
欄位改名或刪除須同步更新本表、`snapshot.rs` 與 `frontend/app.js`。

| 欄位 | 型別 | 說明 |
|---|---|---|
| `dataSource` | `"live"\|"mock"` | Tauri 版新增；`"mock"` 時前端顯示「模擬資料 · 非實機」 |
| `version` | string | 應用版本 |
| `hostname`, `machineModel` | string | 主機名稱、機型（DMI）；啟動與硬體重新偵測時讀取，缺失 `"--"` |
| `cpuName` | string | 缺失為 `"--"` |
| `cpuCores`, `cpuThreads` | number \| null | 實體核心數、邏輯 CPU 數；啟動與硬體重新偵測時讀取 |
| `cpu` | Metric | 見下方 Metric 子物件 |
| `cpuPerCore` | (number\|null)[] | 各邏輯 CPU 使用率 0–100，依 `/proc/stat` 順序 |
| `cpuFreqText`, `cpuTempText`, `uptimeText` | string | 已格式化文字，缺失 `"--"` |
| `uptimeSeconds` | number \| null | 供 HTML 本地每秒內插 |
| `gpuIndex` | number | 選擇的 GPU 索引 |
| `gpuName` | string | |
| `gpuVendor` | `"nvidia"\|"amd"\|"intel"\|"other"` | |
| `gpuSource` | `"nvml"\|"drm"\|"unavailable"\|"mock"` | 舊版 v0.0.5 另有 `"nvidia-smi"`、`"ksystemstats"` |
| `gpus` | `{index, name, vendor}[]` | 所有偵測到的 GPU（`index` 即 `gpuIndex` 設定值），啟動與硬體重新偵測時更新 |
| `gpu`, `vram`, `encoder`, `decoder` | Metric | `vram` 含 used/total |
| `gpuTempText` | string | |
| `gpuClockText` | string | GPU 圖形核心時脈（`x.xx GHz`），缺失 `"--"` |
| `ram`, `swap` | Metric | 含 used/total |
| `ramSubtitle` | string | 目前等於 `ram.totalText` |
| `ramSpeedText` | string | 例如 `"1600 MT/s"`，缺失 `"--"` |
| `ramBreakdown` | `{app, cache, free, other}`: Metric | MemTotal 的組成（used/total pair，四者相加＝總量；無 AnonPages 時 `other` 為缺失） |
| `memPressure` | number \| null | PSI `/proc/pressure/memory` `some avg10`（%），無 PSI 時 `null` |
| `disks` | `{name, model, sizeText, transport}[]` | 只在清單變動時重建 DOM |
| `diskRates` | `{[name]: {read: number\|null, write: number\|null}}` | B/s |
| `diskUsage` | `{[name]: Metric}` | 該硬碟上所有已掛載檔案系統的已用／總量（pair）；未掛載的硬碟不出現；每 10 秒更新 |
| `battery` | object \| null | 筆電電池 `{percent, status, cycles, health, nowText, fullText, designText, timeText}`；`status` 為 `charging`／`discharging`／`full`／`notCharging`／`unknown`；沒有電池時 `null` |
| `sensors` | `{chip, label, kind: "temp"\|"fan", value: number\|null}[]` | 所有 hwmon 溫度（°C）與風扇（RPM），依晶片順序 |
| `netHasInterface` | bool | `netInterface !== "all"` |
| `netIpAvailable` | bool | 已取得 IPv4 |
| `netIp` | string | |
| `netInterfaceName`, `netMac`, `netGateway`, `netGatewayMac` | string | 使用中的介面（`all` 時為預設路由介面）、其 MAC、經由該介面的預設閘道與閘道 MAC（ARP 快取）；缺失 `"--"` |
| `netInterfaces` | string[] | 實體介面清單（介面選單用） |
| `netUp`, `netDown` | Metric | `rawValue` 為 B/s |
| `history` | `{cpu, gpu, encode, decode, vram, ram, netUp, netDown, memPressure, diskRead, diskWrite}: (number\|null)[]`（`diskRead`／`diskWrite` 為所有硬碟加總 B/s） | 最多 120 點，缺失保留 `null` |

Metric 子物件：`{displayValue: string, percent: number|null, rawValue: number|null, status: string, usedText: string, totalText: string}`。

`status`：`ready` / `loading`（啟動後 5 秒內缺失）/ `unavailable`。百分比型 Metric 沒有 used/total（為 `"--"`），速率型 `percent` 為 `null`。

`Metric::percent / pair / rate` 建構子負責把缺失、非有限值、負值與總量 0 轉成 `null` / `"--"` / `status: "unavailable"`，不轉成 0。`format_uptime()` 對應舊版 `uptimeFromSeconds`；`format_duration()` 輸出電池剩餘時間（`2 h 17 min`、`45 min`）。

### src/format.rs

自 `FormatUtils.js` 與舊版 QML 格式化函式移植：`valid`、`valid_rate`、`clamp_percent`、`percent_of`、`format_percent`（一位小數、去掉 `.0`）、
`format_bytes`（B…PiB）、`format_rate`（B/s…GiB/s）。

### src/source.rs

`Source::Live(Box<Monitor>)`（預設）或 `Source::Mock(Box<MockSource>)`（環境變數 `RESOURCEWATCH_MOCK=1`，僅供 UI 開發）。
統一提供 `sample`、`peek`、`refresh(RefreshKind)`、`apply_settings`、`processes(now)`（程序清單）。`RefreshKind` = `ipv4` | `disks` | `hardware`。

### src/monitor.rs

實際資料來源。持有各 collector、啟動時取得的靜態資訊（CPU 型號、RAM 速度、硬碟清單、IPv4、GPU 選擇）與歷史緩衝。
- `sample(now, settings)`：一個監控週期。讀取 CPU、記憶體、uptime、GPU、硬碟 I/O、網路各一次；硬碟與網路速率使用排程器提供的
  同一個週期時間戳 `now`；組成快照、寫入歷史，歷史陣列每週期只產生一次。
- `peek()`：只用上次讀值組成快照，不讀硬體、不寫歷史。
- `processes(now)`：交給 `ProcessCollector`，與排程週期無關（`get_processes` 呼叫時才掃描）。
- 溫度／時脈文字不再重複做有限值檢查：collectors 只會輸出有限值（hwmon／NVML 整數已檢查範圍、cpufreq 為正整數平均），`None` 是唯一缺失情況。
- `refresh(kind)`：IPv4 重新查詢；硬碟清單重新掃描並重設 I/O 計數（下一個樣本無速率）；硬體資訊重新讀 CPU 型號、主機名稱、機型、RAM 速度並重掃 GPU。
- `apply_settings()`：`gpuIndex` 改變時切換 GPU；`netInterface` 改變時切換統計介面，並因為是使用者明確操作而重新查詢一次 IPv4。
- 啟動後 5 秒內缺失的指標標為 `status: "loading"`（速率需兩次取樣），之後為 `"unavailable"`。
- 文字格式：CPU 時脈 `x.xx GHz`、CPU 溫度一位小數、GPU 溫度整數、`°C`。

### src/history.rs

`HistoryBuffer`：十一條序列（cpu、gpu、encode、decode、vram、ram、netUp、netDown、memPressure、diskRead、diskWrite），Monitor 與 MockSource 固定保留 `KEPT_POINTS`（240）點，與 `historyPoints` 無關（`historyPoints` 只決定詳細頁顯示的點數；儀表板 3 px 一格，需要足夠的點才能鋪滿寬欄），
超過時捨棄最舊的點；非有限值存成 `None`。`disk_totals()` 加總各硬碟讀寫速率（有速率的硬碟才計入，全部沒有時為 `None`）。Monitor 與 MockSource 共用。

### src/collectors/mod.rs

collector 共用工具。`Roots {proc_dir, sys_dir, run_dir}`（預設 `/proc`、`/sys`、`/run`，測試時指向假目錄樹）、`read_trimmed`／`read_u64`／`read_i64`／`read_first_line`、
`list_dir`（自然排序，`card10` 在 `card9` 後）、`counter_rate`（計數器倒退或時間差 ≤ 0 回傳 `None`）。collector 只讀資料，不決定何時讀、不格式化文字。

### src/collectors/cpu.rs

- 使用率：讀整個 `/proc/stat`（`parse_stat`）；整體使用 `cpu` 行，total = user+nice+system+idle+iowait+irq+softirq+steal，idle = idle+iowait，取前後兩次差值；第一次、計數器倒退時為 `None`。
  加總使用檢查過的整數運算，異常的極大值會被拒絕（`None`）而不會溢位。
  每核心使用率取 `cpuN` 行，以標籤（不是位置）對應前次讀值，因此 CPU 離線／上線時其他核心不受影響；新上線、第一次或計數器倒退的核心為 `None`。同一時段實測與 KSystemStats 平均值一致（6.99% 對 6.96%）。
- 時脈：啟動時找出各 `cpuN/cpufreq/scaling_cur_freq`，每次取平均（kHz → MHz）。
- 溫度：啟動時掃描 hwmon。`coretemp` 取所有 `Core N` 平均（與 KSystemStats `averageTemperature` 相同，不含 Package），沒有則用 `Package id`；
  `k10temp`/`zenpower` 取 `Tdie`，其次 `Tctl`；`cpu_thermal` 等 SoC 取第一個輸入。超出 −40～150 °C 視為無效。
- 型號：`/proc/cpuinfo` 的 `model name`（或 `Hardware`），啟動與手動重新偵測時讀取。
- 核心／執行緒（`parse_topology`）：同一份 cpuinfo，邏輯 CPU 數＝`processor` 項目數，實體核心數＝不重複的（`physical id`, `core id`）組合（多插槽時 core id 會重複）；沒有 core id（許多 ARM）時核心數為 `None`。

### src/collectors/battery.rs

啟動與硬體重新偵測時找出第一個 `type` = `Battery` 且 `scope` ≠ `Device`（排除滑鼠、耳機等週邊電池）的 `/sys/class/power_supply/*`；每次取樣讀取：
`capacity`（沒有時以 now／full 推算）、`status`、`cycle_count`（0 代表驅動不追蹤 → `None`）、`energy_now/full/full_design`（µWh → Wh）＋`power_now`，
或驅動只提供電荷時的 `charge_*`（µAh → Ah）＋`current_now`。剩餘時間：放電＝now／rate，充電＝(full − now)／rate，超過 96 小時視為不可靠。健康度＝full／design。
本機為桌機，以假 `/sys` 測試（含 µWh 與 µAh 兩種驅動、週邊電池排除）。

### src/collectors/sensors.rs

感測器清單：啟動與硬體重新偵測時掃描所有有 `name` 的 `/sys/class/hwmon/hwmon*`，列出 `tempN_input` 與 `fanN_input`（共用 `cpu.rs` 的 `hwmon_inputs`），
標籤取 `*_label`，沒有時用輸入名稱（`temp1`、`fan2`）。每次取樣只讀這些 `_input` 檔：溫度沿用 CPU 感測器的範圍檢查（−40～150 °C，`read_millicelsius`），
風扇 0～100 000 RPM，其餘為 `None`。NVML 的 GPU 溫度不在 hwmon 中，另由 `gpuTempText` 提供。

### src/collectors/memory.rs

`/proc/meminfo`：RAM used = `MemTotal − MemAvailable`、Swap used = `SwapTotal − SwapFree`（與 KSystemStats 舊版數值逐位元組相同）。
組成：剩餘＝MemTotal − MemFree − 快取、快取＝Buffers + Cached + SReclaimable（舊核心無 SReclaimable 時視為 0）、可用＝MemFree；剩餘再分成
應用程式＝AnonPages（不超過剩餘）與其他＝剩餘 − AnonPages（核心、不可回收 slab、page table、驅動等），四者相加＝總量；無 AnonPages 時應用程式＝剩餘、其他為 `None`；
數字矛盾（快取大於總量等）時全部為 `None`。記憶體壓力：`/proc/pressure/memory` 的 `some avg10`（0–100，超出範圍或無 PSI 為 `None`）。
`SwapTotal` 為 0 時保留 0（UI 顯示「0 B / --」且無百分比），欄位缺失或矛盾（Available > Total）時為 `None`。

### src/collectors/system.rs

- 主機名稱：`/proc/sys/kernel/hostname`。機型：DMI `/sys/class/dmi/id/product_name`，佔位字串（`To be filled by O.E.M.`、`System Product Name`、`Default string` 等）時改用 `board_name`，都沒有則未知；一般使用者可讀。
- Uptime：`/proc/uptime` 第一欄（含睡眠時間，等同 CLOCK_BOOTTIME），每次取樣讀取；前端以此為錨點每秒內插。
- RAM 速度：先讀 `/sys/firmware/dmi/entries/17-*/raw`（SMBIOS Type 17，configured 優先、其次 rated；一般使用者通常無權限），
  再讀舊版相同的快取 `${XDG_STATE_HOME:-~/.local/state}/resourcewatch/ram_speed.json`（`{"speedsMTs":[…]}`）；
  輸出 `N MT/s` 或 `最小–最大 MT/s`，都沒有則未知。不要求 root。

### src/collectors/disk.rs

- 清單（啟動與手動刷新）：掃描 `/sys/block`，依 `dev` 主次號排序（與 `lsblk` 相同，硬碟編號與舊版一致）。
  排除 `/virtual/`（loop、zram、dm、md）、USB、SCSI type ≠ 0（光碟）、容量 0（空讀卡機）、`vd*`／`mmcblk*` 等；
  transport：`nvmeXnY` → `nvme`，路徑含 `/ata` → `sata`，含 `end_device-`/`sas_` → `sas`，其餘 `sd*` → `scsi`。
  型號與 `lsblk` 相同規則：優先讀 udev 資料庫 `/run/udev/data/b<主>:<次>` 的 `ID_MODEL_ENC`（解碼 `\xHH`），沒有時才用 sysfs `device/model`
  （SCSI 的 sysfs 型號只有 16 字元，例如 `TOSHIBA DT01ABA2`，udev 為完整的 `TOSHIBA DT01ABA200V`）。只讀檔案，不啟動 `udevadm`。
- 容量（`usage`）：解析 `/proc/self/mountinfo`（`parse_mountinfo`，還原 `\040` 等八進位跳脫）。裝置名稱優先取掛載來源 `/dev/...`（btrfs 的裝置號是匿名的 `0:N`；`/dev/mapper/*` 以 canonicalize 解析成 `dm-N`），否則以 `major:minor` 查 `/sys/dev/block`；
  再對應到實體硬碟：硬碟本身、`/sys/block/<disk>/<分割區>`，或 device-mapper（LUKS／LVM）沿 `slaves` 往下找（跨多顆硬碟的 LVM 只算在第一顆）。
  每個區塊裝置只計一次（btrfs 子卷、bind mount 重複掛載），以 `statvfs(3)` 取已用＝(f_blocks − f_bfree)×f_frsize、總量＝f_blocks×f_frsize（與 `df` 相同），依硬碟加總。
  Monitor 在啟動、手動重新掃描硬碟時立即計算，之後每 10 秒一次（不在每個週期 statvfs）。實測與 `df -B1` 加總逐位相同。
- 讀寫速率（每次取樣）：直接讀 `/proc/diskstats` 第 6／10 欄（512-byte sectors），先算 sectors/s 再乘 512（避免溢位）；
  第一次、裝置不在 diskstats、計數器倒退時為 `None`。`reset()` 用於重新掃描後。

### src/collectors/process.rs

程序清單（`get_processes`）。每次掃描列出 `/proc` 下的數字目錄，每個程序只讀一個 `/proc/<pid>/stat`：
`parse_stat` 取 comm（到最後一個 `)`，名稱可含空白與括號）、utime+stime（欄位 14、15）、rss 頁數（欄位 24）、flags（欄位 9）。
略過 kernel thread（`PF_KTHREAD`）、掃描途中結束或讀不到的程序。CPU% = 與上次掃描的 tick 差 ÷ `CLK_TCK` ÷ 經過秒數 ÷ 線上邏輯 CPU 數 × 100
（整機 = 100%，與處理器卡片的整體使用率同一尺度，上限 100）；第一次看到的程序為 `null`；PID 被重用且 tick 變少時該次為 0。
記憶體 = rss × 頁大小（`sysconf`）。名稱為 kernel 截斷的 comm（最多 15 字元）。

### src/collectors/network.rs

- 流量：`/proc/net/dev` 的 rx/tx bytes 差值。`all` 只加總**實體介面**（有 `/sys/class/net/<if>/device`；結果快取），排除 lo、bridge、veth、tun/VPN，避免轉送或封裝流量重複計算。
  指定介面時只看該介面；介面不存在、剛出現／消失或計數器重置的那一次略過，全部略過則為 `None`。切換介面會清除前次計數。
- 網路細節（`details`，與 IPv4 同時機查詢）：預設路由取 `/proc/net/route` 目的地 `00000000`、旗標含 UP|GATEWAY、metric 最小者（位址為記憶體中的 `__be32`，以 `to_ne_bytes` 還原）；
  介面：指定介面，`all` 時為預設路由介面；MAC 讀 `/sys/class/net/<if>/address`（全零視為無）；閘道只在經由該介面時顯示；閘道 MAC 由 `parse_arp` 從 `/proc/net/arp` 找同介面、旗標非 0x0、非全零的項目（與 details 同時機讀取，ARP 項目之後才出現時需手動刷新）；實體介面清單＝有 `device` 的介面。
- IPv4：`getifaddrs(3)`，取選定介面（`all` 則任一非 loopback 介面）第一個非 loopback、非 169.254 的位址。只在啟動、手動刷新、切換介面時查詢；不啟動 `ip`、不經 shell。

### src/collectors/gpu/mod.rs

- 清單：`/sys/class/drm/cardN` 中 PCI class `0x03xxxx` 的裝置，依 `cardN` 排序；`gpuIndex` 即此清單的索引（舊版是 KDE 的 `gpuN`，多 GPU 時順序可能不同）。
  記錄 PCI 位址（`0000:01:00.0`）、vendor／device ID、驅動。
- NVIDIA 且 NVML 可用：以 PCI 位址取得 NVML handle，讀使用率、NVENC、NVDEC、溫度、圖形時脈、VRAM；名稱用 NVML。
- 其他（AMD、Intel、NVML 不可用的 NVIDIA）：DRM sysfs。`gpu_busy_percent`（AMD）、`mem_info_vram_used/total`、裝置自身 hwmon 溫度（優先 `edge`/`pkg` 標籤）；時脈：amdgpu `pp_dpm_sclk` 標 `*` 的層級（`parse_dpm_clock`），否則 i915/xe `drm/<card>/gt_cur_freq_mhz`；
  NVENC/NVDEC 一律 `None`；名稱查 `pci.ids`（`/usr/share/hwdata/pci.ids` 等，僅啟動／手動時讀取）。
- `source()`：`nvml` | `drm` | `unavailable`（索引超出範圍）。NVML 只在偵測到 NVIDIA 卡時嘗試載入一次。
- 偵測（啟動與 `rediscover()` 手動重新偵測）時為每張卡一次解析好名稱與讀取後端（NVML handle 或 DRM 路徑），`pci.ids` 每次偵測最多讀一次；
  `select(index)` 切換 GPU 只改索引，不重新掃描、不讀 `pci.ids`、不查 NVML。`list()` 回傳所有已偵測 GPU 的（名稱, 廠牌），供 `gpus` 欄位。

### src/collectors/gpu/nvml.rs

以 `libloading` 在執行期載入 `libnvidia-ml.so.1`（或 `.so`），只綁定 9 個函式（init、shutdown、依 PCI 取 handle、名稱、使用率、
encoder／decoder 使用率、溫度、memory info v1），另外選擇性載入 `nvmlDeviceGetMemoryInfo_v2` 與 `nvmlDeviceGetClockInfo`（`NVML_CLOCK_GRAPHICS`，缺少時時脈為 `None`，不影響載入）。
VRAM 優先用 v2：其 `used` 不含驅動保留記憶體，與 `nvidia-smi` 及舊版一致（v1 的 `used` 含保留部分，本機約多 59 MiB）；
v2 不存在（R510 以前的驅動）或失敗時退回 v1。`nvmlInit_v2` 成功後才建立物件，`Drop` 時 `nvmlShutdown`。整個程式生命週期只初始化一次。
回傳碼非成功（含 `NOT_SUPPORTED`，例如無 NVENC 的 GT 1030）或數值不合理時為 `None`。

### src/mock.rs

mock 資料來源（`RESOURCEWATCH_MOCK=1`）：以正弦波產生數值，名稱一律帶 `Mock`，IPv4 使用文件保留位址 `192.0.2.10`，RAM 速度刻意為 `--` 以測試缺失顯示；
不讀硬體、不啟動程序。`sample()` 推進時間並寫入歷史（共用 `HistoryBuffer`），`peek()` 不推進。
`refresh(Disks)` 後第一個樣本的硬碟速率為 `null`（模擬舊版計數器差值行為）。`processes()` 回傳 6 個 `mock-*` 程序，CPU 隨 mock 時鐘變化。

### src/robustness.rs（僅測試）

容錯測試：以固定種子的 xorshift 亂數產生 3000 組隨機位元組與類真實檔案的 token，餵給所有解析器（`/proc/stat`、meminfo、diskstats、
net/dev、SMBIOS、pci.ids、udev 解碼、`/proc/<pid>/stat`、設定檔 JSON），確認不會 panic；極大計數器值（溢位、回捲）；執行中硬碟、網卡、hwmon 消失
以及 `/proc` 檔案變成垃圾或不存在後，數值變為 `None`／`--`（歷史保留空缺），裝置恢復後數值也恢復；檔案權限不足（chmod 000）時為缺失值；
SMBIOS 無權限且沒有快取時 RAM 速度為未知。

### examples/sample.rs

`cargo run -p resourcewatch-core --example sample -- [次數] [間隔ms]`：以 `sample_live()` 輸出每次快照的 JSON 行，供與 `kstatsviewer`、`nvidia-smi`、`free` 等比對。不隨應用打包。

## src-tauri/

Rust + Tauri 2 桌面應用（workspace 成員），資料層來自 `crates/core`。建置：在此目錄 `cargo tauri build`（release + deb／rpm，輸出在根目錄 `target/release/`）；檢查見上方 workspace 根目錄。
開發環境為 distrobox `resourcewatch-dev`（Fedora 44）。

### Cargo.toml、build.rs、tauri.conf.json、capabilities/default.json、icons/

- 套件 `resourcewatch` 0.1.0（Tauri 版自己的版號，與 Plasma 套件 0.0.5 分開；rpm 版號不允許 `-`）；lib 名稱 `resourcewatch_lib`。
  依賴：`resourcewatch-core`（path `../crates/core`）、`tauri`、`serde`、`serde_json`；release profile 由 workspace 根目錄決定；
  Linux 專用：`gtk`、`gdkwayland-sys`（取得 GTK 視窗的 `wl_display`／`wl_surface`，兩者原本就在 Tauri 依賴樹內）、
  `wayland-client`＋`wayland-backend`（`client_system`，接手 GTK 既有的 Wayland 連線）、`wayland-protocols`（`staging`，ext-background-effect-v1）、
  `wayland-protocols-plasma`（舊版 KWin 的 `org_kde_kwin_blur`），皆只供 `blur.rs` 使用；
  dev 依賴啟用 `tauri` 的 `test` feature 供 IPC 測試。`rust-version = 1.85`。
- `tauri.conf.json`：identifier `io.github.local.resourcewatch`；視窗 `main` 初始 1220×790、最小 640×480、可自由縮放、不設最大值；
  `"create": false`：不由設定檔自動建立，而由 `lib.rs` `create_main_window()` 以這份設定建立（透明與否依顯示後端決定）；
  CSP 僅允許本機資源與 Tauri IPC；`withGlobalTauri: true`；打包目標 deb、rpm。
- `capabilities/default.json`：`main` 視窗只有 `core:default`（含事件監聽）；未安裝 fs／shell／http 等外掛。
- `icons/`：由 `icons/source/app-icon.png` 以 `cargo tauri icon` 產生，只保留 Linux 需要的 PNG。`frontend/app-icon.png` 是 `icons/64x64.png` 的複本（側欄品牌圖示；前端只能讀 `frontend/` 內的檔案），換圖示時一起更新。

### src/lib.rs

應用組裝與 IPC。`configure_with(builder, config_dir)` 註冊所有 Command，並在 Tauri `setup` 中：解析設定目錄
（`None` → `app.path().app_config_dir()`，即 `$XDG_CONFIG_HOME/io.github.local.resourcewatch`，通常為 `~/.config/io.github.local.resourcewatch`；
測試傳入暫存目錄）→ 載入設定 → 建立 `AppState` → 建立主視窗 → 啟動唯一的排程器。
`create_main_window()`：以 `tauri.conf.json` 的 `main` 設定建立視窗（測試用 mock 設定沒有視窗時略過）；GDK 顯示為原生 Wayland **且**設定 `sidebarGlass` 為開時才設為透明並呼叫
`blur::enable()`，成功則 `AppState.glass = true`；`sidebarGlass` 關閉時建立不透明視窗、不請求模糊（透明視窗讓 UI 程序 CPU 約 2.5 倍，實測閒置與縮放時皆然）。X11／XWayland 一律不透明：WebKitGTK 在 X11 透明視窗（含停用 DMA-BUF 繪製時）會漏畫頁面的一部分（實測側欄整塊空白）。`configure(builder)` = `configure_with(builder, None)`。
`run()` 在 `RunEvent::Exit`（關閉最後一個視窗）時呼叫 `AppState::shutdown()` 停止並 join 排程器執行緒。
`prepare_environment()`（由 `src/main.rs` 在建立任何執行緒之前呼叫）：載入 NVIDIA 核心模組（`/sys/module/nvidia`）且使用者沒有自行設定時，
設定 `WEBKIT_DISABLE_DMABUF_RENDERER=1` 的條件為「X11／XWayland」或「原生 Wayland 且 `sidebarGlass` 關閉」：X11 上 DMA-BUF 繪製失敗會讓視窗空白；
原生 Wayland 上每次縮放都要重新配置 GBM 緩衝區，網頁內容落後視窗邊框 0.3–0.5 秒並露出空白（螢幕錄影確認，關閉 DMA-BUF 後順暢）；
但關閉 DMA-BUF 時透明視窗的側欄畫不出來（整塊空白，與 X11 透明視窗相同），所以毛玻璃開啟時保留 DMA-BUF。
`sidebarGlass` 由 `early_config_dir()`（`$XDG_CONFIG_HOME` 或 `~/.config` ＋ `APP_IDENTIFIER`，單元測試確認與 `tauri.conf.json` 一致）與
`settings_store::peek_sidebar_glass()` 在 WebKit 啟動前讀取。判斷邏輯 `needs_dmabuf_workaround()` 為純函式並有單元測試。

| Command | 輸入 | 輸出 | 錯誤 |
|---|---|---|---|
| `get_snapshot` | — | 排程器最近一次產生的 `Snapshot`（不觸發硬體讀取；排程器尚未產生時才用上次讀值組成） | — |
| `get_settings` | — | `Settings` | — |
| `get_settings_info` | — | `{path: string \| null, loadWarning: string \| null}`：設定檔路徑與啟動載入時的問題 | — |
| `update_settings` | `{patch: SettingsPatch}`（部分欄位，未知鍵拒絕） | 驗證並**已寫入磁碟**的 `Settings`，並發出 `settings-changed` | `invalidSetting`（含 `field`）；`settingsWriteFailed`（`field: null`，記憶體／排程器／磁碟皆不變）；型別錯誤為 Tauri 反序列化錯誤字串 |
| `reset_settings` | — | 預設 `Settings`（已寫入磁碟），並發出 `settings-changed` | `settingsWriteFailed` |
| `get_scheduler_stats` | — | `SchedulerStats`（間隔、週期數、略過數、panic 數、耗時與延遲） | — |
| `get_processes` | — | `ProcessInfo[]`：`{pid, name, cpu: number \| null, memory}`（`collectors/process.rs`；CPU 為與上次呼叫之間的整機比例；mock 時為固定假清單）。只取 source 鎖（實機約 500 個程序時單次約 10 ms），不經排程器、不發事件 | — |
| `get_window_info` | — | `{glass: bool}`：合成器已對主視窗套用背景模糊（前端據此決定側欄是否半透明） | — |
| `refresh_devices` | `{kind: "ipv4" \| "disks" \| "hardware"}`：IPv4／硬碟清單／CPU 型號＋RAM 速度＋GPU 清單與名稱 | 刷新後的 `Snapshot`，並立即發出 `snapshot` | 未知 `kind` 為反序列化錯誤 |

| Event | Payload | 時機 |
|---|---|---|
| `snapshot` | `Snapshot` | 每個 `updateInterval` 一次；手動刷新後立即一次 |
| `settings-changed` | `Settings` | 設定更新或恢復預設後 |

`AppState` 分成獨立的鎖（中毒時復原而非崩潰）：`settings`（目前設定 `Arc<Settings>`、`SettingsStore`、載入警告）、`source`（Monitor／Mock）、
`latest`（最新快照）、`scheduler`（`OnceLock`，只能設定一次 → 不會有第二個排程器）與 `glass`（`AtomicBool`，只在 setup 寫入）。鎖順序固定為 settings → source；
排程週期只先短暫取 settings（複製 `Arc`）再取 source，不會巢狀，因此不會死結。命令不再等待整個取樣持有的大鎖。

排程週期（`start_scheduler`）：`source.sample(now, &settings)`（只持有 source 鎖）→ `emit("snapshot")`（以參考序列化，不複製快照）→
存入 `latest`。`RESOURCEWATCH_DEBUG_STATS=1` 時每 10 秒與結束時在 stderr 輸出 `RW_STATS …`。執行期間不啟動任何外部程序。

設定變更（`commit_settings`，整段持有 settings 鎖，因此連續或同時的修改會依序處理）：驗證 → 與目前值相同則直接回傳 →
寫入 `config.json`（失敗即回傳錯誤，不套用任何變更）→ `Source::apply_settings`（切換 GPU／網路介面）→ 更新記憶體 →
間隔有變才 `Scheduler::set_interval` → 發出 `settings-changed`。因此磁碟、記憶體與排程器不會長期不一致。
手動刷新在呼叫端執行緒進行，以 source 鎖與排程週期互斥，不重啟排程器。

### src/blur.rs（Linux）

主視窗的合成器背景模糊（側欄毛玻璃），只用於原生 Wayland。透過 `gdk_wayland_display_get_wl_display`／`gdk_wayland_window_get_wl_surface`
取得 GTK 的連線與 surface，以 `Backend::from_foreign_display` 共用同一連線（不另開連線）：
- 優先 `ext_background_effect_manager_v1`（KWin 6.8 起提供，舊協定已不再公開）：綁定後 roundtrip 讀取 `capabilities`，含 `blur` 才建立
  `ext_background_effect_surface_v1` 並以涵蓋整個 surface 的 `wl_region` 呼叫 `set_blur_region`（合成器會裁切到 surface 大小）。
- 沒有新協定時使用 `org_kde_kwin_blur_manager`（僅在 KWin 模糊特效載入時公開），不設 region 即整個 surface。（舊版 KWin 路徑 NOT TESTED）
- 兩者都是 double-buffered，於 GTK 下一次 `wl_surface.commit`（每次重繪）生效。顯示區不透明，因此整個 surface 模糊只會從側欄透出。
- 事件佇列刻意洩漏（`mem::forget`），避免之後的 capability 事件送到已釋放的佇列；因此執行中關閉 KWin 模糊特效後，前端不會得知（側欄仍半透明但不模糊）。
- 回傳 `Err(原因)` 時於 stderr 輸出 `sidebar blur unavailable: …`，前端保持不透明側欄。

### tests/ipc.rs

以 Tauri mock runtime 走真實的 invoke 路由；每個測試使用自己的暫存設定目錄，並以 `run_iteration` 執行一次 `setup`（載入設定、啟動排程器）。
涵蓋：快照欄位契約與 `dataSource: "live"`、設定讀寫／驗證錯誤格式／未知鍵／失敗不改值／恢復預設、三種 `refresh_devices` 與未知 `kind`、
設定存檔後重新啟動仍保留（排程器以儲存的間隔啟動）、非法輸入不寫檔不改排程、損壞設定檔回報並保留備份、
快速連續修改後磁碟／記憶體／排程器一致且無暫存檔殘留、目錄唯讀時寫入失敗不改變任何狀態、排程器實際執行並餵給 `get_snapshot`、`get_processes` 列出測試程序本身（第一次 `cpu` 為 null、第二次為數字）。各模組的單元測試寫在該檔案的 `#[cfg(test)]` 內，collector 測試使用暫存目錄建立的假 `/proc`、`/sys`。

## 與舊版（v0.0.5）的資料來源對照

舊版程式已從此分支移除，本表保留作為遷移紀錄；比對結果見 `REFACTOR_RUST_TAURI.md` Phase 2、Phase 4。

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
| VRAM | KSystemStats `usedVram/totalVram` | NVIDIA：NVML memory v2（不含保留，缺少時 v1）；其他：`mem_info_vram_*`（若有） | 更新間隔 | `nvidia-smi`、KSystemStats |
| GPU 使用率（AMD/Intel） | KSystemStats `gpu/gpuN/usage` | AMD：`gpu_busy_percent`；Intel：不支援（sysfs 無此指標） | 更新間隔 | NOT TESTED（無硬體） |
| GPU 名稱／廠牌 | KSystemStats name + DRM PCI vendor | NVML 名稱／`pci.ids` + DRM PCI vendor | 啟動／手動 | 舊版輸出 |
| RAM / Swap | KSystemStats `memory/*` | `/proc/meminfo`（MemTotal − MemAvailable；SwapTotal − SwapFree） | 更新間隔 | `kstatsviewer`（逐位元組相同） |
| RAM 速度 | `hardware_info.py` SMBIOS Type 17 / 使用者快取 | 相同來源與快取檔，權限不足顯示未知 | 啟動／手動 | 舊版輸出 |
| 硬碟清單 | `lsblk` JSON + 篩選 | `/sys/block/*`，依主次號排序；型號取 udev `ID_MODEL_ENC`；不啟動 `lsblk` | 啟動／手動按鈕 | `lsblk` |
| 硬碟讀寫 | 每間隔 `cat /proc/diskstats` | 程序內直接讀 `/proc/diskstats` | 更新間隔 | `kstatsviewer disk/*` |
| 網路上下行 | KSystemStats `network/*` | `/proc/net/dev` 差值（`all` 只計實體介面） | 更新間隔 | `kstatsviewer` |
| IPv4 | `ip -j -4 address show` + KSystemStats `ipv4address` | `getifaddrs(3)` | 啟動／手動按鈕／切換介面 | `kstatsviewer` |
| 歷史 | QML 陣列 `historyPoints` 長度 | `HistoryBuffer`，缺失保留 `None` | 更新間隔 | 單元測試 |
| HTML 快照 | QtWebChannel `snapshotReady` | Tauri Event `snapshot` | 更新間隔 | IPC 整合測試 |
| 設定 | KConfig `main.xml` | Rust `Settings` + `config.json`（Tauri 設定目錄，原子寫入） | 使用者操作 | 單元／IPC 測試、GUI 重新啟動測試 |
| 排程 | `SensorManager.qml` 單一 one-shot Timer | `scheduler.rs` 單一執行緒、固定網格、略過過期週期 | `updateInterval` | 單元／IPC 測試、實機統計 |
