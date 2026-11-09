# Resource Watch

**目前版本：v0.0.3** · KDE Plasma 6 原生系統資源監控小工具（Plasmoid）

Resource Watch 使用 **QML / Qt Quick** 製作介面，結合 KDE **KSysGuard / KSystemStats** 感測器顯示 CPU、GPU、記憶體、硬碟與網路狀態。目標環境為 **Bazzite KDE、Plasma 6、Wayland**；其他 Plasma 6 發行版的相容性仍需各自驗證。

* **小工具 ID**：`io.github.local.resourcewatch`
* **授權**：MIT
* **專案狀態**：持續開發中；功能及感測器可用性依系統、驅動與 Plasma 版本而異。

## 功能

|分類|顯示資訊|備註|
|-|-|-|
|CPU|使用率、平均頻率、溫度、處理器名稱、系統運作時間|使用率歷史圖|
|GPU|使用率、溫度、顯示卡名稱、VRAM 已用／總量|使用率及 VRAM 歷史圖；可設定 GPU 編號|
|RAM|使用率、已用／總量、Swap 已用／總量|RAM 使用率歷史圖|
|硬碟|自動辨識實體硬碟的 **NAME、SIZE、MODEL**，以及讀寫速度|不再需要手動填寫硬碟 ID 或 Label；目前完整視圖最多顯示前兩顆|
|網路|上傳／下載速度、傳輸歷史圖|可指定網路介面；指定介面時顯示 IPv4（若感測器支援）|

小工具包含 **Full View**（桌面／展開視圖）和 **Compact View**（面板簡潔視圖，顯示 CPU、GPU、RAM 使用率）。介面字串依系統語系在繁體中文與英文間切換。

無法取得的資料以 `--` 表示，不刻意將缺失數值當作 `0`。

## v0.0.3：硬碟自動偵測

本版主要調整硬碟資料來源：**不再透過設定頁輸入 `disk1Id`、`disk1Label`、`disk2Id`、`disk2Label`**。

### 裝置辨識

透過系統提供的 `lsblk` 取得實體磁碟資訊：

```bash
lsblk -J -b -d -o NAME,SIZE,MODEL,TRAN,TYPE
```

篩選條件：

* 只接受 `TYPE=disk`，且容量大於 0 的裝置。
* 接受程式允許清單中的傳輸類型：`sata`、`ata`、`pata`、`ide`、`sas`、`scsi`、`nvme`。
* 若 `TRAN` 缺失，仍可依 `nvme0n1` 等原生 NVMe 裝置名稱辨識。
* 排除不符合條件的 USB、光碟、分割區、zram 和空讀卡機等項目。

例如，若主機有 `sda`（TOSHIBA）和 `nvme0n1`（Micron），卡片會自動使用裝置名稱、二進位容量（GiB／TiB）與型號作為顯示資訊。**裝置出現順序取決於 `lsblk` 的列舉結果**，目前不提供手動排序。

### 讀取／寫入速度

硬碟讀寫速度改由 `/proc/diskstats` 的累計扇區數計算：

```text
速度（B/s）＝兩次取樣的扇區數差 × 512 ÷ 取樣間隔（秒）
```

* 首次取樣只建立基準值，須等待下一次取樣才會出現速度。
* 採樣間隔沿用設定頁的「更新間隔」。
* 裝置清單約每 **60 秒**重新偵測一次。
* 裝置不存在、資料無法讀取或計數器重設時，顯示 `--`，不虛構讀寫速度。
* 目前介面最多顯示兩顆硬碟，即使資料層偵測到更多裝置亦然。

此功能使用 Plasma 6 的 `org.kde.plasma.plasma5support` 相容層 `DataSource` 呼叫固定的本機唯讀命令。未來若 Plasma 移除或限制該介面，需要調整執行方式。

## 執行環境

* KDE **Plasma 6**（目標為 Wayland 工作階段）。
* Qt 6、Qt Quick、Kirigami、Plasma QML 模組及 KSysGuard/KSystemStats 感測器服務。
* 自動硬碟辨識所需的 `lsblk`（通常由 `util-linux` 提供）、`/proc/diskstats`、`cat`。
* `kpackagetool6` 用於安裝／升級 Plasmoid；`plasmawindowed` 用於獨立視窗測試。

一般安裝與監控**不需要 `sudo`**；但不同發行版的 Qt／Plasma 套件名稱及功能可能不同。

## 安裝、更新與解除安裝

在專案根目錄執行：

```bash
cd /home/h97m/ResourceWatch

# 安裝或更新現有小工具
./install.sh

# 獨立視窗測試
plasmawindowed io.github.local.resourcewatch

# 解除安裝
./install.sh uninstall
```

亦可手動更新：

```bash
kpackagetool6 --type Plasma/Applet --upgrade ./package
```

更新後若畫面仍顯示舊版，請重新開啟測試視窗或重新載入小工具，並確認 `package/metadata.json` 的 `KPlugin.Version` 與實際發行版本一致。

## 設定

在 Full View 右上角點擊齒輪可調整：

|設定|範圍／內容|預設|
|-|-|-|
|更新間隔|500–10,000 ms，以 500 ms 遞增|2,000 ms|
|歷史資料點數|10–120 點|30 點|
|GPU 編號|0–15|0|
|網路介面|例如 `enp3s0`；`all` 代表彙總|`all`|

硬碟現在**自動偵測，無需人工設定 ID 或顯示名稱**。預設 30 點、2 秒間隔時，歷史資料大約覆蓋 60 秒；實際圖表更新仍依感測器可用性而定。

### 檢查裝置與感測器

```bash
# 確認可被列舉的實體硬碟與傳輸介面
lsblk -o NAME,TYPE,SIZE,MODEL,TRAN

# 確認 Linux 核心的硬碟 I/O 統計
cat /proc/diskstats

# 查看網卡名稱
ip -br a

# 如果系統提供 kstatsviewer，可檢查 KDE 感測器
kstatsviewer --list
```

## 專案結構

```text
ResourceWatch/
├── AGENTS.md                     # Agent / 開發規範（另有 Agent.md）
├── README.md                     # 本文件
├── LICENSE                       # MIT 授權
├── install.sh                    # 安裝、升級、解除安裝
├── package/
│   ├── metadata.json            # 小工具 ID、名稱與版本
│   └── contents/
│       ├── config/
│       │   ├── config.qml       # 設定頁入口
│       │   └── main.xml         # KConfig 欄位與預設值
│       └── ui/
│           ├── main.qml         # Plasmoid 入口、視圖與資料連接
│           ├── FullView.qml     # 完整監控畫面
│           ├── CompactView.qml  # 面板緊湊畫面
│           ├── configGeneral.qml# 使用者設定頁
│           ├── components/
│           │   ├── SectionCard.qml # 監控卡片容器
│           │   ├── StatItem.qml    # 數值與標籤
│           │   └── Sparkline.qml   # 歷史曲線
│           └── models/
│               ├── SensorManager.qml # 感測器、硬碟發現與採樣
│               ├── FormatUtils.js    # 數值／歷史資料工具
│               └── Strings.js        # 介面文字
└── tests/
    ├── static\\\_check.sh           # 靜態檢查
    └── format\\\_utils\\\_test.js      # JavaScript 工具測試
```

## 開發與測試

```bash
cd /home/h97m/ResourceWatch
bash tests/static\\\_check.sh
node tests/format\\\_utils\\\_test.js  # 如已安裝 Node.js
plasmawindowed io.github.local.resourcewatch
```

靜態檢查只能確認部分檔案規則；**不等於 QML 一定能載入，也不代表硬體資料在實機上正確**。如有 `qmllint-qt6`，可額外用它檢查 QML；KDE 專屬模組在獨立 Lint 環境中可能需要額外的 import path。

建議實機驗證 CPU／GPU／RAM／網路資料、兩顆磁碟的名稱／容量／型號／讀寫速度，以及設定頁、Compact View 與 Full View 的基本運作。硬碟寫入速度可在有實際 I/O 時對照系統工具觀察。

## 已知限制與後續方向

* **GPU 獨立引擎**：尚未實作與 Windows 工作管理員完全相同的 3D／Copy／Video Encode／Video Decode 個別曲線；KSystemStats 提供的 GPU 資料視驅動而定。
* **GPU 使用率定義**：KDE 與 NVIDIA 驅動回報方式可能和 Windows 工作管理員不同，特別是在影片編解碼或遠端串流負載時。
* **RAM 規格資訊**：尚未顯示 RAM 頻率（MT/s）或 DDR 類型。
* **磁碟數量**：Full View 目前只顯示前兩顆，尚無自動擴充卡片與手動排序。
* **實體裝置辨識**：部分控制器、虛擬化環境或特殊傳輸方式可能不符合現有 `TRAN` 篩選清單。
* **UI 與圖表**：版面細節、各項目圖表及效能仍持續調整。

## 版本紀錄

* **v0.0.3** — 硬碟自動辨識（NAME／SIZE／MODEL），利用 `/proc/diskstats` 取得整顆硬碟讀寫速度，移除手動硬碟 ID／Label 設定。
* **v0.0.2** — CPU／GPU／RAM／磁碟／網路的完整監控介面、歷史圖、設定頁及緊湊視圖（依既有專案文件）。
* **v0.0.1** — 初始專案結構。

## 授權

本專案使用 [MIT License](LICENSE)。

