import QtQuick
import org.kde.ksysguard.sensors as Sensors
import "FormatUtils.js" as FormatUtils

// 唯一的感測器訂閱 / 正規化層。UI 只讀取下列 property，不接觸感測器 ID。
// 每個 metric 的資料契約：
//   { id, label, status: "loading"|"ready"|"unavailable",
//     rawValue: number|null, displayValue: string("--" 為不可用),
//     percent: 0..100|null }
QtObject {
    id: manager

    property int updateInterval: 2000      // ms，預設 2000
    property bool graceElapsed: false      // 啟動寬限期結束後，無資料才視為 unavailable
    property var _warned: ({})

    readonly property var cpu: _percentMetric("cpu", "CPU", cpuSensor)
    readonly property var gpu: _percentMetric("gpu", "GPU", _gpuSensor)
    readonly property var ram: _ramMetric()
    readonly property var diskRead: _rateMetric("diskRead", "Disk read", diskReadSensor)
    readonly property var diskWrite: _rateMetric("diskWrite", "Disk write", diskWriteSensor)
    readonly property var netDown: _rateMetric("netDown", "Download", netDownSensor)
    readonly property var netUp: _rateMetric("netUp", "Upload", netUpSensor)

    // GPU：優先彙總值；無值時退回 gpu0。先確認實機感測器清單（Agent.md §6.1）。
    readonly property var _gpuSensor:
        FormatUtils.isValidNumber(gpuAll.value) ? gpuAll
        : (FormatUtils.isValidNumber(gpuFirst.value) ? gpuFirst : gpuAll)

    function _status(valid) {
        return valid ? "ready" : (graceElapsed ? "unavailable" : "loading");
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
            id: id, label: label,
            status: _status(valid),
            rawValue: valid ? raw : null,
            displayValue: text,
            percent: valid ? FormatUtils.clampPercent(raw) : null
        };
    }

    function _rateMetric(id, label, s) {
        const raw = s ? s.value : undefined;
        const valid = FormatUtils.isValidRate(raw);
        return {
            id: id, label: label,
            status: _status(valid),
            rawValue: valid ? raw : null,
            displayValue: valid ? FormatUtils.displayText(s.formattedValue) : "--",
            percent: null            // 吞吐量沒有百分比
        };
    }

    function _ramMetric() {
        const used = memUsed.value;
        const total = memTotal.value;
        const p = FormatUtils.percentOf(used, total);   // 總量缺失/為 0 時為 null
        const ok = p !== null;
        return {
            id: "ram", label: "RAM",
            status: _status(ok),
            rawValue: ok ? used : null,
            displayValue: ok ? FormatUtils.formatPercent(p) : "--",
            percent: ok ? FormatUtils.clampPercent(p) : null,
            usedText: FormatUtils.isValidNumber(used) ? FormatUtils.displayText(memUsed.formattedValue) : "--",
            totalText: (FormatUtils.isValidNumber(total) && total > 0) ? FormatUtils.displayText(memTotal.formattedValue) : "--"
        };
    }

    // 來源超出 0..100 時只警告一次，避免每 2 秒洗 journal。
    function _checkRange(key, v) {
        if (FormatUtils.isValidNumber(v) && (v < 0 || v > 100) && !_warned[key]) {
            _warned[key] = true;
            console.warn("ResourceWatch: sensor '" + key + "' reported out-of-range percent:", v);
        }
    }

    property Timer _grace: Timer {
        interval: 5000
        running: true
        repeat: false
        onTriggered: manager.graceElapsed = true
    }

    property Sensors.Sensor cpuSensor: Sensors.Sensor {
        sensorId: "cpu/all/usage"
        updateRateLimit: manager.updateInterval
        onValueChanged: manager._checkRange("cpu", value)
    }
    property Sensors.Sensor gpuAll: Sensors.Sensor {
        sensorId: "gpu/all/usage"
        updateRateLimit: manager.updateInterval
        onValueChanged: manager._checkRange("gpu/all", value)
    }
    property Sensors.Sensor gpuFirst: Sensors.Sensor {
        sensorId: "gpu/gpu0/usage"
        updateRateLimit: manager.updateInterval
        onValueChanged: manager._checkRange("gpu/gpu0", value)
    }
    property Sensors.Sensor memUsed: Sensors.Sensor {
        sensorId: "memory/physical/used"
        updateRateLimit: manager.updateInterval
    }
    property Sensors.Sensor memTotal: Sensors.Sensor {
        sensorId: "memory/physical/total"
        updateRateLimit: manager.updateInterval
    }
    property Sensors.Sensor diskReadSensor: Sensors.Sensor {
        sensorId: "disk/all/read"
        updateRateLimit: manager.updateInterval
    }
    property Sensors.Sensor diskWriteSensor: Sensors.Sensor {
        sensorId: "disk/all/write"
        updateRateLimit: manager.updateInterval
    }
    property Sensors.Sensor netDownSensor: Sensors.Sensor {
        sensorId: "network/all/download"
        updateRateLimit: manager.updateInterval
    }
    property Sensors.Sensor netUpSensor: Sensors.Sensor {
        sensorId: "network/all/upload"
        updateRateLimit: manager.updateInterval
    }
}
