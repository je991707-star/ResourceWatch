# ResourceWatch 小工具（GNOME Shell）規劃

## Context
現有的桌面／面板小工具是 KDE Plasma 6 plasmoid（QML），GNOME 無法使用。目標是在 GNOME 上提供**頂欄**（top bar）版本：CPU／GPU／RAM／網路／磁碟項目，點一下彈出該項卡片（對應 Plasma 面板小工具）。
決定：
- **只做頂欄**，不做桌布上的桌面小工具（GNOME 沒有公開的桌布圖層 API，私有 API 改版易失效）。
- 最低支援 GNOME 46（涵蓋 Ubuntu 24.04 LTS）。
- 測試環境：distrobox 容器內的巢狀 GNOME Shell，以獨立的家目錄、設定與 D-Bus 工作階段執行，不安裝任何東西到主機、不寫入主機的 dconf。

GNOME 沒有 plasmoid 這類小工具機制，唯一能把東西放進頂列或桌布的方式是 **GNOME Shell 擴充功能**（GJS，JavaScript）。Mutter 不支援 wlr-layer-shell，獨立視窗（例如讓 Tauri 開一個置底視窗）在 Wayland 上無法固定在桌布層，所以不考慮。

本計畫核准後，每完成一個 Phase 就在對應項目打 ✅（打勾前先重新讀檔確認有無新增內容）。

## 不變的部分（沿用）
- **資料**：仍由 `resourcewatch --widget` 提供 `{ config, cpuModel, snapshot }` JSON，擴充功能不自己讀 `/proc`。
- **設定**：仍是 `~/.config/resourcewatch/widget.json`，由 ResourceWatch App 設定頁編輯；頂欄使用 `panel`／`interval`／`panelNic`／`panelGpu`，意義同 Plasma 面板。
- **視覺**：沿用 Theme.qml／style.css 的色彩 token 與卡片尺寸（寬 282、圓角 16、間距 12）。

## 架構
```text
ResourceWatch 設定頁 ── set_widget_config ──> ~/.config/resourcewatch/widget.json
                                                   │ Gio.FileMonitor（變更即套用，取代 Plasma 版每 0.3 秒 cat）
GNOME Shell 擴充功能（gnome/）
  ├─ data.js   每 interval 秒 Gio.Subprocess 非同步執行 resourcewatch --widget → 解析 → 歷史 60 點 → 通知各 UI
  ├─ panel.js  PanelMenu.Button：依 panel 排出項目（文字＋迷你圖），點擊開 PopupMenu 顯示該項卡片
  └─ cards/*.js、chart.js、theme.js、fmt.js：St 元件＋St.DrawingArea（Cairo）重畫卡片、曲線、圓環、條狀圖
```
- **非阻塞**：Shell 主執行緒卡住會凍結整個桌面，所有子程序一律 `Gio.Subprocess` 非同步、可取消；上一次還沒結束就不啟動下一次。
- **執行檔位置**：gnome-shell 的 PATH 不一定有 `~/.local/bin`，先找 `~/.local/bin/resourcewatch`，再用 `GLib.find_program_in_path`。
- **生命週期**：所有物件在 `enable()` 建立、`disable()` 全部銷毀並斷開訊號與計時器（GNOME 鎖定螢幕時會停用擴充功能，也是 extensions.gnome.org 審查重點）。
- **QML 無法重用**：卡片 UI 需以 St／Clutter 重寫；`fmt.js` 的格式化邏輯可改寫成 ES 模組沿用。

## 支援版本
- GNOME 46 以上（46 為 Ubuntu 24.04 LTS；45 起擴充功能改為 ES 模組，45 以前不支援）。
- `metadata.json` 的 `shell-version` 列出實際測過的版本；GNOME 49 對擴充功能沒有破壞性變更。
- 只支援 Wayland 工作階段（GNOME 49 起預設停用 X11）。

---

## Phase 0：測試環境
- [x] ✅ distrobox 容器 `resourcewatch-gnome`（Fedora）：安裝 `gnome-shell`、巢狀執行所需的 `mutter-devkit`（GNOME 48 以前為 `--nested --wayland`），以及 `resourcewatch` 執行所需的 WebKitGTK 執行期函式庫。
- [x] ✅ 巢狀 Shell 以無頭模式（`--headless --virtual-monitor`，主機上不開視窗）、`dbus-run-session`、私有的空系統匯流排，加上獨立的 `HOME`／`XDG_CONFIG_HOME`／`XDG_DATA_HOME`（放在 `~/.local/share/resourcewatch-gnome-test/`）執行：不讀寫主機的 dconf 與擴充功能資料夾；`widget.json` 由測試腳本複製進去。
- [x] ✅ 測試腳本 `gnome/dev/nested.sh`：同步擴充功能到測試家目錄、啟用、開啟巢狀 Shell。
- [x] ✅ 確認巢狀 Shell 內 `resourcewatch --widget` 讀得到資料（容器內讀不到 NVML 屬已知限制）。

## Phase 1：App 設定頁
- [x] ✅ 設定頁依桌面環境（`SystemInfo.desktop`）調整：GNOME 時「面板小工具」卡片改稱「頂欄」，說明改為「在『擴充功能』App 啟用 ResourceWatch」；桌面小工具卡片在 GNOME 隱藏。
- [x] ✅ 更新 Architecture（app.js），commit。

## Phase 2：擴充功能骨架與資料流（新目錄 `gnome/`）
- [x] ✅ `metadata.json`（uuid `resourcewatch@je991707-star.github.io`、`shell-version`、名稱與說明）、`extension.js`（`Extension` 子類別，`enable`／`disable`）、`stylesheet.css`。
- [x] ✅ `data.js`：非同步執行 `--widget`、錯誤狀態（找不到執行檔／結束碼／JSON 解析失敗，訊息同 Plasma 版）、歷史 60 點、`FileMonitor` 監看 widget.json。
- [x] ✅ 頂欄先放一個純文字按鈕顯示 CPU %，驗證資料流與 enable／disable 不洩漏（反覆停用啟用）。
- [x] ✅ 新增 Architecture `gnome/` 章節，commit。

## Phase 3：頂欄項目
- [x] ✅ `panel.js`：依 `panel` 排出項目——CPU／GPU／RAM 為上下兩行（標籤／%）加迷你面積圖；網路／磁碟為圖示加兩行速率與對稱條狀圖（`fmt.short`），網路卡／顯示卡用 `panelNic`／`panelGpu`。
- [x] ✅ 每個項目各自是一個 PanelMenu 按鈕，點擊彈出該項的選單（Phase 4 完成前先顯示文字）。
- [x] ✅ commit。

## Phase 4：彈出卡片（St＋Cairo）
- [x] ✅ `theme.js`：指標色；文字、格線與軌道取選單的文字色，跟隨 Shell 的淺色／深色樣式。
- [x] ✅ `chart.js`：面積曲線、對稱條狀圖、圓環、堆疊條、執行緒直條（St.DrawingArea＋Cairo，資料變更時 `queue_repaint`，只重畫開啟中的卡片）。
- [x] ✅ 五張卡片：處理器、記憶體、顯示卡、網路、磁碟，版面與 Plasma 版一致。
- [x] ✅ 巢狀 Shell 截圖與 Plasma 版比對，commit。

## Phase 5：安裝、文件
- [ ] `install.sh`：偵測到 `gnome-extensions` 時，把 `gnome/`（不含 `dev/`）複製到 `~/.local/share/gnome-shell/extensions/<uuid>/` 並 `gnome-extensions enable <uuid>`；提示 Wayland 下新安裝或更新的擴充功能要重新登入才會載入。`uninstall` 一併移除。
- [ ] README：安裝需求與功能介紹加入 GNOME 頂欄與截圖。
- [ ] 更新 Architecture（install.sh、gnome/），commit。
- [ ] （選擇性）上架 extensions.gnome.org：擴充功能本身不含執行檔，需註明須另外安裝 ResourceWatch。

---

## 風險
- **測試環境**：開發機不是 GNOME，巢狀 Shell 在容器內可能缺 GPU／輸入裝置而跑不起來，需改用虛擬機，迭代較慢；實機（真正的 GNOME 工作階段）只能由使用者驗證。
- **效能**：每 2 秒啟動一次子程序（每次約 1 秒取樣，大部分時間在 sleep），與 Plasma 版相同；Shell 端只做解析與重畫。
- **工作量**：卡片 UI 以 St／Cairo 重寫（Phase 4）是最大的部分。

## 待決定
- 是否上架 extensions.gnome.org（Phase 5 的選擇性項目）。
