# ResourceWatch

**Rust + Tauri 2 桌面應用（`refactor/rust-tauri` 分支，開發中）** · Linux · Wayland · MIT License

ResourceWatch 是 Linux 桌面的系統資源監控程式，可即時查看 **CPU、GPU、RAM／Swap、實體硬碟與網路**的狀態與歷史曲線。

> 此分支已移除舊版「KDE Plasma QML + Python + HTML」Widget（v0.0.5）。舊版可由 `main` 分支、`v0.0.5` Tag 或
> `legacy/plasma-qml` 分支取得。Plasma 桌面小工具將在後續階段（Phase 5）重新實作，目前不提供。

## 功能

| 類別 | 顯示內容 | 資料來源 |
| --- | --- | --- |
| CPU | 使用率、平均時脈、溫度、運作時間、完整型號、使用率歷史圖 | `/proc/stat`、cpufreq sysfs、hwmon、`/proc/cpuinfo`、`/proc/uptime` |
| GPU | 名稱、使用率、溫度、VRAM、NVIDIA 編碼／解碼（NVENC／NVDEC） | NVIDIA：NVML（執行期載入、只初始化一次）；AMD／Intel：DRM sysfs、hwmon |
| RAM | 使用率、已用／總量、Swap、模組速度（MT/s） | `/proc/meminfo`；速度來自 SMBIOS（需權限）或使用者快取 |
| 硬碟 | 實體硬碟名稱、型號、容量、讀寫速率 | `/sys/block`、udev 資料庫、`/proc/diskstats` |
| 網路 | 上傳／下載速率、IPv4 | `/proc/net/dev`、`getifaddrs(3)` |

- 所有即時項目共用**一個**更新間隔：預設 500 ms，範圍 100–10000 ms，步進 50 ms。
- 執行期間不啟動任何外部程序（沒有 Python、`cat`、`nvidia-smi`、`lsblk`、`ip`）。
- 讀不到的數值顯示 `--`，不會當成 0。不支援的項目（例如 Intel GPU 使用率、AMD／Intel 的編解碼使用率）也顯示 `--`。
- 設定存在 `~/.config/io.github.local.resourcewatch/config.json`（依 `$XDG_CONFIG_HOME`），重新啟動後保留。

## 建置與執行

開發環境：Rust stable、tauri-cli 2、WebKitGTK 4.1、libsoup 3（在 Bazzite 上建議使用 distrobox，例如 `resourcewatch-dev`）。

```bash
cd src-tauri
cargo tauri dev            # 開發執行
cargo tauri build          # release 執行檔與 deb／rpm 套件
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

- 安裝到使用者目錄（建置＋執行檔、啟動器、圖示）：`./install.sh`；解除安裝：`./install.sh uninstall`
- release 執行檔：`src-tauri/target/release/resourcewatch`
- **NVIDIA**：NVML（`libnvidia-ml.so.1`）由主機的驅動提供。在沒有整合 NVIDIA 的 distrobox 容器內執行時，GPU 使用率、溫度與 VRAM 會顯示 `--`；
  請在主機上執行 release 執行檔。
- **RAM 速度**：一般使用者通常無法讀取 SMBIOS。若顯示 `--`，可自行建立快取
  `${XDG_STATE_HOME:-~/.local/state}/resourcewatch/ram_speed.json`，格式為 `{"speedsMTs": [3200, 3200]}`。
- 搭配 NVIDIA 專有驅動時，程式會自動設定 `WEBKIT_DISABLE_DMABUF_RENDERER=1`：X11 上避免 WebKitGTK 視窗空白；Wayland 上只在「側欄毛玻璃」關閉時設定（縮放時內容才跟得上視窗邊框；開啟毛玻璃時為了讓透明側欄正常繪製而保留 DMA-BUF，縮放會落後）。

開發用環境變數：

| 變數 | 作用 |
| --- | --- |
| `RESOURCEWATCH_MOCK=1` | 改用標示為「模擬資料 · 非實機」的假資料（UI 開發用） |
| `RESOURCEWATCH_DEBUG_STATS=1` | 每 10 秒與結束時在 stderr 輸出排程器統計 |

與系統工具比對數值：`cargo run --release --example sample -- 6 1000`（每秒輸出一筆快照 JSON）。

## 文件

- [Readme_Architecture.md](docs/Readme_Architecture.md)：各檔案職責、IPC 介面、快照 JSON 格式、資料來源
- [REFACTOR_RUST_TAURI.md](docs/REFACTOR_RUST_TAURI.md)：重構階段進度、測試證據、效能數據與已知限制
- [UI_REDESIGN_PLAN.md](docs/UI_REDESIGN_PLAN.md)：介面改版計畫

## 版本紀錄

| 版本 | 主要變更 |
| --- | --- |
| Tauri 0.1.0（開發中） | Rust + Tauri 2 桌面應用：Rust 硬體監控、統一排程器、設定持久化（此分支） |
| v0.0.5 | 舊版 Plasma Widget：NVIDIA GPU 監控與編解碼、CPU 型號與 RAM 速度、HTML 設定頁 |
| v0.0.4 | 舊版主監控視圖改為 HTML/CSS/JavaScript（QtWebEngine／QtWebChannel） |
| v0.0.3 | 實體硬碟自動偵測與讀寫速率 |
| v0.0.2 | QML 卡片介面、圖表與 KConfig 設定 |
| v0.0.1 | 專案初始化 |

## 授權

本專案採用 [MIT License](LICENSE)。
