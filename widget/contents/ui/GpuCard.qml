import QtQuick
import QtQuick.Layouts
import "fmt.js" as Fmt

Card {
    id: c
    property var snap
    property var hist
    property var g: null
    title: "顯示卡"
    subtitle: g ? g.name : ""
    extra: g ? Fmt.temp(g.temp) : ""
    Text { visible: !c.g; text: "沒有顯示卡"; color: c.theme.muted }
    RowLayout {
        visible: !!c.g
        Layout.fillWidth: true
        spacing: 16
        Item {
            implicitWidth: ring.implicitWidth
            implicitHeight: ring.implicitHeight
            Ring { id: ring; anchors.fill: parent; value: c.g?.usage ?? 0; color: c.theme.cyan; track: c.theme.track }
            Text { anchors.centerIn: parent; text: Fmt.pct(c.g?.usage); color: c.theme.text; font.bold: true; font.pixelSize: 15 }
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 6
            Repeater {
                model: [
                    ["顯示記憶體", Fmt.ratio(c.g?.memUsed, c.g?.memTotal), c.g?.memTotal ? Fmt.bytes(c.g.memUsed) + " / " + Fmt.bytes(c.g.memTotal) : "--"],
                    ["編碼器", c.g?.encoder, Fmt.pct(c.g?.encoder)],
                    ["解碼器", c.g?.decoder, Fmt.pct(c.g?.decoder)],
                ]
                ColumnLayout {
                    required property var modelData
                    spacing: 3
                    Layout.fillWidth: true
                    Legend { theme: c.theme; label: modelData[0]; value: modelData[2] }
                    StackBar { Layout.fillWidth: true; implicitHeight: 5; color: c.theme.track; total: 100; segments: [{ value: modelData[1], color: c.theme.cyan }] }
                }
            }
        }
    }
    Chart { visible: !!c.g; values: c.hist.gpu ?? []; color: c.theme.cyan; grid: c.theme.line; Layout.fillWidth: true; Layout.preferredHeight: implicitHeight }
}
