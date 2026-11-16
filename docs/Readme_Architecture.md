# ResourceWatch 架構說明

本文件依「程式檔／目錄」分章，描述每個檔案的職責、資料流與對外契約。修改任何程式時，請把變更寫入對應章節。

Rust + Tauri 2 單一程式：後端只負責讀 `/proc`、`/sys`（以及資料夾掃描）並回傳 JSON；前端（無打包工具、無 npm 依賴的 HTML／CSS／JS）輪詢、保存歷史、繪圖。
沒有常駐背景執行緒、沒有設定檔、不啟動外部程序、不連網路。缺值一律為 JSON `null`，前端顯示 `--`。

```text
frontend/app.js ── 每 1 秒 invoke("snapshot")                 ─┐
               ── 儀表板／應用程式頁每 2 秒 invoke("processes") │
               ── 啟動時 invoke("system_info")                 ├─ src-tauri/src/main.rs ── Mutex<Monitor>（sys.rs，保存上次計數器以算速率）
               ── 儲存頁 invoke("scan_dir") / ("dir_children") │                         ├─ Gpus（gpu.rs：NVML／DRM sysfs）
               ── 「選擇資料夾…」invoke("plugin:dialog|open")   ─┘                         └─ Mutex<du::Cache>（du.rs：資料夾大小）
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

- 依賴：`tauri` 2（`devtools` feature）、`webkit2gtk`、`gtk`（皆與 Tauri 內部同一版本，用來切換 developer extras 與自建檢查器視窗）、`serde`、`libc`（`statvfs`、`sysconf`、`getifaddrs`）、`libloading`（執行期載入 NVML，沒有 NVIDIA 驅動也能啟動）、
  `tauri-plugin-dialog`（儲存頁「選擇資料夾…」的系統資料夾選擇視窗）。
- 視窗：系統標題列、不透明、1100×720（最小 720×480）、置中。`withGlobalTauri` 讓前端用 `window.__TAURI__.core.invoke`。
- CSP：`script-src 'self'; style-src 'self' 'unsafe-inline'`——腳本只能來自 App 本身；`style-src` 允許內嵌樣式，是為了讓開發者模式的檢查器能新增 CSS（Style Attribute 與「+」新規則都是內嵌樣式）。程式碼本身仍不在 HTML 寫 `style` 屬性：長條、環形圖、曲線都用 SVG 屬性畫；資料夾色塊的比例寬度與 CPU 核心直條高度在 `render` 之後以 CSSOM（`el.style.flexGrow`、`el.style.height`）設定。
- capabilities：`core:default`＋`dialog:allow-open`，沒有 fs／shell／http 外掛。

### src/main.rs

| 指令 | 回傳 | 說明 |
| --- | --- | --- |
| `snapshot` | `Snapshot` | 一次取樣；速率類欄位第一次呼叫為 `null` |
| `processes` | `Proc[]` | 掃描所有程序；第一次呼叫 `cpu` 為 `null` |
| `system_info` | `SystemInfo` | 不變的資料（主機、機型、CPU、家目錄、發行版 logo） |
| `scan_dir(path)` | `Scan` 或錯誤字串 | 驗證路徑後在 `spawn_blocking` 執行緒掃描整棵樹（視窗不會卡住），結果取代 `du::Cache` |
| `dir_children(path)` | `Entry[]` | 從快取列出一層，大到小排序 |
| `open_devtools` | — | 以獨立視窗開啟 WebKit 檢查器（`setup` 時攔截 inspector 的 `attach` 訊號並回傳 `true` 取消嵌入 App，改由 `inspector_window` 把檢查器的 view 放進自建的 GTK 視窗「ResourceWatch 檢查器」；已開啟時只把視窗帶到前面，關閉視窗會一併關閉檢查器，反之亦然；`tauri` 啟用 `devtools` feature，release 版也可用） |
| `set_dev_mode(on)` | — | 切換 WebKit `enable-developer-extras`：關閉時右鍵沒有「檢查元素」、也開不了檢查器 |

`Monitor` 與 `du::Cache` 各放一個 `Mutex` 由 Tauri 管理（`lock` 在鎖中毒時沿用內部值）。
啟動時偵測到 NVIDIA 專有驅動（`/proc/driver/nvidia`）且使用者沒有自行設定時，設 `WEBKIT_DISABLE_DMABUF_RENDERER=1`：WebKitGTK 的 DMA-BUF 渲染在 X11 會空白、在 Wayland 縮放會落後。

### src/sys.rs

解析器都吃字串、有單元測試（`stat`、`meminfo`、`pressure`、`cpuinfo`、`net/route`、`diskstats`、`net/dev`、`mountinfo`、`[pid]/stat`、base64）。
`Monitor` 保存上次的 CPU jiffies、磁碟扇區、網路位元組、各程序 ticks 與取樣時間，速率＝差值／實際經過秒數。

JSON（camelCase）：

```text
Snapshot {
  cpu:     { usage, cores: (number|null)[], freqMhz, temp }   // %；temp 取第一個 coretemp/k10temp/zenpower/cpu_thermal 溫度
  mem:     { total, used, available, free, cached, swapTotal, swapUsed, pressure }
           // bytes；used = MemTotal − MemAvailable；free = MemFree；cached = Buffers+Cached+SReclaimable；pressure = /proc/pressure/memory some avg10（%）
  gpus:    [{ name, usage, memUsed, memTotal, temp, encoder, decoder }] // encoder/decoder：NVENC／NVDEC 使用率 %，非 NVIDIA 為 null
  disks:   [{ name, model, size, readBps, writeBps, used, total, mounts }]
           // 只列有 /sys/block/<n>/device、容量 > 0、不是光碟機（sr*）的實體磁碟；used/total/mounts 為掛載在這顆硬碟上的檔案系統加總（沒有掛載時 used/total 為 null）
  mounts:  [{ mount, device, total, used }]                     // bytes
  net:     [{ name, up, mac, ip, gateway, rxTotal, txTotal, rxBps, txBps }]
           // 只列有 /sys/class/net/<n>/device 的實體介面；up = operstate "up"；ip 為第一個 IPv4；gateway 為該介面的預設路由
  sensors: [{ chip, label, kind: "temp"|"fan", value }]         // °C 或 RPM；沒有 label 時用 temp1 等
  uptime                                                         // 秒
}
Proc       { pid, name, cpu, mem }      // cpu 以整機 100% 計；mem = rss × page size；name 為 kernel comm（最多 15 字）
SystemInfo { hostname, os, kernel, model, cpuModel, cpuCores, cpuThreads, home, osId, logo }
```

- CPU：`/proc/stat` 前 8 欄（guest 已含在 user）加總為 total，idle＋iowait 為閒置；第一列為整體，其餘為各執行緒。頻率為 `/proc/cpuinfo` 各 `cpu MHz` 平均。型號（`cpuModel`）經 `clean_cpu_name` 整理：移除 `(R)`／`(TM)`／`®`／`™`、獨立的 `CPU`／`Processor` 字樣，以及緊接在 `Processor` 前的 `N-Core`（AMD）；品牌、系列、型號與 `@ 頻率` 保留，例如 `Intel(R) Xeon(R) CPU E3-1231 v3 @ 3.40GHz` → `Intel Xeon E3-1231 v3 @ 3.40GHz`、`AMD Ryzen 7 5800X 8-Core Processor` → `AMD Ryzen 7 5800X`。
  `cpuCores`＝`(physical id, core id)` 不重複組數（讀不到為 `null`），`cpuThreads`＝`processor` 數。
- 機型：DMI `product_name`；空的或是佔位字串（`To be filled by O.E.M.`、`System Product Name`、`Default string`）時改用 `board_vendor board_name`。
- 網路：IPv4 由 `getifaddrs` 取得；閘道讀 `/proc/net/route` 目的地 `00000000` 且 flags 含 RTF_GATEWAY 的列，位址是以本機位元組序印出的網路序數值（`to_ne_bytes` 還原）。
- 檔案系統：`/proc/self/mountinfo` 只取 `/dev/` 裝置（排除 loop）。同一裝置會掛很多次（btrfs 子卷、ostree 的 `/etc` bind mount），
  取「檔案系統內路徑最淺」者，同深度取最短掛載點（Bazzite 上根分割區顯示為 `/var`）；容量用 `statvfs`（used＝blocks−bfree）。
- 檔案系統 → 實體硬碟（`disk_of`）：裝置路徑先 `canonicalize`（`/dev/mapper/…` → `/dev/dm-0`），dm／md 沿 `/sys/block/<n>/slaves` 往下找到分割區，
  分割區再由 `/sys/class/block/<n>` 連結的上一層目錄得到硬碟名稱；整顆硬碟直接格式化時就是它本身。
- 發行版 logo（`distro_logo`）：os-release `LOGO` 是圖示名稱，在 `/usr/share/icons/hicolor`（3 層內）與 `/usr/share/pixmaps` 找 PNG／SVG，
  優先順序：同名 → `<LOGO>-icon` → 其他以 `<LOGO>` 開頭（排除 `white` 版）；同順位 SVG 優先，其次尺寸最大的 PNG。讀檔（< 1 MB）轉成 `data:` URL（CSP 只允許 'self' 與 data: 圖片）。找不到為 `null`。
- 程序名稱可能含空白與括號，以第一個 `(` 到最後一個 `)` 截取。

### src/du.rs

資料夾空間分析。`scan(path)`：路徑必須是絕對路徑且為資料夾（否則回傳中文錯誤訊息），遞迴走訪一次，記錄每個資料夾的總大小到 `Cache`（`HashMap<PathBuf, u64>`）。
- 大小為實際佔用（`st_blocks × 512`，同 `du`，稀疏檔不會被高估）；不跟隨符號連結；不進入其他檔案系統（`st_dev` 不同的資料夾算 0）；讀不到的資料夾視為空的，不中斷掃描。
  硬連結會重複計算。
- `Scan { path, size, items }`：`items` 為走訪到的項目數（檔案＋資料夾＋連結）。
- `children(dir)` → `Entry { name, path, size, dir }[]`，大到小、同大小依名稱；資料夾大小取自快取（不在快取內為 0），檔案即時讀 metadata。
- 單元測試在暫存目錄建立樹（含符號連結）驗證大小、排序、項目數與路徑驗證。

### src/gpu.rs

- NVIDIA：`libnvidia-ml.so.1` 以 `libloading` 載入、`nvmlInit_v2` 只呼叫一次；每次取樣依索引取 handle，讀名稱、使用率、記憶體（v1 `used`）、溫度、編碼／解碼使用率（`nvmlDeviceGetEncoder/DecoderUtilization`，舊驅動沒有這兩個符號時為 `null`），任一呼叫失敗該欄為 `null`。函式指標與 `Library` 同生命週期（`_lib` 欄位）。
- 其他：`/sys/class/drm/cardN/device`，vendor `0x1002` AMD、`0x8086` Intel、`0x10de`（只在 NVML 載入失敗時，例如 nouveau）；讀 `gpu_busy_percent`、`mem_info_vram_used/total`、`hwmon/*/temp1_input`。
  Intel 與 nouveau 通常沒有這些檔案，對應欄位為 `null`。名稱只有廠牌（不讀 PCI ID 資料庫）。

## frontend/

### index.html

骨架：左側 `nav`（收合鈕、`#pages` 由 app.js 產生、底部「設定」與主題快速切換鈕）、右側 `#host` 主機列與 `#content`。`<html data-theme>` 由 app.js 設定。

### vendor/fontawesome/

Font Awesome Free 7.3.1（npm `@fortawesome/fontawesome-free`）只取 `fontawesome`／`solid`／`regular`／`brands` 四個 `.min.css` 與對應 `woff2`，授權見 `LICENSE.txt`（圖示 CC BY 4.0、字型 SIL OFL 1.1、程式 MIT；設定頁「關於」有註記）。
App 不連網，所以字型與 CSS 內建在前端目錄，CSP `'self'` 即可載入。

### style.css

- 圖示一律是 Font Awesome 字型（`.i` 固定 20 px 寬、置中），不再有自繪 SVG 圖示。
- 色彩 token 在 `:root`（淺色）與 `:root[data-theme="dark"]`（深色）；主題的「跟隨系統」由 app.js 換算成 `data-theme`，CSS 只有兩套值。
  深色為中性灰：主區 `#262627`、側欄 `#2f2f31`、卡片 `#353537`（圓角 16、無邊框、無透明、無模糊）。
- 指標色（`.c-*` 同時設定 stroke／fill／background）：應用程式記憶體／磁碟用量黃、快取／CPU 曲線藍、上傳／讀取桃紅、下載／寫入藍、可用綠、CPU 環洋紅、顯示卡環青、記憶體壓力橘、交換灰。
- 卡片內間距：卡片內距 20／22（上／其他）、標題下 18、分區（`.sect` 分隔線）上下各 18、區塊之間 16（圖表後的 `.foot`、`.addr` 與速率、GPU 各列）、同一區塊的行距 10（`.kv`、`.legend`、環下數值；標籤與數值間 2）、小標題 `.cap` 下 8、表格儲存格 6；卡片之間 24；卡片內一般表格首尾欄不留左右內距，與卡片文字對齊（`.zebra`、`.files` 除外）。
- 版面：側欄 220 px（收合 68 px，只留圖示）；收合鈕展開時靠左（左移 3 px 對齊選單圖示），收合時置中；儀表板用 CSS 多欄（`columns: 3 330px`）做瀑布流，卡片 `break-inside: avoid`；其他頁用 `.stack`（直向）或 `.grid`。
- 儲存頁：`.levels` 左欄資料夾名稱、右欄色塊條 `.strip`；色塊顏色 `.s0`–`.s9` 依大小（100 MB 綠 → 50 GB 以上紅），選取的色塊有黃色內框。右側 `.files` 清單。

### app.js

- 每秒 `snapshot` → `record` 把 CPU、記憶體壓力、各 GPU、磁碟讀寫總和、各網卡上下行存入 60 點歷史 → `render` 以目前頁的函式產生 HTML 換掉 `#content`。
  儲存頁與應用程式頁不隨每秒快照重畫（只更新主機列），各自在自己的事件（掃描、點擊、程序資料）後重畫。
- 程序：儀表板或應用程式頁可見（且視窗未隱藏）時每 2 秒 `processes`；儀表板顯示 CPU 前 5／記憶體前 5，應用程式頁最多 300 列、點欄位標題排序（CPU／記憶體預設降冪，缺值永遠排最後）。
- 頁面（`PAGES`）：
  - 儀表板：摘要（三層環形圖：外圈記憶體應用程式＋快取、中圈 CPU、內圈第一張顯示卡使用率，環下標註記憶體總量／處理器／顯示卡（圓環上方留 20px、與下方數值相隔 32px）；記憶體組成、CPU 型號／核心／執行緒、磁碟 ‹ › 翻頁，預設最大的檔案系統）、
    處理器（各執行緒直條：flex 排列、3 px 間隔、上方圓角，高度以 `data-h` 在 `render` 後經 CSSOM 設定；使用率曲線；分隔線下前 5 程序）、記憶體（堆疊條、組成與交換、壓力曲線（上限取近期峰值、至少 10%）、前 5 程序）、網路（頂端一列：網卡圖示（`wl*` 用 Wi-Fi，其餘乙太網路）與名稱、‹ › 切換鈕（`dashNic` 記住所選介面名稱，未選時用 `primaryNet`，即設定中的儀表板網路卡；改設定時清除），靠右為累計流量與重設鈕。重設時把該介面當下的 `rxTotal`／`txTotal` 存成起點（`localStorage` `rw-netbase`），累計顯示與起點的差；計數器小於起點（重開機歸零）時起點視為失效、顯示原值；其下 `flow`：兩欄大字速率（數字左側為顏色圓點，下方依序為標籤與近 120 筆峰值）、對稱條狀圖）、
    磁碟（檔案系統用量條）、顯示卡、磁碟 I/O（`flow`：讀取在上、寫入在下，版面同網路卡）、感測器（前 8 筆）。「顯示應用程式…」「顯示更多…」以 `data-goto` 切頁。
  - 主機列：左側為發行版 logo（`SystemInfo.logo`）；沒有 logo 檔時依 `osId` 用 Font Awesome 品牌圖示（fedora、ubuntu、debian、redhat、centos、suse），其餘用 `fa-linux`。
  - 摘要的硬碟區與磁碟卡片以實體硬碟為單位，顯示型號（NVMe 型號的 `_` 換成空格；沒有型號時用裝置名稱）與其上檔案系統的用量；未掛載的硬碟顯示「未掛載 · 容量」，滑鼠提示有裝置名稱與掛載點。
  - 顯示卡卡片：第一列型號（過長截斷）＋右側溫度；第二列左為使用率環，右為顯示記憶體／編碼器／解碼器三條使用率；分隔線下為使用率曲線。
  - 設定：主題（跟隨系統／深色／淺色）、更新間隔（0.5／1／2／5 秒，改變時重設計時器；歷史保留 120 筆：條狀圖顯示全部，曲線顯示最後 60 筆）、溫度單位（°C／°F，所有溫度都經 `temp()` 換算）、
    儀表板網路卡（自動＝有預設閘道者，或指定介面）、開發者模式（開啟後顯示「開啟檢查器」按鈕，並可用 F12／Ctrl+Shift+I 呼叫 `open_devtools`；關閉時不攔截按鍵。開發者模式下左下角多一個暫停鈕（或 F8）：暫停時 `tick` 與 `pollProcs` 直接返回、畫面不重繪，檢查器的元素樹不會被重建，切頁與點擊仍正常；關閉開發者模式即解除暫停。`applyPrefs` 每次都以 `set_dev_mode` 同步，啟動時也會關掉 wry 預設開啟的 developer extras）；「關於」顯示版本、系統與 Font Awesome 授權。
  - 網路：每張網卡一張卡，標題右側綠／紅點為連線狀態；連線中的顯示位址、大字速率與累計、上傳在上／下載在下的大條狀圖。
  - 感測器：依晶片分組。
  - 儲存：第一次進入時掃描家目錄（`SystemInfo.home`）。上方路徑、項目數、總大小、「選擇資料夾…」（`plugin:dialog|open`，`directory: true`）與重新掃描。
    每一層一列色塊（前 40 項，其餘合併為灰色「其他」），點色塊選取並展開下一層（後面的層會被截掉）；右側清單為最後一層的內容（前 200 項），點資料夾往下一層。寬度小於 40 px 的色塊不顯示名稱（滑鼠提示仍有）。
- 繪圖：`chart`（SVG `viewBox 0 0 59 100`、`preserveAspectRatio="none"`、`non-scaling-stroke`，最新點在右緣、`null` 斷線，`solid` 為實心面積）、
  `mirror`（上下對稱條狀圖：中線以上為上傳／讀取（桃紅）、以下為下載／寫入（藍），上下各自依近期最大值縮放，0 畫成中線上的短條；以 HTML flex 排 120 根長條（間隔 1 px、外側端點 2 px 圓角：上半部圓在頂端、下半部圓在底端），高度以 `data-h` 經 CSSOM 設定，不用 SVG 以免圓角被拉伸變形；用於 `flow` 與網路頁）、`bar`（多段堆疊條）、`ring`（同心圓環，`pathLength=100` 以 dasharray／dashoffset 畫各段；線端皆為圓頭；多段時反向繪製，讓前一段的圓頭疊在下一段上）。
- 偏好：`localStorage` `rw-prefs`（JSON：`theme`、`interval`、`unit`、`nic`、`dev`，壞掉時用預設值）；左下主題鈕以 `system` → `dark` → `light` 循環；`system` 時監聽 `prefers-color-scheme`。側欄收合存 `rw-collapsed`。`localStorage` 讀寫失敗時忽略。
- 所有來自系統的字串（程序名稱、感測器標籤、型號、檔名、路徑）經 `esc` 跳脫後才放進 HTML。
- `window.__TAURI__` 不存在時改用 `window.mockInvoke`：只供在一般瀏覽器載入頁面、注入假資料檢查排版，App 內不會用到。
