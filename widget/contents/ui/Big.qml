import QtQuick
import QtQuick.Layouts

// Big number with a colored dot and a muted caption under it, like the dashboard's `flow` rates.
ColumnLayout {
    id: b
    required property Theme theme
    property string value
    property string caption
    property color dot: "transparent"
    spacing: 2
    RowLayout {
        spacing: 6
        Layout.fillWidth: true // lets the column grow, so two Bigs in a row split it evenly
        Rectangle { visible: b.dot.a > 0; implicitWidth: 8; implicitHeight: 8; radius: 4; color: b.dot }
        Text { text: b.value; color: b.theme.text; font.pixelSize: 22; font.bold: true }
    }
    Text { text: b.caption; color: b.theme.muted; font.pixelSize: 12; visible: b.caption !== ""; Layout.fillWidth: true }
}
