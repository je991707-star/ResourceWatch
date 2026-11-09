import QtQuick
import org.kde.plasma.plasmoid
import "models"

// 組裝層：只建立「一個」SensorManager，兩種視圖共用；設定由 KConfig 綁入。
PlasmoidItem {
    id: root

    SensorManager {
        id: sensors
        updateInterval: Plasmoid.configuration.updateInterval
        historyPoints: Plasmoid.configuration.historyPoints
        gpuIndex: Plasmoid.configuration.gpuIndex
        // v0.0.2 版本的硬碟設定
        // disk1Id: Plasmoid.configuration.disk1Id.trim()
        // disk1Label: Plasmoid.configuration.disk1Label.trim()
        // disk2Id: Plasmoid.configuration.disk2Id.trim()
        // disk2Label: Plasmoid.configuration.disk2Label.trim()
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

    fullRepresentation: FullView {
        manager: sensors
    }
}
