# Resource Watch v0.0.1

KDE Plasma 6 原生資源監控 Plasmoid（QML + KSysGuard Sensors）。
監控 CPU / GPU / RAM / Disk 讀寫 / Network 上下行。Package ID：`io.github.local.resourcewatch`。

> ⚠️ **狀態：尚未在 Bazzite + Plasma 6 + Wayland 實機驗證（NOT TESTED）。**
> 靜態檢查通過不代表感測器與 UI 已正常，請依下列步驟驗證。

## 安裝（使用者空間，不需 sudo）

```bash
./install.sh            # 安裝或更新
./install.sh uninstall  # 解除安裝
plasmawindowed io.github.local.resourcewatch   # 獨立測試視窗，不影響桌面
```

安裝後：右鍵桌面或面板 →「新增小工具」→ 搜尋「Resource Watch」。

## 現有功能
- Compact（面板）：CPU / GPU / RAM 百分比，點擊展開；水平與垂直面板各有排版。
- Full（桌面 / 彈窗）：五張卡片，寬度足夠時自動雙欄。
- 單一 `SensorManager` 供兩種視圖共用；預設 2000 ms 更新。
- 感測器無資料 / 無效 / 負速率 → 顯示 `--` 與「Sensor unavailable」，不會顯示成 0%。

## 尚未實作（規劃）
設定頁（KConfig）、單一 GPU / 磁碟 / 網卡選擇、歷史曲線、stale 偵測、毛玻璃。

## 實機驗證清單
1. `plasmawindowed io.github.local.resourcewatch`：能開啟，五張卡有資料。
2. 與 KDE「系統監視器」對照 CPU / RAM 趨勢是否一致。
3. 複製大檔案 → Disk 讀寫變動；下載檔案 → Network 下行變動。
4. GPU（如 Quadro P620）：若顯示 `--`，用 `kstatsviewer --help` 確認實際感測器 ID。
5. 加入水平 / 垂直面板，確認不溢出、點擊可展開。
6. 淺色 / 深色主題切換。

## 故障排除
- **小工具載入失敗**：執行 `plasmawindowed io.github.local.resourcewatch` 看終端錯誤。
  若錯誤指出 `updateRateLimit` 不是 `Sensor` 的屬性，將 `models/SensorManager.qml` 中各 `updateRateLimit:` 行刪除即可（會改用系統預設更新率）。
- **GPU 一直是 `--`**：`gpu/all/usage`、`gpu/gpu0/usage` 在你的系統上可能不存在，請改用實際列出的 ID。
- **Network 數值偏高**：`network/all` 彙總含 VPN / Docker 等虛擬介面。

## 專案結構
`Agent.md` / `AGENTS.md` 為開發規範；`package/` 是唯一被打包的目錄；`tests/static_check.sh` 為靜態檢查。
