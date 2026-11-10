import QtQuick
import org.kde.ksysguard.sensors as Sensors
import org.kde.plasma.plasma5support as Plasma5Support
import "FormatUtils.js" as FormatUtils

// 唯一的感測器訂閱 / 正規化 / 歷史緩衝層。UI 不接觸感測器 ID。
// 數值型 metric：{ id, label, status, rawValue, displayValue, percent }
// 成對型（RAM/VRAM/Swap）另有 usedText / totalText。
QtObject {
    id: manager

    // ---- 設定（由 main.qml 從 Plasmoid.configuration 綁定）----
    // 所有即時監控、歷史採樣與 HTML 快照共用同一個更新間隔。
    // KSystemStats 仍以 updateRateLimit 訂閱，GPU／Disk／History／HTML 由單一 Timer 排程。
    property int updateInterval: 500
    property int ipRefreshSerial: 0
    property int diskRefreshSerial: 0
    property int historyPoints: 30
    property int gpuIndex: 0
    onGpuIndexChanged: { if (_schedulerStarted) requestGpuProbe(); }
    // 自動辨識的實體硬碟清單
    property var disks: []

    // 每顆硬碟的即時讀寫速度
    property var diskRates: ({})

    // 上次讀取的磁碟累計計數器
    property var _previousDiskCounters: ({})

    readonly property var disk1:
        disks.length > 0 ? disks[0] : null

    readonly property var disk2:
        disks.length > 1 ? disks[1] : null

    property string netInterface: "all"  // "all" = network/all 彙總

    property bool graceElapsed: false
    signal snapshotDue()
    property bool _schedulerStarted: false
    property double _graceDeadline: 0
    property var _nextDue: ({})
    property string _manualNetIp: ""
    property bool _hasManualNetIp: false
    onNetInterfaceChanged: {
        _hasManualNetIp = false;
        _manualNetIp = "";
    }
    onIpRefreshSerialChanged: { if (_schedulerStarted) refreshNetworkIp(); }
    onDiskRefreshSerialChanged: { if (_schedulerStarted) refreshDisks(); }
    property var _warned: ({})

    readonly property bool disk2Enabled: disk2 !== null
    readonly property bool netHasInterface: netInterface.length > 0 && netInterface !== "all"

    readonly property string _gpuPrefix: "gpu/gpu" + gpuIndex

    // Linux DRM PCI vendor ID -> NVIDIA / AMD / Intel / other.
    // Helper performs discovery before querying NVIDIA-specific counters.
    property var _gpuProbeData: ({ devices: [] })
    readonly property var gpuDevice: _selectGpuDevice()
    readonly property string gpuVendor: gpuDevice ? gpuDevice.vendor : _vendorFromName(gpuNameSensor.value)
    readonly property string gpuSource: gpuVendor === "nvidia"
        ? (gpuDevice ? gpuDevice.source : "unavailable") : "ksystemstats"
    readonly property string _netPrefix: "network/" + (netHasInterface ? netInterface : "all")

    // ---- 對 UI 公開的資料 ----
    readonly property var cpu: _percentMetric("cpu", "CPU", cpuSensor)
    readonly property string cpuFreqText: _text(cpuFreqSensor)
    readonly property string cpuTempText: _text(FormatUtils.isValidNumber(cpuTempAvg.value) ? cpuTempAvg : cpuTempMax)
    readonly property string uptimeText: _text(uptimeSensor)
    // KSystemStats cpu/all/name 可能回傳「全部」而非實際 CPU 型號。
    // 先用 /proc/cpuinfo 的型號，失敗才回退至有效的 KDE 字串。
    readonly property string cpuName: _firstString([
        _hardwareProbeData.cpuModel,
        _usableCpuName(cpuName0.value),
        _usableCpuName(cpuNameAll.value)
    ]) || "--"
    readonly property string ramSpeedText:
        _firstString([_hardwareProbeData.ramSpeedText]) || "--"

    // NVIDIA: NVML GPU-Util；絕不使用有爭議的 KDE 合成總負載代替。
    // 其他品牌: 優先選定 GPU 的 KDE 感測器，而非 gpu/all 彙總。
    readonly property var _gpuUsageSensor:
        FormatUtils.isValidNumber(gpuIdx.value) ? gpuIdx : gpuAll
    readonly property var gpu: gpuVendor === "nvidia"
        ? _probePercentMetric("gpu", "GPU", gpuDevice ? gpuDevice.usage : null)
        : _percentMetric("gpu", "GPU", _gpuUsageSensor)
    readonly property var encoder: _probePercentMetric("encoder", "NVENC",
        gpuVendor === "nvidia" && gpuDevice ? gpuDevice.encoder : null)
    readonly property var decoder: _probePercentMetric("decoder", "NVDEC",
        gpuVendor === "nvidia" && gpuDevice ? gpuDevice.decoder : null)
    readonly property string gpuTempText: _text(gpuTemp)
    readonly property string gpuName: _firstString([gpuNameSensor.value,
        gpuDevice ? gpuDevice.name : ""])
    readonly property var vram: _pairMetric("vram", "VRAM", vramUsed, vramTotal)

    readonly property var ram: _pairMetric("ram", "RAM", memUsed, memTotal)
    readonly property var swap: _pairMetric("swap", "Swap", swapUsed, swapTotal)

    readonly property var disk1Read:
        _diskMetric("disk1Read", "Disk 1 read", disk1, "read")

    readonly property var disk1Write:
        _diskMetric("disk1Write", "Disk 1 write", disk1, "write")

    readonly property var disk2Read:
        _diskMetric("disk2Read", "Disk 2 read", disk2, "read")

    readonly property var disk2Write:
        _diskMetric("disk2Write", "Disk 2 write", disk2, "write")

    readonly property var netDown: _rateMetric("netDown", "Download", netDownS)
    readonly property var netUp: _rateMetric("netUp", "Upload", netUpS)
    readonly property string netIp: _hasManualNetIp
        ? _manualNetIp : (_firstString([netIpS.value]) || "--")

    // 歷史：由統一排程器按 updateInterval 記錄，緩衝長度 historyPoints
    property var history: ({ cpu: [], gpu: [], encode: [], decode: [], vram: [], ram: [], netDown: [], netUp: [] })

    // ---- 內部函式 ----
    function _status(valid) {
        return valid ? "ready" : (graceElapsed ? "unavailable" : "loading");
    }

    function _str(v) { return (typeof v === "string") ? v : ""; }
    function _usableCpuName(name) {
        const value = String(name || "").trim();
        // 顯示器的泛用名稱不是 CPU 型號。
        if (/^(all|all cores|cpu|cpu\s*[0-9]+|全部|總計)$/i.test(value)) return "";
        return value;
    }


    function _firstString(list) {
        for (let i = 0; i < list.length; i++) {
            const s = _str(list[i]);
            if (s.length > 0) return s;
        }
        return "";
    }

    function _text(s) {
        return (s && FormatUtils.isValidNumber(s.value)) ? FormatUtils.displayText(s.formattedValue) : "--";
    }

    function _percentMetric(id, label, s) {
        const raw = s ? s.value : undefined;
        const valid = FormatUtils.isValidNumber(raw);
        let text = "--";
        if (valid) {
            text = FormatUtils.displayText(s.formattedValue);
            if (text === "--") text = FormatUtils.formatPercent(raw);
        }
        return {
            id: id, label: label, status: _status(valid),
            rawValue: valid ? raw : null, displayValue: text,
            percent: valid ? FormatUtils.clampPercent(raw) : null
        };
    }

    function _probePercentMetric(id, label, value) {
        // The NVIDIA backend already bounds percentages to 0..100.
        // Only the missing/null case needs checking at this internal boundary.
        const valid = FormatUtils.isValidNumber(value);
        return {
            id: id, label: label, status: _status(valid),
            rawValue: valid ? value : null,
            displayValue: valid ? FormatUtils.formatPercent(value) : "--",
            percent: valid ? value : null
        };
    }

    function _vendorFromName(name) {
        const v = String(name || "").toLowerCase();
        if (/nvidia|geforce|quadro|tesla|\bgtx\b|\brtx\b/.test(v)) return "nvidia";
        if (/\bamd\b|radeon|\bati\b/.test(v)) return "amd";
        if (/\bintel\b|\biris\b|\buhd\b|\barc\b/.test(v)) return "intel";
        return "other";
    }

    function _gpuNameKey(name) {
        return String(name || "").toLowerCase()
            .replace(/nvidia|geforce|corporation|graphics|controller|series/g, "")
            .replace(/[^a-z0-9]/g, "");
    }

    // KDE 的 gpu0 與 NVIDIA 的 index 0 在混合顯卡系統中不保證相同。
    // 優先使用 KSystemStats 名稱匹配，再使用唯一廠牌 / DRM card 順序。
    function _selectGpuDevice() {
        const inventory = _gpuProbeData && _gpuProbeData.devices;
        if (!Array.isArray(inventory) || inventory.length === 0) return null;
        const kdeName = String(gpuNameSensor.value || "");
        const target = _gpuNameKey(kdeName);
        const vendor = _vendorFromName(kdeName);
        if (target.length >= 4) {
            const matchingName = inventory.filter(function(device) {
                const candidate = _gpuNameKey(device.name);
                return candidate.length >= 4
                    && (target.indexOf(candidate) >= 0 || candidate.indexOf(target) >= 0);
            });
            if (matchingName.length === 1) return matchingName[0];
        }
        if (vendor !== "other") {
            const sameVendor = inventory.filter(d => d.vendor === vendor);
            if (sameVendor.length === 1) return sameVendor[0];
            if (sameVendor.length > 1 && gpuIndex < sameVendor.length)
                return sameVendor[gpuIndex];
        }
        // No identified vendor: use DRM order only if it is unambiguous.
        return inventory.length === 1 ? inventory[0] :
            (gpuIndex >= 0 && gpuIndex < inventory.length ? inventory[gpuIndex] : null);
    }

    function _rateMetric(id, label, s) {
        const raw = s ? s.value : undefined;
        const valid = FormatUtils.isValidRate(raw);
        return {
            id: id, label: label, status: _status(valid),
            rawValue: valid ? raw : null,
            displayValue: valid ? FormatUtils.displayText(s.formattedValue) : "--",
            percent: null
        };
    }

    // 已用 / 總量；總量缺失或為 0 時不計算百分比
    function _pairMetric(id, label, usedS, totalS) {
        const used = usedS.value;
        const total = totalS.value;
        const p = FormatUtils.percentOf(used, total);
        const ok = p !== null;
        return {
            id: id, label: label, status: _status(ok),
            rawValue: ok ? used : null,
            displayValue: ok ? FormatUtils.formatPercent(p) : "--",
            percent: ok ? FormatUtils.clampPercent(p) : null,
            usedText: FormatUtils.isValidNumber(used) ? FormatUtils.displayText(usedS.formattedValue) : "--",
            totalText: (FormatUtils.isValidNumber(total) && total > 0) ? FormatUtils.displayText(totalS.formattedValue) : "--"
        };
    }

    function _sample() {
        const h = history;
        const n = historyPoints;
        history = {
            cpu: FormatUtils.pushHistory(h.cpu, cpu.percent, n),
            gpu: FormatUtils.pushHistory(h.gpu, gpu.percent, n),
            encode: FormatUtils.pushHistory(h.encode, encoder.percent, n),
            decode: FormatUtils.pushHistory(h.decode, decoder.percent, n),
            vram: FormatUtils.pushHistory(h.vram, vram.percent, n),
            ram: FormatUtils.pushHistory(h.ram, ram.percent, n),
            netDown: FormatUtils.pushHistory(h.netDown, netDown.rawValue, n),
            netUp: FormatUtils.pushHistory(h.netUp, netUp.rawValue, n)
        };
    }

    // ---- 統一排程：只用一個 one-shot Timer，睡到下一個工作到期 ----
    // KSystemStats 本身是推播訂閱；不在此處重啟它的系統服務。
    // 使用者在 HTML 設定頁變更更新間隔後，重新安排四個工作的下次到期時間。
    // 不重新查詢固定硬體資訊，不連帶重掃 IPv4 或硬碟清單。
    function _rescheduleAll() {
        if (!_schedulerStarted) return;
        const deadline = Date.now() + updateInterval;
        _nextDue = { gpu: deadline, disk: deadline, history: deadline, html: deadline };
        _armScheduler();
    }
    onUpdateIntervalChanged: _rescheduleAll()

    function _armScheduler() {
        if (!_schedulerStarted) return;
        const due = _nextDue;
        // Fixed four jobs: avoid temporary arrays on every scheduler wakeup.
        const first = Math.min(due.gpu, due.disk, due.history, due.html,
                               graceElapsed ? Infinity : _graceDeadline);
        const wait = Math.max(1, first - Date.now());
        _schedulerTimer.interval = Math.max(1, Math.ceil(wait));
        _schedulerTimer.restart();
    }

    function _runScheduled() {
        if (!_schedulerStarted) return;
        const now = Date.now();
        if (!graceElapsed && now >= _graceDeadline)
            graceElapsed = true;
        const next = Object.assign({}, _nextDue);
        // 每項工作至多執行一次；固定工作不需每次建立 jobs 陣列。
        const deadline = now + updateInterval;
        if (now >= next.gpu) { next.gpu = deadline; requestGpuProbe(); }
        if (now >= next.disk) { next.disk = deadline; requestDiskStats(); }
        if (now >= next.history) { next.history = deadline; _sample(); }
        if (now >= next.html) { next.html = deadline; snapshotDue(); }
        _nextDue = next;
        _armScheduler();
    }

    property Timer _schedulerTimer: Timer {
        interval: 500
        repeat: false
        onTriggered: manager._runScheduled()
    }

    function requestGpuProbe() {
        if (_gpuProbe.connectedSources.length === 0)
            _gpuProbe.connectSource(_gpuProbeCommand);
    }
    function requestDiskStats() {
        if (disks.length > 0 && _diskStats.connectedSources.length === 0)
            _diskStats.connectSource(_diskStatsCommand);
    }
    function refreshDisks() {
        if (_diskDiscovery.connectedSources.length === 0)
            _diskDiscovery.connectSource(_lsblkCommand);
    }
    function refreshNetworkIp() {
        if (_ipRefresh.connectedSources.length === 0)
            _ipRefresh.connectSource(_ipRefreshCommand);
    }

    Component.onCompleted: {
        refreshDisks();                  // 裝置清單只在啟動與手動要求時掃描
        _requestHardwareProbe();         // 靜態資訊只查詢一次
        _schedulerStarted = true;
        const now = Date.now();
        _graceDeadline = now + 5000;
        _nextDue = { gpu: now, disk: now, history: now, html: now };
        _runScheduled();
    }

    function _shellQuote(input) {
        return "'" + String(input).replace(/'/g, "'\\''") + "'";
    }

    // Path is resolved relative to this QML file, including when installed by KPackage.
    readonly property string _gpuProbeCommand: "/usr/bin/python3 " +
        _shellQuote(decodeURIComponent(String(Qt.resolvedUrl("../../scripts/gpu_backend.py"))
            .replace(/^file:\/\//, "")))

    property Plasma5Support.DataSource _gpuProbe: Plasma5Support.DataSource {
        engine: "executable"
        connectedSources: []
        onNewData: (source, data) => {
            disconnectSource(source);
            if (Number(data["exit code"]) !== 0) {
                manager._gpuProbeData = { devices: [] };
                if (!manager._warned.gpuProbe) {
                    console.warn("ResourceWatch: GPU probe failed:", data["stderr"]);
                    manager._warned.gpuProbe = true;
                }
                return;
            }
            try {
                const result = JSON.parse(String(data["stdout"] || ""));
                manager._gpuProbeData = Array.isArray(result.devices)
                    ? result : { devices: [] };
            } catch (error) {
                manager._gpuProbeData = { devices: [] };
                console.warn("ResourceWatch: invalid GPU probe JSON:", error);
            }
        }
    }

    // GPU 查詢由共用 scheduler 觸發；同時只允許一個查詢執行。

    // CPU 型號 + RAM 模組速度：低頻率查詢，不持續要求 root。
    property var _hardwareProbeData: ({})
    readonly property string _hardwareProbeCommand: "/usr/bin/python3 " +
        _shellQuote(decodeURIComponent(String(Qt.resolvedUrl("../../scripts/hardware_info.py"))
            .replace(/^file:\/\//, "")))

    property Plasma5Support.DataSource _hardwareProbe: Plasma5Support.DataSource {
        engine: "executable"
        connectedSources: []
        onNewData: (source, data) => {
            disconnectSource(source);
            if (Number(data["exit code"]) !== 0) {
                if (!manager._warned.hardwareProbe) {
                    console.warn("ResourceWatch: CPU/RAM info probe failed:", data["stderr"]);
                    manager._warned.hardwareProbe = true;
                }
                return;
            }
            try {
                const parsed = JSON.parse(String(data["stdout"] || ""));
                manager._hardwareProbeData = {
                    cpuModel: typeof parsed.cpuModel === "string" ? parsed.cpuModel : "",
                    ramSpeedText: typeof parsed.ramSpeedText === "string" ? parsed.ramSpeedText : ""
                };
            } catch (error) {
                console.warn("ResourceWatch: invalid CPU/RAM probe JSON:", error);
            }
        }
    }

    function _requestHardwareProbe() {
        if (_hardwareProbe.connectedSources.length === 0)
            _hardwareProbe.connectSource(_hardwareProbeCommand);
    }

    // ---- 感測器（ID 皆為候選；實機以 kstatsviewer --list 確認）----
    readonly property string _lsblkCommand:
        "/usr/bin/lsblk -J -b -d -o NAME,SIZE,MODEL,TRAN,TYPE"

    readonly property string _diskStatsCommand:
        "/usr/bin/cat /proc/diskstats"

    // 容量顯示：GiB / TiB
    function _formatDiskSize(bytes) {
        let value = bytes;
        const units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
        let index = 0;

        while (value >= 1024 && index < units.length - 1) {
            value /= 1024;
            index++;
        }

        return (index === 0 ? value.toFixed(0) : value.toFixed(1))
            + " " + units[index];
    }

    // 自動查詢實體硬碟
    property Plasma5Support.DataSource _diskDiscovery:
        Plasma5Support.DataSource {
            engine: "executable"
            connectedSources: []

            onNewData: (source, data) => {
                disconnectSource(source);

                if (Number(data["exit code"]) !== 0) {
                    console.warn("ResourceWatch: lsblk failed",
                                data["stderr"]);
                    return;
                }

                try {
                    const result = JSON.parse(data["stdout"]);
                    const allowed = [
                        "sata", "ata", "pata", "ide",
                        "sas", "scsi", "nvme"
                    ];

                    const found = [];

                    for (const device of (result.blockdevices || [])) {
                        const name = String(device.name || "");
                        const transport =
                            String(device.tran || "").toLowerCase();
                        const size = Number(device.size);

                        // 有些環境的原生 NVMe 不回報 TRAN
                        const nativeNvme =
                            /^nvme[0-9]+n[0-9]+$/.test(name);

                        if (device.type !== "disk") continue;
                        if (!name || !isFinite(size) || size <= 0) continue;
                        if (allowed.indexOf(transport) === -1
                            && !nativeNvme) continue;

                        found.push({
                            name: name,
                            model: String(device.model || "").trim() || "--",
                            sizeBytes: size,
                            sizeText: manager._formatDiskSize(size),
                            transport: transport
                        });
                    }

                    manager.disks = found;
                    manager._previousDiskCounters = {};
                    manager.diskRates = {};
                    manager.requestDiskStats();
                    manager.snapshotDue();

                } catch (error) {
                    console.warn("ResourceWatch: invalid lsblk JSON",
                                error);
                }
            }
        }

    // 磁碟枚舉僅在啟動及設定頁要求時執行，不再每 60 秒輪詢。

        // B/s 轉成可讀文字
        function _formatDiskRate(bytesPerSecond) {
            // _diskMetric() already validated this number before calling us.
            let value = bytesPerSecond;
            const units = ["B/s", "KiB/s", "MiB/s", "GiB/s"];
            let index = 0;

            while (value >= 1024 && index < units.length - 1) {
                value /= 1024;
                index++;
            }

            return (index === 0 ? value.toFixed(0) : value.toFixed(1))
                + " " + units[index];
        }

        function _diskMetric(id, label, disk, direction) {
            const data = disk ? diskRates[disk.name] : undefined;
            const rate = data ? data[direction] : undefined;
            const valid = FormatUtils.isValidRate(rate);

            return {
                id: id,
                label: label,
                status: _status(valid),
                rawValue: valid ? rate : null,
                displayValue: valid ? _formatDiskRate(rate) : "--",
                percent: null
            };
        }

        // 處理 /proc/diskstats 中的實體硬碟讀寫計數器
        function _processDiskStats(stdout) {
            const now = Date.now();
            const next = {};
            const rates = {};
            const selected = new Set(disks.map(d => d.name));

            for (const line of stdout.split("\n")) {
                const fields = line.trim().split(/\s+/);
                if (fields.length < 10) continue;

                const name = fields[2];
                if (!selected.has(name)) continue;
                const readSectors = Number(fields[5]);
                const writeSectors = Number(fields[9]);

                if (!isFinite(readSectors) || !isFinite(writeSectors))
                    continue;

                next[name] = {
                    readBytes: readSectors * 512,
                    writeBytes: writeSectors * 512,
                    time: now
                };
            }

            for (const disk of disks) {
                const current = next[disk.name];
                const previous = _previousDiskCounters[disk.name];

                if (!current || !previous) continue;

                const seconds = (current.time - previous.time) / 1000;
                if (seconds <= 0) continue;

                const readDelta =
                    current.readBytes - previous.readBytes;
                const writeDelta =
                    current.writeBytes - previous.writeBytes;

                // 裝置重設、重新連線時不產生負速率
                if (readDelta < 0 || writeDelta < 0) continue;

                rates[disk.name] = {
                    read: readDelta / seconds,
                    write: writeDelta / seconds
                };
            }

            _previousDiskCounters = next;
            diskRates = rates;
        }

        property Plasma5Support.DataSource _diskStats:
            Plasma5Support.DataSource {
                engine: "executable"
                connectedSources: []

                onNewData: (source, data) => {
                    disconnectSource(source);

                    if (Number(data["exit code"]) !== 0) {
                        if (!manager._warned.diskstats) {
                            console.warn(
                                "ResourceWatch: /proc/diskstats failed",
                                data["stderr"]);
                            manager._warned.diskstats = true;
                        }
                        manager.diskRates = {};
                        return;
                    }

                    manager._processDiskStats(
                        String(data["stdout"] || ""));
                }
            }

    // 讀寫速度由統一排程器控制，不另建 Timer。

    // IPv4 只在設定頁手動刷新時執行一次固定命令。
    // 不拼接使用者的介面名稱到 Shell；僅在解析 JSON 後篩選。
    readonly property string _ipRefreshCommand: "/usr/sbin/ip -j -4 address show"
    property Plasma5Support.DataSource _ipRefresh: Plasma5Support.DataSource {
        engine: "executable"
        connectedSources: []
        onNewData: (source, data) => {
            disconnectSource(source);
            if (Number(data["exit code"]) !== 0) {
                console.warn("ResourceWatch: IPv4 refresh unavailable:", data["stderr"]);
                return;
            }
            try {
                const adapters = JSON.parse(String(data["stdout"] || "[]"));
                const addresses = Array.isArray(adapters) ? adapters : [];
                const selected = addresses.filter(function(a) {
                    return manager.netHasInterface
                        ? a.ifname === manager.netInterface
                        : a.ifname !== "lo";
                });
                let ip = "--";
                for (const adapter of selected) {
                    const found = (adapter.addr_info || []).find(function(a) {
                        return a.family === "inet" && a.scope === "global";
                    });
                    if (found && found.local) { ip = String(found.local); break; }
                }
                manager._manualNetIp = ip;
                manager._hasManualNetIp = true;
                manager.snapshotDue();
            } catch (error) {
                console.warn("ResourceWatch: IPv4 JSON parse failed:", error);
            }
        }
    }

    property Sensors.Sensor cpuSensor: Sensors.Sensor {
        sensorId: "cpu/all/usage"; updateRateLimit: manager.updateInterval
    }
    property Sensors.Sensor cpuFreqSensor: Sensors.Sensor { sensorId: "cpu/all/averageFrequency"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor cpuTempAvg: Sensors.Sensor { sensorId: "cpu/all/averageTemperature"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor cpuTempMax: Sensors.Sensor { sensorId: "cpu/all/maximumTemperature"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor cpuNameAll: Sensors.Sensor { sensorId: "cpu/all/name"; updateRateLimit: 86400000 }
    property Sensors.Sensor cpuName0: Sensors.Sensor { sensorId: "cpu/cpu0/name"; updateRateLimit: 86400000 }
    property Sensors.Sensor uptimeSensor: Sensors.Sensor { sensorId: "os/system/uptime"; updateRateLimit: 60000 }

    property Sensors.Sensor gpuAll: Sensors.Sensor {
        sensorId: "gpu/all/usage"; updateRateLimit: manager.updateInterval
    }
    property Sensors.Sensor gpuIdx: Sensors.Sensor {
        sensorId: manager._gpuPrefix + "/usage"; updateRateLimit: manager.updateInterval
    }
    property Sensors.Sensor gpuTemp: Sensors.Sensor { sensorId: manager._gpuPrefix + "/temperature"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor gpuNameSensor: Sensors.Sensor { sensorId: manager._gpuPrefix + "/name"; updateRateLimit: 86400000 }
    property Sensors.Sensor vramUsed: Sensors.Sensor { sensorId: manager._gpuPrefix + "/usedVram"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor vramTotal: Sensors.Sensor { sensorId: manager._gpuPrefix + "/totalVram"; updateRateLimit: manager.updateInterval }

    property Sensors.Sensor memUsed: Sensors.Sensor { sensorId: "memory/physical/used"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor memTotal: Sensors.Sensor { sensorId: "memory/physical/total"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor swapUsed: Sensors.Sensor { sensorId: "memory/swap/used"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor swapTotal: Sensors.Sensor { sensorId: "memory/swap/total"; updateRateLimit: manager.updateInterval }

    property Sensors.Sensor netDownS: Sensors.Sensor { sensorId: manager._netPrefix + "/download"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor netUpS: Sensors.Sensor { sensorId: manager._netPrefix + "/upload"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor netIpS: Sensors.Sensor { sensorId: manager.netHasInterface ? manager._netPrefix + "/ipv4address" : ""; updateRateLimit: 86400000 }
}
