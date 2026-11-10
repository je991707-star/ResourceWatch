import QtQuick
import QtQuick.Layouts
import QtWebEngine
import QtWebChannel
import org.kde.kcmutils as KCM

// KDE/Plasma owns persistence, Apply and Cancel. HTML is only the editor.
// Do not write directly to Plasmoid.configuration from this page.
KCM.SimpleKCM {
    id: page

    // Names must continue to match contents/config/main.xml exactly.
    // Plasma copies saved values into these properties and commits them on Apply.
    property int cfg_updateInterval: 2000
    property int cfg_historyPoints: 30
    property int cfg_gpuIndex: 0
    property string cfg_netInterface: "all"

    function settingsSnapshot() {
        return JSON.stringify({
            updateInterval: cfg_updateInterval,
            historyPoints: cfg_historyPoints,
            gpuIndex: cfg_gpuIndex,
            netInterface: cfg_netInterface
        });
    }

    function publishSettings() {
        // WebChannel retains this property even if the page is not ready yet.
        settingsBridge.settingsJson = settingsSnapshot();
    }

    function updateSetting(key, value) {
        // Only known keys are writable. Ranges mirror config/main.xml.
        if (key === "netInterface") {
            if (typeof value !== "string") return;
            const clean = value.trim();
            if (clean.length > 64) return;
            cfg_netInterface = clean || "all";
            return;
        }
        const n = Number(value);
        if (!Number.isFinite(n) || !Number.isInteger(n)) return;
        if (key === "updateInterval" && n >= 500 && n <= 10000 && n % 500 === 0)
            cfg_updateInterval = n;
        else if (key === "historyPoints" && n >= 10 && n <= 120 && n % 10 === 0)
            cfg_historyPoints = n;
        else if (key === "gpuIndex" && n >= 0 && n <= 15)
            cfg_gpuIndex = n;
    }

    onCfg_updateIntervalChanged: publishSettings()
    onCfg_historyPointsChanged: publishSettings()
    onCfg_gpuIndexChanged: publishSettings()
    onCfg_netInterfaceChanged: publishSettings()
    Component.onCompleted: Qt.callLater(publishSettings)

    Item {
        Layout.fillWidth: true
        Layout.fillHeight: true
        implicitWidth: 600
        implicitHeight: 495

        QtObject {
            id: settingsBridge
            WebChannel.id: "resourceSettings"
            property string settingsJson: "{}"
            function setSetting(key, value) { page.updateSetting(key, value); }
            function refresh() { page.publishSettings(); }
        }

        WebChannel {
            id: settingsChannel
            registeredObjects: [settingsBridge]
        }

        WebEngineView {
            id: settingsWeb
            anchors.fill: parent
            url: Qt.resolvedUrl("../web/settings/index.html")
            webChannel: settingsChannel
            backgroundColor: "#24272c"

            onNavigationRequested: function(request) {
                if (request.url.toString().split("#")[0] !== settingsWeb.url.toString().split("#")[0])
                    request.reject();
            }
            onLoadingChanged: function(info) {
                if (info.status === WebEngineView.LoadSucceededStatus)
                    page.publishSettings();
                else if (info.status === WebEngineView.LoadFailedStatus)
                    console.error("ResourceWatch settings HTML failed:", info.errorString);
            }
        }
    }
}
