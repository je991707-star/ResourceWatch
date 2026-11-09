import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import "components"

// 桌面 / 彈出視窗的完整版。只讀取 SensorManager 的正規化資料。
Item {
    id: view

    property var manager
    readonly property int margin: Kirigami.Units.largeSpacing

    // 兩個吞吐量值只要有一個有效，整張卡就視為 ready。
    function combine(a, b) {
        if (!a || !b) return "loading";
        return (a.status === "ready" || b.status === "ready") ? "ready" : a.status;
    }

    Layout.minimumWidth: Kirigami.Units.gridUnit * 14
    Layout.minimumHeight: grid.implicitHeight + margin * 2
    Layout.preferredWidth: Kirigami.Units.gridUnit * 22
    Layout.preferredHeight: grid.implicitHeight + margin * 2

    GridLayout {
        id: grid
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: view.margin }
        columns: view.width >= Kirigami.Units.gridUnit * 30 ? 2 : 1
        columnSpacing: Kirigami.Units.largeSpacing
        rowSpacing: Kirigami.Units.largeSpacing

        MetricCard {
            Layout.fillWidth: true; Layout.alignment: Qt.AlignTop
            title: "CPU"
            headline: view.manager ? view.manager.cpu.displayValue : "--"
            percent: view.manager ? view.manager.cpu.percent : null
            status: view.manager ? view.manager.cpu.status : "loading"
        }
        MetricCard {
            Layout.fillWidth: true; Layout.alignment: Qt.AlignTop
            title: "GPU"
            headline: view.manager ? view.manager.gpu.displayValue : "--"
            percent: view.manager ? view.manager.gpu.percent : null
            status: view.manager ? view.manager.gpu.status : "loading"
        }
        MetricCard {
            Layout.fillWidth: true; Layout.alignment: Qt.AlignTop
            title: "RAM"
            headline: view.manager ? view.manager.ram.displayValue : "--"
            percent: view.manager ? view.manager.ram.percent : null
            status: view.manager ? view.manager.ram.status : "loading"
            row1Label: i18n("Used / Total")
            row1Value: view.manager ? view.manager.ram.usedText + " / " + view.manager.ram.totalText : "--"
        }
        MetricCard {
            Layout.fillWidth: true; Layout.alignment: Qt.AlignTop
            title: "Disk"
            status: view.manager ? view.combine(view.manager.diskRead, view.manager.diskWrite) : "loading"
            row1Label: i18n("Read")
            row1Value: view.manager ? view.manager.diskRead.displayValue : "--"
            row2Label: i18n("Write")
            row2Value: view.manager ? view.manager.diskWrite.displayValue : "--"
        }
        MetricCard {
            Layout.fillWidth: true; Layout.alignment: Qt.AlignTop
            title: "Network"
            status: view.manager ? view.combine(view.manager.netDown, view.manager.netUp) : "loading"
            row1Label: i18n("Download")
            row1Value: view.manager ? view.manager.netDown.displayValue : "--"
            row2Label: i18n("Upload")
            row2Value: view.manager ? view.manager.netUp.displayValue : "--"
        }
    }
}
