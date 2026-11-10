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

    // 按「目前 Widget 所在螢幕」的實體垂直解析度等比設定預設大小。
    // 1440p: 650x741；1080p: 488x556；900p: 406x463。
    // 900p 為縮小下限，螢幕更高時可等比放大。
    // Wayland/HiDPI: Screen.height 是邏輯尺寸；先換算實體像素再還原。
    readonly property real screenDpr: Screen.devicePixelRatio > 0
                                      ? Screen.devicePixelRatio : 1
    readonly property real physicalScreenHeight: Screen.height > 0
        ? Screen.height * screenDpr : 1440
    readonly property real resolutionFactor: Math.max(900, physicalScreenHeight) / 1440
    readonly property int targetWidth: Math.round(650 * resolutionFactor / screenDpr)
    readonly property int targetHeight: Math.round(741 * resolutionFactor / screenDpr)

    // 桌面 Widget 的初始尺寸。Plasma 可能覆寫已儲存的既有桌面幾何設定。
    width: targetWidth
    height: targetHeight
    Layout.preferredWidth: targetWidth
    Layout.preferredHeight: targetHeight
    Layout.minimumWidth: targetWidth
    Layout.minimumHeight: targetHeight
    Layout.maximumWidth: targetWidth
    Layout.maximumHeight: targetHeight

    // plasmawindowed 是獨立 Qt 視窗；原本的 Layout 限制只約束 PlasmoidItem，
    // 不會鎖住外面的 QQuickWindow。僅在此獨立程序內對外層 Window 施加尺寸限制。
    // 不能在 plasmashell 施加這些限制，否則會影響整個 KDE 桌面視窗。
    readonly property bool isPlasmaWindowed: {
        const name = String(Application.name || "").toLowerCase().replace(/[\s_-]/g, "");
        const args = Application.arguments || [];
        const argv0 = args.length > 0 ? String(args[0] || "").toLowerCase() : "";
        return name.indexOf("plasmawindowed") !== -1
               || argv0.indexOf("plasmawindowed") !== -1;
    }

    // 先取得 Window.window；Item 附著到視窗後 Binding 會自動生效。
    // 寬高限制相等，KWin/Wayland 一般會阻止使用者拖曳調整大小。
    Component.onCompleted: {
        console.info("ResourceWatch window sizing:",
                     "app=" + Application.name,
                     "standalone=" + isPlasmaWindowed,
                     "screenHeight=" + Screen.height,
                     "dpr=" + screenDpr,
                     "target=" + targetWidth + "x" + targetHeight);
    }

    Binding {
        target: root.isPlasmaWindowed ? root.Window.window : null
        property: "minimumWidth"
        value: root.targetWidth
    }
    Binding {
        target: root.isPlasmaWindowed ? root.Window.window : null
        property: "maximumWidth"
        value: root.targetWidth
    }
    Binding {
        target: root.isPlasmaWindowed ? root.Window.window : null
        property: "minimumHeight"
        value: root.targetHeight
    }
    Binding {
        target: root.isPlasmaWindowed ? root.Window.window : null
        property: "maximumHeight"
        value: root.targetHeight
    }
    Binding {
        target: root.isPlasmaWindowed ? root.Window.window : null
        property: "width"
        value: root.targetWidth
    }
    Binding {
        target: root.isPlasmaWindowed ? root.Window.window : null
        property: "height"
        value: root.targetHeight
    }

    SensorManager {
        id: sensors
        updateInterval: Plasmoid.configuration.updateInterval
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
        // 非桌面情境（如 plasmawindowed、面板彈窗）採用相同尺寸提示。
        Layout.preferredWidth: root.targetWidth
        Layout.preferredHeight: root.targetHeight
        Layout.minimumWidth: root.targetWidth
        Layout.minimumHeight: root.targetHeight
        Layout.maximumWidth: root.targetWidth
        Layout.maximumHeight: root.targetHeight
        manager: sensors
        version: Plasmoid.metaData.version || "0.0.3"
        onConfigureRequested: {
            const action = Plasmoid.internalAction("configure");
            if (action) action.trigger();
        }
    }
}
