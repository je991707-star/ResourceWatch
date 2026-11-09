import QtQuick
import QtQuick.Layouts
import org.kde.plasma.components as PlasmaComponents
import org.kde.kirigami as Kirigami

// 區塊卡：左上標題、右上副標題（裝置名稱），下方內容由子項目自行排版。
Rectangle {
    id: card

    property string title
    property string subtitle
    default property alias content: body.data

    readonly property real pad: Kirigami.Units.largeSpacing

    implicitWidth: layout.implicitWidth + pad * 2
    implicitHeight: layout.implicitHeight + pad * 2
    radius: Kirigami.Units.smallSpacing * 2
    color: Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.06)

    ColumnLayout {
        id: layout
        anchors { left: parent.left; right: parent.right; top: parent.top; margins: card.pad }
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.largeSpacing

            PlasmaComponents.Label {
                text: card.title
                font.pointSize: Kirigami.Theme.defaultFont.pointSize * 1.4
            }
            PlasmaComponents.Label {
                Layout.fillWidth: true
                text: card.subtitle
                horizontalAlignment: Text.AlignRight
                elide: Text.ElideRight
                opacity: 0.85
            }
        }

        RowLayout {
            id: body
            Layout.fillWidth: true
            spacing: Kirigami.Units.largeSpacing * 2
        }
    }
}
