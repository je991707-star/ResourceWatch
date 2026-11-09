import QtQuick
import QtQuick.Layouts
import QtWebEngine
import QtWebChannel 1.0

// KDE Plasma 6 HTML 顯示層：只對頁面公開快照和「開啟設定」。
// 沒有公開命令執行、檔案存取、網路存取等特權給 HTML。
Item {
    id: view
    property var manager
    property string version: "0.0.3"
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
            percent: Number.isFinite(m.percent) ? m.percent : null,
            rawValue: Number.isFinite(m.rawValue) ? m.rawValue : null,
            status: m.status || "unavailable",
            usedText: m.usedText || "--",
            totalText: m.totalText || "--"
        };
    }

    function historyArray(values) {
        return Array.isArray(values) ? values.slice(0, 120) : [];
    }

    function createSnapshot() {
        const m = manager;
        if (!m) return { version: version, history: {}, disks: [], diskRates: {} };
        const h = m.history || {};
        const allDisks = Array.isArray(m.disks) ? m.disks : [];
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
            gpuIndex: m.gpuIndex, gpuName: m.gpuName || "--",
            gpu: metric(m.gpu), gpuTempText: m.gpuTempText,
            vram: metric(m.vram),
            encoder: null, decoder: null, // 尚無經證實的編/解碼感測器
            ram: metric(m.ram), ramSubtitle: m.ram.totalText,
            ramSpeedText: "--", // 現有 SensorManager 尚未提供記憶體速度
            swap: metric(m.swap),
            disks: list, diskRates: rates,
            netHasInterface: m.netHasInterface, netIp: m.netIp,
            netUp: metric(m.netUp), netDown: metric(m.netDown),
            history: {
                cpu: historyArray(h.cpu), gpu: historyArray(h.gpu),
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
        url: Qt.resolvedUrl("../web/index.html")
        webChannel: channel
        backgroundColor: "#24272c"

        // 防止使用者操作時離開本機 HTML 頁面。
        onNavigationRequested: function(request) {
            if (request.url.toString().split("#")[0] !== page.url.toString().split("#")[0])
                request.reject();
        }
        onLoadingChanged: function(info) {
            if (info.status === WebEngineView.LoadFailedStatus)
                console.error("ResourceWatch: HTML failed to load:", info.errorString);
        }
    }

    Timer {
        interval: view.manager ? Math.max(500, view.manager.updateInterval) : 2000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: view.publishSnapshot()
    }
}
