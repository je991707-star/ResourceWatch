import QtQuick
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import "models"

// Plasma 6 容器與唯一的 KSystemStats 感測器實例。
// 桌面/展開畫面由 HTML/CSS/JS 繪製；面板緊湊模式仍由 QML 提供開啟入口。
PlasmoidItem {
    id: root

    // 以 HTML 視圖作為 Widget 預設顯示內容，避免獨立視窗誤選緊湊模式。
    preferredRepresentation: fullRepresentation

    width: 820
    height: 710

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
        manager: sensors
        version: Plasmoid.metaData.version || "0.0.3"
        onConfigureRequested: {
            const action = Plasmoid.internalAction("configure");
            if (action) action.trigger();
        }
    }
}
