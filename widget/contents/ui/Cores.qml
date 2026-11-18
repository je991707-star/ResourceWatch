import QtQuick
import QtQuick.Layouts

// Per-thread usage bars like the dashboard's `.cores`: 54 px tall, 3 px gaps, rounded tops, at least 3% so idle threads still show.
RowLayout {
    id: r
    property var values: []
    property color color
    // a layout overwrites its own implicitHeight, so the height is a Layout hint
    readonly property real barHeight: 54
    Layout.preferredHeight: barHeight
    spacing: 3
    Repeater {
        model: r.values
        Rectangle {
            required property var modelData
            Layout.fillWidth: true
            Layout.minimumWidth: 2
            Layout.alignment: Qt.AlignBottom
            Layout.preferredHeight: Math.max(modelData ?? 0, 3) / 100 * r.barHeight
            color: r.color
            topLeftRadius: 3
            topRightRadius: 3
        }
    }
}
