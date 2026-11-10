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
    property int updateInterval: 2000
    property int historyPoints: 30
    property int gpuIndex: 0
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

    // v0.0.2 版本的硬碟設定
    // property string disk1Id: ""          // "" = disk/all 彙總
    // property string disk1Label: ""
    // property string disk2Id: ""          // "" = 隱藏第二顆
    // property string disk2Label: ""
    property string netInterface: "all"  // "all" = network/all 彙總

    property bool graceElapsed: false
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
    // readonly property string _disk1Prefix: "disk/" + (disk1Id.length > 0 ? disk1Id : "all")
    // readonly property string _disk2Prefix: "disk/" + disk2Id
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

    // v0.0.2 版本的硬碟設定
    // readonly property var disk1Read: _rateMetric("disk1Read", "Disk 1 read", disk1ReadS)
    // readonly property var disk1Write: _rateMetric("disk1Write", "Disk 1 write", disk1WriteS)
    // readonly property var disk2Read: _rateMetric("disk2Read", "Disk 2 read", disk2ReadS)
    // readonly property var disk2Write: _rateMetric("disk2Write", "Disk 2 write", disk2WriteS)

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
    readonly property string netIp: _firstString([netIpS.value])

    // 歷史：每 updateInterval 取一點，上限 historyPoints（有界環形緩衝）
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
        const valid = FormatUtils.isValidNumber(value) && value >= 0 && value <= 100;
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

    function _checkRange(key, v) {
        if (FormatUtils.isValidNumber(v) && (v < 0 || v > 100) && !_warned[key]) {
            _warned[key] = true;
            console.warn("ResourceWatch: sensor '" + key + "' reported out-of-range percent:", v);
        }
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

    property Timer _grace: Timer {
        interval: 5000; running: true; repeat: false
        onTriggered: manager.graceElapsed = true
    }

    property Timer _sampler: Timer {
        interval: manager.updateInterval; running: true; repeat: true
        onTriggered: manager._sample()
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

    // Short-lived Python/NVML queries, never a resident daemon.
    property Timer _gpuProbeTimer: Timer {
        interval: Math.max(1000, manager.updateInterval)
        repeat: true
        running: true
        triggeredOnStart: true
        onTriggered: {
            if (manager._gpuProbe.connectedSources.length === 0)
                manager._gpuProbe.connectSource(manager._gpuProbeCommand);
        }
    }

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

    property Timer _hardwareProbeTimer: Timer {
        interval: 300000
        repeat: true
        running: true
        triggeredOnStart: true
        onTriggered: {
            if (manager._hardwareProbe.connectedSources.length === 0)
                manager._hardwareProbe.connectSource(manager._hardwareProbeCommand);
        }
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

                } catch (error) {
                    console.warn("ResourceWatch: invalid lsblk JSON",
                                error);
                }
            }
        }

    // 每 60 秒重新檢查裝置；首次啟動立即查詢
    property Timer _diskDiscoveryTimer: Timer {
        interval: 60000
        repeat: true
        running: true
        triggeredOnStart: true

        onTriggered: {
            if (manager._diskDiscovery.connectedSources.length === 0)
                manager._diskDiscovery.connectSource(
                    manager._lsblkCommand);
        }
    }

        // B/s 轉成可讀文字
        function _formatDiskRate(bytesPerSecond) {
            if (!FormatUtils.isValidRate(bytesPerSecond))
                return "--";

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

            for (const line of stdout.split("\n")) {
                const fields = line.trim().split(/\s+/);
                if (fields.length < 10) continue;

                const name = fields[2];
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

        // 遵循既有 updateInterval 設定
        property Timer _diskStatsTimer: Timer {
            interval: manager.updateInterval
            repeat: true
            running: true
            triggeredOnStart: true

            onTriggered: {
                if (manager._diskStats.connectedSources.length === 0)
                    manager._diskStats.connectSource(
                        manager._diskStatsCommand);
            }
        }
    property Sensors.Sensor cpuSensor: Sensors.Sensor {
        sensorId: "cpu/all/usage"; updateRateLimit: manager.updateInterval
        onValueChanged: manager._checkRange("cpu", value)
    }
    property Sensors.Sensor cpuFreqSensor: Sensors.Sensor { sensorId: "cpu/all/averageFrequency"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor cpuTempAvg: Sensors.Sensor { sensorId: "cpu/all/averageTemperature"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor cpuTempMax: Sensors.Sensor { sensorId: "cpu/all/maximumTemperature"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor cpuNameAll: Sensors.Sensor { sensorId: "cpu/all/name"; updateRateLimit: 10000 }
    property Sensors.Sensor cpuName0: Sensors.Sensor { sensorId: "cpu/cpu0/name"; updateRateLimit: 10000 }
    property Sensors.Sensor uptimeSensor: Sensors.Sensor { sensorId: "os/system/uptime"; updateRateLimit: 5000 }

    property Sensors.Sensor gpuAll: Sensors.Sensor {
        sensorId: "gpu/all/usage"; updateRateLimit: manager.updateInterval
        onValueChanged: manager._checkRange("gpu/all", value)
    }
    property Sensors.Sensor gpuIdx: Sensors.Sensor {
        sensorId: manager._gpuPrefix + "/usage"; updateRateLimit: manager.updateInterval
        onValueChanged: manager._checkRange("gpu/idx", value)
    }
    property Sensors.Sensor gpuTemp: Sensors.Sensor { sensorId: manager._gpuPrefix + "/temperature"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor gpuNameSensor: Sensors.Sensor { sensorId: manager._gpuPrefix + "/name"; updateRateLimit: 10000 }
    property Sensors.Sensor vramUsed: Sensors.Sensor { sensorId: manager._gpuPrefix + "/usedVram"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor vramTotal: Sensors.Sensor { sensorId: manager._gpuPrefix + "/totalVram"; updateRateLimit: manager.updateInterval }

    property Sensors.Sensor memUsed: Sensors.Sensor { sensorId: "memory/physical/used"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor memTotal: Sensors.Sensor { sensorId: "memory/physical/total"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor swapUsed: Sensors.Sensor { sensorId: "memory/swap/used"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor swapTotal: Sensors.Sensor { sensorId: "memory/swap/total"; updateRateLimit: manager.updateInterval }

    // v0.0.2 版本的硬碟設定
    // property Sensors.Sensor disk1ReadS: Sensors.Sensor { sensorId: manager._disk1Prefix + "/read"; updateRateLimit: manager.updateInterval }
    // property Sensors.Sensor disk1WriteS: Sensors.Sensor { sensorId: manager._disk1Prefix + "/write"; updateRateLimit: manager.updateInterval }
    // // 第二顆未設定時 sensorId 為空字串＝不訂閱
    // property Sensors.Sensor disk2ReadS: Sensors.Sensor { sensorId: manager.disk2Enabled ? manager._disk2Prefix + "/read" : ""; updateRateLimit: manager.updateInterval }
    // property Sensors.Sensor disk2WriteS: Sensors.Sensor { sensorId: manager.disk2Enabled ? manager._disk2Prefix + "/write" : ""; updateRateLimit: manager.updateInterval }

    property Sensors.Sensor netDownS: Sensors.Sensor { sensorId: manager._netPrefix + "/download"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor netUpS: Sensors.Sensor { sensorId: manager._netPrefix + "/upload"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor netIpS: Sensors.Sensor { sensorId: manager.netHasInterface ? manager._netPrefix + "/ipv4address" : ""; updateRateLimit: 10000 }
}
