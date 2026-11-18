import QtQuick
import QtQuick.Layouts
import "fmt.js" as Fmt

Card {
    id: c
    property var snap
    readonly property var m: snap.mem
    readonly property real free: Math.max(0, m.total - m.used - m.cached)
    title: "記憶體"
    extra: Fmt.bytes(m.total)
    Big { theme: c.theme; value: Fmt.pct(Fmt.ratio(c.m.used, c.m.total)); caption: Fmt.bytes(c.m.used) + " 已使用" }
    StackBar {
        Layout.fillWidth: true
        color: c.theme.track
        total: c.m.total
        segments: [{ value: c.m.used, color: c.theme.yellow }, { value: c.m.cached, color: c.theme.blue }, { value: c.free, color: c.theme.green }]
    }
    ColumnLayout {
        spacing: 4
        Layout.fillWidth: true
        Legend { theme: c.theme; dot: c.theme.yellow; label: "應用程式"; value: Fmt.bytes(c.m.used) }
        Legend { theme: c.theme; dot: c.theme.blue; label: "快取"; value: Fmt.bytes(c.m.cached) }
        Legend { theme: c.theme; dot: c.theme.green; label: "可用"; value: Fmt.bytes(c.free) }
        Legend { theme: c.theme; dot: c.theme.gray; label: "SWAP"; value: Fmt.bytes(c.m.swapUsed) + " / " + Fmt.bytes(c.m.swapTotal); visible: c.m.swapTotal > 0 }
    }
}
