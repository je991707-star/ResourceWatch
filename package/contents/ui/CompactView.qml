import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.components as PlasmaComponents
import org.kde.kirigami as Kirigami

// 面板 / 工具列用的緊湊版。點擊發出 clicked()，由 main.qml 切換展開。
Item {
    id: view

    property var manager
    signal clicked()

    readonly property bool vertical: Plasmoid.formFactor === PlasmaCore.Types.Vertical

    function val(m) { return m ? m.displayValue : "--"; }

    // 以最寬情況固定欄寬，避免數字跳動造成面板抖動。
    TextMetrics {
        id: tm
        font: Kirigami.Theme.defaultFont
        text: view.vertical ? "C 100%" : "CPU 100%"
    }

    Layout.minimumWidth: grid.implicitWidth
    Layout.minimumHeight: grid.implicitHeight
    Layout.preferredWidth: grid.implicitWidth
    Layout.preferredHeight: grid.implicitHeight

    GridLayout {
        id: grid
        anchors.centerIn: parent
        columns: view.vertical ? 1 : 3
        columnSpacing: Kirigami.Units.largeSpacing
        rowSpacing: 0

        PlasmaComponents.Label {
            Layout.preferredWidth: tm.width
            horizontalAlignment: Text.AlignHCenter
            text: (view.vertical ? "C " : "CPU ") + view.val(view.manager ? view.manager.cpu : null)
        }
        PlasmaComponents.Label {
            Layout.preferredWidth: tm.width
            horizontalAlignment: Text.AlignHCenter
            text: (view.vertical ? "G " : "GPU ") + view.val(view.manager ? view.manager.gpu : null)
        }
        PlasmaComponents.Label {
            Layout.preferredWidth: tm.width
            horizontalAlignment: Text.AlignHCenter
            text: (view.vertical ? "R " : "RAM ") + view.val(view.manager ? view.manager.ram : null)
        }
    }

    MouseArea {
        anchors.fill: parent
        onClicked: view.clicked()
    }
}
