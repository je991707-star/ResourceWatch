# ResourceWatch

**v0.0.4（README 文件版）** · KDE Plasma 6 · Wayland · HTML / CSS / JavaScript · MIT

ResourceWatch（資源監控）是一款供 **KDE Plasma 6 桌面**使用的系統資源監控小工具（Plasmoid），顯示 CPU、GPU、RAM、實體硬碟及網路活動。

目前採用 **QML 作為 KDE 小工具容器與感測器後端，HTML + CSS + JavaScript 作為完整監控畫面**。因此，調整卡片位置、字體、配色、圖表和 RWD 時，可以使用瀏覽器開發者工具與熟悉的網頁開發流程，而不需要重寫感測器程式。

> **版本提醒：** 本文件以 **v0.0.4** 為目標整理；我所能核對的最近一份專案封裝中，`package/metadata.json` 的 `KPlugin.Version` 仍為 `0.0.3`。這次僅製作 README，**沒有修改程式或宣稱 v0.0.4 已完成實機驗收**。正式發布 v0.0.4 前，應由專案維護者同步版本號並測試。

## 功能現況

| 模組 | 目前功能 | 狀態或限制 |
| --- | --- | --- |
| CPU | 整體使用率、時脈、溫度、運作時間、處理器名稱、歷史曲線 | 個別數值取決於 KDE 感測器是否可用 |
| GPU | 使用率、溫度、顯示卡名稱、VRAM 用量與歷史曲線 | 獨立的 Copy、Encode、Decode 使用率尚未接入 |
| RAM | 使用率、已用／總容量、Swap、歷史曲線 | RAM 傳輸速度（MT/s）尚無資料來源 |
| 硬碟 | 自動偵測實體磁碟的名稱、容量、型號及讀／寫速度 | 僅納入允許的裝置類型與傳輸介面；可用性需在實機確認 |
| 網路 | 上傳／下載速度、歷史曲線、網卡 IPv4（適用時） | 依 KSystemStats 與選定介面而定 |
| UI | 深色卡片、SVG 圖表、40:60 資訊／圖表比例、RWD | QtWebEngine 嵌入 Plasma 的結果仍需實機驗證 |

### 硬碟自動辨識

- 透過 `lsblk` 讀取 `NAME`、`SIZE`、`MODEL`、`TRAN`、`TYPE`。
- 只選擇容量大於 0、`TYPE=disk` 且傳輸介面符合 SATA／ATA／IDE／SAS／SCSI／NVMe 等規則的裝置，並針對原生 NVMe 名稱提供辨識後備條件。
- 排除不符合條件的 USB 裝置、光碟機、空讀卡機、zram 及分割區。
- 透過 `/proc/diskstats` 的累計讀寫計數器差值換算 **B/s**，不需手動填入 `diskId` 或 `diskLabel`。
- HTML 介面可依自動辨識的磁碟清單產生卡片；首次採樣未完成前，速度可能顯示 `--`。

## 核心架構

```text
KSystemStats / KSysGuard sensors
              +
    lsblk / /proc/diskstats
              │
              ▼
 package/contents/ui/models/SensorManager.qml
              │
              ▼
 package/contents/ui/main.qml
              │  建立 SensorManager、切換顯示視圖
              ▼
 package/contents/ui/HtmlView.qml
              │  QtWebEngine + QtWebChannel
              │  JSON 感測器快照
              ▼
 package/contents/web/index.html
              ├── styles.css   排版、主題、RWD
              └── app.js       數值更新、SVG 曲線
```

- **QML：** 負責 Plasma 的生命週期、設定介面、系統感測器及 HTML 橋接。
- **QtWebEngine：** 在小工具內渲染 HTML；不是另外開一個瀏覽器視窗。
- **QtWebChannel：** 以 `resourceWatch` 橋接物件將即時 JSON 快照傳給網頁，並供設定齒輪呼叫原生設定頁面。
- **HTML/CSS/JS：** 只處理顯示及使用者操作，不直接存取系統感測器、執行 shell 命令或讀寫任意本機檔案。

**目前的 `main.qml` 強制 `preferredRepresentation: fullRepresentation`，以避免 `plasmawindowed` 選到只顯示三項數值的 CompactView。** 專案仍保留 `CompactView.qml`，但面板體驗尚需單獨驗收。

## 執行需求

- KDE Plasma **6**，建議在 Wayland 工作階段測試。
- Qt 6 與可供 QML 匯入的 `QtWebEngine`、`QtWebChannel` 模組。
- KDE Kirigami、KSysGuard Sensors、Plasma5Support 等專案已使用的 QML 模組。
- `lsblk`（util-linux）、`/proc/diskstats`、`cat`，供既有硬碟後端使用。
- `kpackagetool6`；開發測試時建議使用 `plasmawindowed`。

Bazzite 上可先確認套件：

```bash
rpm -q qt6-qtwebengine qt6-qtwebchannel
command -v kpackagetool6
command -v plasmawindowed
```

如果缺少 Qt 模組，請依目前 Bazzite／Fedora 版本確認官方套件名稱與安裝方式。**不需要為了啟動 WebEngine 停用 Chromium 沙盒，也不建議直接把未驗證版本附加到 `plasmashell` 除錯。**

## 安裝與更新（Bazzite KDE）

專案預期位於 `/home/h97m/ResourceWatch/`；若你的使用者名稱不同，請改用實際路徑。

```bash
cd /home/h97m/ResourceWatch

# 先檢查既有程式與 HTML 同步狀態
bash tests/static_check.sh

# 安裝或升級 Plasma Widget，並同步 web/ 至安裝套件
./install.sh upgrade

# 先用獨立視窗驗證
plasmawindowed io.github.local.resourcewatch
```

實測確認 HTML 卡片和即時資料正常後，再於 KDE 桌面右鍵選擇 **新增小工具**，搜尋「資源監控」或 `Resource Watch`。

卸載：

```bash
cd /home/h97m/ResourceWatch
./install.sh uninstall
```

以上是使用者空間的小工具安裝流程，**安裝 Plasmoid 本身不需要 `sudo`**。

## 使用 VS Code 開發 HTML UI

請修改 **專案根目錄的 `web/`**：

| 檔案 | 用途 |
| --- | --- |
| `web/index.html` | CPU／GPU／RAM／硬碟／網路卡片的 HTML 元素 |
| `web/styles.css` | 顏色、字型、留白、卡片尺寸、4:6 比例、CSS 容器查詢與 RWD |
| `web/app.js` | JSON 快照接收、DOM 更新、硬碟卡片生成及 SVG 歷史圖表 |

在 VS Code 使用 Live Server，或在終端機啟動本機預覽：

```bash
cd /home/h97m/ResourceWatch/web
python3 -m http.server 8000
```

瀏覽器開啟 `http://localhost:8000/`，按 **F12** 可以檢查並調整 HTML/CSS。此模式會清楚標示**示範資料**；放進 Plasma 的 QtWebEngine 後，則會等待 QtWebChannel 的**真實快照**，不應把瀏覽器的示範數值當成硬體讀值。

完成修改後：

```bash
cd /home/h97m/ResourceWatch
./install.sh upgrade
```

`install.sh` 會把 `web/index.html`、`web/styles.css`、`web/app.js` 複製到 `package/contents/web/`，再透過 `kpackagetool6` 安裝或更新。**不要只修改 `package/contents/web/` 中的副本**；下一次執行安裝腳本時它會被覆蓋。

## 設定項目

KDE 原生設定頁目前提供：

| 設定 | 範圍／預設值 |
| --- | --- |
| 更新間隔 | 500–10,000 毫秒；預設 2,000 毫秒 |
| 歷史資料點數 | 10–120；預設 30 |
| GPU Index | 0–15；預設 0 |
| 網路介面 | 文字指定；預設 `all` |

硬碟改採自動辨識，**沒有**手動磁碟 ID 或標籤欄位。HTML 畫面的設定齒輪透過 QtWebChannel 呼叫 KDE 的小工具設定。

## 專案目錄

```text
ResourceWatch/
├── README.md
├── AGENTS.md
├── LICENSE
├── install.sh
├── web/                          # HTML 原始碼／瀏覽器開發入口
│     ├── index.html
│     ├── styles.css
│     └── app.js     
├── package/
│     ├── metadata.json              # Plasmoid ID、版本、名稱
│     └── contents/
│            ├── web/                   # 由 install.sh 同步的部署副本
│            │     ├── index.html
│            │     ├── styles.css
│            │     └── app.js
│            ├── config/
│            │     ├── config.qml
│            │     └── main.xml
│            └── ui/
│                   ├── main.qml           # Plasma 主入口
│                   ├── HtmlView.qml       # WebEngine + WebChannel
│                   ├── CompactView.qml    # 舊有緊湊視圖
│                   ├── configGeneral.qml  # KDE 原生設定
│                   └── models/
│                          ├── SensorManager.qml
│                          ├── FormatUtils.js
│                          └── Strings.js
└── tests/
       ├── static_check.sh
       └── format_utils_test.js
```

## 排查問題

### 視窗只出現 `CPU xx% GPU xx% RAM xx%`

這代表顯示了 `CompactView`，未必是感測器故障。檢查 `package/contents/ui/main.qml` 是否為：

```qml
preferredRepresentation: fullRepresentation
```

更新安裝後，關閉舊測試視窗並重新啟動 `plasmawindowed`。

### 視窗空白或 HTML 沒有載入

先確認 `package/contents/web/` 下的三個檔案存在，然後在終端機觀察錯誤：

```bash
cd /home/h97m/ResourceWatch
./install.sh upgrade
plasmawindowed io.github.local.resourcewatch 2>&1 | tee /tmp/resourcewatch-plasmawindowed.log
```

留意 `QtWebEngine`、`QtWebChannel`、QML import 或 HTML 路徑錯誤。`bash tests/static_check.sh` 通過**不代表** WebEngine 已成功在 KDE 實際載入。

### 網頁卡片出現，但數值都是 `--`

代表 HTML 已渲染，還需檢查 `HtmlView.qml` 的 WebChannel 連線和 `SensorManager.qml` 的感測器回傳；不要把無資料直接判定為 0%。

### 硬碟沒有出現或沒有速度

```bash
lsblk -J -b -d -o NAME,SIZE,MODEL,TRAN,TYPE
cat /proc/diskstats | grep -E ' (sda|nvme0n1) '
```

上面的 `sda|nvme0n1` 僅是示範裝置名稱，請依你的主機替換。確認裝置符合篩選條件，並等待至少兩次磁碟計數器採樣。

## 測試與限制

在專案根目錄執行：

```bash
bash tests/static_check.sh
node --check web/app.js
```

最近一份可核對的封裝中，上述原有靜態檢查與 JavaScript 語法檢查通過；**未執行 `qmllint`，也未在本環境進行 Bazzite Plasma 6／QtWebEngine 的完整執行驗證**。正式測試仍應涵蓋不同視窗寬度、Wayland、設定按鈕、WebChannel 初始化、感測器更新與資源占用。

尚未完成或待確認的部分：

- GPU Copy、Video Encode、Video Decode 的獨立使用率來源。
- RAM 速度（MT/s）的可靠資料來源。
- QtWebEngine 在目標 Bazzite／Plasma 組合上的載入與穩定性。
- 強制完整視圖下的面板模式體驗，以及不同 DPI／視窗尺寸的實機驗證。
- 與純 QML 版本相比，QtWebEngine 可能增加 CPU／RAM 消耗。

## 版本紀錄

| 版本 | 說明 |
| --- | --- |
| **v0.0.4（本 README）** | 整理 HTML Widget 現行架構、安裝／預覽流程、排查方式與尚待驗證項目；**本次不包含程式修改** |
| v0.0.3 | HTML + CSS + JavaScript 顯示層、QtWebEngine／QtWebChannel 整合；延續硬碟自動辨識功能 |
| v0.0.2 | QML 資源監控介面與感測器功能開發階段 |
| v0.0.1 | 專案初始化 |

正式將程式標記為 `v0.0.4` 之前，至少要同步 `package/metadata.json` 的版本號，檢查 HTML 與 QML 裡的舊版預設字串，完成實機驗證後再建立 Git Tag。

## 授權

本專案使用 **MIT License**，詳見 [`LICENSE`](LICENSE)。
