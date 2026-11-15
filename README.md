# ResourceWatch

**Rust + Tauri 2 桌面應用** · Linux · MIT License

ResourceWatch 是 Linux 桌面的系統資源監控程式：左側導覽、右側詳細頁，即時顯示 **CPU、記憶體、顯示卡、磁碟、網路、感測器與程序**，淺色／深色跟隨系統。

> v1.0.0 為全新重寫。舊版（Tauri＋毛玻璃、Slint 版、KWin 模糊外掛）保留在 `refactor/rust-tauri` 分支；v0.0.5 Plasma Widget 在 `main`／`v0.0.5` Tag。

## 功能

| 頁面 | 顯示內容 | 資料來源 |
| --- | --- | --- |
| 總覽 | 各資源目前數值＋60 秒曲線、檔案系統容量 | 以下全部 |
| 處理器 | 使用率曲線、平均頻率、溫度、各執行緒使用率 | `/proc/stat`、`/proc/cpuinfo`、hwmon |
| 記憶體 | 使用率曲線、已使用／可用／快取／總量、交換空間 | `/proc/meminfo` |
| 顯示卡 | 使用率曲線、顯示記憶體、溫度 | NVIDIA：NVML（執行期載入）；AMD：DRM sysfs |
| 儲存 | 讀寫曲線、實體磁碟速率、檔案系統容量 | `/proc/diskstats`、`/sys/block`、`/proc/self/mountinfo`＋`statvfs` |
| 網路 | 上下行曲線、實體介面速率 | `/proc/net/dev` |
| 感測器 | 所有溫度與風扇，依晶片分組 | `/sys/class/hwmon` |
| 程序 | 名稱、PID、CPU、記憶體，可點欄位排序 | `/proc/[pid]/stat` |

- 每秒更新一次；不啟動外部程序、不連網路。讀不到的數值顯示 `--`，不當成 0。
- 沒有設定檔：主題跟隨系統。

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
