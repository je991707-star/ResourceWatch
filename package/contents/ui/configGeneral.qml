import QtQuick
import QtQuick.Layouts
import QtWebEngine
import QtWebChannel
import org.kde.kcmutils as KCM
import org.kde.plasma.plasmoid

// KDE/Plasma owns persistence, Apply and Cancel. HTML is only the editor.
// Do not write directly to Plasmoid.configuration from this page.
KCM.SimpleKCM {
    id: page

    // Names must continue to match contents/config/main.xml exactly.
    // Plasma copies saved values into these properties and commits them on Apply.
    property int cfg_updateInterval: 500
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
        if (key === "netInterface") {
            if (typeof value !== "string") return false;
            const clean = value.trim();
            if (clean.length > 64) return false;
            cfg_netInterface = clean || "all";
            return true;
        }
        const n = Number(value);
        if (!Number.isInteger(n)) return false;
        if (key === "updateInterval") {
            if (n < 100 || n > 10000 || (n - 100) % 50 !== 0) return false;
            cfg_updateInterval = n;
            return true;
        }
        if (key === "historyPoints" && n >= 10 && n <= 120 && n % 10 === 0) {
            cfg_historyPoints = n;
            return true;
        }
        if (key === "gpuIndex" && n >= 0 && n <= 15) {
            cfg_gpuIndex = n;
            return true;
        }
        return false;
    }

    function requestRefresh(kind) {
        // Unlike editable fields, buttons are immediate commands. KDE config
        // property map is shared with the running widget, so Apply is not needed.
        if (kind === "ipv4") {
            Plasmoid.configuration.ipRefreshSerial =
                (Number(Plasmoid.configuration.ipRefreshSerial) + 1) % 2000000000;
            return true;
        }
        if (kind === "disks") {
            Plasmoid.configuration.diskRefreshSerial =
                (Number(Plasmoid.configuration.diskRefreshSerial) + 1) % 2000000000;
            return true;
        }
        return false;
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
        implicitHeight: 760

        QtObject {
            id: settingsBridge
            WebChannel.id: "resourceSettings"
            property string settingsJson: "{}"
            function setSetting(key, value) { return page.updateSetting(key, value); }
            function refresh() { page.publishSettings(); }
            function refreshDevice(kind) { return page.requestRefresh(kind); }
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
