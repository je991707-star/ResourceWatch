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
    property var cfg: ({ items: ["cpu", "gpu", "net", "disk", "temp", "mem"], interval: 2, nic: null })
    property var snap: null
    property string cpuModel: ""
    property string error: ""
    // 60 points per series, oldest first; drawn by the cards
    property var hist: ({})
    // card reordering: the pointer is over a card's content area / the key of the card being dragged
    property bool overBody: false
    property string dragKey: ""
    // order saved by a drag but not yet seen in `--widget` output (a run already under way prints the old one)
    property var pendingItems: null
    // card under the pointer at the last right press, for the "close card" entry in Plasma's context menu
    property string menuKey: ""
    readonly property var titles: ({ cpu: "處理器", mem: "記憶體", gpu: "顯示卡", net: "網路", disk: "磁碟", temp: "溫度" })

    Plasmoid.contextualActions: [
        PlasmaCore.Action {
            text: "關閉「" + (root.titles[root.menuKey] ?? "") + "」卡片"
            icon.name: "window-close"
            // keep at least one card; it comes back from ResourceWatch's settings page
            visible: root.menuKey !== "" && root.cfg.items.length > 1
            onTriggered: root.setOrder(root.cfg.items.filter(k => k !== root.menuKey))
        }
    ]

    Theme { id: colors }

    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground
    preferredRepresentation: fullRepresentation

    // Move the whole widget with a plain press on a card's title row or the gaps, while a long press on a
    // card's content reorders cards: the desktop wraps each widget in a containmentlayoutmanager
    // ItemContainer whose editModeCondition decides whether a press starts a move, and the container sees
    // the press before the card does, so the condition is switched as the pointer moves: AfterPress
    // outside card contents, Manual (no move) over them or while a card is dragged. Private API; if a
    // Plasma release renames it, `container()` finds nothing and Plasma's own press-and-hold still applies.
    function container() {
        let p = root.parent;
        while (p && p.editModeCondition === undefined) p = p.parent;
        return p;
    }
    function allowDrag() {
        const c = container();
        if (c) c.editModeCondition = Plasmoid.immutable ? 0 /* Locked */ : overBody || dragKey ? 1 /* Manual */ : 3 /* AfterPress */;
    }
    onParentChanged: allowDrag()
    onOverBodyChanged: allowDrag()
    onDragKeyChanged: allowDrag()

    function setOrder(items) {
        pendingItems = items;
        cfg = Object.assign({}, cfg, { items: items });
        saver.connectSource('PATH="$HOME/.local/bin:$PATH" resourcewatch --widget-order ' + items.join(","));
    }
    P5Support.DataSource {
        id: saver
        engine: "executable"
        onNewData: (source, data) => {
            disconnectSource(source);
            if (data["exit code"] !== 0) {
                root.pendingItems = null;
                root.error = "無法儲存排列：" + (data.stderr || data["exit code"]);
            }
        }
    }
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
                if (root.pendingItems) {
                    if (out.config.items.join() === root.pendingItems.join()) root.pendingItems = null;
                    else out.config.items = root.pendingItems;
                }
                root.cfg = out.config;
                root.snap = out.snapshot;
                root.cpuModel = out.cpuModel ?? "";
                root.record(out.snapshot);
                root.error = "";
            } catch (e) {
                root.error = "無法解析 resourcewatch 輸出";
            }
        }
    }

    // Fixed layout: two columns of 282 px cards (item i goes to column i % 2, so the default six are
    // three on the left, three on the right). Minimum = maximum = the content size, so the desktop's
    // resize handles cannot change it; the size follows the content when cards are added or removed.
    fullRepresentation: Item {
        id: full
        readonly property int cardWidth: 282
        readonly property int cols: 2
        property point dragFrom
        property real dragDx: 0
        property real dragDy: 0
        // cfg.items index of the card under the pointer while dragging, -1 when none
        property int dropIndex: -1

        // cfg.items index of the slot at `p` (full's coordinates): the card in that column whose bottom is
        // below p, else the empty slot after the column's last card (clamped to the last index).
        function slotAt(p) {
            const items = root.cfg.items;
            const col = p.x < cardWidth + cards.spacing / 2 ? 0 : 1;
            const column = columns.itemAt(col);
            if (!column) return -1;
            const keys = items.filter((_, i) => i % cols === col);
            const loaders = column.children.filter(ch => ch.modelData !== undefined);
            const top = column.mapToItem(full, 0, 0).y;
            let row = keys.length;
            for (let r = 0; r < keys.length; r++) {
                const l = loaders.find(x => x.modelData === keys[r]);
                if (l && p.y < top + l.y + l.height) { row = r; break; }
            }
            return Math.min(row * cols + col, items.length - 1);
        }
        // key of the card containing `p` (full's coordinates), "" for the gaps and the app card
        function cardAt(p) {
            for (let c = 0; c < columns.count; c++) {
                const column = columns.itemAt(c);
                for (const l of column.children.filter(ch => ch.modelData !== undefined)) {
                    const q = l.mapFromItem(full, p.x, p.y);
                    if (q.x >= 0 && q.y >= 0 && q.x < l.width && q.y < l.height) return l.modelData;
                }
            }
            return "";
        }
        // passive: Plasma still opens its context menu, which then shows the close entry for this card
        PointHandler {
            acceptedButtons: Qt.RightButton
            onActiveChanged: if (active) root.menuKey = full.cardAt(point.position)
        }
        function holdStart(key, p) { dragFrom = p; dragDx = 0; dragDy = 0; root.dragKey = key; dropIndex = -1; }
        function holdMove(p) { dragDx = p.x - dragFrom.x; dragDy = p.y - dragFrom.y; dropIndex = slotAt(p); }
        // dropping on another slot swaps the two cards; the order is saved through `--widget-order`
        function holdEnd(p) {
            const items = root.cfg.items.slice();
            const from = items.indexOf(root.dragKey);
            const to = p.x < 0 ? -1 : slotAt(p);
            root.dragKey = ""; dropIndex = -1; dragDx = 0; dragDy = 0;
            if (from < 0 || to < 0 || to === from) return;
            [items[from], items[to]] = [items[to], items[from]];
            root.setOrder(items);
        }
        Layout.minimumWidth: cards.implicitWidth
        Layout.maximumWidth: cards.implicitWidth
        Layout.preferredWidth: cards.implicitWidth
        Layout.minimumHeight: cards.implicitHeight
        Layout.maximumHeight: cards.implicitHeight
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
                spacing: 12
                Repeater {
                    id: columns
                    model: full.cols
                    ColumnLayout {
                        required property int index
                        readonly property int col: index
                        // the dragged card's column draws above the other one
                        z: root.cfg.items.indexOf(root.dragKey) % full.cols === col ? 1 : 0
                        Layout.preferredWidth: full.cardWidth
                        Layout.alignment: Qt.AlignTop
                        spacing: 12
                        // height of this column's cards alone (without the app card), so the app card can size
                        // itself from the difference without depending on its own height
                        readonly property real cardsHeight: {
                            let h = 0;
                            for (let i = 0; i < cardRep.count; i++) h += cardRep.itemAt(i)?.implicitHeight ?? 0;
                            return h + Math.max(0, cardRep.count - 1) * spacing;
                        }
                        // App icon and name above the CPU card, filling what the other column is taller by
                        AppCard {
                            readonly property var other: columns.count > 1 ? columns.itemAt(1 - col) : null
                            readonly property real gap: (other?.cardsHeight ?? 0) - parent.cardsHeight - parent.spacing
                            theme: colors
                            visible: root.cfg.items.indexOf("cpu") % full.cols === col && cardRep.count > 0 && gap >= 48
                            Layout.fillWidth: true
                            Layout.preferredHeight: gap
                        }
                        Repeater {
                            id: cardRep
                            model: root.snap ? root.cfg.items.filter((_, i) => i % full.cols === col) : []
                            Loader {
                                id: ld
                                required property string modelData
                                readonly property bool dragged: root.dragKey === modelData
                                Layout.fillWidth: true
                                z: dragged ? 1 : 0
                                scale: dragged ? 1.04 : 1
                                opacity: !dragged && full.dropIndex >= 0 && full.dropIndex === root.cfg.items.indexOf(modelData) ? 0.5 : 1
                                Behavior on scale { NumberAnimation { duration: 120 } }
                                transform: Translate { x: ld.dragged ? full.dragDx : 0; y: ld.dragged ? full.dragDy : 0 }
                                Connections {
                                    target: ld.item
                                    function onHoldStarted(p) { full.holdStart(ld.modelData, ld.item.mapToItem(full, p.x, p.y)); }
                                    function onHoldMoved(p) { full.holdMove(ld.item.mapToItem(full, p.x, p.y)); }
                                    function onHoldEnded(p) { full.holdEnd(p.x < 0 ? p : ld.item.mapToItem(full, p.x, p.y)); }
                                    function onBodyHoveredChanged() { root.overBody = ld.item.bodyHovered; }
                                }
                                sourceComponent: ({ cpu: cpu, mem: mem, gpu: gpu, net: net, disk: disk, temp: temps })[modelData] ?? null
                            }
                        }
                    }
                }
            }
        }
    }

    // Inline components so the cards bind to this file's snap/hist/cfg.
    Component { id: cpu; CpuCard { theme: colors; snap: root.snap; hist: root.hist; model: root.cpuModel } }
    Component { id: mem; MemCard { theme: colors; snap: root.snap } }
    Component { id: gpu; GpuCard { theme: colors; snap: root.snap; hist: root.hist } }
    Component { id: net; NetCard { theme: colors; snap: root.snap; hist: root.hist; cfg: root.cfg } }
    Component { id: disk; DiskCard { theme: colors; snap: root.snap; hist: root.hist } }
    Component { id: temps; TempCard { theme: colors; snap: root.snap } }
}
