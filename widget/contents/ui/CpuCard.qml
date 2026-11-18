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
    Cores { values: c.snap.cpu.cores; color: c.theme.blue; Layout.fillWidth: true }
    Chart { values: c.hist.cpu ?? []; color: c.theme.blue; grid: c.theme.line; Layout.fillWidth: true; Layout.preferredHeight: implicitHeight }
}
