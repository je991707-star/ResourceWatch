# ResourceWatch 架構說明

本文件依「程式檔／目錄」分章，描述每個檔案的職責、資料流與對外契約。修改任何程式時，請把變更寫入對應章節。

Rust + Tauri 2 單一程式：後端只負責讀 `/proc`、`/sys`（以及資料夾掃描）並回傳 JSON；前端（無打包工具、無 npm 依賴的 HTML／CSS／JS）輪詢、保存歷史、繪圖。
沒有常駐背景執行緒、不連網路；唯一的設定檔是桌面小工具的 `~/.config/resourcewatch/widget.json`（App 自己的偏好存在 WebView 的 `localStorage`）；外部程序只有啟動時讀桌面環境版本的一次 `<shell> --version`、有無線網卡時每次取樣的 `iw dev <n> link`，以及使用者在感測器頁按下按鈕時的 `pkexec`（`elevate`）。缺值一律為 JSON `null`，前端顯示 `--`。

```text
frontend/app.js ── 每 1 秒 invoke("snapshot")                 ─┐
               ── 儀表板／應用程式頁每 2 秒 invoke("processes") │
               ── 網路頁每 2 秒 invoke("connections")         │
               ── 啟動時 invoke("system_info")；新程序名 invoke("app_icons") ├─ src-tauri/src/main.rs ── Mutex<Monitor>（sys.rs，保存上次計數器以算速率）
               ── 儲存頁 invoke("scan_dir") / ("dir_children") │                         ├─ Gpus（gpu.rs：NVML／DRM sysfs）
               ── 「選擇資料夾…」invoke("plugin:dialog|open")   ─┘                         └─ Mutex<du::Cache>（du.rs：資料夾大小）
Plasma 桌面小工具 ── 定時執行 `resourcewatch --widget` ── widget.rs：讀 widget.json＋取樣兩次 → stdout JSON（不啟動 Tauri）
```

---

## Cargo.toml（workspace 根目錄）

只有一個成員 `src-tauri`，讓 `target/` 留在根目錄（`install.sh` 從 `target/release/resourcewatch` 安裝）。release 設定：`lto`、`codegen-units = 1`、`strip`。

## install.sh

使用者空間安裝／解除安裝，不需要 sudo（適用 Bazzite 等不可變系統）。
- `install`（預設）：能進入 distrobox 容器 `resourcewatch-dev`（可用 `RESOURCEWATCH_BOX` 改名）時在容器內 `cargo tauri build --no-bundle`，否則用主機的 cargo-tauri；
  先 `pkill -x resourcewatch`，再安裝執行檔到 `~/.local/bin/resourcewatch`、圖示到 `~/.local/share/icons/hicolor/128x128/apps/io.github.local.resourcewatch.png`、
  啟動器到 `~/.local/share/applications/resourcewatch.desktop`（檔名須等於 Wayland app_id，也就是執行檔名，KWin 才找得到圖示）。
  有 `kpackagetool6`（KDE Plasma 6）時安裝 `widget/` 桌面小工具（`io.github.local.resourcewatch.widget`）：已安裝則 `--upgrade` 並提示 plasmashell 會快取舊版 QML，需重新登入或 `systemctl --user restart plasma-plasmashell`；沒有 `kpackagetool6` 時略過並提示。
  舊版 Plasma Widget（id `io.github.local.resourcewatch`，v0.0.5）仍在時提示移除指令；`has_applet` 以整行比對清單，因為舊 id 是新 id 的前綴。
- `uninstall`：刪除上述檔案、移除桌面小工具，並刪除小工具設定資料夾 `${XDG_CONFIG_HOME:-~/.config}/resourcewatch`。

## src-tauri/

### Cargo.toml、build.rs、tauri.conf.json、capabilities/default.json

- 依賴：`tauri` 2（`devtools` feature）、`serde_json`（`--widget` 輸出與 widget.json）、`webkit2gtk`、`gtk`（皆與 Tauri 內部同一版本，用來切換 developer extras 與自建檢查器視窗）、`serde`、`libc`（`statvfs`、`sysconf`、`getifaddrs`）、`libloading`（執行期載入 NVML，沒有 NVIDIA 驅動也能啟動）、
  `tauri-plugin-dialog`（儲存頁「選擇資料夾…」的系統資料夾選擇視窗）。
- 版本：只寫在 `src-tauri/Cargo.toml`（目前 0.4.2，對應 tag `v0.4.2`）；`tauri.conf.json` 不設 `version`，Tauri 會沿用 Cargo 的版本。設定頁「關於」的版本字串在 app.js、桌面小工具版本在 `widget/metadata.json`，改版時一併更新。
- 視窗：系統標題列、不透明、1100×720（最小 720×480）、置中。`withGlobalTauri` 讓前端用 `window.__TAURI__.core.invoke`。
- CSP：`script-src 'self'; style-src 'self' 'unsafe-inline'`——腳本只能來自 App 本身；`style-src` 允許內嵌樣式，是為了讓開發者模式的檢查器能新增 CSS（Style Attribute 與「+」新規則都是內嵌樣式）。程式碼本身仍不在 HTML 寫 `style` 屬性：長條、環形圖、曲線都用 SVG 屬性畫；資料夾色塊的比例寬度與 CPU 核心直條高度在 `render` 之後以 CSSOM（`el.style.flexGrow`、`el.style.height`）設定。
- capabilities：`core:default`＋`dialog:allow-open`，沒有 fs／shell／http 外掛。

### src/main.rs

| 指令 | 回傳 | 說明 |
| --- | --- | --- |
| `snapshot` | `Snapshot` | 一次取樣；速率類欄位第一次呼叫為 `null` |
| `processes` | `Proc[]` | 掃描所有程序；第一次呼叫 `cpu` 為 `null` |
| `connections` | `{ list: Conn[], tcpStates: { 狀態: 數量 }, outSegs, retransSegs }`；`Conn`＝`{ pid, name, proto: "tcp"\|"udp", local, remote, state }` | `tcpStates` 為全系統（本網路命名空間所有 socket，含 inode 0）TCP 各狀態數；`outSegs`／`retransSegs` 為 `/proc/net/snmp` 的累計值；`list` 為讀得到 `/proc/<pid>/fd` 的程序（自己的）所持有的 socket；`remote` 在監聽／未連線時為 null；`state` 為小寫 TCP 狀態名，未連線的 UDP 為 `"listen"`；在 `spawn_blocking` 執行緒執行 |
| `system_info` | `SystemInfo` | 不變的資料（主機、發行版、桌面環境、CPU、記憶體規格、家目錄、發行版 logo） |
| `app_icons(names)` | `{ 名稱: { icon, name } 或 null }` | 程序名稱或 desktop id → 應用圖示 data URL（找不到圖示檔時 `icon` 為 null）與 `.desktop` 名稱（`icons.rs`）；第一次呼叫要建立索引，所以在 `spawn_blocking` 執行緒執行 |
| `scan_dir(path)` | `Scan` 或錯誤字串 | 驗證路徑後在 `spawn_blocking` 執行緒掃描整棵樹（視窗不會卡住），結果取代 `du::Cache`；執行緒數：路徑在傳統硬碟上（`sys::rotational`）用 4 個（不用全部核心：太多執行緒會讓磁頭在各自的資料夾間來回尋軌），否則每個 CPU 一個 |
| `scan_progress()` | `Progress { items, top }` | 執行中的掃描目前走訪到的項目數，與根目錄各項目到目前為止累計的大小（大到小） |
| `scan_cancel()` | — | 要求中止執行中的掃描；`scan_dir` 回傳「已取消掃描」，快取維持上一次的結果 |
| `dir_children(path)` | `Entry[]` | 從快取列出一層，大到小排序 |
| `elevate(action)` | `null` 或錯誤字串 | 以 `pkexec /bin/sh -c` 執行寫死的 root 指令（`elevate_script`，只接受 `rapl`／`modules`，其他值不執行），效果只到重開機；在 `spawn_blocking` 執行緒等待 polkit 密碼視窗。`rapl`：把 `/sys/class/powercap/intel-rapl:*/energy_uj` 改為呼叫者主要群組（`id -gn $PKEXEC_UID`）可讀、`0440`，不開放給所有使用者（PLATYPUS 側通道，CVE-2020-8694）；`modules`：`modprobe drivetemp` 與 `modprobe it87 ignore_resource_conflict=1`，任一失敗即回傳失敗。結束碼 126／127 →「已取消或授權失敗」 |
| `open_devtools` | — | 以獨立視窗開啟 WebKit 檢查器（`setup` 時攔截 inspector 的 `attach` 訊號並回傳 `true` 取消嵌入 App，改由 `inspector_window` 把檢查器的 view 放進自建的 GTK 視窗「ResourceWatch 檢查器」；已開啟時只把視窗帶到前面，關閉視窗會一併關閉檢查器，反之亦然；`tauri` 啟用 `devtools` feature，release 版也可用） |
| `set_dev_mode(on)` | — | 切換 WebKit `enable-developer-extras`：關閉時右鍵沒有「檢查元素」、也開不了檢查器 |
| `get_widget_config()` | `widget::Config` | 讀桌面小工具設定（`widget::load`） |
| `set_widget_config(cfg)` | `widget::Config` 或錯誤字串 | 整理後寫入 widget.json，回傳實際存入的值 |

`main()` 第一個參數為 `--widget` 時直接執行 `widget::run()`、為 `--widget-order <清單>`／`--widget-app on|off` 時執行 `widget::set_order`／`set_app`（失敗時錯誤訊息寫到 stderr、結束碼 1），之後結束，不建立 Tauri／GTK／WebKit。
`Monitor` 與 `du::Cache` 各放一個 `Mutex` 由 Tauri 管理（`lock` 在鎖中毒時沿用內部值）。
啟動時偵測到 NVIDIA 專有驅動（`/proc/driver/nvidia`）且使用者沒有自行設定時，設 `WEBKIT_DISABLE_DMABUF_RENDERER=1`：WebKitGTK 的 DMA-BUF 渲染在 X11 會空白、在 Wayland 縮放會落後。

### src/sys.rs

解析器都吃字串、有單元測試（`stat`、`meminfo`、`cpuinfo`（含 CPU 名稱整理）、記憶體規格（udev）、`net/route`、`diskstats`、`net/dev`、網卡型號（udev）、`iw dev link`、`resolv.conf` nameserver、`mountinfo`、`[pid]/stat`、`[pid]/io`、cgroup 分類與 desktop id、base64）。
`Monitor` 保存上次的 CPU jiffies、磁碟扇區與 io_ticks、網路位元組、各程序 ticks 與磁碟位元組、取樣時間，速率＝差值／實際經過秒數。

JSON（camelCase）：

```text
Snapshot {
  cpu:     { usage, cores: (number|null)[], freqMhz, temp }   // %；temp 取第一個 coretemp/k10temp/zenpower/cpu_thermal 溫度
  mem:     { total, used, available, free, cached, swapTotal, swapUsed }
           // bytes；used = MemTotal − MemAvailable；free = MemFree；cached = Buffers+Cached+SReclaimable
  gpus:    [{ name, usage, memUsed, memTotal, temp, encoder, decoder }] // encoder/decoder：NVENC／NVDEC 使用率 %，非 NVIDIA 為 null
  disks:   [{ name, model, size, readBps, writeBps, busy, used, total, mounts }]
           // busy：活動時間 %（diskstats io_ticks 差值／經過時間，上限 100）
           // 只列有 /sys/block/<n>/device、容量 > 0、不是光碟機（sr*）的實體磁碟；used/total/mounts 為掛載在這顆硬碟上的檔案系統加總（沒有掛載時 used/total 為 null）
  mounts:  [{ mount, device, total, used }]                     // bytes
  net:     [{ name, up, mac, ip, prefix, ipv6, gateway, model, driver, driverOpen, speed, duplex, mtu,
             rxTotal, txTotal, rxBps, txBps, rxPackets, txPackets, rxErrors, txErrors, rxDropped, txDropped }]
           // 只列有 /sys/class/net/<n>/device 的實體介面；up = operstate "up"；ip 為第一個 IPv4、prefix 為其遮罩位元數；
           // ipv6 為所有 IPv6（"位址/前綴"）；gateway 為該介面的預設路由；speed 為 Mb/s（sysfs 回 -1 時為 null，Wi-Fi 常見）；
           // duplex 只取 "full"／"half"；封包／錯誤／丟棄為 /proc/net/dev 累計值
           // wifi: { ssid, bssid, freq (MHz), signal (dBm), rxRate, txRate (Mb/s), generation (4–7), width (MHz) } | null（非無線或未連線）
  dns:     ["1.1.1.1", …]                                        // 全系統 nameserver
  rapl_locked → raplLocked                                       // 有 RAPL 區域但 energy_uj 讀不到（需 elevate("rapl")）
  sensors: [{ chip, device, label, kind: "temp"|"fan"|"percent"|"volt"|"power", value, crit }] // °C、RPM、%、V、W；crit 為臨界溫度 °C（沒有時 null）；沒有 label 時用 temp1 等；device 為裝置顯示名稱（sensor_device）
  uptime                                                         // 秒
}
Proc       { pid, name, cpu, mem, state, nice, kind, unit, app, disk, gpu, gpuEngine }
           // cpu 以整機 100% 計；mem = rss × page size；name 為 kernel comm（最多 15 字）；state 為 stat 狀態字母（R/S/D/T/Z…）
           // kind："app"|"bg"|"sys"；unit 為 cgroup 最後一段（核心執行緒為 "kernel"）；app 為 desktop id；disk 為 read_bytes+write_bytes 每秒（讀不到或第一次為 null）
           // gpu：最忙引擎的 %（gpu.rs `proc_util`；NVML 涵蓋所有程序，沒有資料即 0；只有 DRM 時別人的程序為 null）；gpuEngine："GPU <索引> - <引擎>"，使用率為 0 時 null
SystemInfo { hostname, os, desktop, kernel, cpuModel, cpuCores, cpuThreads, home, osId, logo, memSpec }
```

- CPU：`/proc/stat` 前 8 欄（guest 已含在 user）加總為 total，idle＋iowait 為閒置；第一列為整體，其餘為各執行緒。頻率為 `/proc/cpuinfo` 各 `cpu MHz` 平均。型號（`cpuModel`）經 `clean_cpu_name` 整理：移除 `(R)`／`(TM)`／`®`／`™`、獨立的 `CPU`／`Processor` 字樣，以及緊接在 `Processor` 前的 `N-Core`（AMD）；品牌、系列、型號與 `@ 頻率` 保留，例如 `Intel(R) Xeon(R) CPU E3-1231 v3 @ 3.40GHz` → `Intel Xeon E3-1231 v3 @ 3.40GHz`、`AMD Ryzen 7 5800X 8-Core Processor` → `AMD Ryzen 7 5800X`。
- 記憶體規格（`memSpec`）：SMBIOS 原始表只有 root 可讀，改讀 systemd-udev 的 `/run/udev/data/+dmi:id`（`E:MEMORY_DEVICE_<n>_SIZE／TYPE／CONFIGURED_SPEED_MTS／SPEED_MTS`）。容量加總為 GB（整數不帶小數），類型取第一條非 Unknown，速度取各條實際運作速度的最小值（沒有時用額定速度），例如 `16 GB DDR3 1600`；沒有任何記憶體條資料（虛擬機）時為 `null`。
  `cpuCores`＝`(physical id, core id)` 不重複組數（讀不到為 `null`），`cpuThreads`＝`processor` 數。
- 發行版（`os`）：os-release `NAME` + `VERSION_ID`（例如 `Bazzite 45`）；沒有 `VERSION_ID` 時只有名稱，沒有 `NAME` 時用 `PRETTY_NAME`。
- 桌面環境（`desktop`）：`XDG_CURRENT_DESKTOP` 第一項對應顯示名稱與 shell 執行檔（KDE → `KDE Plasma`／`plasmashell`、GNOME／`gnome-shell`、Cinnamon、Xfce、MATE、LXQt、Budgie），啟動時執行一次 `<執行檔> --version`，取第一個點分數字為版本（`first_version`），例如 `KDE Plasma 6.8.2`；執行檔不存在時只有名稱，未知桌面直接顯示 XDG 名稱，沒有該變數時為 `null`。
- 網路：IPv4（含遮罩→前綴長度）與 IPv6 由 `getifaddrs` 取得（`if_addrs`）；`/proc/net/dev` 每介面保留 16 個計數（`self.net` 存整列，速率取索引 0／8）；
  型號讀 udev `/run/udev/data/n<ifindex>` 的 `ID_VENDOR_FROM_DATABASE` 第一個字＋`ID_MODEL_FROM_DATABASE` 中的晶片名（第一個只含英數與 `-/*`、且同時有字母與數字的字，取 `/` 前並去掉 `*`，如 RTL8111、AX210、I219-LM；找不到時用整串並去掉括號附註）；驅動為 `device/driver` 連結檔名；`driverOpen` 為驅動模組 `device/driver/module/taint` 不含 `P`（專有授權）時 true，核心內建驅動沒有 module 連結、讀到空字串，也算開源；沒有驅動時為 null；
  速度、雙工、MTU 讀 `/sys/class/net/<n>/{speed,duplex,mtu}`；DNS 先讀 `/run/systemd/resolve/resolv.conf`（resolved 的上游清單，`/etc/resolv.conf` 只有 stub 127.0.0.53），空的才退回 `/etc/resolv.conf`；
  Wi-Fi：只對有 `/sys/class/net/<n>/wireless` 的介面每次取樣執行 `iw dev <n> link`（`parse_iw_link`；輸出 "Not connected." 時為 null），
  世代由 rx bitrate 的 `EHT`／`HE`／`VHT`／`MCS`（HT）判斷為 7／6／5／4，頻寬取 `<n>MHz` 字樣；
  連線（`connections`）：先走訪 `/proc/*/fd` 把 `socket:[inode]` 對到 pid，再解析 `/proc/net/{tcp,tcp6,udp,udp6}`（`parse_proc_net`，第 9 欄為 inode；inode 0 為 TIME_WAIT 等已無程序持有者，略過）；
  `parse_snmp_tcp` 取 `/proc/net/snmp` 第一對 `Tcp:` 標題列／數值列對成 名稱→數值；
  位址（`proc_addr`）的 IP 是以本機位元組序印出的 32 位元字組（IPv4 一個、IPv6 四個），port 為大端十六進位；IPv6 的 `::ffff:a.b.c.d` 轉成 IPv4 顯示，全零位址且 port 0 視為無遠端；
  閘道讀 `/proc/net/route` 閘道讀 `/proc/net/route` 目的地 `00000000` 且 flags 含 RTF_GATEWAY 的列，位址是以本機位元組序印出的網路序數值（`to_ne_bytes` 還原）。
- 檔案系統：`/proc/self/mountinfo` 只取 `/dev/` 裝置（排除 loop）。同一裝置會掛很多次（btrfs 子卷、ostree 的 `/etc` bind mount），
  取「檔案系統內路徑最淺」者，同深度取最短掛載點（Bazzite 上根分割區顯示為 `/var`）；容量用 `statvfs`（used＝blocks−bfree）。
- 檔案系統 → 實體硬碟（`disk_of`）：裝置路徑先 `canonicalize`（`/dev/mapper/…` → `/dev/dm-0`），dm／md 沿 `/sys/block/<n>/slaves` 往下找到分割區，
  分割區再由 `/sys/class/block/<n>` 連結的上一層目錄得到硬碟名稱；整顆硬碟直接格式化時就是它本身。
- `cpu_model()`：`/proc/cpuinfo` 經 `parse_cpuinfo` 的型號（同 `cpuModel`），供 `--widget` 使用。
- `rotational(path)`：在完整的 mountinfo 中找包含該路徑、掛載點最長的那筆（同長度取後面的，疊在上面的掛載），經 `disk_of` 讀 `/sys/block/<disk>/queue/rotational`，為 1 表示傳統硬碟；找不到時當成非傳統硬碟。
- 發行版 logo（`distro_logo`）：os-release `LOGO` 是圖示名稱，在 `/usr/share/icons/hicolor`（3 層內）與 `/usr/share/pixmaps` 找 PNG／SVG，
  優先順序：同名 → `<LOGO>-icon` → 其他以 `<LOGO>` 開頭（排除 `white` 版）；同順位 SVG 優先，其次尺寸最大的 PNG。讀檔（< 1 MB）轉成 `data:` URL（CSP 只允許 'self' 與 data: 圖片）。找不到為 `null`。
- 程序名稱可能含空白與括號，以第一個 `(` 到最後一個 `)` 截取。
- 程序分類（`classify`）：`/proc/<pid>/cgroup` 的 `0::` 路徑，最後一段為 unit。路徑不在 `/user-<自己 uid>.slice/` 下 → `sys`（系統服務、其他使用者）；在其下且上一層為 `app.slice`、unit 以 `app-` 開頭 → `app`；其他（session.slice、background.slice、`session-N.scope`、app.slice 內非 `app-` 的服務）→ `bg`。pid 2 與 ppid 為 2 的核心執行緒為 `sys`、unit `kernel`。
  `app_id`：`app[-<launcher>]-<id>-<n>.scope` 去掉最後一段、`app-<id>[@<x>].service` 去掉 `@` 之後，`\x2d` 還原為 `-`，後面是反向網域名稱時去掉 `flatpak-`／`gnome-`／`kde-` 啟動器前綴（`app-flatpak-org.fcitx.Fcitx5-3782739276.scope` → `org.fcitx.Fcitx5`）。
- 程序磁碟（`parse_proc_io`）：`/proc/<pid>/io` 的 `read_bytes + write_bytes`，只讀得到自己的程序，其他為 `null`。io 讀得到的程序即為「自己的程序」，交給 `Gpus::proc_util` 讀 DRM fdinfo。
- 感測器：`/sys/class/hwmon/*` 的檔案經 `hwmon_kind` 分類：`tempN_input`（m°C）、`fanN_input`（RPM）、`inN_input`（mV → V）、`powerN_input`／`powerN_average`（µW → W，兩者都有時只取 `_input`），其他檔案略過；每個晶片內依溫度、風扇、電壓、功耗排序。溫度 ≤ -100 °C 略過（it87 在沒接熱敏電阻的接腳回報 -128 °C）。
  ITE 晶片（`it8*`）沒有 `_label` 的項目經 `ite_label` 命名：溫度依 `tempN_type`（6 → CPU（PECI）、5 → CPU（AMD TSI）、4 → 主機板、3 → 主機板（二極體））；主機板廠商（`/sys/class/dmi/id/board_vendor`，`OnceLock`）為 Gigabyte 時，`in0` → CPU Vcore、`in1`／`in2`／`in3` → +3.3V／+12V／+5V 並乘上分壓倍數 1.65／6／2.5、`fan1` → CPU 風扇；其餘（`in4` 以後、`fan2` 以後）各代主機板接法不同，維持原名。
  CPU 功耗：`/sys/class/powercap/intel-rapl:*`（`Monitor.rapl` 保存各區域上次的 `energy_uj`），功耗＝差值／經過秒數（µJ → W），計數器歸零時以 `energy_delta` 加上 `max_energy_range_uj`；第一次取樣不輸出。以 chip `intel-rapl`、device 為 CPU 型號、label 為區域名稱（`package-0`／`core`／`dram`／`uncore`）、kind `power` 加入 sensors；`energy_uj` 讀不到時 `raplLocked` 為 `true`。溫度的 `crit` 取 `tempN_crit`，沒有時 `tempN_max`，超出 0–150 °C 視為無效（nvme 會回報 65261 °C）。`device`（`sensor_device`）：`coretemp`／`k10temp`／`zenpower`／`cpu_thermal` → `clean_cpu_name` 後的 CPU 型號（`/proc/cpuinfo` 只讀一次，`OnceLock`）；
  `nvme`／`drivetemp` → `<hwmon>/device/model`（`_` 換空格，沒有時為「硬碟」）；`acpitz` → 主機板（ACPI）；`it8*`／`nct*` → 主機板；`amdgpu`／`radeon`／`nouveau`／`i915`／`xe` → 顯示卡；其他沿用 chip 名稱。`chip` 保留給 CPU 溫度判斷。
  NVIDIA 的溫度不在 hwmon：`nvml_sensors` 把 NVML 顯示卡（`Gpu.nvml`）的溫度（「核心」）、風扇 %（`percent`）、功耗（`power`，W）以 chip `nvidia`、device 為顯示卡型號附加在最後；讀不到的項目略過。

### src/widget.rs

桌面小工具（Plasma plasmoid）的設定與資料來源。
- 設定檔：`$XDG_CONFIG_HOME/resourcewatch/widget.json`（沒有該變數時 `~/.config`）。`Config { items, interval, nic, gpu, panelNic, panelGpu, app, opacity, panel }`：`items` 為卡片順序（`cpu`／`mem`／`gpu`／`net`／`disk`／`temp`）、`interval` 為更新秒數、`nic` 為網路卡介面（null＝有預設閘道者）、`gpu` 為顯示卡卡片的顯示卡索引（預設 0，顯示時超出則夾到最後一張）、`panelNic`／`panelGpu` 為面板項目與其彈出卡片用的網路卡／顯示卡（與桌面分開，同樣意義）、`app` 為是否顯示處理器上方的音樂播放格子（預設 true）、`opacity` 為卡片底色不透明度 %（60–100，預設 100）、`panel` 為放在面板時顯示的項目與順序（可選 `PANEL_ITEMS`＝`cpu`、`gpu`、`mem`、`net`、`disk`，面板沒有溫度；預設全部、依此順序）。
  缺欄位用預設值（六項依序 `cpu`、`gpu`、`net`、`disk`、`temp`、`mem`——兩欄時左欄處理器／網路／溫度、右欄顯示卡／磁碟／記憶體——2 秒、自動），JSON 壞掉或檔案不存在時整份用預設；讀寫都經 `clean`：去掉未知與重複的項目、`interval` 限制在 2–60（每次執行本身約需 1 秒）、`opacity` 限制在 60–100、`panel` 去掉不在 `PANEL_ITEMS` 中與重複的項目，保留原順序。`save` 自動建立資料夾。
- `set_order(list)`（`resourcewatch --widget-order cpu,mem,…`）：小工具拖曳重排後呼叫，讀入設定、以逗號分隔的清單取代 `items` 後存檔（同樣經 `clean`，未知與重複名稱會被去掉）。
- `set_app(on)`（`resourcewatch --widget-app on|off`）：小工具右鍵關閉音樂播放格子時呼叫，其他參數回傳用法錯誤。
- `run()`（`resourcewatch --widget`）：新建 `Monitor`，取樣一次、等 1 秒再取樣（速率需要兩次讀數，不保存狀態檔），印出一行 `{ "config": Config, "cpuModel": 字串, "snapshot": Snapshot }`（`cpuModel` 為 `sys::cpu_model()`，與 `SystemInfo.cpuModel` 相同），`Snapshot` 與 `snapshot` 指令相同。
- 單元測試：設定解析（壞 JSON、空物件、未知／重複項目、間隔下限、`gpu`／`panelGpu`／`panelNic`）。

### src/du.rs

資料夾空間分析。`scan(path)`：路徑必須是絕對路徑且為資料夾（否則回傳中文錯誤訊息），走訪一次，記錄每個資料夾的總大小到 `Cache`（`HashMap<PathBuf, u64>`）。
- 多執行緒：`scan(path, threads)` 先在呼叫端讀根目錄一層，其子資料夾放進共用堆疊；`threads` 個工作執行緒（`thread::scope`）從堆疊取資料夾、讀一層（`read_level`）、把子資料夾放回堆疊，各自記下每個資料夾「自己的檔案大小」。堆疊空且沒有執行緒在忙時結束（`Condvar` 叫醒等待者）。最後依路徑深度由深到淺，把每個資料夾的總和加進上一層，得到各資料夾總大小——結果與執行緒數無關（有單元測試比對 1 與 4 個執行緒）。
- 即時進度：`ITEMS`（`AtomicU64`）為已走訪項目數；`TOP`（`Mutex<Vec<Entry>>`）為根目錄的各項目，執行緒每讀完一個資料夾，就把它的檔案大小加到所屬的根目錄項目（路徑去掉根目錄後的第一段）。兩者在每次掃描開始時歸零，`progress()` 回傳排序後的副本。
- 取消：`cancel()` 設定 `CANCEL`，執行緒之後取出的資料夾都不讀、直接丟掉，堆疊很快清空，`scan` 回傳錯誤。
- 大小為實際佔用（`st_blocks × 512`，同 `du`，稀疏檔不會被高估）；不跟隨符號連結；不進入其他檔案系統（`st_dev` 不同的資料夾算 0）；讀不到的資料夾視為空的，不中斷掃描。
  硬連結會重複計算。
- 每個資料夾的項目先依 inode 編號排序再逐一 stat（同 coreutils `du`）：傳統硬碟上會依序讀 inode 表，不必來回尋軌。
- `Scan { path, size, items }`：`items` 為走訪到的項目數（檔案＋資料夾＋連結）。
- `children(dir)` → `Entry { name, path, size, dir }[]`，大到小、同大小依名稱；資料夾大小取自快取（不在快取內為 0），檔案即時讀 metadata。
- 單元測試在暫存目錄建立樹（含符號連結）驗證大小、排序、項目數與路徑驗證。

### src/icons.rs

程序名稱（kernel comm，最多 15 字）或 desktop id → `App { icon, name }`：應用圖示的 data URL（頁面 CSP 只允許 data: 圖片）與 `.desktop` 的名稱（`Name[zh_TW]` → `Name[zh]` → `Name`）。

- 索引在第一次 `lookup` 時建立一次（`OnceLock`），依序走 XDG 資料目錄：`~/.local/share`、使用者 flatpak exports、`XDG_DATA_DIRS`（沒有時 `/usr/local/share:/usr/share`）、`/var/lib/flatpak/exports/share`；同一個鍵先找到的優先（使用者的 .desktop 蓋過系統的）。
- `.desktop`：只讀 `[Desktop Entry]` 區段的 `Exec`／`Icon`／`StartupWMClass`／`Name`（`desktop_keys`），沒有 `Icon` 的項目不收；索引值為（Icon, Name）。比對鍵（小寫）：檔名、檔名最後一段（`dev.lizardbyte.app.Sunshine` → `sunshine`）、`StartupWMClass`、`Exec` 的程式名（`exec_program`：略過 `env` 與其 `VAR=值`／選項；`systemctl`、`flatpak`、`sh`、`gio` 等啟動器不當鍵，以免所有同名程序都配到別人的圖示）。超過 15 字的鍵另存截斷版，對應被截斷的 comm。
- 圖示檔：各資料目錄的 `icons/{hicolor,breeze,Adwaita}`（深度 3）與 `pixmaps`，只收 svg／png，以檔名為鍵。`Icon` 為絕對路徑時直接讀；`-symbolic` 單色圖示先找同名彩色版。多個候選時 SVG 優先，否則取至少 32 px 中最小的 PNG，都沒有時取最大的（`best`）。檔案超過 512 KB 不用。

### src/gpu.rs

- NVIDIA：`libnvidia-ml.so.1` 以 `libloading` 載入、`nvmlInit_v2` 只呼叫一次；每次取樣依索引取 handle，讀名稱、使用率、記憶體（v1 `used`）、溫度、編碼／解碼使用率（`nvmlDeviceGetEncoder/DecoderUtilization`，舊驅動沒有這兩個符號時為 `null`），任一呼叫失敗該欄為 `null`。另讀風扇 %（`nvmlDeviceGetFanSpeed`）與功耗（`nvmlDeviceGetPowerUsage`，mW → W），兩個符號皆為可選，結果與 `nvml` 旗標只供感測器使用（`serde(skip)`，不在 GPU JSON 中）。函式指標與 `Library` 同生命週期（`_lib` 欄位）。
- 每程序 GPU（`proc_util(own, secs)`，由 `processes` 呼叫）：
  - NVML：可選符號 `nvmlDeviceGetProcessUtilization`，先以 null 緩衝區取筆數再讀；`lastSeenTimeStamp` 用上次讀到的最新時間戳（第一次為現在 − 3 秒，避免拿到驅動整個緩衝區），同一程序多筆樣本取平均，sm／enc／dec 取最大者為 `3D`／`Video Encode`／`Video Decode`，GPU 索引即 NVML 索引。有此符號時所有程序都涵蓋（回傳旗標 `true`）。
  - DRM（AMD／Intel）：只掃自己的程序 `/proc/<pid>/fd` 中指向 `/dev/dri/` 的 fd，`parse_fdinfo` 讀 `drm-client-id`、`drm-pdev`、`drm-engine-<名稱>: <ns> ns`（略過 `capacity-`；xe 的 cycles 計數器與 NVIDIA 專有驅動沒有這些行，不支援）。同一個 (PCI 位址, client id, 引擎) 只算一次（fork 後多個程序共用），% ＝ ns 差值／經過時間，上一回的值存在 `drm_prev`；`drm-pdev` 對應 `drm` 清單中卡片的 PCI 位址（`device` 連結 canonicalize 後的名稱），索引接在 NVML 之後。引擎名稱經 `engine_label`：`gfx`／`render` → 3D、`compute` → Compute、`dec`／`video`／`jpeg` → Video Decode、`enc*` → Video Encode、`video-enhance` → Video Processing、`dma`／`copy` → Copy。
  - 每個程序保留最忙的一筆（`put`）。單元測試涵蓋 fdinfo 解析與引擎名稱。
- 其他：`/sys/class/drm/cardN/device`，vendor `0x1002` AMD、`0x8086` Intel、`0x10de`（只在 NVML 載入失敗時，例如 nouveau）；讀 `gpu_busy_percent`、`mem_info_vram_used/total`、`hwmon/*/temp1_input`。
  Intel 與 nouveau 通常沒有這些檔案，對應欄位為 `null`。名稱只有廠牌（不讀 PCI ID 資料庫）。

## widget/

KDE Plasma 6 小工具（plasmoid，id `io.github.local.resourcewatch.widget`，`FormFactors: desktop`——指桌機平台，可放在桌面也可放進面板），以 `kpackagetool6 -t Plasma/Applet` 安裝到 `~/.local/share/plasma/plasmoids/`。純 QML，沒有編譯步驟；資料全部來自 `resourcewatch --widget`（見 src/widget.rs），不必開著主程式。

### metadata.json

名稱、說明（含 zh_TW）、圖示沿用 App 圖示名稱、分類 System Information、`X-Plasma-API-Minimum-Version: 6.0`；`Version` 與 Cargo 版本一起更新。

### contents/ui/main.qml

- 外部指令都經 `exec`（`P5Support.DataSource`，`executable` 引擎）以一次性方式執行：`run(command)` 連接來源，`onNewData` 收到結果就斷開並交給 `onSample`／`onConfigRead`；不用 DataSource 的 `interval` 輪詢，因為輪詢模式要到下一次輪詢才把結果交出，整整慢一個間隔。兩個 `Timer` 觸發：
  - 每 `cfg.interval` 秒（啟動時立即一次）以 shell 執行 `PATH="$HOME/.local/bin:$PATH" resourcewatch --widget`（plasmashell 的 PATH 不一定有 `~/.local/bin`）；`onSample` 解析 stdout 的 `{ config, cpuModel, snapshot }`。結束碼 127 顯示「找不到 resourcewatch，請先執行 install.sh」，其他非 0 顯示 stderr，JSON 解析失敗顯示「無法解析 resourcewatch 輸出」；第一次取得資料前顯示「讀取中…」。
  - 每 0.3 秒 `cat` widget.json（約 1 ms），`onConfigRead` 套用，所以 App 設定頁的變更約半秒內生效，不必等下一次 `--widget`（間隔＋1 秒取樣）；檔案不存在時略過（由 `--widget` 輸出帶出預設值），解析失敗（寫到一半）等下一次。
  - 同一個指令還在執行時不重複啟動（`run` 先檢查 `connectedSources`）。兩個來源的設定都經 `applyConfig`：先以 `pending` 蓋過小工具自己剛存、檔案尚未跟上的欄位，內容有變才取代 `cfg`。原始檔案不經後端 `clean`，但設定頁與小工具只寫入整理過的值。
  結束碼 127 顯示「找不到 resourcewatch，請先執行 install.sh」，其他非 0 顯示 stderr，JSON 解析失敗顯示「無法解析 resourcewatch 輸出」；第一次取得資料前顯示「讀取中…」。
- 放在面板（`inPanel`：`Plasmoid.formFactor` 為 Horizontal／Vertical）：`preferredRepresentation` 改為 `compactRepresentation`、背景用 Plasma 預設；`compactRepresentation` 以 `GridLayout`（水平面板一列、垂直面板一欄，間距 10／6）依 `cfg.panel`（與桌面的 `cfg.items` 分開）排出 `PanelItem`，點一項呼叫 `togglePopup(key)`：記下 `popupKey` 並展開，再點同一項收合；第一次取得資料前只顯示 App 圖示。彈出視窗沿用 `fullRepresentation`，`single` 為 true 時只有一欄、只放 `popupKey` 那張卡片，不顯示 音樂播放格子、不能長按重排。`allowDrag` 在面板中不動作（容器的編輯模式只對桌面）。間隔與桌面版共用同一份 widget.json，顯示項目各自為 `panel`／`items`；網卡與顯示卡也分開：`nicName`／`gpuIndex` 在面板時取 `cfg.panelNic`／`cfg.panelGpu`，桌面取 `cfg.nic`／`cfg.gpu`，`pickGpu(s)` 依 `gpuIndex` 取顯示卡（超出夾到最後一張）；網路卡、顯示卡卡片與 `PanelItem` 都由 main.qml 傳入選好的介面（`n`／`nic`）與顯示卡（`g`／`gpu`），不自己挑。
- 歷史 `hist`：`record` 每次建立新物件（讓繫結更新），各序列保留 60 點：`cpu`、`mem`（使用率 %，面板迷你圖用）、`gpu`（`pickGpu` 選出的顯示卡）、`rx`／`tx`（`Fmt.nic(s, nicName)` 選出的網卡）、`r`／`w`（所有硬碟讀寫加總，`Fmt.sum`）。
- `backgroundHints: NoBackground`、`preferredRepresentation: fullRepresentation`：不用 Plasma 的面板背景，直接畫卡片；版面固定兩欄、每張卡片寬 282（`cardWidth`）、間距 12，第 i 個項目放在第 i % 2 欄（預設六項為左三：處理器／網路／溫度，右三：顯示卡／磁碟／記憶體；main.qml 的預設 `cfg` 與後端預設相同），各欄靠上、卡片高度依內容。
  處理器所在欄的最上方有 `MediaCard`（音樂播放格子，設定鍵仍為 `app`），高度＝另一欄卡片總高 − 本欄卡片總高 − 間距，讓兩欄底部對齊；各欄的 `cardsHeight` 只加總 `Repeater` 產生的卡片（不含 `MediaCard`），避免高度互相依賴；差距小於 72 或 `cfg.app` 為 false 時不顯示。處理器被拖到另一欄時跟著移過去。小工具的最小＝最大＝偏好尺寸＝內容大小（`Layout.minimum/maximum/preferredWidth/Height`），桌面的縮放把手無法改變大小，增減項目時跟著內容變。依 `cfg.items` 順序以 `Repeater` 產生 `Loader`，`sourceComponent` 對到檔案底部的六個 inline `Component`（`CpuCard`／`MemCard`／`GpuCard`／`NetCard`／`DiskCard`／`TempCard`）——卡片是其他檔案的型別，看不到 main.qml 的 id，用 inline Component 才能把 `root.snap`／`hist`／`cfg` 以繫結傳入、每次更新自動重畫。

- 移動整個小工具：桌面把每個小工具包在 containmentlayoutmanager 的 `ItemContainer` 裡，其 `editModeCondition` 決定按下時是否開始移動，而容器比卡片先收到按下事件。`allowDrag` 沿 `parent` 往上找到有 `editModeCondition` 的容器，依滑鼠位置切換：在卡片標題列或卡片之間的空白時為 `AfterPress`（按下即可拖動，同時出現縮放／設定／移除把手，點別處結束），滑鼠在卡片內容區（`overBody`）或正在拖曳卡片（`dragKey`）時為 `Manual`（不移動，讓卡片收到長按）；小工具被鎖定（`Plasmoid.immutable`）時為 `Locked`。在 `onParentChanged`、`overBody`／`dragKey`／`immutable` 改變時重設（會蓋掉桌面原本的繫結，所以鎖定狀態要自己處理）。
- 重新排列卡片：`Card` 回報內容區的滑鼠停留（`bodyHovered`）與長按 400 ms 後的拖曳（`holdStarted`／`holdMoved`／`holdEnded`）；每個 `Loader` 以 `Connections` 轉給 `full` 的 `holdStart`／`holdMove`／`holdEnd`（座標換成 `full` 的座標）。拖曳中的卡片以 `Translate` 跟著滑鼠、放大 1.04、`z` 與所在欄提高到上層；`slotAt` 依 x 決定欄、依該欄各卡片的底部決定列（超過最後一張為該欄的空格），換算成 `cfg.items` 索引（超出時為最後一個），目標卡片半透明。放開時與目標交換位置（同一格或取消不動），`setOrder` 立即更新 `cfg.items`，並以第二個 executable `DataSource`（`saver`）執行 `resourcewatch --widget-order …` 存檔（`setApp` 同理執行 `--widget-app`，共用 `save`）；存檔前已開始的 `--widget` 或讀檔會拿到舊值，所以 `pending` 保留小工具自己改過的欄位，蓋過舊值，直到讀到的設定一致為止（`applyConfig`）；存檔失敗時清空 `pending` 並顯示錯誤。這是私有 API，未來 Plasma 改名時找不到容器就什麼都不做，仍可長按拖動。
- 關閉卡片：`Plasmoid.contextualActions` 在 Plasma 的右鍵選單加一項「關閉「<卡片標題>」卡片」（`window-close` 圖示，標題對照 `titles`）。桌面容器會先攔下右鍵按下事件並開啟選單，小工具收不到，所以改在 `contextualActionsAboutToShow` 時以 `full` 上 `HoverHandler` 的游標位置經 `cardAt` 找出卡片存進 `menuKey`（`cardAt` 也認得音樂播放格子，回傳 `app`）；游標在卡片間空白、或只剩一張卡片時不顯示此項。選取後一般卡片以 `setOrder` 從 `cfg.items` 移除、音樂播放格子以 `setApp(false)` 隱藏，都寫回 widget.json，可在 ResourceWatch 設定頁重新加入／顯示。

### contents/ui/PanelItem.qml

面板中的一項（iStat Menus 風格），整項可點擊（`clicked` 訊號）；網路與顯示卡項目用 main.qml 傳入的 `nic`／`gpu`（面板自己的選擇）。
- 處理器／顯示卡／記憶體：沒有圖示，兩行置中——上為「CPU」「GPU」「RAM」、下為使用率 %；右側為無格線的 `Chart`（寬 16、高為厚度 60%；藍／青／黃）。
- 網路／磁碟：Plasma 圖示（`network-wired`／`drive-harddisk`）、兩行靠右的「↓下載」「↑上傳」或「R 讀取」「W 寫入」（`Fmt.short`），右側為 `Mirror`（寬 32，上傳／讀取桃紅在上）。
- 文字色跟隨面板（`Kirigami.Theme.textColor`），字級依面板厚度（單行 42%、兩行各 30%），數值欄以 `TextMetrics` 量最寬的樣本（`100%`／`↓9.9M`）固定寬度，數字變動時不會左右跳動。
- 垂直面板只顯示圖示（或標籤）與第一個數值，上下排列（寬度放不下迷你圖）。

### contents/ui/MediaCard.qml

與卡片相同底色與圓角，顯示播放中的媒體。資料來自 Plasma 媒體控制器同一套 MPRIS 模組 `org.kde.plasma.private.mpris`（私有 API）的 `Mpris2Model.currentPlayer`：左為專輯圖（`Kirigami.ShadowedImage`，圓角 8，`color` 設透明——預設白底會蓋住佔位底色；邊長 min(高度 − 28, 96)；沒有 `artUrl` 或載入失敗時顯示播放器圖示 `iconName`，再沒有則 `media-default-album`），右為粗體曲名（沒有時為播放器名稱 `identity`）、muted 演出者（沒有時隱藏）、藍色進度條（`position / length`，皆為微秒；串流 `length` 為 0 時進度條為空、總長顯示 `--:--`）、最下一列兩端為目前時間與總長（`m:ss`），中間為上一首／播放暫停／下一首（`Ctl`：18 px 單色 `Kirigami.Icon`（`isMask`、文字色），滑鼠移上不透明、平時 0.75、不能用時 0.3；`media-skip-backward`／`media-playback-pause` 或 `-start`／`media-skip-forward`，分別呼叫 `Previous`／`PlayPause`／`Next`，依 `canGoPrevious`／`canPause` 或 `canPlay`／`canGoNext` 啟用）。這一列有 `HoverHandler`，`controlsHovered` 由 main.qml 寫入 `overBody`，讓桌面容器在按鈕上不把按下當成移動小工具。播放器只在跳轉時回報位置，所以播放中且可見時每秒呼叫 `updatePosition()`。沒有播放器時置中顯示「沒有播放中的媒體」。高度由 main.qml 決定；不屬於 `cfg.items`，不能拖曳重排，按住它可移動整個小工具。

### contents/ui/Theme.qml

與 style.css 相同的色彩 token（卡片、欄位、線、文字、muted、軌道與各指標色）；`card` 套用 `cardOpacity`（main.qml 由 `cfg.opacity / 100` 設定）作為 alpha，只有卡片與音樂播放格子的底色變透明，文字與圖表不受影響；深／淺色依 Plasma 色彩配置的背景亮度（`Kirigami.ColorUtils.brightnessForColor`）切換。main.qml 以 id `colors` 建立一份傳給各卡片（不用 `theme`／`palette`，兩者會與卡片屬性或 Item 內建屬性同名）。

### contents/ui/Card.qml

儀表板卡片：圓角 16、無邊框、`theme.card` 底色，內容區上方疊一個 `MouseArea`（`hold`，與內容區同位置大小、`hoverEnabled`，長按 400 ms 後回報拖曳；取消時以 (-1, -1) 結束），內距上 14／其他 18，標題列（15 px 粗體，過長截斷）右端可放 muted 小字 `extra`（溫度、總量、網卡名稱），其下可放一行 muted 12 px 副標 `subtitle`（與標題列間距 4，過長截斷，空字串時隱藏；屬於標題區，按住可移動整個小工具），標題與內容、內容各區塊之間距 10；`default property` 讓子元件放進內容欄。

### contents/ui/CpuCard.qml、MemCard.qml、GpuCard.qml、NetCard.qml、DiskCard.qml、TempCard.qml

各自繼承 `Card`，屬性 `snap`（必要）、`hist`；網路卡的 `n` 與顯示卡的 `g` 由 main.qml 傳入。
- 處理器：`extra` 為 CPU 溫度、`subtitle` 為處理器型號（`--widget` 輸出的 `cpuModel`，經 main.qml `cpuModel` 傳入 `model`）；兩欄大字（`uniformCellSizes` 平分）：使用率（藍點）、平均頻率（GHz）；其下一列左右並排、寬度 30:70（間距 12，靠底對齊）：左為各執行緒使用率直條（`Cores`），右為藍色曲線（`hist.cpu`，高 54 與直條同高）；比例以兩者相同比例的 `Layout.preferredWidth`（3／7）與 `Layout.horizontalStretchFactor`（3／7）分配，不綁定列本身的寬度（那會讓 Layout 遞迴重排、plasmashell 記錄 "Detected recursive rearrange"）。
- 記憶體：`extra` 為總量；大字為使用率、其下「N 已使用」；堆疊條：應用程式（used，黃）、快取（藍）、可用（total − used − cached，綠）；圖例四列（含 SWAP 已用／總量，沒有 SWAP 時隱藏）。
- 顯示卡：一張（`g`）；標題為「顯示卡」、`subtitle` 為型號、`extra` 為溫度；左為青色使用率圓環（中間百分比），右為顯示記憶體／編碼器／解碼器三條細條（`Legend` 加 5 px `StackBar`）；下方青色曲線。沒有顯示卡時顯示「沒有顯示卡」。
- 網路：介面為傳入的 `n`，`extra` 為介面名；兩欄大字下載（藍點）／上傳（桃紅點）；下方 `Mirror` 上傳在上、下載在下；`subtitle` 為「本機 IPv4 `ip/prefix`」（沒有 IPv4 時 `--`）。
- 磁碟：兩欄大字為所有硬碟讀取（桃紅）／寫入（藍）加總，`Mirror` 讀取在上；其下每顆有掛載的硬碟一列型號（`_` 換空格）與「已用 / 總量」，配黃色細用量條。
- 溫度：`Fmt.temps` 前 8 個裝置，名稱 muted、數值靠右；達到 `crit` 時紅色粗體。

### contents/ui/Chart.qml、Mirror.qml、StackBar.qml、Ring.qml、Big.qml、Legend.qml、Cores.qml

對應 app.js 的繪圖函式，皆以 `Canvas` 繪製，資料或寬度改變時 `requestPaint`。
- `Chart`：面積曲線，固定 60 格、最新點在右緣，`null` 斷線；背景頂端與中間虛線、底部實線（`grid` 色）；線寬 1.5、面積 25% 透明；`max` 預設 100。
- `Mirror`：上下對稱條狀圖，60 格、間隔 1 px，上下各自依序列最大值（至少 1）縮放，0 畫成 1 px 短條。
- `StackBar`：以圓角矩形 `clip` 的軌道（`color`）上依序畫各段（`segments: [{ value, color }]`，對 `total` 的比例）——圓角用 clip 路徑而不是 Rectangle，各段才不會在兩端露出直角。
- `Ring`：圓環，圓頭線端，從 12 點鐘方向順時針，`value` 0–100。
- `Big`：彩色圓點＋22 px 粗體數值，其下 12 px muted 說明；內部兩列都 `Layout.fillWidth`，否則欄的最大寬度被限制為內容寬，兩個 `Big` 並排時無法平分。
- `Legend`：（可選彩色圓點）標籤靠左 muted、數值靠右，12 px。
- `Cores`（Cores.qml）：各執行緒使用率直條，同儀表板 `.cores`：高 54、間距 3、最小寬 2、上方圓角 3、靠底對齊，高度＝max(使用率, 3)%；高度以 `Layout.preferredHeight` 指定（Layout 會覆寫自己的 implicitHeight）。

### contents/ui/fmt.js

`.pragma library`，與 app.js 同邏輯的 `bytes`／`rate`／`pct`／`ratio`；`temp` 固定 °C（App 的 °C／°F 存在 WebView localStorage，小工具讀不到）。
`nic(s, name)`：指定介面 → 有預設閘道 → 已連線 → 第一個（同儀表板 `primaryNet`）；`temps(s)`：每個感測器裝置的第一個溫度（`{ device, value, crit }`）；`sum(list, key)`：各硬碟某欄位的加總，全部缺值時為 null；`short(b)`：面板用的精簡速率（B／K／M／G，以 1000 為進位門檻、1024 為除數，個位數時一位小數）。

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
- 指標色（`.c-*` 同時設定 stroke／fill／background）：應用程式記憶體／磁碟用量黃、快取／CPU 曲線藍、上傳／讀取桃紅、下載／寫入藍、可用綠、CPU 環洋紅、顯示卡環青、SWAP 灰。
- 卡片內間距：卡片內距 20／22（上／其他）、標題下 18、分區（`.sect` 分隔線）上下各 18、區塊之間 16（圖表後的 `.foot`、`.addr` 與速率、GPU 各列）、同一區塊的行距 10（`.kv`、`.legend`、環下數值；標籤與數值間 2）、小標題 `.cap`（無底線）下 8、表格儲存格 6；卡片之間 24；卡片內一般表格首尾欄不留左右內距，與卡片文字對齊（`.zebra`、`.files` 除外）。
- 應用程式頁 `.apps`：欄位標題上方的整機 % 為 `th b`（18 px）；區段標題列放在獨立的 `tbody.sec`，不上斑馬紋；群組列有展開箭頭 `.tw`（無子項時留同寬空位以對齊），展開後的子列 `tr.kid` 名稱縮排 54 px；低優先權的葉子 `.leaf` 為綠色。
- 斑馬紋 `.zebra`：只替資料列上色（`tbody` 的奇數列，標題列放在 `<thead>`），所以有無標題列的清單都從第一列資料開始深色；標題儲存格左右內距與資料列同為 10。程序圖示 `.app-ic` 為固定 18×18 的框（圖片與通用圖示字型都垂直置中），接在名稱前。儀表板的兩個程序清單 `.procs` 是固定大小的格子：`table-layout: fixed`、每列高 34、數值欄寬 90、名稱欄填滿其餘並在過長時截斷，列的大小不受名稱、數值長度或圖示種類影響。
- 版面：側欄 220 px（收合 68 px，只留圖示）；收合鈕展開時靠左（左移 3 px 對齊選單圖示），收合時置中；儀表板用 CSS 多欄（`columns: 3 330px`）做瀑布流，卡片 `break-inside: avoid`；其他頁用 `.stack`（直向）或 `.grid`。
- 設定頁小工具卡片：`.wchips` 按鈕 `cursor: grab`、`touch-action: none`，未顯示的半透明，拖動中的浮在上層（`--sel` 底色與陰影），放置目標為藍色內框；`.wrange` 滑桿寬 200、`accent-color` 藍色，`.wrange-val` 固定寬 40、等寬數字靠右。
- 儲存頁：`.du-head` 可換行——標題欄（`.path`，基準寬 240 px）放不下時，按鈕組 `.du-actions` 整組移到下一行；統計列每一項（`<i>`）不拆開，只在項目之間換行。`.du` 為 flex-wrap：圖表卡（基準 420 px）與檔案清單（基準 340 px）放得下就並排，否則清單移到下方佔滿整行；色階尺寬度 `min(280px, 100%)`。`.levels` 左欄資料夾名稱、右欄色塊條 `.strip`；色塊顏色 `.s0`–`.s9` 依大小（100 MB 綠 → 50 GB 以上紅），選取的色塊有黃色內框。右側 `.files` 清單。

### app.js

- 每秒 `snapshot` → `record` 把 CPU、各 GPU、各磁碟讀寫（`r:`／`w:` + 裝置名）、各網卡上下行存入 120 點歷史 → `render` 以目前頁的函式產生 HTML 換掉 `#content`。
  儲存頁與應用程式頁不隨每秒快照重畫（只更新主機列），各自在自己的事件（掃描、點擊、程序資料）後重畫。
- 程序：儀表板或應用程式頁可見（且視窗未隱藏）時每 2 秒 `processes`；網路頁改為每 2 秒 `connections`（清單存在 `conns`；`tcp` 保存狀態數、上次計數與重傳率＝ΔRetransSegs／ΔOutSegs；取回後重繪並載入圖示）；儀表板顯示 CPU 前 5／記憶體前 5（每列前有應用圖示：`loadIcons` 只對還沒查過的名稱呼叫 `app_icons`，結果 `{ icon, name }` 存在 `appIcons`，查詢中或找不到時用通用圖示 `fa-window-maximize`；兩個清單都是斑馬紋）；在應用程式頁時查詢所有程序名稱與 desktop id。
  應用程式頁（`appsPage`）：依 `Proc.kind` 分三區「應用程式／背景處理程序／系統處理程序」（`APP_SECTIONS`，區段標題括號內為群組數），每區以 `unit` 分組（`procGroups`；沒有 unit 的程序自成一組，`kernel` 顯示為「核心執行緒」）；群組名稱為 `app` 對應的 `.desktop` 名稱，沒有時為 PID 最小的程序名稱，圖示同理。群組數值為成員加總（`sumOf`，全部缺值時為 null；GPU 上限 100），GPU 引擎取 GPU 最高的成員；狀態：單一程序照原狀態，多個程序全部暫停才顯示「已暫停」，全部 nice > 0 才顯示葉子。超過一個程序的群組可點擊展開（`data-group`，展開的鍵存在 `appOpen`，每 2 秒重繪仍保留），子列依同一排序列出、滑鼠提示 PID。
  欄位：名稱、狀態（標題不顯示文字；`T`／`t` → 已暫停、`Z` → 殭屍、nice > 0 → 葉子）、CPU、記憶體、磁碟（MB/秒）、GPU、GPU 引擎；CPU／記憶體／磁碟／GPU 標題上方為整機 %（最近一次快照：CPU 使用率、記憶體 used/total、各硬碟 `busy` 最大值、各顯示卡使用率最大值）。點欄位標題排序，群組與子列都依同一鍵（名稱預設升冪，其他降冪，缺值永遠排最後）。
- 頁面（`PAGES`）：
  - 儀表板：摘要（三層環形圖：外圈記憶體應用程式＋快取、中圈 CPU、內圈 `gpuIndex` 選出的顯示卡使用率，環下標註記憶體總量／處理器／顯示卡（圓環上方留 20px、與下方數值相隔 32px）；記憶體組成、CPU 型號（去掉 `@ 頻率`）／核心／執行緒、磁碟 ‹ › 翻頁，預設最大的檔案系統）、
    處理器（標題下一列為晶片圖示與完整 CPU 型號，過長截斷；各執行緒直條：flex 排列、3 px 間隔、上方圓角，高度以 `data-h` 在 `render` 後經 CSSOM 設定；使用率曲線；分隔線下前 5 程序）、記憶體（標題右側為 `memSpec`，讀不到時為系統可用總量；堆疊條、組成與 SWAP（數值不換行）、前 5 程序）、網路（頂端一列：網卡圖示（`wl*` 用 Wi-Fi，其餘乙太網路）與名稱、‹ › 切換鈕（`pick.nic` 記住所選介面名稱，未選時用 `primaryNet`，即設定中的儀表板網路卡；改設定時清除）。網路、顯示卡、磁碟 I/O 的 ‹ › 都由 `flipper(kind, keys, i)` 產生，點擊時把相鄰項的鍵存進 `pick[kind]`（`nic`／`gpu` 索引／`io` 裝置名），靠右為累計流量與重設鈕。重設時把該介面當下的 `rxTotal`／`txTotal` 存成起點（`localStorage` `rw-netbase`），累計顯示與起點的差；計數器小於起點（重開機歸零）時起點視為失效、顯示原值；其下 `flow`：兩欄大字速率（數字左側為顏色圓點，下方依序為標籤與近 120 筆峰值）、對稱條狀圖）、
    磁碟（檔案系統用量條）、顯示卡、磁碟 I/O（頂端一列硬碟圖示、型號與 ‹ ›，預設第一顆硬碟；其下 `flow`：讀取在上、寫入在下）、感測器（`sensorGroups` 依裝置分組，每個裝置一列：圖示、裝置名稱與第一個溫度（封裝／Composite／顯示卡核心），沒有溫度時用第一筆；最多 8 個裝置，滑鼠提示為項目名稱；總筆數多於列數時顯示「顯示更多…」）。「顯示應用程式…」「顯示更多…」以 `data-goto` 切頁。
  - 主機列：左側為發行版 logo（`SystemInfo.logo`），接著主機名稱與運作時間，再來是發行版與版本（`os`）、桌面環境與版本（`desktop`）；沒有 logo 檔時依 `osId` 用 Font Awesome 品牌圖示（fedora、ubuntu、debian、redhat、centos、suse），其餘用 `fa-linux`。
  - 摘要的硬碟區與磁碟卡片以實體硬碟為單位，顯示型號（NVMe 型號的 `_` 換成空格；沒有型號時用裝置名稱）與其上檔案系統的用量；未掛載的硬碟顯示「未掛載 · 容量」，滑鼠提示有裝置名稱與掛載點。
  - 顯示卡卡片：只有一張，第一列晶片圖示、型號（過長截斷）與 ‹ ›（多張顯示卡時切換，預設為設定中的儀表板顯示卡），右側溫度；第二列左為使用率環，右為顯示記憶體／編碼器／解碼器三條使用率；分隔線下為使用率曲線。
  - 設定：主題（跟隨系統／深色／淺色）、更新間隔（0.5／1／2／5 秒，改變時重設計時器；歷史保留 120 筆：條狀圖顯示全部，曲線顯示最後 60 筆）、溫度單位（°C／°F，所有溫度都經 `temp()` 換算）、
    儀表板網路卡（下拉選單：自動＝有預設閘道者，或指定介面）、儀表板顯示卡（下拉選單，有顯示卡時顯示，選項為各卡名稱、值為索引，預設第一張；`gpuIndex` 取 `pick.gpu` 否則此偏好，超出時夾到最後一張，顯示卡卡片與摘要卡的顯示卡環都用它；改設定時清除 `pick.gpu`）、開發者模式（開啟後顯示「開啟檢查器」按鈕，並可用 F12／Ctrl+Shift+I 呼叫 `open_devtools`；關閉時不攔截按鍵。開發者模式下左下角多一個暫停鈕（或 F8）：暫停時 `tick` 與 `pollProcs` 直接返回、畫面不重繪，檢查器的元素樹不會被重建，切頁與點擊仍正常；關閉開發者模式即解除暫停。`applyPrefs` 每次都以 `set_dev_mode` 同步，啟動時也會關掉 wry 預設開啟的 developer extras）；「桌面小工具」卡片（`widgetCard`，在「一般」與「關於」之間）：設定存在 widget.json 而不是 `rw-prefs`，啟動時 `get_widget_config` 讀入 `wcfg`（讀取前顯示「讀取設定中…」、失敗顯示錯誤），頂端一行加入方式提示；
    「顯示項目」為一列 `.seg` 按鈕（`data-w="item"`，`WIDGET_ITEMS` 對照名稱）：顯示中的依 `items` 順序在前、隱藏的在後，點擊 `setWidget('item', key)` 切換（隱藏＝移除、顯示＝接在最後）；設定頁不提供排序，位置在小工具上長按卡片拖曳調整；更新間隔 2／5／10 秒；網路卡（下拉選單，自動或指定介面）；顯示卡（下拉選單，有顯示卡時顯示，`gpu`）；音樂播放格子（顯示／隱藏，`data-w="app"`，小工具右鍵關閉後從這裡打開）；設定頁另有「面板小工具」卡片：說明文字與一列 `.seg.wchips` 按鈕（`panelChips`：顯示中的項目依 `panel` 順序在前、隱藏的依 `PANEL_ITEMS` 順序在後並淡化；`data-w="panel"`、`data-wchip`），與桌面卡片的 `items` 分開，其下為面板自己的網路卡／顯示卡下拉選單（`panelNic`／`panelGpu`）。網路卡與顯示卡的下拉選單都由 `dropdown(attr, key, cur, opts)` 產生（選項 `nicOpts`／`gpuOpts`）：`label.btn.pick` 疊透明 `select`，屬性為 `data-pref-sel`／`data-w-sel`（不用 `data-pref`／`data-w`，否則點開選單時 click 處理會先存一次目前值並重畫），`#content` 的 `change` 事件呼叫 `setPref`／`setWidget`；選單有焦點時 `tick` 不重畫頁面，以免展開中的選單被換掉。點擊 `setWidget('panel', key)` 切換（關閉＝移除、開啟＝接在最後）；按住拖動以指標事件處理（不用 HTML5 拖放，webview 的拖放由 Tauri 接手）：`pointerdown` 記下 `wDrag` 並 `setPointerCapture`，移動超過 4 px 後按鈕以 `translateX` 跟著滑鼠、加 `.dragging`，指標下的另一顆按鈕加 `.drop`；`pointerup` 把拖動的項目移到該按鈕的位置，只保留顯示中的項目以 `setWidget('panelOrder', …)` 存檔；拖動後緊接的 click 不當成切換（`wDrag.moved`）；拖動期間 `wSliding` 為 true，設定頁不重畫；卡片透明度為滑桿（`input[type=range][data-wrange]`，0–100%、每格 5%，右側即時顯示 %），對應實際透明度 0–40%（`W_MAX_TRANS`），存入不透明度 `opacity`＝100 − 滑桿值 × 0.4（每格 2%）；拖動中（`pointerdown`／`input` 到 `change`／`pointerup`）`wSliding` 為 true，`tick` 不重畫設定頁以免滑桿被換掉，放開（`change`）才 `setWidget('opacity', …)` 存檔。
    按鈕帶 `data-w`（`item`／`interval`／`app`／`panel` 等），`setWidget` 改動副本後呼叫 `set_widget_config`，以回傳值（後端整理過）取代 `wcfg`，失敗時卡片底部紅字；每次進入設定頁都重新 `get_widget_config`（小工具拖曳重排會改寫 widget.json，沿用啟動時讀到的舊值會在下次修改時蓋掉新順序）；「關於」顯示版本、系統與 Font Awesome 授權。
  - 網路：每張網卡一張卡，標題左側為兩行（`.nic-title`）：介面名稱、其下以 · 串接網卡型號（廠牌＋晶片）、驅動（依 `driverOpen` 顯示「開源驅動 r8169」／「專有驅動 …」，未知時「驅動程式 …」）與「MTU …」（muted，過長截斷，提示為完整字串）；右側為連線速度與雙工（如「1 Gbps 全雙工」；Wi-Fi 連線中改顯示 SSID 與 dBm）及綠／紅點連線狀態；連線中的依序為：
    `.speeds` 兩欄（左 240 px；視窗寬度 ≤ 960 px 時改為單欄，位址在上、每列兩個）：左為 `addrBlock(n, dns)`（`.addr` 直排四列、垂直置中：IPv4 帶 `/前綴` 與 MAC、預設閘道、DNS（逗號串接，提示逐行列出）、IPv6（顯示第一個，多於一個時小字附「另 N 個」，提示逐行列出全部）），右為 `.rates-row` 橫排的下載、上傳大字速率（`.rate.rx`／`.tx`：方向箭頭以藍／桃紅著色、對應條狀圖顏色；數字下方小字為「下載／上傳 · 累計 …」，有 speed 時再接「頻寬 %」＝位元速率／連線速度），其下高條狀圖；
    其下的 `.details` 為次要資訊面板（不畫分隔線，改以 `--field` 底色圓角區塊與上方區隔，字級 12）（flex 換行、欄間距 64 px，每欄與表格都依內容寬度、靠左排列，不平均撐滿面板）：Wi-Fi 連線中第一欄為 `wifiDetails`（SSID、訊號（dBm 與 NetworkManager 換算的 %＝2×(dBm+100)，限 0–100）、頻段（由頻率算 2.4／5／6 GHz 與頻道號）與頻寬、收／發連線速率、Wi-Fi 世代、BSSID），
    接著 `nicDetails` 的「封包統計（開機以來累計）」：封包／錯誤／丟棄 × 接收／傳送的小表格，欄位標頭帶與速率同色的 ↓／↑；錯誤與丟棄為 0 時顯示淡色「無」，非 0 時錯誤紅字（`.bad`）、丟棄黃字（`.warn`），並附佔同方向封包數的比例（取到小數 2 位，過小顯示 `<0.01%`）；列標籤為虛線底線的 `abbr`，提示說明該項的意義。
    這些清單用 `lv(title, items)`：`.cap` 小標題加 `.lv` 表格，標籤靠左（muted，`th:first-child` 縮到內容寬）、數值緊接在標籤右側（提示為完整值；統計表的數字欄 `.num` 靠右、左側留 32 px），每列第三個值可作為數值的 class。
    網卡卡片之後是「連線」卡（`connCard`）：標題右側小字「程序清單僅含目前使用者」（提示說明其他使用者需管理員權限、上方數字為全系統 TCP）；最上方 `tcpStats` 為一排 `.stats` 白話數字（第一次取回前不顯示，每格提示說明意義與原始狀態名及各自數量）：使用中（ESTABLISHED）、等待連入（LISTEN）、建立中（SYN_SENT＋SYN_RECV）、關閉中（TIME_WAIT、CLOSE_WAIT、FIN_WAIT1／2、LAST_ACK、CLOSING、CLOSE）與重傳率（兩次輪詢間，取到小數 2 位；< 1% 綠字「良好」、< 3% 黃字「普通」、其餘紅字「不佳」）；
    分隔線下為 `.procs.conns` 斑馬紋表格，每個程序一列：應用程式（圖示與名稱，提示含 PID）、連線對象（`peers`：遠端去重後依主機分「網際網路 N 台」「區域網路 N 台」（`isLan`：10／172.16–31／192.168／127／169.254、IPv6 fc／fd／fe80／::1），再接服務名稱（`HTTPS_LIKE` 常見埠對照，其餘「埠 N」，最多 2 種、其餘「等 N 種」）；提示列出全部遠端位址；`splitAddr` 拆 `ip:port`／`[v6]:port` 並把 `::ffff:` 映射位址還原為 IPv4）、使用中（有遠端的連線數）、等待連入（埠）（監聽中的本機埠號升冪，提示附 TCP／UDP）；依使用中、監聽埠數、名稱排序。
  - 感測器：依張數平均分欄（`evenCols`：欄數比照 `.masonry`，由 `#content` 內寬算出最多 3 欄、每欄至少 330 px、間距 24；張數平均分配、前面的欄多放餘數，例如 6 張為 3:3 或 2:2:2；`.cols`／`.col` 為 flex；視窗縮放時在感測器頁重繪），所有磁碟（`sensorIcon` 為 `storage` 的 nvme／drivetemp）合併成一張「磁碟」卡排在最後，每顆一段（`h3.sub` 圖示＋型號，第二顆起以 `.sect` 分隔線隔開），讓硬碟排在一起；依 `device` 分組（`sensorGroups`，與儀表板共用），卡片標題為類型圖示（`sensorIcon`：處理器晶片、硬碟、顯示卡（含 `nvidia`）`fa-display`、其他為主機板 `fa-server`）與裝置名稱（過長截斷）；
    項目名稱經 `sensorLabel`（`SENSOR_LABEL` 正規式對照）翻譯：`Package id 0` → 處理器封裝、`Core N` → 核心 N、`Tctl`／`Tdie` → 處理器、`TccdN` → CCD N、`Composite` → 整體、`Sensor N` → 感測點 N、`edge`／`junction`／`mem` → 核心／熱點／顯示記憶體、無 label 的 `tempN`／`fanN`／`inN` → 溫度／風扇／電壓 N，其他原樣顯示。數值依 `kind` 格式化（`fmtSensor`：溫度經 `temp()`、rpm、%、V 取兩位、W 取一位小數），數值格由 `sensorTd` 產生：有 `crit` 時滑鼠提示臨界溫度，達到臨界值時加 `.hot`（紅色粗體）。`powerN` 無 label 時顯示「功耗 N」；RAPL 區域 `package-N`／`core`／`dram`／`uncore` → 封裝／核心／記憶體／內顯功耗，與 CPU 溫度同一張卡。頁首提示（`.hint`）：需要時顯示「載入需要管理員權限的感測器」（`.hint > span` 佔滿一行），下方為按鈕（`data-elevate`）——沒有 `it8*`／`nct*` 晶片、或有 `sd*` 硬碟但沒有 `drivetemp` 時顯示「更多感測器」→ `elevate("modules")`；`raplLocked` 時顯示「CPU功耗」→ `elevate("rapl")`；兩者都不需要時整列不顯示。`elev` 記錄執行中的動作（按鈕停用、顯示「等待授權…」）與上次錯誤（`.hint.err` 紅字）；成功後新數值由下一次快照帶出。App 不自動提權，開機自動載入仍依 `docs/Sensors_Improvement.md` 手動設定。同型號的兩顆硬碟會併在同一張卡。
    卡片內容由 `sensorBody` 產生：有名稱的讀數依種類分段（`KIND_NAME`：溫度／風扇（含 %）／電壓／功耗，只有一種時不顯示分段標題）；沒有名稱的（`tempN`／`inN`／`powerN`）與 0 rpm 的風扇（多半是沒接的接口）收進 `<details class="raw">`「其他讀數（N）」，預設收合，展開狀態以裝置名稱存在 `rawOpen`（每秒重繪仍保留；點 `summary[data-raw]` 時取消瀏覽器預設切換、改由狀態重繪）。整張卡都沒有名稱時（例如 ACPI 溫度區）全部直接顯示。
  - 儲存：第一次進入時掃描家目錄（`SystemInfo.home`）。上方路徑、項目數、總大小、「選擇磁碟…」下拉選單（`label.btn.pick` 外觀與其他按鈕相同，透明的 `select[data-du=disk]` 疊在上面開系統選單；選項為 `snap.disks` 各硬碟的掛載點，排除 `/boot`、`/efi` 開頭的啟動磁區，顯示「型號 · 掛載點」；`#content` 的 `change` 事件選了就掃描該掛載點，掃描不跨檔案系統，所以只算這個分割區）、「選擇資料夾…」（`plugin:dialog|open`，`directory: true`）與重新掃描。
    `du.path` 為目前掃描的路徑：開始掃描時立刻清掉舊結果（`root`、`levels`），標題改為新的磁碟／路徑，不會在長時間掃描中顯示上一顆硬碟；掃描中每 0.5 秒呼叫 `scan_progress`（與快照輪詢無關，暫停也照常），副標顯示「掃描中… 已掃描 N 項」，並以 `top` 當第一層畫出色塊與清單（大小隨掃描成長）；掃描中點色塊或清單不會展開（快取仍是上一次的），重新掃描按鈕換成取消按鈕（`data-du=cancel` → `scan_cancel`）。取消或失敗時清掉不完整的第一層，只顯示錯誤訊息。
    掃描路徑是某個掛載點（`snap.mounts` 找得到）時：標題改為硬碟圖示與型號，下方第一行為掛載路徑，第二行為「N 項 · 已用 · 總容量 · 可用」（每行單行、過長截斷，滑鼠提示有全文），磁碟按鈕只顯示型號（選單選項仍為「型號 · 掛載點」以區分同一顆硬碟的分割區）；左欄名稱下方標示總容量；色塊條只畫掃描到的項目，不放可用空間或無法讀取的部分（會把資料夾擠成細條）。
    每一層一列色塊（前 40 項，其餘合併為灰色「其他」），點色塊選取並展開下一層（後面的層會被截掉）；右側清單為最後一層的內容（前 200 項），點資料夾往下一層。寬度小於 40 px 的色塊不顯示名稱（滑鼠提示仍有）。
- 繪圖：`chart`（處理器、顯示卡；SVG `viewBox 0 0 59 100`、背景三條線：頂端（上限）長虛線、中間（一半）短虛線、底部（0）實線；`preserveAspectRatio="none"`、`non-scaling-stroke`，最新點在右緣、`null` 斷線；半透明面積上再畫一條線，0 的位置在 y=99，全為 0 的序列仍看得到底線）、
  `mirror`（上下對稱條狀圖：中線以上為上傳／讀取（桃紅）、以下為下載／寫入（藍），上下各自依近期最大值縮放，0 畫成中線上的短條；以 HTML flex 排 120 根長條（間隔 1 px、外側端點 2 px 圓角：上半部圓在頂端、下半部圓在底端），高度以 `data-h` 經 CSSOM 設定，不用 SVG 以免圓角被拉伸變形；用於 `flow` 與網路頁）、`bar`（多段堆疊條）、`ring`（同心圓環，`pathLength=100` 以 dasharray／dashoffset 畫各段；線端皆為圓頭；多段時反向繪製，讓前一段的圓頭疊在下一段上）。
- 偏好：`localStorage` `rw-prefs`（JSON：`theme`、`interval`、`unit`、`nic`、`gpu`、`dev`，壞掉時用預設值）；左下主題鈕以 `system` → `dark` → `light` 循環；`system` 時監聽 `prefers-color-scheme`。側欄收合存 `rw-collapsed`。`localStorage` 讀寫失敗時忽略。
- 所有來自系統的字串（程序名稱、感測器標籤、型號、檔名、路徑）經 `esc` 跳脫後才放進 HTML。
- `window.__TAURI__` 不存在時改用 `window.mockInvoke`：只供在一般瀏覽器載入頁面、注入假資料檢查排版，App 內不會用到。
