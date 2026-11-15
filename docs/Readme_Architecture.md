# ResourceWatch 架構說明

本文件依「程式檔／目錄」分章，描述每個檔案的職責、資料流與對外契約。修改任何程式時，請把變更寫入對應章節。

Rust + Tauri 2 單一程式：後端只負責讀 `/proc`、`/sys` 並回傳 JSON；前端（無打包工具、無 npm 依賴的 HTML／CSS／JS）每秒輪詢、保存歷史、繪圖。
沒有背景執行緒、沒有設定檔、不啟動外部程序、不連網路。缺值一律為 JSON `null`，前端顯示 `--`。

```text
frontend/app.js ── 每 1 秒 invoke("snapshot")   ─┐
               ── 程序頁每 2 秒 invoke("processes") ├─ src-tauri/src/main.rs ── Mutex<Monitor>（sys.rs，保存上次計數器以算速率）
               ── 啟動時 invoke("system_info")    ─┘                         └─ Gpus（gpu.rs：NVML／DRM sysfs）
```

---

## Cargo.toml（workspace 根目錄）

只有一個成員 `src-tauri`，讓 `target/` 留在根目錄（`install.sh` 從 `target/release/resourcewatch` 安裝）。release 設定：`lto`、`codegen-units = 1`、`strip`。

## install.sh

使用者空間安裝／解除安裝，不需要 sudo（適用 Bazzite 等不可變系統）。
- `install`（預設）：能進入 distrobox 容器 `resourcewatch-dev`（可用 `RESOURCEWATCH_BOX` 改名）時在容器內 `cargo tauri build --no-bundle`，否則用主機的 cargo-tauri；
  先 `pkill -x resourcewatch`，再安裝執行檔到 `~/.local/bin/resourcewatch`、圖示到 `~/.local/share/icons/hicolor/128x128/apps/io.github.local.resourcewatch.png`、
  啟動器到 `~/.local/share/applications/resourcewatch.desktop`（檔名須等於 Wayland app_id，也就是執行檔名，KWin 才找得到圖示）。
- `uninstall`：刪除上述檔案。

## src-tauri/

### Cargo.toml、build.rs、tauri.conf.json、capabilities/default.json

- 依賴：`tauri` 2、`serde`、`libc`（`statvfs`、`sysconf`）、`libloading`（執行期載入 NVML，沒有 NVIDIA 驅動也能啟動）。
- 視窗：系統標題列、不透明、1100×720（最小 720×480）、置中。`withGlobalTauri` 讓前端用 `window.__TAURI__.core.invoke`。
- CSP：`script-src 'self'; style-src 'self'`——不能用行內 `style` 屬性，長條與圖表都用 SVG 屬性畫。
- capabilities 只有 `core:default`，沒有 fs／shell／http 外掛。

### src/main.rs

三個 Tauri 指令，`Monitor` 放在 `Mutex` 裡由 Tauri 管理（鎖中毒時沿用內部值）：

| 指令 | 回傳 | 說明 |
| --- | --- | --- |
| `snapshot` | `Snapshot` | 一次取樣；速率類欄位第一次呼叫為 `null` |
| `processes` | `Proc[]` | 掃描所有程序；第一次呼叫 `cpu` 為 `null` |
| `system_info` | `SystemInfo` | 主機名稱、作業系統、核心版本、CPU 型號（不變的資料） |

啟動時偵測到 NVIDIA 專有驅動（`/proc/driver/nvidia`）且使用者沒有自行設定時，設 `WEBKIT_DISABLE_DMABUF_RENDERER=1`：WebKitGTK 的 DMA-BUF 渲染在 X11 會空白、在 Wayland 縮放會落後。

### src/sys.rs

解析器都吃字串、有單元測試（`stat`、`meminfo`、`diskstats`、`net/dev`、`mountinfo`、`[pid]/stat`）。`Monitor` 保存上次的 CPU jiffies、磁碟扇區、網路位元組、各程序 ticks 與取樣時間，速率＝差值／實際經過秒數。

JSON（camelCase）：

```text
Snapshot {
  cpu:     { usage, cores: (number|null)[], freqMhz, temp }      // %；temp 取第一個 coretemp/k10temp/zenpower/cpu_thermal 溫度
  mem:     { total, used, available, cached, swapTotal, swapUsed } // bytes；used = MemTotal − MemAvailable；cached = Buffers+Cached+SReclaimable
  gpus:    [{ name, usage, memUsed, memTotal, temp }]
  disks:   [{ name, model, size, readBps, writeBps }]              // 只列有 /sys/block/<n>/device 的實體磁碟
  mounts:  [{ mount, device, total, used }]                        // bytes
  net:     [{ name, rxBps, txBps }]                                // 只列有 /sys/class/net/<n>/device 的實體介面
  sensors: [{ chip, label, kind: "temp"|"fan", value }]            // °C 或 RPM；沒有 label 時用 temp1 等
  uptime                                                            // 秒
}
Proc { pid, name, cpu, mem }          // cpu 以整機 100% 計；mem = rss × page size；name 為 kernel comm（最多 15 字）
SystemInfo { hostname, os, kernel, cpuModel }
```

- CPU：`/proc/stat` 前 8 欄（guest 已含在 user）加總為 total，idle＋iowait 為閒置；第一列為整體，其餘為各執行緒。頻率為 `/proc/cpuinfo` 各 `cpu MHz` 平均。
- 檔案系統：`/proc/self/mountinfo` 只取 `/dev/` 裝置（排除 loop）。同一裝置會掛很多次（btrfs 子卷、ostree 的 `/etc` bind mount），
  取「檔案系統內路徑最淺」者，同深度取最短掛載點（Bazzite 上根分割區顯示為 `/var`）；容量用 `statvfs`（used＝blocks−bfree）。
- 程序名稱可能含空白與括號，以第一個 `(` 到最後一個 `)` 截取。

### src/gpu.rs

- NVIDIA：`libnvidia-ml.so.1` 以 `libloading` 載入、`nvmlInit_v2` 只呼叫一次；每次取樣依索引取 handle，讀名稱、使用率、記憶體（v1 `used`）、溫度，任一呼叫失敗該欄為 `null`。函式指標與 `Library` 同生命週期（`_lib` 欄位）。
- 其他：`/sys/class/drm/cardN/device`，vendor `0x1002` AMD、`0x8086` Intel、`0x10de`（只在 NVML 載入失敗時，例如 nouveau）；讀 `gpu_busy_percent`、`mem_info_vram_used/total`、`hwmon/*/temp1_input`。
  Intel 與 nouveau 通常沒有這些檔案，對應欄位為 `null`。名稱只有廠牌（不讀 PCI ID 資料庫）。

## frontend/

### index.html

左側 `nav`（App 圖示、各頁按鈕附該資源的色點、底部系統資訊），右側 `header`（頁名、運作時間）與 `#content`。

### style.css

色彩 token 在 `:root`，`prefers-color-scheme: dark` 覆寫成深色版（跟隨系統，無切換鈕）。版面：側欄 190 px；卡片白底＋1 px 邊框＋圓角 10，無陰影、無透明、無模糊。
每個資源一個色：處理器藍、記憶體紫、顯示卡綠、儲存橘、網路青、感測器紅，第二條曲線（寫入、上傳）灰（`.c-*` 同時設定 stroke／fill／background）。數字用 `tabular-nums`。

### app.js

- 每秒 `snapshot` → `record` 把 CPU、記憶體%、各 GPU、磁碟讀寫總和、網路上下總和存入 60 點歷史 → `render` 以目前頁的函式產生 HTML 換掉 `#content`。
  程序頁可見（且視窗未隱藏）時每 2 秒 `processes`，最多顯示 200 列，點欄位標題排序（CPU／記憶體預設降冪，缺值永遠排最後）。
- `chart(series, max, tall)`：SVG `viewBox 0 0 59 100`、`preserveAspectRatio="none"`、`vector-effect: non-scaling-stroke`；最新點在右緣，`null` 斷線；`max` 未給時取資料最大值 ×1.1。`bar(p)`：SVG 長條。
- 所有來自系統的字串（程序名稱、感測器標籤、型號）經 `esc` 跳脫後才放進 HTML。
- 總覽的卡片（`data-goto`）點擊後切到對應頁。
- `window.__TAURI__` 不存在時改用 `window.mockInvoke`：只供在一般瀏覽器載入頁面、注入假資料檢查排版，App 內不會用到。
