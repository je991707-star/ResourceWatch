# 感測器改善

感測器區塊的現況、問題、分階段改善方案與驗收條件。每個 Phase 完成後在驗收條件打 ✅。

## 1. 目前環境

- 硬體：Gigabyte H97M-D3H（Intel 4 核，ITE Super I/O）、NVIDIA 顯示卡（專有驅動，NVML）、nvme0n1、SATA 傳統硬碟 sda（TOSHIBA DT01ABA2）、讀卡機 sdc。
- 系統：Bazzite（Fedora 45，kernel 6.18），不可變系統，`/etc` 可寫。
- 目前讀得到的 hwmon（`/sys/class/hwmon`）：

  | chip | 項目 | label |
  | --- | --- | --- |
  | `acpitz` | temp1、temp2 | （無） |
  | `nvme` | temp1、temp2 | Composite、Sensor 1 |
  | `coretemp` | temp1–5 | Package id 0、Core 0–3 |

- 核心有但未載入的模組：`it87.ko`、`nct6775.ko`、`drivetemp.ko`。
- 程式位置：後端 `src-tauri/src/sys.rs` `sensors()`（只讀 `temp*_input`／`fan*_input`，`Sensor { chip, label, kind, value }`）；CPU 溫度取自 sensors 的 `coretemp/k10temp/zenpower/cpu_thermal`；前端 `frontend/app.js` `sensorsCard`（儀表板前 8 筆，顯示 `chip · label`）、`PAGES.sensors`（依 chip 分組）。

## 2. 遇到的問題

1. **看不懂**：直接顯示核心原始名稱，例如 `coretemp · Package id 0`、`acpitz · temp1`、`nvme · Composite`；不知道是哪個裝置、哪個位置。
2. **數量少，原因：**
   - 主機板 Super I/O 驅動（it87）沒載入 → 沒有風扇、電壓、主機板溫度。
   - SATA 硬碟溫度需要 `drivetemp` → sda 沒有溫度。
   - NVIDIA 溫度走 NVML，不在 hwmon → 感測器頁沒有顯示卡（只出現在 GPU 卡片）。
   - 後端忽略 hwmon 的 `in*`（電壓）、`power*`（瓦數），也不讀 `temp*_crit`／`_max` 臨界值。
3. **儀表板卡片**只取前 8 筆原始資料，常被同一顆 CPU 的各核心佔滿。

## 3. 改善方案（分 Phase）

### Phase 1：名稱看得懂
- 後端 `Sensor` 加 `device`（裝置顯示名稱），`chip` 保留供 CPU 溫度判斷。
  - `coretemp`／`k10temp`／`zenpower` → `clean_cpu_name` 後的 CPU 型號
  - `nvme`／`drivetemp` → 沿 `hwmon/device` 讀 `model`（同磁碟型號，`_` 換空格）
  - `acpitz` → 主機板（ACPI）；`it87`／`nct*` → 主機板；`amdgpu`／`nouveau` → 顯示卡；其他保留 chip 名
- 前端 label 對照：`Package id N` → 處理器封裝、`Core N` → 核心 N、`Tctl`／`Tdie` → 處理器、`Composite` → 整體、`Sensor N` → 感測點 N、`edge`／`junction`／`mem` → 核心／熱點／顯示記憶體；無 label 的 `tempN`／`fanN`／`inN` → 溫度 N／風扇 N／電壓 N。
- 感測器頁依 `device` 分組，卡片標題加類型圖示（處理器、硬碟、顯示卡、主機板）。

### Phase 2：補上顯示卡
- 用已載入的 NVML 把 NVIDIA 溫度、風扇 %（`nvmlDeviceGetFanSpeed`）、功耗（`nvmlDeviceGetPowerUsage`，mW→W）加入 sensors，`device` 為 GPU 名稱；符號不存在時略過（同 encoder 作法）。

### Phase 3：更多 hwmon 類型
- 讀 `in*_input`（mV→V）、`power*_input`／`power*_average`（µW→W）；`kind` 擴充為 `"temp"|"fan"|"volt"|"power"`。
- 讀 `temp*_crit`（沒有則 `_max`）帶入 `crit`，前端超過時標紅。
- `fmtSensor` 支援 °C/°F（經 `temp()`）、rpm、%、V、W。
- JSON 契約：`sensors: [{ chip, device, label, kind, value, crit }]`。

### Phase 4：系統模組指引
- 文檔寫明使用者手動一次的設定（App 不自動 modprobe，需要 root）：
  - `/etc/modules-load.d/sensors.conf`：`drivetemp`、`it87`
  - H97M-D3H 的 ITE 晶片常需 `options it87 ignore_resource_conflict=1`（或 kernel 參數 `acpi_enforce_resources=lax`），附風險說明與 `sudo modprobe` 驗證指令、`rpm-ostree kargs` 寫法。
- 感測器頁在偵測不到主機板晶片時顯示一行提示。

#### 操作步驟（本機：Gigabyte H97M-D3H，ITE Super I/O）

App 不會自己載入模組（需要 root）。以下指令需要 `sudo`，請自行執行。

**1. 先試載（立即生效，重開機後就沒了）**

```bash
sudo modprobe drivetemp                              # SATA 硬碟溫度（sda TOSHIBA DT01ABA2；sdc 是讀卡機，不會有溫度）
sudo modprobe it87 ignore_resource_conflict=1        # 主機板：風扇、電壓、主機板溫度
cat /sys/class/hwmon/*/name                          # 應多出 drivetemp 與 it86xx/it87xx
sudo dmesg | grep -iE 'it87|drivetemp' | tail
```

- `it87` 載入失敗、dmesg 出現 `ACPI resource conflict`：確認有帶 `ignore_resource_conflict=1`。
- dmesg 顯示找不到晶片（`Found ... not supported` 或沒有任何訊息）：這塊主機板的晶片不受此驅動支援，停在這一步，不要用 `force_id` 硬套。
- 重開 ResourceWatch，感測器頁應出現「主機板」與硬碟型號卡片。

**2. 確認沒問題後，設定為開機自動載入**

```bash
printf 'drivetemp\nit87\n' | sudo tee /etc/modules-load.d/sensors.conf
echo 'options it87 ignore_resource_conflict=1' | sudo tee /etc/modprobe.d/it87.conf
```

Bazzite 的 `/etc` 會在系統更新時保留，不需要 `rpm-ostree` 重新部署。

**3. 只有模組參數無效時才用的備案：kernel 參數**

```bash
rpm-ostree kargs --append=acpi_enforce_resources=lax   # 需要重開機；影響所有驅動，範圍比模組參數大
rpm-ostree kargs --delete=acpi_enforce_resources=lax   # 還原
```

**風險**
- `ignore_resource_conflict`：BIOS（ACPI）與驅動會同時存取晶片。只讀取通常沒問題，少數主機板可能偶爾讀到錯誤數值；App 只讀不寫，不會改風扇設定（不要手動寫 `pwm*` 檔案）。
- `drivetemp`：App 每次更新都會讀溫度，傳統硬碟進入待機後可能因此被喚醒或無法待機；介意的話只載入 `it87`。

**還原**

```bash
sudo rm /etc/modules-load.d/sensors.conf /etc/modprobe.d/it87.conf
sudo modprobe -r it87 drivetemp
```

### Phase 5：儀表板精簡
- 儀表板感測器卡片每個裝置只取一筆代表值（封裝／Composite／GPU 溫度／第一個風扇），取代前 8 筆。

**不做**：歷史曲線、警報通知、自動載入模組、PCI ID 資料庫。

## 4. 驗收條件（分 Phase）

每個 Phase 共同條件：`cargo test` 通過（新增 label／單位換算測試）、`cargo tauri build` 無錯誤、`Readme_Architecture.md` 對應章節已更新、已 commit。

- **Phase 1** ✅
  - `coretemp · Package id 0` 顯示為「〈CPU 型號〉· 處理器封裝」，各核心為「核心 0–3」
  - `acpitz · temp1` 顯示為「主機板（ACPI）· 溫度 1」
  - nvme 卡片標題為硬碟型號，項目為「整體」「感測點 1」
  - 儀表板 CPU 溫度不受影響
- **Phase 2** ✅
  - 感測器頁出現顯示卡卡片（溫度、風扇 %、功耗）
  - 溫度與 GPU 卡片一致；無 NVIDIA 的機器不出錯
- **Phase 3** ✅
  - 有 `in*`／`power*` 的晶片顯示 V／W，單位正確
  - 有 `crit` 的溫度超過時標紅；°F 設定下換算正確
- **Phase 4** ☐
  - 依文檔載入模組後重開：sda 出現溫度（sdc 為讀卡機，不會有）、主機板出現風扇與電壓，且名稱為可讀名稱
  - 未載入時感測器頁顯示提示 ✅
- **Phase 5** ✅
  - 儀表板感測器卡片一個裝置一列，不再被核心溫度佔滿

