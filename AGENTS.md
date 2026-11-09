# 架構更新（HTML Widget 整合版，優先於本文件舊規劃）

**使用者已決定採用 HTML/CSS/JavaScript 作為 Plasma 6 Widget 的正式完整畫面**。本次修改已以 `HtmlView.qml` + QtWebEngine / QtWebChannel 將 `web/` 的 HTML 接入桌面小工具，`SensorManager.qml` 沿用現有感測器邏輯。原本「禁止 HTML/QtWebEngine UI」「正式 UI 只能純 QML」「FullView.qml 為活躍主視圖」等敘述已被此決策取代，不可再用來否決現在的架構。`FullView.qml` 及 `components/` 保留為舊版參考，不再是主要 UI。

- 正式 HTML 編輯位置：`web/index.html`、`web/styles.css`、`web/app.js`。
- 安裝用 HTML 副本：`package/contents/web/`；執行 `./install.sh upgrade` 同步後安裝。
- 容器與資料橋接：`package/contents/ui/main.qml`、`package/contents/ui/HtmlView.qml`。
- 資料後端：`package/contents/ui/models/SensorManager.qml`。
- 面板仍以 `CompactView.qml` 提供輕量緊湊表示，完整視圖全部由 HTML 繪製。
- 不得新增網頁任意 shell 存取；真實桌面 Widget 不可顯示示範資料當作感測器數據。保留失效時 `--`。
- 真正加入桌面前必須用 `plasmawindowed` 測試 QtWebEngine 相容性，不使用 `--no-sandbox` 迴避。

---

# Resource Watch — 開發代理指引（Agent.md）

> 專案：Resource Watch（KDE Plasma 6 / Qt 6 / QML 原生資源監控小工具）  
> 文件版本：1.0（2026-11-09）  
> 目標平台：Bazzite KDE / Fedora Atomic 系統、Plasma 6、Wayland  
> 文件用途：給 Codex / AI coding agent / 人類維護者使用的**專案架構、開發邊界與驗收規範**。

## 0. 代理必讀：現況與規則

1. **先讀現有程式碼，再動手修改。** 本專案已有 v0.1.0 原型，不是空專案。對實際檔案與本文規劃有衝突時，先盤點並回報，勿擅自刪除現有功能。
2. 本文的「現有」與「規劃」必須嚴格區分。**不能把文件規劃、靜態檢查或假資料展示當成已完成的實機驗證。**
3. 優先使用 **Qt Quick / QML + KDE 原生 Plasmoid + KSysGuard Sensors（底層由 KSystemStats 提供資料）**。不使用 HTML/QtWebEngine 作為主要 UI，也不預設需要 C++、Python 常駐程序或外部監控 daemon。
4. 首要要求是 **穩定、低資源消耗、Wayland 相容、無 root 權限、可在 Bazzite 使用者目錄安裝**。
5. 任何顯示值必須真實來自感測器。感測器缺失、載入中、斷線、數值無效時，顯示 `--` 或明確的「不可用」狀態；**禁止以 0% 假裝資料有效**。
6. 只修改本專案範圍內的檔案；不要未經使用者許可修改系統服務、GPU 驅動、KWin、Plasma 設定、全域 QML 模組或其他專案。
7. 每次工作以小規模、可驗證、可回退的變更為單位。重大重構先說明影響與相容性，再執行。
8. 沒有使用者明確指示，不執行 `git push`、發版、建立 PR，也不自行進行破壞性 Git 操作（例如 `reset --hard`、`clean -fdx`）。

## 1. 專案使命與功能範圍

開發一款可直接加入 **KDE Plasma 6 桌面與面板** 的原生硬體監控 Plasmoid；平時低負載運作，無需另外開啟系統監控視窗。

### 1.1 核心指標（必須支援）

| 指標 | 最低必備顯示 | 後續擴充 |
| --- | --- | --- |
| CPU | 總使用率（%） | 各核心、頻率、溫度 |
| GPU | 使用率（%） | 顯卡切換、VRAM 已用/總量、溫度 |
| RAM | 使用率（%）、已用/總量 | 快取/Swap（明確標示定義） |
| Disk | 讀取/寫入吞吐量（B/s、KiB/s、MiB/s 等） | 選擇實體磁碟、容量、溫度 |
| Network | 下載/上傳吞吐量（B/s、KiB/s、MiB/s 等） | 選擇網路介面、累計流量 |

速度指標是**單位時間資料傳輸速率**，不是磁碟容量、網卡協商速度或每日總流量。避免將 MB/s 與 MiB/s 混用。

### 1.2 顯示模式

- **Compact View**：面板/Dock 中的一行或緊湊資訊；不能溢出面板；點擊可展開詳細資訊。
- **Full View**：桌面小工具或面板彈出視窗；五類指標完整呈現，支援尺寸變更。
- 兩種模式**共用同一套感測器資料與設定**，不各自建立不必要的重複訂閱。

### 1.3 視覺設計原則

- Qt Quick 原生 QML，適應 KDE 深色/淺色主題與系統字型；優先 Kirigami Theme 的語意顏色。
- 可調整圓角、內距、卡片排列與標籤，所有指標元件可重用。
- 背景半透明**不等於真正高斯模糊**。毛玻璃效果列為後續研究項目，必須實際驗證在 Plasma / Wayland / 面板與桌面容器中的效果，不能用透明色冒充 blur。
- 動畫與歷史圖表必須限制資料點與刷新成本，避免常駐造成持續 GPU/CPU 高負載。

## 2. 技術選擇與非目標

### 2.1 指定技術

- KDE Plasma 6 Plasmoid（`KPackageStructure: "Plasma/Applet"`）
- Qt 6：`QtQuick`、`QtQuick.Layouts`、必要時 `QtQuick.Controls`
- KDE：`org.kde.plasma.plasmoid`、`org.kde.kirigami`
- 感測器：`org.kde.ksysguard.sensors` 的 `Sensor` / 必要時 `SensorDataModel` 或感測器搜尋模型（是否可用與精確 API 先在目標系統確認）
- `metadata.json` 作為 Plasma 6 套件資訊；必要時使用 KConfig XT 設定
- 預設不要求 CMake/C++ 編譯與 root 安裝；若未來確需原生擴充，應另立設計決策及包裝方案。

### 2.2 非目標（第一階段）

- 不做跨桌面環境通用程式（GNOME/XFCE 非第一優先）。
- 不開發獨立背景 daemon 或 Python/Node.js 常駐採樣程序。
- 不用 Chromium / QtWebEngine 長期執行 UI，除非日後有經測量的充分理由且經使用者同意。
- 不直接讀取高權限硬體節點或繞過權限；若 KDE 感測器未提供某項數據，先做清楚的 fallback。
- 不更改驅動或系統核心設定來強行提供 GPU 感測器。

## 3. 既有 v0.1.0 原型盤點（已存在，未經本機執行驗證）

目前提供過的原始碼封裝為 `plasma-resource-watch-source.zip`，內含：

```text
plasma-resource-watch-source/
├── LICENSE
├── README.md
├── install.sh
└── package/
    ├── metadata.json
    └── contents/
        └── ui/
            ├── main.qml
            └── MetricRow.qml
```

**既有 package ID：`io.github.local.resourcewatch`**。除非確認要改名與遷移使用者設定，請保留 ID；版本起點 `0.1.0`，授權 MIT。

原型的 `main.qml` 已將感測器訂閱與 UI 放在同一檔案，包含 CPU/GPU/RAM/Disk/Network 資料、簡易 Compact / Full Representation；`MetricRow.qml` 是原始的共用監控列。**以下分層結構是建議重構的「目標」，不是宣稱原型已具有全部檔案。**

## 4. 目標專案目錄

```text
ResourceWatch/
├── AGENTS.md                         # Codex 自動讀取的根目錄指引（即本文件）
├── Agent.md                          # 若需要相容使用者指定檔名，內容應與 AGENTS.md 同步
├── README.md                         # 安裝方式、支援平台、功能、故障排除
├── LICENSE
├── CHANGELOG.md                      # 規劃；發版後維護
├── install.sh                        # 使用者安裝/更新，不要求 sudo
├── tests/                            # 規劃；靜態檢查、格式化/資料運算測試
└── package/                          # 只有此目錄內檔案被包成 Plasmoid
    ├── metadata.json
    └── contents/
        ├── ui/
        │   ├── main.qml              # PlasmoidItem 入口與元件組裝
        │   ├── CompactView.qml       # 面板/工具列小型版
        │   ├── FullView.qml          # 桌面及彈出視窗完整面板
        │   ├── components/
        │   │   ├── MetricCard.qml     # CPU / GPU / RAM 等通用監控卡
        │   │   ├── MetricRow.qml      # 可沿用/移入既有元件
        │   │   ├── ProgressBar.qml    # 百分比視覺元件
        │   │   └── HistoryGraph.qml   # 規劃；歷史小圖
        │   ├── models/
        │   │   ├── SensorManager.qml  # 感測器管理、訂閱、資料正規化
        │   │   └── FormatUtils.js     # 單位與數值格式化純函式
        │   ├── configGeneral.qml     # 規劃；設定頁 UI
        │   └── configAppearance.qml  # 規劃；外觀設定 UI
        └── config/
            ├── config.qml            # 規劃；設定分頁
            └── main.xml              # 規劃；KConfig 儲存欄位
```

建立新檔案前，請先確認 QML 相對路徑、module 匯入、KPackage 打包行為及組件生命週期。**不要為了符合目錄圖一次製造大量空白檔案**；按階段移動、測試、保留相容性。

## 5. 分層架構與資料流

```text
Linux kernel / GPU driver / system interfaces
                  │
                  ▼
         KSystemStats backend
                  │
                  ▼
    KSysGuard Sensors QML interfaces
      (Sensor / SensorDataModel)
                  │
                  ▼
     SensorManager.qml (唯一邏輯層)
    ├── sensor discovery & selection
    ├── subscription & status handling
    ├── raw reading → normalized state
    ├── unit / precision formatting
    └── optional bounded history buffer
                  │
          readonly UI-facing state
             ┌────┴─────┐
             ▼          ▼
       CompactView   FullView
                          │
                      MetricCard
                       ├── Text
                       ├── ProgressBar
                       └── HistoryGraph (later)
```

### 5.1 `main.qml`（組裝層）

- 根物件使用 `PlasmoidItem`，不是 Plasma 5 舊範例的任意根 `Item`。
- 設定 `compactRepresentation`、`fullRepresentation` 與尺寸偏好；需要時依 `Plasmoid` 相關屬性控制面板行為。
- 建立**一個** `SensorManager`，將資料傳給兩種視圖，勿分別重複 polling。
- 不塞進大量格式化、感測器選擇與 UI 繪圖邏輯。
- 只使用 Plasma 6 / Qt 6 可用的 QML import，優先不帶 Qt 5 式版本尾碼。

### 5.2 `SensorManager.qml`（資料與狀態層）

職責：

1. 找出可用感測器及其 metadata（名稱、單位、可用狀態）。
2. 綁定 CPU、GPU、RAM、Disk、Network 預設值；後續支援選擇具體 GPU、磁碟與網卡。
3. 將數據正規化成明確結構（或適當的 QML property），使 UI 不依賴感測器 ID 字串。
4. 保留可用性與更新時間，區分 loading、ready、unavailable、stale/error。
5. 套用合理的刷新頻率，優先依 API 正常訂閱，不自建頻繁 shell polling。
6. 若感測器清單變更（USB 網卡、VPN、GPU 等），安全地重新選擇或顯示 unavailable。

**建議的 UI 資料契約**（邏輯示意，不是預設已有 JS class）：

```js
{
  id: "cpu",
  label: "CPU",
  status: "ready",           // loading | ready | unavailable | stale | error
  rawValue: 31.7,            // number or null
  unit: "%",                // never infer unknown sensor units
  displayValue: "31.7%",     // fallback "--"
  percent: 31.7,             // 0..100 or null; throughput has no percent
  updatedAt: 0               // sample timestamp if available
}
```

不得在值缺失時用 `Number(null)` 直接形成 `0`，也不得以 `NaN` 送進百分比長條寬度；UI 應先檢查狀態與 `isFinite`。

### 5.3 `FormatUtils.js`（單位與顯示）

- 集中處理百分比四捨五入、小數位、速率換算與未知值。
- **優先尊重 Sensors 提供的 `formattedValue` 與 unit metadata**；如需 raw 值轉換必須辨明是 bytes/s 或 bits/s。
- 百分比條以 `clamp(value, 0, 100)` 限制視覺寬度，但**不可默默裁剪並隱藏來源超界異常**；可在內部 log 中記錄。
- 負值/非數字/Infinity/空值不得顯示為正常速率。

### 5.4 `CompactView.qml`（面板）

- 簡短標籤 + 數值；應對水平/垂直面板、不同面板厚度。
- 點擊展開 `FullView`；避免點擊區太小。
- 緊湊度優先：可在後期設定顯示 CPU/GPU/RAM 或網路速度的子集合。
- 不為每個字串建立額外感測器實例。

### 5.5 `FullView.qml` 與 `MetricCard.qml`（完整畫面）

- 五類指標保持一致的卡片、行距、數字對齊與空值提示。
- 數值、歷史圖、說明、按鈕都應遵循 QML 的 layout 最小/理想尺寸，避免硬編碼導致裁切。
- 圖表只讀正規化數據，不負責資料來源與採樣。

### 5.6 設定系統

規劃使用 `contents/config/main.xml` 定義 KConfig 預設值，`config.qml` 定義設定分頁，`configGeneral.qml` 和 `configAppearance.qml` 提供 UI。預計選項：

- 刷新間隔（建議起點 **2000 ms**，可選 1000 / 2000 / 5000 ms；實際最小值依感測器 API / 效能測試調整）
- 要顯示哪些監控卡與面板指標
- GPU / 硬碟 / 網卡來源（自動或手動選擇）
- 數值精度與速率單位偏好
- 透明度、圓角、大小、主題顏色
- 是否啟用歷史曲線與最大保留點數

讀取使用者設定應支援缺省值與舊版設定遷移，不應每次升級重設。

## 6. 感測器來源與 fallback

**以下 ID 為目前原型使用的候選來源，不是對所有硬體都保證可用的固定清單。實際名稱、單位、裝置索引與 availability 必須在目標主機確認。**

| 指標 | 優先候選 ID | fallback / 注意事項 |
| --- | --- | --- |
| CPU % | `cpu/all/usage` | 多核心資料後續再支援 |
| GPU % | `gpu/all/usage` | `gpu/gpu0/usage`、`gpu/gpu1/usage` 等；先列舉再選擇 |
| RAM 已用 | `memory/physical/used` | 未取得總量時不計算百分比 |
| RAM 總量 | `memory/physical/total` | 防範 0、null、單位不一致 |
| Disk 讀/寫 | `disk/all/read`、`disk/all/write` | 彙總可能與個別磁碟所見不同，先確認單位與範圍 |
| Network 下/上 | `network/all/download`、`network/all/upload` | `all` 可能含實體/虛擬/VPN 介面，可能影響彙總意義 |

### 6.1 GPU 特別規則

- 目標環境可能使用 **NVIDIA Quadro P620**，但不能把 GPU ID 或驅動實作寫死為 NVIDIA。
- `gpu/all/usage` 有值才優先選用；否則透過實際可用 sensor 清單選擇單一卡，不要假設 GPU 一定是 `gpu0`。
- 若有多張 GPU，使用者後續可指定感測器；合併多卡使用率不能隨意平均當作「整機 GPU 使用率」。
- VRAM / 溫度不存在時，單獨隱藏或顯示不可用，核心 GPU 使用率仍可運作。
- 不可把 `nvidia-smi` 當作高頻定時 subprocess 的預設實作；除非經過權衡、授權與嚴格頻率/timeout/失敗處理。

### 6.2 Disk / Network 特別規則

- `all` 代表服務提供的彙總，不等於選定使用者最關心的單一硬碟或實體網卡。
- 介面如 `tailscale0`、Docker bridge、Wi-Fi / 乙太網路可能同時存在；不能假設彙總網路值只包含 Internet 流量。
- 開發後續的「單裝置選擇」功能時，顯示人類可理解的名稱，保存穩定 ID；裝置消失時回退到自動選擇或 unavailable，不能默默顯示錯誤裝置。

## 7. 效能、穩定性與隱私要求

- **預設感測器更新間隔：2000 ms**；不得在每次 UI re-render 中反覆啟動採樣。
- 優先一個中央資料管理者；無必要時不重複訂閱、不做同步阻塞操作。
- UI 隱藏/展開時是否降低圖表刷新頻率，需與共用資料及面板模式一併評估；不能因彈窗未展開就讓仍可見的 compact view 完全停止更新。
- 歷史圖預設可考慮 60 秒、每 2 秒一點，採**有界環形緩衝區**；不可無限制堆積採樣資料。
- 避免不受控的 QML Timer、無上限 `Repeater`/`Canvas` 更新及極高頻率動畫。
- **不得上傳或對外傳送**硬體資訊；不做 telemetry、不要求網路服務。
- 錯誤 logging 應限流，避免每 2 秒大量輸出到 journal。
- 優先遵循 Plasma 色彩、字級、縮放及 DPI；高對比與淺色模式需有基本可讀性。

## 8. Bazzite / Plasma / Wayland 相容規則

1. Bazzite 為 atomic/不可變系統，開發與安裝採使用者目錄，**不要為這個純 QML 小工具修改 OS 映像**。
2. 不依賴 X11 特有 API、`xrandr` 或視窗擷取技巧；UI 應由 Plasma 容器管理。
3. 不用 `sudo` 全域複製至 `/usr/share/plasma/plasmoids/`；預設 `kpackagetool6` 安裝至使用者空間。
4. 不執行會中斷工作階段的 `systemctl restart ...`、`plasmashell --replace`、重開機，除非使用者明確同意；常態測試使用獨立測試視窗。
5. 安裝或查詢工具若不存在，先列出需求與替代方式，不直接在主機安裝大量套件。
6. 系統支援範圍以 Plasma 6 / Qt 6 為準。看到 Plasma 5 舊文件時先比對 KF6 porting guide，**不得**直接拷貝帶版本的舊 API。

## 9. 安裝、執行與本機驗證命令

以下命令**應由使用者或經授權的代理在 Bazzite 主機執行**；本文建立時未在目標主機實測。

```bash
# 查看 Plasma 版本（可用時）
plasmashell --version

# 查看可用的測試/打包工具
command -v kpackagetool6
command -v plasmawindowed
command -v plasmoidviewer
command -v kstatsviewer

# 在專案根目錄安裝（若尚未安裝）
kpackagetool6 --type Plasma/Applet --install ./package

# 已存在時更新
kpackagetool6 --type Plasma/Applet --upgrade ./package

# 由套件 ID 打開獨立測試視窗
plasmawindowed io.github.local.resourcewatch

# 需要 plasma-sdk 且 plasmoidviewer 存在時，可模擬面板模式
plasmoidviewer -a ./package -l topedge -f horizontal

# 可選：查看感測器資訊；實際參數依本機版本先用 --help 確認
kstatsviewer --help
```

提醒：`--install` / `--upgrade` 是不同情境，不能把安裝結果當作 UI 與感測器皆正常。`plasmoidviewer` 不保證 Bazzite 預裝。

## 10. 測試與驗收標準

### 10.1 每輪修改至少驗證

- JSON：`package/metadata.json` 可解析，`KPlugin.Id` 穩定，`KPackageStructure` 與 `X-Plasma-API-Minimum-Version` 正確。
- QML：import 與相對路徑正確，沒有無法解析元件、binding loop 或明顯語法錯誤。
- 資料：有效值能正常呈現；`null` / `NaN` / `Infinity` / sensor unavailable 顯示為 `--`，而非 0。
- 互動：Compact 點擊展開；Full 在桌面和面板彈窗布局正確。
- 顯示：深淺主題、不同 DPI、狹窄面板寬度至少以可用環境驗證。
- 安全：沒有新增非必要 daemon、root 權限、網路 telemetry 或未經同意的主機設定改動。

### 10.2 目標主機實測矩陣

| 測試 | 成功標準 |
| --- | --- |
| 空閒 / CPU 負載 | CPU 使用率與 KDE 系統監控趨勢一致；不要求瞬間完全同數 |
| NVIDIA / 多 GPU | 正確偵測支援的 GPU；不支援時顯示 unavailable |
| RAM | 用量與總量語義正確，顯示百分比不超出合理範圍 |
| Disk I/O | 複製檔案時讀寫速率有適當變化；注意快取與來源磁碟 |
| Network I/O | 傳輸時速率變動符合所選介面；虛擬介面不誤標為實體流量 |
| 面板 / 桌面 | 面板不裁切，桌面可縮放，彈窗能正常展開/關閉 |
| 異常情境 | 感測器消失、服務重啟、裝置更名不使 plasmashell 當機 |
| 長時間 | 觀察數十分鐘以上使用量是否持續無界增長、是否大量噴 log |

如測試工具/主機不可得，明確標示 `NOT TESTED` / `BLOCKED`，不能自行宣告 `PASS`。

## 11. 開發里程碑（按優先順序）

### Phase 0 — 基準盤點（優先執行）

- 閱讀現有 `main.qml`、`MetricRow.qml`、`metadata.json`、`install.sh`。
- 記錄可用感測器、Bazzite / Plasma / Qt 版本，以及與系統監控的差異。
- 確認現有原型能否安裝、啟動；產出失敗日誌與原因。
- **驗收**：不修改功能即可形成可信的基準報告。

### Phase 1 — 乾淨的 QML 模組化

- 從 `main.qml` 拆出 `CompactView.qml`、`FullView.qml`、`SensorManager.qml`，保持功能等價。
- `MetricRow` 原有外觀與資料精度不得因重構退化。
- **驗收**：五項監控、兩種表示模式與舊版功能均正常。

### Phase 2 — 感測器健壯性

- 建立可靠的 status handling、fallback 與 GPU/磁碟/網卡自動探索。
- 列出實際裝置及感測器 ID，不盲信候選字串。
- **驗收**：缺失數據不會偽裝成 0，切換裝置不會崩潰。

### Phase 3 — 響應式介面

- 改善面板緊湊排版、桌面可縮放、深/淺主題、HiDPI。
- 建立通用 `MetricCard` / `ProgressBar` 等元件。
- **驗收**：水平面板、桌面和彈窗皆可正常使用。

### Phase 4 — 設定與持久化

- 加入 KConfig 設定頁，可調刷新頻率、顯示項目及監控來源。
- **驗收**：重啟 Plasma / 登出登入後設定保留；不破壞舊版設定。

### Phase 5 — 歷史圖表與進階外觀

- 增加有界時間序列、CPU/GPU/網路/Disk sparklines。
- 經實測再考慮毛玻璃、額外動畫與外觀自訂。
- **驗收**：長時間常駐沒有明顯性能或記憶體問題。

### Phase 6 — 發行準備

- 更新 README、CHANGELOG、套件 metadata、授權與螢幕截圖。
- 測試安裝、更新、解除安裝與設定相容性。
- 版本號依實際變更調整；**不得未經同意直接公開發版或上傳 KDE Store**。

## 12. AI Agent 工作流程與交付格式

每次收到開發任務，依下列順序：

1. **Inspect**：先確認專案根目錄、`git status`、適用的 `AGENTS.md`、現有程式與近期變更。
2. **Scope**：說明這輪要達成的功能及明確不處理的範圍；複雜工作先列短計畫。
3. **Implement**：最小必要變更，保留可工作狀態；不可順手大改無關模組。
4. **Validate**：做可執行的靜態與實機測試；涉及 UI 要盡可能用 `plasmawindowed` / `plasmoidviewer` 驗證。
5. **Report**：交付實際修改檔案、執行命令、測試證據、已知限制、下一步建議。

每輪回報請使用下列格式：

```text
### 本輪目標
### 變更摘要
### 修改檔案
### 驗證（PASS / FAIL / NOT TESTED / BLOCKED）
### 風險與未解決事項
### 下一輪建議
```

不要誇大測試：`qmlformat`、JSON parse、安裝成功 ≠ 已驗證 NVIDIA GPU 感測器與 Plasma Wayland 整體穩定。若實機無法存取，交付明確的人工驗證步驟。

## 13. 重要設計決策紀錄（ADR 摘要）

- **ADR-001：QML 優先，不用 HTML 當正式 UI。** 原因：與 Plasma 原生整合、減少 QtWebEngine 常駐負擔。
- **ADR-002：KDE 感測器優先。** 原因：共享 KDE 系統監控服務，不必重複發明硬體資料採樣。
- **ADR-003：中央 SensorManager。** 原因：統一 fallback、資料契約、單位與訂閱生命週期。
- **ADR-004：可用性是資料的一部分。** 原因：0% 與無法監控是兩種不同狀態。
- **ADR-005：使用者空間安裝。** 原因：Bazzite 原子系統及避免修改系統環境。
- **ADR-006：效能優先，視覺效果漸進。** 原因：桌面監控工具需要長時間背景運行。

## 14. 參考資料（以最新 Qt 6 / Plasma 6 文件為準）

- KDE Plasma Widget Tutorial：<https://develop.kde.org/docs/plasma/widget/>
- KDE Widget Setup：<https://develop.kde.org/docs/plasma/widget/setup/>
- KDE Widget Testing：<https://develop.kde.org/docs/plasma/widget/testing/>
- KDE Porting Plasmoids to KF6：<https://develop.kde.org/docs/plasma/widget/porting_kf6/>
- KDE Creating Sensor Faces（`Sensor` 與 `SensorDataModel`）：<https://develop.kde.org/docs/apps/sensor-faces/>
- Qt QML First Steps：<https://doc.qt.io/qt-6/qmlfirststeps.html>
- Qt Quick Layouts：<https://doc.qt.io/qt-6/qtquicklayouts-index.html>

---

**核心交付原則：先確認原型能在 Bazzite + Plasma 6 + Wayland 實際運作，再逐層重構和擴充。每個功能需有真實感測器資料、清楚的不可用狀態及可重現的驗證證據。**
