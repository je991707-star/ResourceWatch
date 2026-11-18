import QtQuick
import QtQuick.Layouts

// Label with a colored dot on the left and its value on the right.
RowLayout {
    id: l
    required property Theme theme
    property string label
    property string value
    property color dot: "transparent"
    Layout.fillWidth: true
    spacing: 6
    Rectangle { visible: l.dot.a > 0; implicitWidth: 8; implicitHeight: 8; radius: 4; color: l.dot }
    Text { text: l.label; color: l.theme.muted; font.pixelSize: 12; Layout.fillWidth: true; elide: Text.ElideRight }
    Text { text: l.value; color: l.theme.text; font.pixelSize: 12 }
}
