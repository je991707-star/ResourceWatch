import QtQuick
import QtQuick.Layouts
import "fmt.js" as Fmt

Card {
    id: c
    property var snap
    property var hist
    property var cfg
    readonly property var n: Fmt.nic(snap, cfg.nic) ?? null
    title: "網路"
    extra: n ? n.name : ""
    subtitle: n ? "本機 IPv4 " + (n.ip ? n.ip + "/" + n.prefix : "--") : ""
    Text { visible: !c.n; text: "沒有網路介面"; color: c.theme.muted }
    RowLayout {
        uniformCellSizes: true
        visible: !!c.n
        Layout.fillWidth: true
        Big { theme: c.theme; value: Fmt.rate(c.n?.rxBps); caption: "下載"; dot: c.theme.blue; Layout.fillWidth: true }
        Big { theme: c.theme; value: Fmt.rate(c.n?.txBps); caption: "上傳"; dot: c.theme.magenta; Layout.fillWidth: true }
    }
    Mirror { visible: !!c.n; up: c.hist.tx ?? []; down: c.hist.rx ?? []; upColor: c.theme.magenta; downColor: c.theme.blue; Layout.fillWidth: true; Layout.preferredHeight: implicitHeight }
}
