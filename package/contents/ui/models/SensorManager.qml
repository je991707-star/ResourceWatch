import QtQuick
import org.kde.ksysguard.sensors as Sensors
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
    property string disk1Id: ""          // "" = disk/all 彙總
    property string disk1Label: ""
    property string disk2Id: ""          // "" = 隱藏第二顆
    property string disk2Label: ""
    property string netInterface: "all"  // "all" = network/all 彙總

    property bool graceElapsed: false
    property var _warned: ({})

    readonly property bool disk2Enabled: disk2Id.length > 0
    readonly property bool netHasInterface: netInterface.length > 0 && netInterface !== "all"

    readonly property string _gpuPrefix: "gpu/gpu" + gpuIndex
    readonly property string _disk1Prefix: "disk/" + (disk1Id.length > 0 ? disk1Id : "all")
    readonly property string _disk2Prefix: "disk/" + disk2Id
    readonly property string _netPrefix: "network/" + (netHasInterface ? netInterface : "all")

    // ---- 對 UI 公開的資料 ----
    readonly property var cpu: _percentMetric("cpu", "CPU", cpuSensor)
    readonly property string cpuFreqText: _text(cpuFreqSensor)
    readonly property string cpuTempText: _text(FormatUtils.isValidNumber(cpuTempAvg.value) ? cpuTempAvg : cpuTempMax)
    readonly property string uptimeText: _text(uptimeSensor)
    readonly property string cpuName: _firstString([cpuNameAll.value, cpuName0.value])

    // GPU 使用率：gpu/all 有值才優先（Agent.md §6.1），否則退回指定編號
    readonly property var _gpuUsageSensor:
        FormatUtils.isValidNumber(gpuAll.value) ? gpuAll
        : (FormatUtils.isValidNumber(gpuIdx.value) ? gpuIdx : gpuAll)
    readonly property var gpu: _percentMetric("gpu", "GPU", _gpuUsageSensor)
    readonly property string gpuTempText: _text(gpuTemp)
    readonly property string gpuName: _firstString([gpuNameSensor.value])
    readonly property var vram: _pairMetric("vram", "VRAM", vramUsed, vramTotal)

    readonly property var ram: _pairMetric("ram", "RAM", memUsed, memTotal)
    readonly property var swap: _pairMetric("swap", "Swap", swapUsed, swapTotal)

    readonly property var disk1Read: _rateMetric("disk1Read", "Disk 1 read", disk1ReadS)
    readonly property var disk1Write: _rateMetric("disk1Write", "Disk 1 write", disk1WriteS)
    readonly property var disk2Read: _rateMetric("disk2Read", "Disk 2 read", disk2ReadS)
    readonly property var disk2Write: _rateMetric("disk2Write", "Disk 2 write", disk2WriteS)

    readonly property var netDown: _rateMetric("netDown", "Download", netDownS)
    readonly property var netUp: _rateMetric("netUp", "Upload", netUpS)
    readonly property string netIp: _firstString([netIpS.value])

    // 歷史：每 updateInterval 取一點，上限 historyPoints（有界環形緩衝）
    property var history: ({ cpu: [], gpu: [], vram: [], ram: [], netDown: [], netUp: [] })

    // ---- 內部函式 ----
    function _status(valid) {
        return valid ? "ready" : (graceElapsed ? "unavailable" : "loading");
    }

    function _str(v) { return (typeof v === "string") ? v : ""; }

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

    // ---- 感測器（ID 皆為候選；實機以 kstatsviewer --list 確認）----
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

    property Sensors.Sensor disk1ReadS: Sensors.Sensor { sensorId: manager._disk1Prefix + "/read"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor disk1WriteS: Sensors.Sensor { sensorId: manager._disk1Prefix + "/write"; updateRateLimit: manager.updateInterval }
    // 第二顆未設定時 sensorId 為空字串＝不訂閱
    property Sensors.Sensor disk2ReadS: Sensors.Sensor { sensorId: manager.disk2Enabled ? manager._disk2Prefix + "/read" : ""; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor disk2WriteS: Sensors.Sensor { sensorId: manager.disk2Enabled ? manager._disk2Prefix + "/write" : ""; updateRateLimit: manager.updateInterval }

    property Sensors.Sensor netDownS: Sensors.Sensor { sensorId: manager._netPrefix + "/download"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor netUpS: Sensors.Sensor { sensorId: manager._netPrefix + "/upload"; updateRateLimit: manager.updateInterval }
    property Sensors.Sensor netIpS: Sensors.Sensor { sensorId: manager.netHasInterface ? manager._netPrefix + "/ipv4address" : ""; updateRateLimit: 10000 }
}
