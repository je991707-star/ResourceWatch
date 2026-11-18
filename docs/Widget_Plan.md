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
- [ ] 設定頁新增「桌面小工具」區塊：項目勾選＋上下排序、更新間隔、網路卡；改變即 `set_widget_config`，進頁時 `get_widget_config`。
- [ ] 一行說明：右鍵桌面 → 新增小工具 → ResourceWatch。
- [ ] 更新 Architecture（app.js 設定段、style.css），commit。
- 驗收：設定頁變更後 `cat ~/.config/resourcewatch/widget.json` 與 `--widget` 輸出同步。

## Phase 3：Plasmoid 骨架與資料流（新目錄 `widget/`）
- [ ] `metadata.json`：id `io.github.local.resourcewatch.widget`、`Plasma/Applet`、API 6.0。
- [ ] `contents/ui/main.qml`：`P5Support.DataSource`（executable，`resourcewatch --widget`，interval＝config.interval）；`onNewData` 解析 JSON、推進歷史；找不到執行檔／解析失敗時顯示「找不到 resourcewatch」。
- [ ] `preferredRepresentation: fullRepresentation`、`NoBackground`，依 `config.items` 以 `Repeater` 排卡片（先放純文字數值）。
- [ ] `Theme.qml`：照 style.css 深／淺色 token，依 `Kirigami.Theme` 背景亮度切換；`fmt.js`：複製 app.js 的 bytes／速率／溫度格式化。
- [ ] 新增 Architecture `widget/` 章節，commit。
- 驗收：`plasmoidviewer -a widget/` 顯示數值且每 N 秒更新、項目增刪排序會反映。

## Phase 4：卡片視覺（儀表板風格）
- [ ] `CpuCard`：大字 %、迷你面積曲線（Canvas）、溫度。
- [ ] `MemCard`：%、堆疊條（應用程式黃／快取藍／可用綠）。
- [ ] `GpuCard`：圓環＋使用率、VRAM、溫度。
- [ ] `NetCard`：↓藍／↑桃紅兩欄速率＋上下對稱小條狀圖。
- [ ] `DiskCard`：讀寫速率＋用量條。
- [ ] `TempCard`：依裝置分組的前幾個溫度。
- [ ] 更新 Architecture，commit。
- 驗收：plasmoidviewer 深／淺色截圖比對儀表板卡片樣式。

## Phase 5：安裝與收尾（install.sh、版本）
- [ ] `install`：`kpackagetool6 -t Plasma/Applet -i widget/`（已安裝則 `-u`）；`uninstall`：`-r io.github.local.resourcewatch.widget`；無 `kpackagetool6` 時略過並提示。
- [ ] 更新 Architecture（install.sh），bump 版本（Cargo.toml＋app.js「關於」），commit。
- 驗收：`./install.sh` → 桌面新增小工具正常運作；`./install.sh uninstall` 清乾淨。
- [ ] 回頭重讀 `docs/Widget_Plan.md`，確認全部 ✅、無新增任務。

## 每個 Phase 通用驗證
`cargo test`、`cargo tauri build --no-bundle` 無錯（程式執行中先 `pkill -x resourcewatch`）。

## 不做（需要再說）
程序前 5 名清單、點卡片開啟主程式對應頁、面板下拉版本、非 KDE 桌面支援。
