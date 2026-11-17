# 應用程式頁改版

應用程式頁比照 Windows 工作管理員「處理程序」分頁的功能改版：現況、需求對照、分階段方案與驗收條件。每個 Phase 完成後在驗收條件打 ✅。
整體風格不變（卡片＋斑馬紋）。

## 1. 現況

- 一張扁平表格：名稱、PID、CPU、記憶體；最多 300 列，點欄位標題排序。
- 後端 `Proc { pid, name, cpu, mem }`，每 2 秒 `processes`。
- 沒有分組，同一個應用程式的多個程序（瀏覽器、VS Code）散在各處；看不出哪些是應用程式、哪些是背景服務。

## 2. 需求對照

| 功能 | Linux 做法 |
| --- | --- |
| 應用程式／背景處理程序分區 | `/proc/<pid>/cgroup`：使用者 `app.slice/app-*`（.scope／.service）→ 應用程式；使用者的其他 unit（session.slice、background.slice…）→ 背景處理程序；`system.slice`、其他使用者、核心執行緒 → 系統處理程序 |
| 群組（例如 Chrome (13)） | 同一個 cgroup unit 為一組；只有 1 個程序時不顯示展開箭頭；核心執行緒收成一組「核心執行緒」 |
| 應用程式名稱／圖示 | unit 名稱解析出 desktop id（`app-[launcher-]<id>[-<n>.scope／@<x>.service]`，`\x2d` 還原，`flatpak-` 前綴去掉）→ `.desktop` 的 `Name`（`Name[zh_TW]` 優先）與圖示；查不到時用群組中 PID 最小的程序名稱 |
| 狀態 | `stat` 狀態：`T`／`t` → 已暫停、`Z` → 殭屍；`nice > 0` 顯示綠色葉子（低優先權） |
| 磁碟（MB/秒） | `/proc/<pid>/io` 的 `read_bytes + write_bytes` 差值 |
| GPU %、GPU 引擎 | NVIDIA：NVML `nvmlDeviceGetProcessUtilization`（sm → 3D、enc → Video Encode、dec → Video Decode）；AMD／Intel：`/proc/<pid>/fdinfo` 的 `drm-engine-*` 差值；引擎欄顯示 `GPU N - 3D` |
| 欄位標題上方的整機 % | CPU＝`cpu.usage`、記憶體＝used／total、磁碟＝`/proc/diskstats` 活動時間（各磁碟取最大）、GPU＝各 GPU 使用率最大值 |
| PID | 改為列的滑鼠提示 |

不做：上方工具列（執行新工作、結束工作、效率模式、⋯）、網路欄（一般使用者讀不到每個程序的流量）、熱度底色。

## 3. 分階段方案

### Phase 1：後端程序資料
- `parse_proc_stat` 多取狀態、ppid、nice。
- `/proc/<pid>/cgroup` → 分類（`app`／`bg`／`sys`）、unit 名稱、desktop id。
- `/proc/<pid>/io` → 磁碟速率（第一次為 `null`）。
- `Proc` 新欄位：`state`、`nice`、`kind`、`unit`、`app`、`disk`；`Disk` 新增 `busy`（活動時間 %）。

驗收：
- [x] ✅ `cargo test` 通過（stat、cgroup 分類、io、diskstats 活動時間）
- [x] ✅ `processes` 回傳新欄位；自己的程序有 `disk`，讀不到的為 `null`

### Phase 2：前端分區、群組與新欄位
- `icons.rs` 讀 `.desktop` 的 `Name`，`app_icons` 回傳 `{ icon, name }`。
- 應用程式頁改為三區（應用程式／背景處理程序／系統處理程序）、可展開群組、加總數值、展開狀態在重繪後保留。
- 欄位：名稱、狀態、CPU、記憶體、磁碟、GPU、GPU 引擎；標題上方顯示整機 %。

驗收：
- [x] ✅ VS Code、Steam 等出現在「應用程式」且可展開
- [x] ✅ Plasma 服務在「背景處理程序」，系統服務與核心執行緒在「系統處理程序」
- [x] ✅ 排序、展開在每 2 秒重繪後維持；儀表板前 5 清單正常

### Phase 3：GPU 欄位
- NVML 每程序使用率（可選符號，缺少時略過）。
- DRM fdinfo 引擎時間（依 `drm-client-id` 去重，`drm-pdev` 對應顯示卡）。
- `Proc` 新增 `gpu`、`gpuEngine`；群組 GPU 取加總（上限 100），引擎取使用率最高的子程序。

驗收：
- [x] ✅ `cargo test` 通過（fdinfo 解析）
- [x] ✅ 播放影片或串流時，對應程序出現 `GPU 0 - Video Decode` 等引擎（本機實測：sunshine 串流時為 `GPU 0 - Video Encode` 40%）

### Phase 4：收尾
- `Readme_Architecture.md` 各章節更新、`mockInvoke` 假資料補上新欄位、release 編譯無誤。

驗收：
- [ ] 文件與程式一致，編譯無錯誤

## 4. 已知限制

- 其他使用者／root 的程序：磁碟與 GPU（fdinfo）讀不到，顯示 `--`。NVIDIA 的 NVML 不受此限。
- 非 systemd 或不建立 app scope 的桌面環境：程序會落在背景或系統區。
