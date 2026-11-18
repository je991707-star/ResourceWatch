import QtQuick
import QtQuick.Layouts

// Dashboard card: rounded, no border, title then content.
Rectangle {
    id: root
    required property Theme theme
    property string title
    // muted text at the right end of the title row
    property string extra
    // muted line under the title row (CPU model, local IPv4); hidden when empty
    property string subtitle
    default property alias content: body.data
    // Content area (below the title): hovering it and long-pressing it are reported so main.qml can
    // reorder cards; the title row and the gaps stay free for moving the whole widget.
    readonly property alias bodyHovered: hold.containsMouse
    signal holdStarted(point p)
    signal holdMoved(point p)
    signal holdEnded(point p)

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
        Text { text: root.subtitle; color: root.theme.muted; font.pixelSize: 12; elide: Text.ElideRight; Layout.fillWidth: true; Layout.topMargin: -6; visible: root.subtitle !== "" }
        ColumnLayout { id: body; spacing: 10; Layout.fillWidth: true }
    }

    MouseArea {
        id: hold
        property bool holding: false
        x: col.x + body.x
        y: col.y + body.y
        width: body.width
        height: body.height
        hoverEnabled: true
        pressAndHoldInterval: 400
        onPressAndHold: mouse => { holding = true; root.holdStarted(mapToItem(root, mouse.x, mouse.y)); }
        onPositionChanged: mouse => { if (holding) root.holdMoved(mapToItem(root, mouse.x, mouse.y)); }
        onReleased: mouse => { if (holding) { holding = false; root.holdEnded(mapToItem(root, mouse.x, mouse.y)); } }
        onCanceled: { if (holding) { holding = false; root.holdEnded(Qt.point(-1, -1)); } }
    }
}
