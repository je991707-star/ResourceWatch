import QtQuick
import QtQuick.Layouts
import "fmt.js" as Fmt

Card {
    id: c
    property var snap
    property var hist
    title: "磁碟"
    RowLayout {
        uniformCellSizes: true
        Layout.fillWidth: true
        Big { theme: c.theme; value: Fmt.rate(Fmt.sum(c.snap.disks, "readBps")); caption: "讀取"; dot: c.theme.magenta; Layout.fillWidth: true }
        Big { theme: c.theme; value: Fmt.rate(Fmt.sum(c.snap.disks, "writeBps")); caption: "寫入"; dot: c.theme.blue; Layout.fillWidth: true }
    }
    Mirror { up: c.hist.r ?? []; down: c.hist.w ?? []; upColor: c.theme.magenta; downColor: c.theme.blue; Layout.fillWidth: true; Layout.preferredHeight: implicitHeight }
    Repeater {
        // mounted disks only; unmounted ones have no usage
        model: c.snap.disks.filter(d => d.total)
        ColumnLayout {
            required property var modelData
            spacing: 3
            Layout.fillWidth: true
            Legend { theme: c.theme; label: (modelData.model || modelData.name).replace(/_/g, " "); value: Fmt.bytes(modelData.used) + " / " + Fmt.bytes(modelData.total) }
            StackBar { Layout.fillWidth: true; implicitHeight: 5; color: c.theme.track; total: modelData.total; segments: [{ value: modelData.used, color: c.theme.yellow }] }
        }
    }
}
