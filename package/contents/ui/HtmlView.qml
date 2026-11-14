import QtQuick
import QtQuick.Layouts
import QtWebEngine
import QtWebChannel 1.0

// KDE Plasma 6 HTML 顯示層：只對頁面公開快照和「開啟設定」。
// 沒有公開命令執行、檔案存取、網路存取等特權給 HTML。
Item {
    id: view
    property var manager
    property string version: "0.0.5"
    property string viewMode: "widget"
    signal configureRequested()

    Layout.minimumWidth: 240
    Layout.minimumHeight: 220
    Layout.preferredWidth: 820
    Layout.preferredHeight: 710

    // 將 QML 感測器資料轉成純 JSON 值，避免暴露 Qt 物件給 JavaScript。
    function metric(m) {
        if (!m) return { displayValue: "--", percent: null, rawValue: null };
        return {
            displayValue: m.displayValue || "--",
            // SensorManager has already normalized both fields to a number or null.
            percent: m.percent,
            rawValue: m.rawValue,
            status: m.status || "unavailable",
            usedText: m.usedText || "--",
            totalText: m.totalText || "--"
        };
    }

    // SensorManager already keeps history arrays within historyPoints (max 120).
    // Re-slicing eight arrays on every HTML snapshot is unnecessary.
    function historyArray(values) {
        return values && values.length > 120 ? values.slice(-120) : (values || []);
    }

    function createSnapshot() {
        const m = manager;
        if (!m) return { version: version, history: {}, disks: [], diskRates: {} };
        const h = m.history;
        const allDisks = m.disks;
        const rates = {};
        const list = allDisks.map(function(d) {
            const name = String(d.name || "");
            const rate = m.diskRates && m.diskRates[name];
            rates[name] = {
                read: rate && Number.isFinite(rate.read) ? rate.read : null,
                write: rate && Number.isFinite(rate.write) ? rate.write : null
            };
            return {
                name: name,
                model: String(d.model || "--"),
                sizeText: String(d.sizeText || "--"),
                transport: String(d.transport || "")
            };
        });
        return {
            version: version,
            cpuName: m.cpuName || "--",
            cpu: metric(m.cpu), cpuFreqText: m.cpuFreqText,
            cpuTempText: m.cpuTempText, uptimeText: m.uptimeText,
            uptimeSeconds: Number.isFinite(m.uptimeSensor.value)
                ? m.uptimeSensor.value : null,
            gpuIndex: m.gpuIndex, gpuName: m.gpuName || "--",
            gpuVendor: m.gpuVendor, gpuSource: m.gpuSource,
            gpu: metric(m.gpu), gpuTempText: m.gpuTempText,
            vram: metric(m.vram),
            encoder: metric(m.encoder), decoder: metric(m.decoder),
            ram: metric(m.ram), ramSubtitle: m.ram.totalText,
            ramSpeedText: m.ramSpeedText || "--",
            swap: metric(m.swap),
            disks: list, diskRates: rates,
            netHasInterface: m.netHasInterface,
            netIpAvailable: m._hasManualNetIp && m._manualNetIp !== "--",
            netIp: m.netIp,
            netUp: metric(m.netUp), netDown: metric(m.netDown),
            history: {
                cpu: historyArray(h.cpu), gpu: historyArray(h.gpu),
                encode: historyArray(h.encode), decode: historyArray(h.decode),
                vram: historyArray(h.vram), ram: historyArray(h.ram),
                netUp: historyArray(h.netUp), netDown: historyArray(h.netDown)
            }
        };
    }

    function publishSnapshot() {
        // WebChannel 對接上後由 HTML 取得最後一次的快照，避免首次渲染競態。
        bridge.snapshotJson = JSON.stringify(createSnapshot());
        bridge.snapshotReady(bridge.snapshotJson);
    }

    QtObject {
        id: bridge
        WebChannel.id: "resourceWatch"
        property string snapshotJson: "{}"
        signal snapshotReady(string json)
        function openSettings() { view.configureRequested(); }
    }

    WebChannel {
        id: channel
        registeredObjects: [bridge]
    }

    WebEngineView {
        id: page
        anchors.fill: parent
        url: Qt.resolvedUrl("../web/index.html?mode=" + view.viewMode)
        webChannel: channel
        backgroundColor: "#24272c"

        // 防止使用者操作時離開本機 HTML 頁面。
        onNavigationRequested: function(request) {
            if (request.url.toString().split("#")[0] !== page.url.toString().split("#")[0])
                request.reject();
        }
        onLoadingChanged: function(info) {
            if (info.status === WebEngineView.LoadSucceededStatus)
                view.publishSnapshot();
            else if (info.status === WebEngineView.LoadFailedStatus)
                console.error("ResourceWatch: HTML failed to load:", info.errorString);
        }
    }

    // Full snapshots are sent only by the unified scheduler; no second Timer.
    Connections {
        target: view.manager
        function onSnapshotDue() { view.publishSnapshot(); }
    }
}
