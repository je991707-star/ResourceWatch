import QtQuick
import QtQuick.Layouts
import "fmt.js" as Fmt

Card {
    id: c
    property var snap
    property var hist
    title: "處理器"
    extra: Fmt.temp(snap.cpu.temp)
    RowLayout {
        uniformCellSizes: true
        Layout.fillWidth: true
        Big { theme: c.theme; value: Fmt.pct(c.snap.cpu.usage); caption: "使用率"; dot: c.theme.blue; Layout.fillWidth: true }
        Big { theme: c.theme; value: c.snap.cpu.freqMhz == null ? "--" : (c.snap.cpu.freqMhz / 1000).toFixed(2) + " GHz"; caption: "平均頻率"; Layout.fillWidth: true }
    }
    // per-thread bars left, usage curve right, 30:70: preferred widths and stretch factors in the same ratio split the
    // width proportionally without binding to the row's own width (that re-triggers the layout)
    RowLayout {
        Layout.fillWidth: true
        spacing: 12
        Cores { values: c.snap.cpu.cores; color: c.theme.blue; Layout.fillWidth: true; Layout.preferredWidth: 3; Layout.horizontalStretchFactor: 3; Layout.alignment: Qt.AlignBottom }
        Chart { values: c.hist.cpu ?? []; color: c.theme.blue; grid: c.theme.line; Layout.fillWidth: true; Layout.preferredWidth: 7; Layout.horizontalStretchFactor: 7; Layout.preferredHeight: 54; Layout.alignment: Qt.AlignBottom }
    }
}
