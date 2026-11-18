import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.plasma5support as P5Support
import "fmt.js" as Fmt

PlasmoidItem {
    id: root

    // ~/.local/bin is not always on plasmashell's PATH
    readonly property string cmd: 'PATH="$HOME/.local/bin:$PATH" resourcewatch --widget'
    property var cfg: ({ items: ["cpu", "mem", "gpu", "net", "disk", "temp"], interval: 2, nic: null })
    property var snap: null
    property string error: ""
    // 60 points per series, oldest first; drawn by the cards
    property var hist: ({})

    Theme { id: colors }

    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground
    preferredRepresentation: fullRepresentation

    // Drag with a plain press instead of Plasma's press-and-hold: the desktop wraps each widget in a
    // containmentlayoutmanager ItemContainer whose editModeCondition decides that. Private API;
    // if a Plasma release renames it, `container()` finds nothing and press-and-hold still works.
    function container() {
        let p = root.parent;
        while (p && p.editModeCondition === undefined) p = p.parent;
        return p;
    }
    function allowDrag() {
        const c = container();
        if (c) c.editModeCondition = Plasmoid.immutable ? 0 /* Locked */ : 3 /* AfterPress */;
    }
    onParentChanged: allowDrag()
    Connections {
        target: Plasmoid
        function onImmutableChanged() { root.allowDrag(); }
    }

    function record(s) {
        const h = Object.assign({}, hist); // a new object so bindings on `hist` update
        const push = (key, v) => { h[key] = (h[key] ?? []).concat([v]).slice(-60); };
        push("cpu", s.cpu.usage);
        if (s.gpus.length) push("gpu", s.gpus[0].usage);
        const n = Fmt.nic(s, cfg.nic);
        if (n) { push("rx", n.rxBps); push("tx", n.txBps); }
        push("r", Fmt.sum(s.disks, "readBps"));
        push("w", Fmt.sum(s.disks, "writeBps"));
        hist = h;
    }

    P5Support.DataSource {
        engine: "executable"
        connectedSources: [root.cmd]
        interval: root.cfg.interval * 1000
        onNewData: (source, data) => {
            if (data["exit code"] !== 0) {
                root.error = data["exit code"] === 127 ? "找不到 resourcewatch，請先執行 install.sh" : "resourcewatch 執行失敗：" + (data.stderr || data["exit code"]);
                return;
            }
            try {
                const out = JSON.parse(data.stdout);
                root.cfg = out.config;
                root.snap = out.snapshot;
                root.record(out.snapshot);
                root.error = "";
            } catch (e) {
                root.error = "無法解析 resourcewatch 輸出";
            }
        }
    }

    // Cards stack from the top in as many 280 px columns as the width fits (item i goes to column i % n,
    // like the dashboard's columns); the minimum height makes the desktop grow the widget to fit them.
    fullRepresentation: Item {
        id: full
        readonly property int cols: Math.max(1, Math.floor((width + 12) / (280 + 12)))
        Layout.preferredWidth: 592 // two columns
        Layout.minimumWidth: 220
        Layout.minimumHeight: cards.implicitHeight
        Layout.preferredHeight: cards.implicitHeight

        ColumnLayout {
            id: cards
            anchors { left: parent.left; right: parent.right; top: parent.top }
            spacing: 12

            Card {
                theme: colors
                title: "ResourceWatch"
                visible: root.error !== "" || !root.snap
                Text { text: root.error || "讀取中…"; color: root.error ? colors.red : colors.muted; wrapMode: Text.Wrap; Layout.fillWidth: true }
            }

            RowLayout {
                Layout.fillWidth: true
                uniformCellSizes: true
                spacing: 12
                Repeater {
                    model: full.cols
                    ColumnLayout {
                        required property int index
                        readonly property int col: index
                        Layout.fillWidth: true
                        Layout.alignment: Qt.AlignTop
                        spacing: 12
                        Repeater {
                            model: root.snap ? root.cfg.items.filter((_, i) => i % full.cols === col) : []
                            Loader {
                                required property string modelData
                                Layout.fillWidth: true
                                sourceComponent: ({ cpu: cpu, mem: mem, gpu: gpu, net: net, disk: disk, temp: temps })[modelData] ?? null
                            }
                        }
                    }
                }
            }
        }
    }

    // Inline components so the cards bind to this file's snap/hist/cfg.
    Component { id: cpu; CpuCard { theme: colors; snap: root.snap; hist: root.hist } }
    Component { id: mem; MemCard { theme: colors; snap: root.snap } }
    Component { id: gpu; GpuCard { theme: colors; snap: root.snap; hist: root.hist } }
    Component { id: net; NetCard { theme: colors; snap: root.snap; hist: root.hist; cfg: root.cfg } }
    Component { id: disk; DiskCard { theme: colors; snap: root.snap; hist: root.hist } }
    Component { id: temps; TempCard { theme: colors; snap: root.snap } }
}
