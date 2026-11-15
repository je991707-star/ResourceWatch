# ResourceWatch

**Rust + Tauri 2 桌面應用** · Linux · MIT License

ResourceWatch 是 Linux 桌面的系統資源監控程式：儀表板式介面，即時顯示 **CPU、記憶體、顯示卡、磁碟、網路、感測器與程序**，並內建資料夾空間分析。

> v1.0.0 為全新重寫。舊版（Tauri＋毛玻璃、Slint 版、KWin 模糊外掛）保留在 `refactor/rust-tauri` 分支；v0.0.5 Plasma Widget 在 `main`／`v0.0.5` Tag。

## 功能

| 頁面 | 顯示內容 | 資料來源 |
| --- | --- | --- |
| 儀表板 | 摘要（記憶體／CPU／磁碟環形圖）、處理器、記憶體（組成、壓力）、網路、磁碟（硬碟型號）、顯示卡（含編碼／解碼）、磁碟 I/O、感測器，含 60 秒曲線與前 5 名程序 | 以下全部 |
| 應用程式 | 所有程序的名稱、PID、CPU、記憶體，可點欄位排序 | `/proc/[pid]/stat` |
| 儲存 | 資料夾空間分析：逐層色塊圖＋檔案清單，可選擇資料夾 | 遞迴掃描（不跨檔案系統、不跟隨連結） |
| 網路 | 每張實體網卡的連線狀態、IP、MAC、閘道、速率、累計流量 | `/proc/net/dev`、`getifaddrs`、`/proc/net/route` |
| 感測器 | 所有溫度與風扇，依晶片分組 | `/sys/class/hwmon` |

- CPU：`/proc/stat`、`/proc/cpuinfo`；記憶體：`/proc/meminfo`、`/proc/pressure/memory`；顯示卡：NVIDIA NVML（執行期載入）、AMD DRM sysfs；磁碟：`/proc/diskstats`、`/proc/self/mountinfo`＋`statvfs`。
- 每秒更新一次；不啟動外部程序、不連網路。讀不到的數值顯示 `--`，不當成 0。
- 設定（側欄左下）：主題、更新間隔、溫度單位、儀表板網路卡；側欄可收合。圖示使用內建的 Font Awesome Free 7.3.1。

## 建置與執行

需要 Rust stable、tauri-cli 2、WebKitGTK 4.1（Bazzite 上用 distrobox，例如 `resourcewatch-dev`）。

```bash
cd src-tauri
cargo tauri dev                     # 開發執行
cargo tauri build --no-bundle       # release 執行檔 → ../target/release/resourcewatch
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

- 安裝到使用者目錄：`./install.sh`；解除安裝：`./install.sh uninstall`
- NVML（`libnvidia-ml.so.1`）由主機驅動提供；在未整合 NVIDIA 的容器內執行時沒有顯示卡資料，請在主機執行。

## 文件

- [Readme_Architecture.md](docs/Readme_Architecture.md)：各檔案職責、指令介面與 JSON 格式
