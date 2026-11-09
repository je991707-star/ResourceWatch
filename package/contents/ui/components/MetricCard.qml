import QtQuick
import QtQuick.Layouts
import org.kde.plasma.components as PlasmaComponents
import org.kde.kirigami as Kirigami

// 通用監控卡：標題 + 主數值 + 可選百分比條 + 最多兩行次要資訊。
Rectangle {
    id: card

    property string title
    property string headline: ""
    property var percent: null
    property string status: "ready"     // loading | ready | unavailable
    property string row1Label
    property string row1Value
    property string row2Label
    property string row2Value

    readonly property real pad: Kirigami.Units.largeSpacing
    readonly property bool hasPercent: typeof percent === "number" && isFinite(percent)

    implicitWidth: content.implicitWidth + pad * 2
    implicitHeight: content.implicitHeight + pad * 2
    radius: Kirigami.Units.smallSpacing * 2
    color: Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.06)

    ColumnLayout {
        id: content
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: card.pad }
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillWidth: true
            PlasmaComponents.Label {
                text: card.title
                font.bold: true
                opacity: 0.8
            }
            Item { Layout.fillWidth: true }
            PlasmaComponents.Label {
                visible: card.headline.length > 0
                text: card.headline
                font.pointSize: Kirigami.Theme.defaultFont.pointSize * 1.5
                opacity: card.status === "ready" ? 1.0 : 0.5
            }
        }

        ProgressBar {
            Layout.fillWidth: true
            visible: card.headline.length > 0
            percent: card.hasPercent ? card.percent : null
        }

        PlasmaComponents.Label {
            visible: card.status !== "ready"
            text: card.status === "loading" ? i18n("Loading…") : i18n("Sensor unavailable")
            opacity: 0.6
        }

        RowLayout {
            Layout.fillWidth: true
            visible: card.row1Label.length > 0
            PlasmaComponents.Label { text: card.row1Label; opacity: 0.7 }
            Item { Layout.fillWidth: true }
            PlasmaComponents.Label { text: card.row1Value }
        }
        RowLayout {
            Layout.fillWidth: true
            visible: card.row2Label.length > 0
            PlasmaComponents.Label { text: card.row2Label; opacity: 0.7 }
            Item { Layout.fillWidth: true }
            PlasmaComponents.Label { text: card.row2Value }
        }
    }
}
