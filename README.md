# ResourceWatch

**Linux 系統資源監控** · Rust + Tauri 2 · KDE Plasma 6 桌面／面板小工具 · MIT License

ResourceWatch 是儀表板式的 Linux 系統監控程式，即時顯示 CPU、記憶體、顯示卡、磁碟、網路、感測器與程序，內建資料夾空間分析。
同一個執行檔也提供 KDE Plasma 6 小工具的資料：小工具可以放在桌面或面板上，不必開著主程式。

- 不連網路、沒有常駐背景服務；只讀 `/proc`、`/sys`。
- 讀不到的數值顯示 `--`，不會當成 0。
- 不需要 sudo 安裝，適用 Bazzite 等不可變系統。

## App

| 頁面 | 內容 |
| --- | --- |
| 儀表板 | 摘要環形圖（記憶體／CPU／顯示卡）、處理器（各執行緒直條與曲線）、記憶體組成與 SWAP、網路速率與累計流量、磁碟用量、顯示卡（含編碼器／解碼器）、磁碟 I/O、感測器摘要，以及 CPU／記憶體前 5 名程序 |
| 應用程式 | 依應用程式／背景／系統分區，同一應用的程序合併成一組並可展開；欄位有 CPU、記憶體、磁碟、GPU 與 GPU 引擎，點標題排序 |
| 儲存 | 資料夾空間分析：逐層色塊圖與檔案清單；可選磁碟或任意資料夾，掃描中即時顯示進度、可取消 |
| 網路 | 每張網卡的位址、閘道、DNS、連線速度、Wi-Fi 訊號與頻段、封包錯誤統計；「連線」卡列出 TCP 狀態、重傳率與各程序的連線對象 |
| 感測器 | 所有溫度、風扇、電壓、功耗，依裝置分卡、名稱翻成中文；需要時可一鍵載入額外的感測器模組或開放 CPU 功耗讀取（經 polkit 授權，效果到重開機為止） |
| 設定 | 主題、更新間隔、溫度單位、儀表板預設網路卡／顯示卡、開發者模式，以及桌面／面板小工具的所有設定 |

顯示卡支援 NVIDIA（執行期載入 NVML，沒有驅動也能啟動）、AMD 與 Intel（DRM sysfs）。

## 桌面小工具（KDE Plasma 6）

右鍵桌面 → 新增或管理小工具 → **ResourceWatch**。

- **卡片**：處理器、記憶體、顯示卡、網路、磁碟、溫度，兩欄排列；在設定頁切換要顯示的卡片。
- **調整位置**：在卡片內容上長按後拖曳；按住標題列或卡片間的空白則移動整個小工具。右鍵選單可以關閉滑鼠下的那張卡片。
- **音樂播放格子**：顯示目前播放的媒體（任何支援 MPRIS 的播放器），包括專輯圖、曲名、進度與上一首／播放暫停／下一首。它填滿處理器那一欄多出的空間，依高度自動切換顯示方式：

  | 可用高度 | 顯示 |
  | --- | --- |
  | 小於 60 px | 隱藏 |
  | 60–100 px | 專輯圖與按鈕 |
  | 100 px 到完整版面 | 再加上曲名與演出者 |
  | 完整版面（約 139 px） | 再加上時間與進度條 |

- **其他**：更新間隔（2／5／10 秒）、網路卡與顯示卡選擇、卡片底色透明度。

## 面板小工具

把同一個小工具拖進面板，每個項目會顯示數值與迷你圖（iStat Menus 風格）；點一下彈出該項的完整卡片。
面板的顯示項目、順序、網路卡與顯示卡都和桌面版分開設定；項目順序在設定頁拖動按鈕調整。

## 安裝

```bash
./install.sh              # 建置並安裝到 ~/.local（執行檔、圖示、啟動器、Plasma 小工具）
./install.sh uninstall    # 移除上述檔案與小工具設定
```

- 有 distrobox 容器 `resourcewatch-dev` 時在容器內建置（可用 `RESOURCEWATCH_BOX` 改名），否則用主機的 `cargo tauri`。
- 更新小工具後，Plasma 會沿用快取的舊版，需重新登入或執行 `systemctl --user restart plasma-plasmashell`。
- 沒有 KDE Plasma 6（找不到 `kpackagetool6`）時只安裝 App。

## 開發

需要 Rust stable、tauri-cli 2 與 WebKitGTK 4.1。

```bash
cd src-tauri
cargo tauri dev                    # 開發執行
cargo tauri build --no-bundle      # release 執行檔 → ../target/release/resourcewatch
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

- 前端在 `frontend/`，是不需打包工具的純 HTML／CSS／JS；小工具在 `widget/`，純 QML，沒有編譯步驟。
- 小工具取得資料的方式：`resourcewatch --widget` 輸出一次 JSON（設定與快照），不會啟動視窗。
- NVML（`libnvidia-ml.so.1`）由主機驅動提供；在沒有整合 NVIDIA 驅動的容器內執行時讀不到顯示卡，請在主機上執行。

## 文件

- [Readme_Architecture.md](docs/Readme_Architecture.md)：各檔案的職責、資料流、指令介面與 JSON 格式
- [Sensors_Improvement.md](docs/Sensors_Improvement.md)：讓額外感測器在開機時自動載入的手動設定

## 授權

MIT License。圖示使用 Font Awesome Free 7.3.1（圖示 CC BY 4.0、字型 SIL OFL 1.1、程式 MIT），App 與小工具各附一份授權檔。
