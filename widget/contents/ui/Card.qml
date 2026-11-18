import QtQuick
import QtQuick.Layouts

// Dashboard card: rounded, no border, title then content.
Rectangle {
    id: root
    required property Theme theme
    property string title
    // muted text at the right end of the title row
    property string extra
    default property alias content: body.data

    radius: 16
    color: theme.card
    implicitHeight: col.implicitHeight + 32
    Layout.fillWidth: true
    Layout.preferredHeight: implicitHeight

    ColumnLayout {
        id: col
        anchors { fill: parent; topMargin: 14; bottomMargin: 18; leftMargin: 18; rightMargin: 18 }
        spacing: 10
        RowLayout {
            Layout.fillWidth: true
            Text { text: root.title; color: root.theme.text; font.bold: true; font.pixelSize: 15; Layout.fillWidth: true; elide: Text.ElideRight }
            Text { text: root.extra; color: root.theme.muted; font.pixelSize: 12; visible: root.extra !== "" }
        }
        ColumnLayout { id: body; spacing: 10; Layout.fillWidth: true }
    }
}
