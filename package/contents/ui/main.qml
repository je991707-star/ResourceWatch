import QtQuick
import org.kde.plasma.plasmoid
import "models"

// 組裝層：只建立「一個」SensorManager，兩種視圖共用。
PlasmoidItem {
    id: root

    SensorManager { id: sensors }

    toolTipMainText: "Resource Watch"
    toolTipSubText: "CPU " + sensors.cpu.displayValue + " · GPU " + sensors.gpu.displayValu + " · RAM " + sensors.ram.displayValue

    compactRepresentation: CompactView {
        manager: sensors
        onClicked: root.expanded = !root.expanded
    }

    fullRepresentation: FullView {
        manager: sensors
    }
}
