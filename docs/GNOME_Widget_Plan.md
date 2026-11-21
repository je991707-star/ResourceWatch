# ResourceWatch 小工具（GNOME Shell）規劃

## Context
現有的桌面／面板小工具是 KDE Plasma 6 plasmoid（QML），GNOME 無法使用。目標是在 GNOME 上提供同樣的兩種形式：
- **面板**：頂列（top bar）上的 CPU／GPU／RAM／網路／磁碟項目，點一下彈出該項卡片（對應 Plasma 面板小工具）。
- **桌面**：桌布上的兩欄卡片，含音樂播放格子（對應 Plasma 桌面小工具）。

GNOME 沒有 plasmoid 這類小工具機制，唯一能把東西放進頂列或桌布的方式是 **GNOME Shell 擴充功能**（GJS，JavaScript）。Mutter 不支援 wlr-layer-shell，獨立視窗（例如讓 Tauri 開一個置底視窗）在 Wayland 上無法固定在桌布層，所以不考慮。

本計畫核准後，每完成一個 Phase 就在對應項目打 ✅（打勾前先重新讀檔確認有無新增內容）。

## 不變的部分（沿用）
- **資料**：仍由 `resourcewatch --widget` 提供 `{ config, cpuModel, snapshot }` JSON，擴充功能不自己讀 `/proc`。
- **設定**：仍是 `~/.config/resourcewatch/widget.json`，由 ResourceWatch App 設定頁編輯；`items`／`panel`／`interval`／`nic`／`gpu`／`panelNic`／`panelGpu`／`app`／`opacity` 意義相同。
- **重排存檔**：沿用 `resourcewatch --widget-order …`、`--widget-app on|off`。
- **視覺**：沿用 Theme.qml／style.css 的色彩 token 與卡片尺寸（寬 282、圓角 16、間距 12）。

## 架構
```text
ResourceWatch 設定頁 ── set_widget_config ──> ~/.config/resourcewatch/widget.json
                                                   │ Gio.FileMonitor（變更即套用，取代 Plasma 版每 0.3 秒 cat）
GNOME Shell 擴充功能（gnome/）
  ├─ data.js   每 interval 秒 Gio.Subprocess 非同步執行 resourcewatch --widget → 解析 → 歷史 60 點 → 通知各 UI
  ├─ panel.js  PanelMenu.Button：依 panel 排出項目（文字＋迷你圖），點擊開 PopupMenu 顯示該項卡片
  ├─ desktop.js 背景圖層（Main.layoutManager._backgroundGroup）上的兩欄卡片容器：拖曳移動、長按重排、右鍵關閉
  ├─ media.js  MPRIS（Gio.DBusProxy，org.mpris.MediaPlayer2.*）：挑選播放器、屬性變更、位置輪詢、控制
  └─ cards/*.js、chart.js、theme.js、fmt.js：St 元件＋St.DrawingArea（Cairo）重畫六張卡片、曲線、圓環、條狀圖
```
- **非阻塞**：Shell 主執行緒卡住會凍結整個桌面，所有子程序一律 `Gio.Subprocess` 非同步、可取消；上一次還沒結束就不啟動下一次。
- **執行檔位置**：gnome-shell 的 PATH 不一定有 `~/.local/bin`，先找 `~/.local/bin/resourcewatch`，再用 `GLib.find_program_in_path`。
- **生命週期**：所有物件在 `enable()` 建立、`disable()` 全部銷毀並斷開訊號與計時器（GNOME 鎖定螢幕時會停用擴充功能，也是 extensions.gnome.org 審查重點）。
- **按鈕圖示**：Shell 不能直接載入套件內的字型檔，播放按鈕改用 Cairo 自行繪製（與目前 Font Awesome 拉伸後的比例相同），不依賴字型。
- **QML 無法重用**：卡片 UI 需以 St／Clutter 重寫；`fmt.js` 的格式化邏輯可改寫成 ES 模組沿用。

## 支援版本
- GNOME 46 以上（46 為 Ubuntu 24.04 LTS；45 起擴充功能改為 ES 模組，45 以前不支援）。
- `metadata.json` 的 `shell-version` 列出實際測過的版本；GNOME 49 對擴充功能沒有破壞性變更。
- 只支援 Wayland 工作階段（GNOME 49 起預設停用 X11）。

---

## Phase 0：測試環境與決策
- [ ] 決定待決事項（見文末）。
- [ ] 建立 GNOME 測試環境（目前開發機是 KDE）：
  - 優先：Fedora GNOME 的 distrobox，安裝 `gnome-shell`、`mutter-devkit`，以 `dbus-run-session -- gnome-shell --devkit` 開啟巢狀 Shell（GNOME 48 以前為 `--nested --wayland`）。
  - 備案：GNOME Boxes／virt-manager 的 Fedora Workstation 與 Ubuntu 24.04 虛擬機（巢狀 Shell 在容器內跑不起來時；也用來測 GNOME 46）。
- [ ] 確認巢狀 Shell 內 `resourcewatch --widget` 讀得到主機資料（容器內讀不到 NVML 屬已知限制）。

## Phase 1：後端與 App 設定頁
- [ ] widget.json 新增桌面小工具位置（例如 `desktopPos: { x, y }`，null＝預設右上角），`clean` 夾在合理範圍；新增 `resourcewatch --widget-pos x,y`，供擴充功能拖曳後存檔；單元測試。
- [ ] 設定頁依桌面環境（`SystemInfo.desktop`）調整說明文字：KDE 為「右鍵桌面 → 新增小工具」，GNOME 為「在『擴充功能』App 啟用 ResourceWatch」；「面板小工具」在 GNOME 稱「頂列」。
- [ ] 更新 Architecture（widget.rs、app.js），commit。

## Phase 2：擴充功能骨架與資料流（新目錄 `gnome/`）
- [ ] `metadata.json`（uuid 例如 `resourcewatch@je991707-star.github.io`、`shell-version`、名稱與說明含 zh_TW）、`extension.js`（`Extension` 子類別，`enable`／`disable`）、`stylesheet.css`。
- [ ] `data.js`：非同步執行 `--widget`、錯誤狀態（找不到執行檔／結束碼／JSON 解析失敗，訊息同 Plasma 版）、歷史 60 點、`FileMonitor` 監看 widget.json。
- [ ] 頂列先放一個純文字按鈕顯示 CPU %，驗證資料流與 enable／disable 不洩漏（反覆停用啟用、鎖定解鎖）。
- [ ] 新增 Architecture `gnome/` 章節，commit。

## Phase 3：頂列項目與彈出卡片
- [ ] `panel.js`：依 `panel` 排出項目——CPU／GPU／RAM 為上下兩行（標籤／%）加 16 px 迷你面積圖；網路／磁碟為圖示加兩行速率與對稱條狀圖（`Fmt.short`），網路卡／顯示卡用 `panelNic`／`panelGpu`。
- [ ] 點擊項目以 PopupMenu 顯示該項的完整卡片（Phase 4 完成前先顯示文字）。
- [ ] commit。

## Phase 4：卡片（St＋Cairo）
- [ ] `theme.js`：深／淺色 token，跟隨 `org.gnome.desktop.interface color-scheme`；卡片底色套用 `opacity`。
- [ ] `chart.js`：面積曲線、對稱條狀圖、圓環、堆疊條、執行緒直條（St.DrawingArea＋Cairo，資料變更時 `queue_repaint`）。
- [ ] 六張卡片：處理器、記憶體、顯示卡、網路、磁碟、溫度，版面與 Plasma 版一致。
- [ ] 以巢狀 Shell 截圖與 Plasma 版並排比對，commit。

## Phase 5：桌面小工具
- [ ] `desktop.js`：容器加入背景圖層（主螢幕），位置取自 `desktopPos`；兩欄排列（item i 放第 i % 2 欄），處理器那欄頂端留音樂格子。
- [ ] 拖曳卡片標題列或空白移動整個小工具（放開時 `--widget-pos` 存檔）；長按卡片內容 400 ms 拖曳重排（`--widget-order`）；右鍵選單「關閉『卡片』」。
- [ ] 解析度或螢幕變更（`monitors-changed`）時重新定位、夾在螢幕內。
- [ ] commit。

## Phase 6：音樂播放格子
- [ ] `media.js`：列出 `org.mpris.MediaPlayer2.*`，監看名稱出現／消失與 `PropertiesChanged`；挑選規則同 Plasma 版（正在播放且有曲名 → 有曲名者）；播放中每秒讀 `Position`；`Previous`／`PlayPause`／`Next`。
- [ ] 專輯圖：`mpris:artUrl` 的 `file://` 直接載入，`http(s)://` 以 Soup 非同步下載到快取（GNOME 不像 Plasma 會幫忙處理）。
- [ ] 三種高度模式與門檻同 Plasma 版（< 60 隱藏、60–100 圖＋按鈕、100 以上加曲名、完整版面加時間與進度條）。
- [ ] commit。

## Phase 7：安裝、文件與發佈
- [ ] `install.sh`：偵測到 `gnome-extensions`（且 `XDG_CURRENT_DESKTOP` 含 GNOME）時，把 `gnome/` 複製到 `~/.local/share/gnome-shell/extensions/<uuid>/` 並 `gnome-extensions enable <uuid>`；提示 Wayland 下新安裝或更新的擴充功能要重新登入才會載入。`uninstall` 一併移除。
- [ ] README：安裝需求加入 GNOME 46 以上，功能介紹加 GNOME 截圖。
- [ ] 更新 Architecture（install.sh、gnome/），版本號一併更新，commit。
- [ ] （選擇性）上架 extensions.gnome.org：擴充功能本身不含執行檔，需註明須另外安裝 ResourceWatch。

---

## 風險
- **私有 API**：`_backgroundGroup` 未列入官方文件，GNOME 改版可能失效；每次大版本都要在巢狀 Shell 驗證。失效時退回只提供頂列。
- **測試環境**：開發機不是 GNOME，巢狀 Shell 在 distrobox 內可能缺 GPU／輸入裝置而跑不起來，需改用虛擬機，迭代較慢。
- **效能**：每 2 秒啟動一次子程序（每次約 1 秒取樣，大部分時間在 sleep），與 Plasma 版相同；Shell 端只做解析與重畫，需確認重畫只發生在可見的卡片。
- **工作量**：卡片 UI 全部以 St／Cairo 重寫，是整個計畫最大的部分（Phase 4–6）。

## 待決定
1. 是否兩種形式都做？建議先完成頂列（Phase 2–4），桌面小工具（Phase 5–6）視私有 API 的穩定度再決定。
2. 最低支援版本：GNOME 46（涵蓋 Ubuntu 24.04 LTS）或只支援最新版？
3. 測試環境：distrobox 巢狀 Shell，還是直接用虛擬機？
4. 是否上架 extensions.gnome.org？
