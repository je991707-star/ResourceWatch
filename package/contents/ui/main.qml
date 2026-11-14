import QtQuick
import QtQuick.Window
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import "models"

// Plasma 6 容器與唯一的 KSystemStats 感測器實例。
// 桌面/展開畫面由 HTML/CSS/JS 繪製；面板緊湊模式仍由 QML 提供開啟入口。
PlasmoidItem {
    id: root

    // 以 HTML 視圖作為 Widget 預設顯示內容，避免獨立視窗誤選緊湊模式。
    preferredRepresentation: fullRepresentation

    // Only plasmawindowed uses the application shell.  plasmashell keeps the
    // compact widget UI, so it never inherits the app window's size policy.
    readonly property bool isPlasmaWindowed: {
        const name = String(Application.name || "").toLowerCase().replace(/[\s_-]/g, "");
        const args = Application.arguments || [];
        return name.includes("plasmawindowed") ||
               (args.length > 0 && String(args[0]).toLowerCase().includes("plasmawindowed"));
    }

    readonly property real dpr: Screen.devicePixelRatio > 0 ? Screen.devicePixelRatio : 1
    readonly property real screenHeightPx: Screen.height > 0 ? Screen.height * dpr : 1440
    readonly property real widgetScale: Math.max(900, screenHeightPx) / 1440
    readonly property int widgetWidth: Math.round(650 * widgetScale / dpr)
    readonly property int widgetHeight: Math.round(741 * widgetScale / dpr)
    readonly property int appWidth: Math.min(1220, Math.max(900, Screen.width - 100))
    readonly property int appHeight: Math.min(790, Math.max(580, Screen.height - 110))
    readonly property int minAppWidth: 900
    readonly property int minAppHeight: 580

    // These values are INITIAL hints.  Do not bind maximumWidth/maximumHeight
    // or the top-level Window.width/height: that prevents normal resizing.
    width: isPlasmaWindowed ? appWidth : widgetWidth
    height: isPlasmaWindowed ? appHeight : widgetHeight
    Layout.preferredWidth: isPlasmaWindowed ? appWidth : widgetWidth
    Layout.preferredHeight: isPlasmaWindowed ? appHeight : widgetHeight
    Layout.minimumWidth: isPlasmaWindowed ? minAppWidth : 360
    Layout.minimumHeight: isPlasmaWindowed ? minAppHeight : 410
    Layout.maximumWidth: 16777215
    Layout.maximumHeight: 16777215
    Layout.fillWidth: true
    Layout.fillHeight: true

    // Restrict only plasmawindowed's independent top-level window, never the
    // plasmashell window (which contains the entire Plasma desktop).
    Binding {
        target: root.isPlasmaWindowed ? root.Window.window : null
        property: "minimumWidth"
        value: root.minAppWidth
    }
    Binding {
        target: root.isPlasmaWindowed ? root.Window.window : null
        property: "minimumHeight"
        value: root.minAppHeight
    }

    SensorManager {
        id: sensors
        updateInterval: Plasmoid.configuration.updateInterval
        ipRefreshSerial: Plasmoid.configuration.ipRefreshSerial
        diskRefreshSerial: Plasmoid.configuration.diskRefreshSerial
        historyPoints: Plasmoid.configuration.historyPoints
        gpuIndex: Plasmoid.configuration.gpuIndex
        netInterface: Plasmoid.configuration.netInterface.trim() || "all"
    }

    toolTipMainText: "Resource Watch"
    toolTipSubText: "CPU " + sensors.cpu.displayValue
                  + " · GPU " + sensors.gpu.displayValue
                  + " · RAM " + sensors.ram.displayValue

    compactRepresentation: CompactView {
        manager: sensors
        onClicked: root.expanded = !root.expanded
    }

    fullRepresentation: HtmlView {
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.preferredWidth: root.isPlasmaWindowed ? root.appWidth : root.widgetWidth
        Layout.preferredHeight: root.isPlasmaWindowed ? root.appHeight : root.widgetHeight
        Layout.minimumWidth: root.isPlasmaWindowed ? root.minAppWidth : 360
        Layout.minimumHeight: root.isPlasmaWindowed ? root.minAppHeight : 410
        Layout.maximumWidth: 16777215
        Layout.maximumHeight: 16777215
        viewMode: root.isPlasmaWindowed ? "application" : "widget"
        manager: sensors
        version: Plasmoid.metaData.version || "0.0.5"
        onConfigureRequested: {
            const action = Plasmoid.internalAction("configure");
            if (action) action.trigger();
        }
    }
}
