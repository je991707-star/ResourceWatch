import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import "fmt.js" as Fmt

// One item in the panel (iStat Menus style); a click opens that item's card. CPU/GPU/RAM: the label over the
// percentage and a narrow mini chart; network/disk: icon, two rate lines and mirror bars.
// In a vertical panel only the icon (or label) and the first value are shown (a mini chart does not fit the width).
Item {
    id: p
    required property string key
    required property Theme theme
    property var snap
    property var hist
    property var cfg
    property bool vertical: false
    signal clicked()

    readonly property var n: key === "net" ? Fmt.nic(snap, cfg.nic) ?? null : null
    readonly property var g: snap.gpus[0] ?? null
    readonly property string icon: ({ net: "network-wired", disk: "drive-harddisk" })[key] ?? ""
    readonly property var lines: {
        switch (key) {
        case "cpu": return ["CPU", Fmt.pct(snap.cpu.usage)];
        case "mem": return ["RAM", Fmt.pct(Fmt.ratio(snap.mem.used, snap.mem.total))];
        case "gpu": return ["GPU", Fmt.pct(g?.usage)];
        case "net": return ["↓" + Fmt.short(n?.rxBps), "↑" + Fmt.short(n?.txBps)];
        case "disk": return ["R " + Fmt.short(Fmt.sum(snap.disks, "readBps")), "W " + Fmt.short(Fmt.sum(snap.disks, "writeBps"))];
        }
        return [];
    }
    // area chart series and color, or mirror series (up, down)
    readonly property var chart: ({ cpu: ["cpu", theme.blue], mem: ["mem", theme.yellow], gpu: ["gpu", theme.cyan] })[key] ?? null
    readonly property var mirror: ({ net: ["tx", "rx", theme.magenta, theme.blue], disk: ["r", "w", theme.magenta, theme.blue] })[key] ?? null
    readonly property real h: vertical ? width : height
    readonly property real font: lines.length > 1 ? Math.max(8, h * 0.3) : Math.max(9, h * 0.42)

    implicitWidth: vertical ? 0 : row.implicitWidth
    implicitHeight: vertical ? col.implicitHeight : 0

    // widest value, so the item keeps its width as the numbers change
    TextMetrics { id: widest; font.pixelSize: p.font; text: p.chart ? "100%" : "↓9.9M" }

    RowLayout {
        id: row
        visible: !p.vertical
        anchors.verticalCenter: parent.verticalCenter
        spacing: 4
        Kirigami.Icon { visible: p.icon !== ""; source: p.icon; Layout.preferredWidth: Math.min(22, p.h * 0.6); Layout.preferredHeight: Layout.preferredWidth }
        ColumnLayout {
            spacing: 0
            Repeater {
                model: p.lines
                Text { required property string modelData; text: modelData; color: Kirigami.Theme.textColor; font.pixelSize: p.font; Layout.preferredWidth: widest.width; horizontalAlignment: p.chart ? Text.AlignHCenter : Text.AlignRight }
            }
        }
        Chart { visible: !!p.chart; values: p.chart ? p.hist[p.chart[0]] ?? [] : []; max: 100; color: p.chart?.[1] ?? "transparent"; grid: "transparent"; Layout.preferredWidth: 16; Layout.preferredHeight: p.h * 0.6 }
        Mirror { visible: !!p.mirror; up: p.mirror ? p.hist[p.mirror[0]] ?? [] : []; down: p.mirror ? p.hist[p.mirror[1]] ?? [] : []; upColor: p.mirror?.[2] ?? "transparent"; downColor: p.mirror?.[3] ?? "transparent"; Layout.preferredWidth: 32; Layout.preferredHeight: p.h * 0.6 }
    }
    ColumnLayout {
        id: col
        visible: p.vertical
        width: parent.width
        spacing: 0
        Kirigami.Icon { visible: p.icon !== ""; source: p.icon; Layout.alignment: Qt.AlignHCenter; Layout.preferredWidth: Math.min(22, p.h * 0.5); Layout.preferredHeight: Layout.preferredWidth }
        Text { visible: !!p.chart; text: p.lines[0] ?? ""; color: Kirigami.Theme.textColor; font.pixelSize: Math.max(8, p.h * 0.26); Layout.alignment: Qt.AlignHCenter }
        Text { text: p.lines[p.chart ? 1 : 0] ?? ""; color: Kirigami.Theme.textColor; font.pixelSize: Math.max(8, p.h * 0.26); Layout.alignment: Qt.AlignHCenter }
    }
    MouseArea { anchors.fill: parent; onClicked: p.clicked() }
}
