# ResourceWatch

**v0.0.5（發版候選）** · KDE Plasma 6 · Wayland · HTML / CSS / JavaScript · MIT License

ResourceWatch（資源監控）是 KDE Plasma 6 的系統監控桌面小工具（Plasmoid），可在桌面上查看 **CPU、GPU、RAM、實體硬碟與網路**的即時狀態及歷史曲線。

專案以 **QML** 整合 KDE、感測器與設定系統；以 **HTML／CSS／JavaScript** 繪製主要監控介面及設定頁。UI 可以透過瀏覽器開發者工具預覽和調整，但在 Plasma 中仍以 Widget 方式執行，而非另開瀏覽器。

> **版本狀態：** 本文件對應預計提交的 **v0.0.5**。製作文件時，專案的 `package/metadata.json` 仍為 `0.0.4`，請在建立 v0.0.5 Commit／Tag 前自行更新。此 README 不代表所有功能已通過 Bazzite／Plasma 實機驗收。

## 功能一覽

| 類別 | 已實作的顯示或資料來源 | 限制與注意事項 |
| --- | --- | --- |
| **CPU** | 整體使用率、時脈、溫度、運作時間、完整處理器型號、使用率歷史圖 | CPU 型號由 `/proc/cpuinfo` 取得；其餘主要使用 KSystemStats 感測器 |
| **GPU** | 品牌與型號、使用率、溫度、VRAM 已用／總量及歷史圖；NVIDIA 編碼／解碼引擎負載 | NVIDIA 使用率優先透過 NVML；失敗時嘗試 `nvidia-smi`，無法取得或不支援的引擎顯示 `--` |
| **RAM** | 使用率、已用容量、總容量、Swap 使用量、歷史圖、可取得時的傳輸速率（MT/s） | RAM 速度依 SMBIOS 權限及資料而定，必要時須一次性快取，沒有資料則顯示 `--` |
| **硬碟** | 自動取得實體硬碟 NAME、SIZE、MODEL；各裝置讀／寫速率 | 依 `lsblk` 篩選裝置，透過 `/proc/diskstats` 的計數器差值估算速度 |
| **網路** | 上傳／下載速率、歷史圖；指定介面時的 IPv4（如有） | 依 KDE 感測器及選擇的介面而定 |
| **介面** | HTML 深色卡片、SVG 歷史圖、響應式縮放、HTML 設定頁 | HTML 由 QtWebEngine 載入，會比純 QML 介面增加部分資源開銷 |

資料缺失與實際 0% 會分開處理；不能取得的資訊不應顯示假造的使用率。

## 系統架構

```text
KDE Plasma 6 (Plasmoid)
└── package/contents/ui/main.qml
    ├── SensorManager.qml
    │   ├── KSystemStats / KSysGuard Sensors (CPU、RAM、網路、部分 GPU)
    │   ├── gpu_backend.py (DRM 裝置辨識；NVIDIA NVML / nvidia-smi)
    │   ├── hardware_info.py (CPU 型號、RAM 速度)
    │   ├── lsblk (實體硬碟清單)
    │   └── /proc/diskstats (硬碟讀寫速率)
    ├── HtmlView.qml (QtWebEngine + QtWebChannel)
    │   └── contents/web/index.html + styles.css + app.js
    └── CompactView.qml (保留的 QML 緊湊表示)

KDE 設定視窗
└── contents/config/config.qml
    └── contents/ui/configGeneral.qml (KCM / QtWebChannel)
        └── contents/web/settings/{index.html,styles.css,settings.js}
            └── contents/config/main.xml (設定值規格)
```

主畫面由 `HtmlView.qml` 將感測器快照轉為 JSON，透過 QtWebChannel 傳給 `app.js` 更新畫面。HTML **不直接執行**硬體監控命令。設定頁則反向把使用者輸入傳到 `configGeneral.qml` 的 `cfg_*` 屬性，由 KDE 的設定機制負責儲存。

### GPU 資料來源

- 先從 Linux `/sys/class/drm` 偵測 GPU 的 PCI 廠牌與裝置資訊，再對應 KDE 所選顯示卡。
- **NVIDIA：** 專用 `gpu_backend.py` 優先透過 NVML 讀取 GPU 使用率、NVENC、NVDEC；必要時使用 `nvidia-smi` 備援。這是為了避免直接採用部分 NVIDIA／KSystemStats 組合可能異常的 GPU 總負載。
- **AMD、Intel、其他：** GPU 使用率仍主要依 KSystemStats；目前不宣稱已實作與 NVIDIA 對等的獨立影片編解碼讀值。
- 沒有 NVENC 的型號（如 GT 1030）不會被視為一定有編碼器；不支援時顯示 `--`，而不是假定為 `0%`。
- NVENC、NVDEC 與 GPU-Util 是不同指標，不應直接加總。

### RAM 速度

`hardware_info.py` 讀取 `/proc/cpuinfo` 的處理器型號，以及可存取的 SMBIOS Type 17 記憶體速率。部分 Fedora／Bazzite 系統無法讓一般使用者直接讀取 DMI 原始資料。

若 RAM 速度仍為 `--`，且系統可使用 `dmidecode`，可由使用者**自行選擇**執行一次：

```bash
cd /home/h97m/ResourceWatch
sudo dmidecode --type 17 | \
  python3 package/contents/scripts/hardware_info.py --cache-ram-from-stdin
```

快取寫入目前使用者的 `${XDG_STATE_HOME:-$HOME/.local/state}/resourcewatch/ram_speed.json`。Widget 本身不需長期以 root 執行。更換記憶體或修改 BIOS 速度設定後，應重新產生快取；實際讀值依硬體韌體回報。

### 實體硬碟

- 透過 `lsblk -J -b -d -o NAME,SIZE,MODEL,TRAN,TYPE` 取得實體裝置資訊。
- 僅納入 `TYPE=disk`、容量大於 0，並符合 SATA／ATA／IDE／SAS／SCSI／NVMe 篩選條件的裝置（部分原生 NVMe 另有名稱回退判斷）。
- 過濾 USB、光碟機、空讀卡機、分割區與不符合條件的虛擬裝置。
- 從 `/proc/diskstats` 計算相鄰採樣間的讀寫速率；初次採樣通常尚無有效速率。

## 環境需求

- **KDE Plasma 6**（主要開發／測試環境：Bazzite KDE Wayland）。
- Qt 6、QtWebEngine、QtWebChannel、Kirigami、Plasma5Support、KSysGuard Sensors，以及 KDE 設定模組。
- Python 3、`lsblk`、`cat`；NVIDIA 使用率需 NVML 驅動函式庫或可用的 `nvidia-smi`。
- `kpackagetool6` 安裝 Widget；`plasmawindowed` 用於獨立視窗測試。
- `node` 為部分測試所需，**不是 Widget 正式運行的必要條件**。

可先檢查：

```bash
command -v python3 lsblk kpackagetool6 plasmawindowed
rpm -q qt6-qtwebengine qt6-qtwebchannel
```

Qt QML 模組與系統 RPM 名稱可能不同；若有缺少，請依 Bazzite 當前版本確認。**不要為了執行 Widget 而關閉 Chromium 沙盒。**

## 安裝、更新與測試

目前 Plasmoid ID：`io.github.local.resourcewatch`。

```bash
cd /home/h97m/ResourceWatch

# 程式碼與檔案結構檢查
bash tests/static_check.sh

# 安裝或更新使用者的 Plasma Widget
./install.sh upgrade

# 用獨立視窗測試，不必直接附加 plasmashell 除錯
plasmawindowed io.github.local.resourcewatch
```

初次安裝也可用 `./install.sh install`。移除：

```bash
./install.sh uninstall
```

安裝 Plasmoid 本身不需要 `sudo`。更新後請關閉舊的獨立測試視窗再開啟；已存在的桌面 Widget 有時會保留使用者先前調整的尺寸。

## HTML UI 開發

**只有一份正式 HTML 原始碼：`package/contents/web/`。** 不再使用專案根目錄的舊 `web/`，`install.sh` 也不會複製第二份 HTML 覆蓋你修改的內容。

| 檔案 | 用途 |
| --- | --- |
| `web/index.html`（位於 `package/contents/` 之下） | 監控卡片、標籤與設定入口 |
| `web/styles.css` | 顏色、字型、間距、卡片布局 |
| `web/app.js` | JSON 快照、圖表、資料更新與縮放 |
| `web/settings/index.html` | 設定頁的 HTML 結構 |
| `web/settings/styles.css` | 設定頁樣式 |
| `web/settings/settings.js` | 設定欄位驗證、QtWebChannel 同步 |

在 VS Code 使用 Live Server，或：

```bash
cd /home/h97m/ResourceWatch/package/contents/web
python3 -m http.server 8000
```

瀏覽器開啟 `http://127.0.0.1:8000/` 預覽監控 UI；設定頁為 `http://127.0.0.1:8000/settings/`。瀏覽器預覽只提供示範資料與外觀編輯，**不會寫入 KDE 設定，也不能視為實際的感測器數值**。

目前 Widget 使用 1440p 為基準的 **650 × 741** 目標尺寸，依螢幕垂直解析度等比縮放，並以 900p 作為縮小下限。HTML 在容器內維持設計比例縮放。`plasmawindowed` 有額外的視窗尺寸限制程式；桌面小工具的實際尺寸仍可能受 Plasma 保存的幾何設定與顯示縮放影響。

## HTML 設定頁

KDE 的設定視窗仍由 `configGeneral.qml` 與 `config/main.xml` 管理；HTML 只負責呈現和編輯值。設計目標是保留 KDE 原生的「套用／確定／取消」行為，**此部分仍需在目標 Plasma 版本實際驗證**。

| 項目 | KConfig 範圍／預設值 |
| --- | --- |
| 更新間隔（`updateInterval`） | 500–10,000 ms；預設 2,000 ms |
| 歷史資料點數（`historyPoints`） | 10–120；預設 30 |
| GPU 編號（`gpuIndex`） | 0–15；預設 0 |
| 網路介面（`netInterface`） | `all` 或介面名稱；預設 `all` |

目前 HTML 設定頁的更新間隔 `<input>` 標示 50ms 步進，但 `settings.js`／`configGeneral.qml` 只接受 500ms 步進與至少 500ms；這是**待同步的 UI 驗證限制**，不是已完成的 50ms 更新功能。硬碟已自動辨識，不需手動設定硬碟 ID／Label。

## 測試

```bash
cd /home/h97m/ResourceWatch
bash tests/static_check.sh
node --check package/contents/web/app.js
node --check package/contents/web/settings/settings.js
python3 -m unittest discover -s tests -p 'gpu_backend_test.py'
python3 -m unittest discover -s tests -p 'hardware_info_test.py'
```

截至撰寫此文件，對上傳的候選原始碼已完成原有 `static_check.sh`、JavaScript 語法檢查與 GPU／硬體資訊單元測試（Python 測試各 5 項，均通過）。**本次環境沒有執行 `qmllint` 或 Bazzite／Plasma 6 的實際 UI、硬體取樣與設定保存驗證。** 靜態測試通過不等於已確認所有執行階段功能。

### 常見問題

**HTML 空白或沒有載入**：從終端機執行 `plasmawindowed io.github.local.resourcewatch`，檢查 QtWebEngine、QtWebChannel、QML import 與相對路徑。先確認 `package/contents/web/index.html` 存在。

**只看到 CPU、GPU、RAM 三個數字**：檢查 `main.qml` 的 `preferredRepresentation: fullRepresentation`，以及是否已重新安裝與開啟測試視窗。

**NVIDIA GPU 使用率不同於 KSystemStats**：這是刻意採用 NVML／`nvidia-smi` 分離資料來源的結果。可執行：

```bash
python3 package/contents/scripts/gpu_backend.py | python3 -m json.tool
nvidia-smi --query-gpu=index,name,utilization.gpu --format=csv,noheader,nounits
```

**RAM 速度為 `--`**：參考前述一次性 SMBIOS 快取流程；若硬體韌體沒有提供可靠資料，應保留 `--`。

**硬碟卡片或速度缺失**：用 `lsblk -J -b -d -o NAME,SIZE,MODEL,TRAN,TYPE` 檢查是否符合篩選條件，再確認 `/proc/diskstats` 是否含對應實體裝置。

## 主要目錄

```text
ResourceWatch/
├── AGENTS.md                        # 開發 Agent 規範（部分舊 QML 章節待整理）
├── README.md                        # 專案說明
├── LICENSE                          # MIT
├── install.sh                       # 安裝／升級／移除
├── package/
│   ├── metadata.json                # Plasmoid ID、名稱與版本
│   └── contents/
│       ├── config/
│       │   ├── config.qml
│       │   └── main.xml
│       ├── ui/
│       │   ├── main.qml
│       │   ├── HtmlView.qml
│       │   ├── CompactView.qml
│       │   ├── configGeneral.qml
│       │   └── models/
│       │       ├── SensorManager.qml
│       │       └── FormatUtils.js
│       ├── scripts/
│       │   ├── gpu_backend.py
│       │   └── hardware_info.py
│       └── web/
│           ├── index.html
│           ├── styles.css
│           ├── app.js
│           └── settings/
│               ├── index.html
│               ├── styles.css
│               └── settings.js
└── tests/
    ├── static_check.sh
    ├── format_utils_test.js
    ├── gpu_backend_test.py
    └── hardware_info_test.py
```

`Strings.js` 尚存在於目前的上傳壓縮檔，但已未被執行中的程式引用，可在發布前自行刪除。Python 的 `__pycache__/` 和 `*.pyc` 為可再生快取，**不應加入 Git**。

## 版本紀錄

| 版本 | 主要變更 |
| --- | --- |
| **v0.0.5（本次候選）** | NVIDIA GPU 獨立監控與編解碼、CPU 型號與 RAM 速度探針、HTML 設定頁、HTML 單一來源、即時圖表標籤、顯示比例與畫面調整 |
| v0.0.4 | 主監控視圖改為 HTML/CSS/JavaScript，透過 QtWebEngine／QtWebChannel 整合 Plasma Widget |
| v0.0.3 | 實體硬碟自動偵測與讀寫速率 |
| v0.0.2 | QML 卡片介面、圖表與 KConfig 設定 |
| v0.0.1 | 專案初始化 |

發布 v0.0.5 前，請自行調整 `package/metadata.json` 的 `KPlugin.Version`，確認有沒有其他顯示版本的預設字串需要同步，檢查 Git 差異與測試結果，再建立 Commit／Tag。

## 授權

本專案採用 [MIT License](LICENSE)。
