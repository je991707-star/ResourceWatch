# Resource Watch v0.0.2

KDE Plasma 6 原生資源監控 Plasmoid（QML + KSysGuard Sensors）。Package ID：`io.github.local.resourcewatch`。

> ⚠️ **狀態：尚未在 Bazzite + Plasma 6 + Wayland 實機驗證（NOT TESTED）。**
> 靜態檢查與 FormatUtils 單元測試通過，不代表感測器與 UI 已正常。

## 安裝（使用者空間，不需 sudo）

```bash
./install.sh                 # 安裝或更新（版本降級時請先 uninstall）
./install.sh uninstall
plasmawindowed io.github.local.resourcewatch   # 獨立測試視窗
```

## 功能（v0.0.2）
- **CPU**：使用率、頻率、溫度、運作時間、CPU 名稱、使用率歷史圖。
- **GPU**：使用率、溫度、VRAM 已用/總量、GPU 名稱、使用率與 VRAM 歷史圖。
- **RAM**：使用率、已用/總量、Swap 已用/總量、歷史圖。
- **硬碟 1 / 2**：讀寫速度；可各自指定感測器 ID 與顯示名稱。
- **網路**：上傳/下載、歷史圖；指定網卡時顯示 IPv4。
- **設定頁**（Full 視圖右上角齒輪）：更新間隔、歷史點數、GPU 編號、硬碟、網卡。
- 介面語言依系統語系：`zh*` 顯示繁體中文，其餘英文（`models/Strings.js`）。
- 資料缺失 → `--`，歷史圖斷線，不會畫成 0。歷史緩衝有上限（預設 30 點 ≈ 60 秒）。

## 設定前先查感測器 ID

KDE 的磁碟感測器 ID 是 **UUID／雜湊碼**，不是 `sda`：

```bash
kstatsviewer --list | grep -E '^disk/.*/(read|write)'   # 硬碟：取 disk/<這段>/read 的中間那段
ip -br a                                                # 網卡名稱，例如 enp4s0
kstatsviewer --list | grep -E '^(gpu|cpu/all|os/system|memory/swap)'
```

硬碟 ID 留空＝使用所有硬碟合計；硬碟 2 留空＝隱藏。網卡填 `all`＝全部介面合計。

## 與參考圖不同之處（刻意不做）
| 項目 | 原因 |
| --- | --- |
| GPU「編碼 / 解碼」圖 | KDE 感測器沒有提供；需呼叫 `nvidia-smi`，依 Agent.md §6.1 須經使用者同意 |
| RAM 速度（MT/s）、DDR 類型 | 需 `dmidecode`（root），違反無 root 原則 |
| 網路單位 Mbps | 使用 KDE 提供的格式化字串（B/s 系列），不自行換算位元 |

## 已知問題
- **NVIDIA GPU 使用率可能偏高**：KDE 上游（ksystemstats）已知會把 NVENC/NVDEC 負載加進使用率（Sunshine 串流時明顯）。小工具如實顯示感測器值。
- 若載入失敗，終端執行 `plasmawindowed io.github.local.resourcewatch` 看錯誤；
  若指出 `updateRateLimit` 不是 `Sensor` 的屬性，刪除 `models/SensorManager.qml` 中各 `updateRateLimit:` 即可。

## 實機驗證清單
1. `plasmawindowed` 能開，各卡片有資料；齒輪能開設定頁。
2. 設定頁填入硬碟 ID / 網卡後，Disk 與 IPv4 顯示正確。
3. 歷史圖隨時間向左推進；感測器無資料處呈現斷線。
4. 加入水平 / 垂直面板，Compact 不溢出，點擊可展開。
5. 淺色 / 深色主題；縮小視窗時 Full 視圖可捲動。
6. 長時間（數十分鐘）觀察記憶體與 CPU 使用不持續增長。

## 專案結構
`Agent.md`/`AGENTS.md` 開發規範；`package/` 唯一被打包的目錄；`tests/` 靜態檢查與單元測試。
`components/MetricCard.qml`、`ProgressBar.qml` 為 v0.1.0 舊元件，v0.0.2 視圖已不使用，暫保留。
