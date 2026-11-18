# ResourceWatch 桌面小工具（KDE Plasma 6）製作流程

## Context
想要一個常駐在桌布上的原生小工具：資訊密度與精緻度參考 iStat Menus（小卡片、數字＋迷你曲線／圓環），視覺語言沿用 ResourceWatch 儀表板（中性灰卡片 `#353537`、圓角 16、指標色：CPU 藍曲線／洋紅環、GPU 青、上傳桃紅、下載藍、記憶體黃）。
決定：
- 形式：**Plasma 6 桌面 plasmoid（QML）**，放桌布，不是面板下拉。
- 資料：**resourcewatch 新增 CLI 模式**，小工具定時執行，不需要主程式開著。
- 顯示哪些項目：**在 ResourceWatch 設定頁控制**。

本計畫核准後，第一步先把本檔存成 `docs/Widget_Plan.md`，之後每完成一個 Phase 就在該檔對應項目打 ✅（打勾前先重新讀檔確認有無新增內容）。

## 架構
```text
ResourceWatch 設定頁「桌面小工具」── invoke("set_widget_config") ──> ~/.config/resourcewatch/widget.json
plasmoid (QML) ── P5Support.DataSource{engine:"executable"} 每 N 秒 ──> resourcewatch --widget
                                                                      └─ 讀 widget.json ＋ 取樣兩次（間隔 1 秒）→ 印出 { config, snapshot } JSON
```
- 速率需兩次取樣：CLI 在同一程序內 `Monitor::new()` → `snapshot()` → sleep 1 s → `snapshot()`，不保存狀態檔（`Monitor` 含 `Instant`／`Gpus`，序列化不划算）。
- 設定與資料同一份輸出，QML 不用另外讀檔。
- 曲線歷史由 QML 自己保留（60 點），CLI 無狀態。

---

## Phase 0：建立計畫文件
- [x] ✅ 本計畫存為 `docs/Widget_Plan.md`，commit。

## Phase 1：後端 CLI 與設定檔（src-tauri/src/main.rs、Cargo.toml）
- [x] ✅ `main()` 開頭偵測 `--widget`：不啟動 Tauri，`sys::Monitor` 取樣兩次，`serde_json` 印 `{ config, snapshot }` 後結束（Cargo.toml 明列 `serde_json`）。確認此路徑不初始化 GTK／WebKit。
- [x] ✅ 新指令 `get_widget_config`／`set_widget_config(cfg)`：讀寫 `$XDG_CONFIG_HOME/resourcewatch/widget.json`（預設 `~/.config`），壞檔或缺檔回預設值。
- [x] ✅ 設定欄位：`items`（有序陣列：`cpu`／`mem`／`gpu`／`net`／`disk`／`temp`）、`interval`（秒）、`nic`（null＝自動，同儀表板邏輯）。
- [x] ✅ 單元測試：設定檔解析（缺欄位、壞 JSON → 預設）。
- [x] ✅ 更新 Architecture（main.rs 章節；開頭「沒有設定檔」改為只有 widget.json），commit。
- 驗收：`resourcewatch --widget | jq` 有 config、速率非 null；執行時間約 1 秒出頭。

## Phase 2：ResourceWatch 設定頁（frontend/app.js、style.css）
- [x] ✅ 設定頁新增「桌面小工具」區塊：項目勾選＋上下排序、更新間隔、網路卡；改變即 `set_widget_config`，進頁時 `get_widget_config`。
- [x] ✅ 一行說明：右鍵桌面 → 新增小工具 → ResourceWatch。
- [x] ✅ 更新 Architecture（app.js 設定段、style.css），commit。
- 驗收：設定頁變更後 `cat ~/.config/resourcewatch/widget.json` 與 `--widget` 輸出同步。

## Phase 3：Plasmoid 骨架與資料流（新目錄 `widget/`）
- [x] ✅ `metadata.json`：id `io.github.local.resourcewatch.widget`、`Plasma/Applet`、API 6.0。
- [x] ✅ `contents/ui/main.qml`：`P5Support.DataSource`（executable，`resourcewatch --widget`，interval＝config.interval）；`onNewData` 解析 JSON、推進歷史；找不到執行檔／解析失敗時顯示「找不到 resourcewatch」。
- [x] ✅ `preferredRepresentation: fullRepresentation`、`NoBackground`，依 `config.items` 以 `Repeater` 排卡片（先放純文字數值）。
- [x] ✅ `Theme.qml`：照 style.css 深／淺色 token，依 `Kirigami.Theme` 背景亮度切換；`fmt.js`：複製 app.js 的 bytes／速率／溫度格式化。
- [x] ✅ 新增 Architecture `widget/` 章節，commit。
- 驗收（系統沒有 plasmoidviewer，改為 kpackagetool6 安裝後加到桌面測試）： 顯示數值且每 N 秒更新、項目增刪排序會反映。

## Phase 4：卡片視覺（儀表板風格）
- [x] ✅ `CpuCard`：大字 %、迷你面積曲線（Canvas）、溫度。
- [x] ✅ `MemCard`：%、堆疊條（應用程式黃／快取藍／可用綠）。
- [x] ✅ `GpuCard`：圓環＋使用率、VRAM、溫度。
- [x] ✅ `NetCard`：↓藍／↑桃紅兩欄速率＋上下對稱小條狀圖。
- [x] ✅ `DiskCard`：讀寫速率＋用量條。
- [x] ✅ `TempCard`：依裝置分組的前幾個溫度。
- [x] ✅ 更新 Architecture，commit。
- 驗收：以 `qml`（offscreen）載入六張卡片與真實 `--widget` 資料截圖比對（淺色）；桌面上需重啟 plasmashell 才會載入新版 QML。

## Phase 5：安裝與收尾（install.sh、版本）
- [x] ✅ 修正 install.sh 舊版 widget 偵測：`grep -q "$ID"` 會誤中新 id `io.github.local.resourcewatch.widget`，改為整行比對。
- [x] ✅ `install`：`kpackagetool6 -t Plasma/Applet -i widget/`（已安裝則 `-u`）；`uninstall`：`-r io.github.local.resourcewatch.widget`；無 `kpackagetool6` 時略過並提示。
- [x] ✅ 更新 Architecture（install.sh），bump 版本（Cargo.toml＋app.js「關於」），commit。
- 驗收：`./install.sh` → 桌面新增小工具正常運作；`./install.sh uninstall` 清乾淨。
- [x] ✅ 回頭重讀 `docs/Widget_Plan.md`，確認全部 ✅、無新增任務。

## 後續調整
- [x] ✅ 卡片從頂端排列（原本被撐開平均分布在小工具高度中）。
- [x] ✅ 修正移動／縮放把手全紅：原因是一欄時內容高度超出螢幕可用範圍（改為固定兩欄後內容高度可放進螢幕）。
- [x] ✅ 固定兩欄（左三右三）、卡片寬 282、高度依內容；小工具大小固定為內容大小，縮放把手無法拉動。
- [x] ✅ 處理器卡片加上各執行緒使用率直條（同儀表板）。
- [x] ✅ 處理器卡片：執行緒直條在左、曲線在右，寬度 30:70。
- [x] ✅ 長按卡片內容區可拖曳到其他格（與該格交換），排列寫回 widget.json；拖動整個小工具改為按標題列或卡片間空白。
- [x] ✅ 網路卡片加上本機 IPv4。
- [x] ✅ 卡片副標：網路的 IPv4 移到標題下方；處理器標題下方加上處理器型號。
- [x] ✅ 顯示卡卡片標題改為「顯示卡」，型號移到標題下方。
- [x] ✅ 預設排列改為左：處理器／網路／溫度，右：顯示卡／磁碟／記憶體。
- [x] ✅ 處理器上方加 App 圖示與名稱的格子，高度依兩欄差距自動計算。
- [x] ✅ 卡片右鍵選單加上「關閉此卡片」（含 ResourceWatch 格子；隱藏後可在設定頁重新顯示）。
- [x] ✅ 卡片透明度設定（設定頁，只影響卡片底色）；改為滑桿 0–100%、每格 5%，對應實際透明度 0–40%。

## 面板小工具
- [x] ✅ 同一個小工具放進面板時改為精簡顯示：每項圖示＋數值＋迷你圖，依設定頁的項目與順序。
- [x] ✅ 點面板上的某一項，彈出視窗只顯示該項卡片；再點一次收合。
- [x] ✅ 垂直面板只顯示圖示與數值。
- [x] ✅ 設定頁「面板小工具」區塊：個別開關面板上的項目（與桌面卡片分開）；按鈕可拖動調整面板上的順序。
- [x] ✅ 面板的 CPU／GPU／RAM 改為文字在上、百分比在下，不顯示圖示，迷你圖寬度減半；面板不顯示溫度。
- [x] ✅ 設定變更即時套用：小工具每 0.3 秒讀 widget.json，指令改為計時器觸發的一次性執行（輪詢模式會晚一個間隔才交出結果）。
- [x] ✅ 預設顯示全部六項。
- [x] ✅ 按下即可拖動（改桌面容器的 `editModeCondition` 為 AfterPress，鎖定時不可拖）。

## 每個 Phase 通用驗證
`cargo test`、`cargo tauri build --no-bundle` 無錯（程式執行中先 `pkill -x resourcewatch`）。

## 不做（需要再說）
程序前 5 名清單、點卡片開啟主程式對應頁、面板下拉版本、非 KDE 桌面支援。
