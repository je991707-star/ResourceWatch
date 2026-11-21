import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.plasma5support as P5Support
import org.kde.kirigami as Kirigami
import "fmt.js" as Fmt

PlasmoidItem {
    id: root

    // ~/.local/bin is not always on plasmashell's PATH
    readonly property string cmd: 'PATH="$HOME/.local/bin:$PATH" resourcewatch --widget'
    property var cfg: ({ items: ["cpu", "gpu", "net", "disk", "temp", "mem"], interval: 2, nic: null, gpu: 0, panelNic: null, panelGpu: 0, app: true, opacity: 100, panel: ["cpu", "gpu", "mem", "net", "disk"] })
    property var snap: null
    property string cpuModel: ""
    property string error: ""
    // 60 points per series, oldest first; drawn by the cards
    property var hist: ({})
    // card reordering: the pointer is over a card's content area / the key of the card being dragged
    property bool overBody: false
    property string dragKey: ""
    // config fields saved from the widget (order, media card) but not yet seen in `--widget` output
    // (a run already under way prints the old values); they override the output until it matches
    property var pending: ({})
    // card under the pointer when Plasma's context menu opened, for its "close card" entry
    property string menuKey: ""
    onContextualActionsAboutToShow: menuKey = fullRepresentationItem?.cardUnderPointer() ?? ""
    // In a panel: compact items, a click opens only that item's card (popupKey) in the popup
    readonly property bool inPanel: Plasmoid.formFactor === PlasmaCore.Types.Horizontal || Plasmoid.formFactor === PlasmaCore.Types.Vertical
    property string popupKey: ""
    // the panel and its popup cards use their own interface/GPU choice
    readonly property var nicName: (inPanel ? cfg.panelNic : cfg.nic) ?? null
    readonly property int gpuIndex: (inPanel ? cfg.panelGpu : cfg.gpu) ?? 0
    function pickGpu(s) { return s?.gpus.length ? s.gpus[Math.min(gpuIndex, s.gpus.length - 1)] : null; }
    function togglePopup(key) {
        if (expanded && popupKey === key) { expanded = false; return; }
        popupKey = key;
        expanded = true;
    }
    readonly property var titles: ({ app: "音樂播放", cpu: "處理器", mem: "記憶體", gpu: "顯示卡", net: "網路", disk: "磁碟", temp: "溫度" })

    Plasmoid.contextualActions: [
        PlasmaCore.Action {
            text: "關閉「" + (root.titles[root.menuKey] ?? "") + "」卡片"
            icon.name: "window-close"
            // keep at least one card; closed cards come back from ResourceWatch's settings page
            visible: root.menuKey === "app" || (root.menuKey !== "" && root.cfg.items.length > 1)
            onTriggered: root.menuKey === "app" ? root.setApp(false) : root.setOrder(root.cfg.items.filter(k => k !== root.menuKey))
        }
    ]

    Theme { id: colors; cardOpacity: (root.cfg.opacity ?? 100) / 100 }

    Plasmoid.backgroundHints: inPanel ? PlasmaCore.Types.DefaultBackground : PlasmaCore.Types.NoBackground
    preferredRepresentation: inPanel ? compactRepresentation : fullRepresentation

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
        if (inPanel) return; // desktop only
        const c = container();
        if (c) c.editModeCondition = Plasmoid.immutable ? 0 /* Locked */ : overBody || dragKey ? 1 /* Manual */ : 3 /* AfterPress */;
    }
    onParentChanged: allowDrag()
    onOverBodyChanged: allowDrag()
    onDragKeyChanged: allowDrag()

    function save(field, value, arg) {
        pending = Object.assign({}, pending, { [field]: value });
        cfg = Object.assign({}, cfg, { [field]: value });
        saver.connectSource('PATH="$HOME/.local/bin:$PATH" resourcewatch ' + arg);
    }
    function setOrder(items) { save("items", items, "--widget-order " + items.join(",")); }
    function setApp(on) { save("app", on, "--widget-app " + (on ? "on" : "off")); }
    P5Support.DataSource {
        id: saver
        engine: "executable"
        onNewData: (source, data) => {
            disconnectSource(source);
            if (data["exit code"] !== 0) {
                root.pending = ({});
                root.error = "無法儲存小工具設定：" + (data.stderr || data["exit code"]);
            }
        }
    }
    Connections {
        target: Plasmoid
        function onImmutableChanged() { root.allowDrag(); }
    }

    // Takes a config read from widget.json (or `--widget` output): fields the widget saved itself override it
    // until the file catches up; cfg is only replaced when something changed, so bindings do not churn.
    function applyConfig(c) {
        const left = {};
        for (const k in pending) {
            if (JSON.stringify(c[k]) !== JSON.stringify(pending[k])) {
                c[k] = pending[k];
                left[k] = pending[k];
            }
        }
        pending = left;
        if (JSON.stringify(c) !== JSON.stringify(cfg)) cfg = c;
    }
    // Commands run one-shot from Timers (connect, then disconnect on the result): a polled executable source
    // (`interval`) hands its result over only at the next poll, one whole interval late. Settings changes would
    // also wait for the next `--widget` run (interval + 1 s of sampling), so the file itself is read every 0.3 s
    // (a `cat`, about a millisecond).
    // shortcut: the raw file is used as saved; the settings page and the widget only write cleaned values, a hand edit
    // with unknown items shows until `--widget` output (cleaned) replaces it; parse with the backend if that matters.
    readonly property string catCmd: 'cat "${XDG_CONFIG_HOME:-$HOME/.config}/resourcewatch/widget.json"'
    P5Support.DataSource {
        id: exec
        engine: "executable"
        onNewData: (source, data) => {
            disconnectSource(source);
            if (source === root.catCmd) root.onConfigRead(data);
            else root.onSample(data);
        }
    }
    function run(command) { if (exec.connectedSources.indexOf(command) < 0) exec.connectSource(command); }
    Timer { interval: 300; running: true; repeat: true; onTriggered: root.run(root.catCmd) }
    Timer { interval: root.cfg.interval * 1000; running: true; repeat: true; triggeredOnStart: true; onTriggered: root.run(root.cmd) }
    function onConfigRead(data) {
        if (data["exit code"] !== 0) return; // no file yet: `--widget` output carries the defaults
        try {
            applyConfig(Object.assign({}, cfg, JSON.parse(data.stdout)));
        } catch (e) { /* half-written file: the next read gets it */ }
    }

    function record(s) {
        const h = Object.assign({}, hist); // a new object so bindings on `hist` update
        const push = (key, v) => { h[key] = (h[key] ?? []).concat([v]).slice(-60); };
        push("cpu", s.cpu.usage);
        push("mem", Fmt.ratio(s.mem.used, s.mem.total));
        const g = pickGpu(s);
        if (g) push("gpu", g.usage);
        const n = Fmt.nic(s, nicName);
        if (n) { push("rx", n.rxBps); push("tx", n.txBps); }
        push("r", Fmt.sum(s.disks, "readBps"));
        push("w", Fmt.sum(s.disks, "writeBps"));
        hist = h;
    }

    function onSample(data) {
        if (data["exit code"] !== 0) {
            error = data["exit code"] === 127 ? "找不到 resourcewatch，請先執行 install.sh" : "resourcewatch 執行失敗：" + (data.stderr || data["exit code"]);
            return;
        }
        try {
            const out = JSON.parse(data.stdout);
            applyConfig(out.config);
            snap = out.snapshot;
            cpuModel = out.cpuModel ?? "";
            record(out.snapshot);
            error = "";
        } catch (e) {
            error = "無法解析 resourcewatch 輸出";
        }
    }

    compactRepresentation: Item {
        id: compact
        readonly property bool vertical: Plasmoid.formFactor === PlasmaCore.Types.Vertical
        Layout.minimumWidth: vertical ? -1 : grid.implicitWidth
        Layout.preferredWidth: vertical ? -1 : grid.implicitWidth
        Layout.minimumHeight: vertical ? grid.implicitHeight : -1
        Layout.preferredHeight: vertical ? grid.implicitHeight : -1
        GridLayout {
            id: grid
            anchors.fill: parent
            flow: compact.vertical ? GridLayout.TopToBottom : GridLayout.LeftToRight
            rows: compact.vertical ? -1 : 1
            columns: compact.vertical ? 1 : -1
            rowSpacing: 6
            columnSpacing: 10
            Repeater {
                model: root.snap ? root.cfg.panel ?? [] : []
                PanelItem {
                    required property string modelData
                    key: modelData
                    theme: colors
                    snap: root.snap
                    hist: root.hist
                    nic: Fmt.nic(root.snap, root.nicName) ?? null
                    gpu: root.pickGpu(root.snap)
                    vertical: compact.vertical
                    Layout.fillHeight: !compact.vertical
                    Layout.fillWidth: compact.vertical
                    onClicked: root.togglePopup(modelData)
                }
            }
        }
        // before the first sample (or on an error) just the app icon; a click shows the status card
        Kirigami.Icon {
            anchors.fill: parent
            visible: !root.snap
            source: "io.github.local.resourcewatch"
            MouseArea { anchors.fill: parent; onClicked: root.togglePopup("") }
        }
    }

    // Fixed layout: two columns of 282 px cards (item i goes to column i % 2, so the default six are
    // three on the left, three on the right). Minimum = maximum = the content size, so the desktop's
    // resize handles cannot change it; the size follows the content when cards are added or removed.
    fullRepresentation: Item {
        id: full
        readonly property int cardWidth: 282
        // in a panel's popup: one column with only the clicked item's card, no media card, no reordering
        readonly property bool single: root.inPanel
        readonly property int cols: single ? 1 : 2
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
        // key of the card containing `p` (full's coordinates), "" for the gaps and the media card
        function cardAt(p) {
            for (let c = 0; c < columns.count; c++) {
                const column = columns.itemAt(c);
                for (const l of column.children.filter(ch => ch.modelData !== undefined || (ch.isApp && ch.visible))) {
                    const q = l.mapFromItem(full, p.x, p.y);
                    if (q.x >= 0 && q.y >= 0 && q.x < l.width && q.y < l.height) return l.isApp ? "app" : l.modelData;
                }
            }
            return "";
        }
        // the desktop's container takes the right press and opens the menu before the widget sees it, so the
        // card is read from the hover position when the menu is about to show (contextualActionsAboutToShow)
        HoverHandler { id: hover }
        function cardUnderPointer() { return hover.hovered ? cardAt(hover.point.position) : ""; }
        function holdStart(key, p) { if (single) return; dragFrom = p; dragDx = 0; dragDy = 0; root.dragKey = key; dropIndex = -1; }
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
                        // height of this column's cards alone (without the media card), so the media card can size
                        // itself from the difference without depending on its own height
                        readonly property real cardsHeight: {
                            let h = 0;
                            for (let i = 0; i < cardRep.count; i++) h += cardRep.itemAt(i)?.implicitHeight ?? 0;
                            return h + Math.max(0, cardRep.count - 1) * spacing;
                        }
                        // now playing at the top of the CPU card's column, filling what the other column is taller by;
                        // hidden below 60 px, compact (art and buttons) below its full layout height
                        MediaCard {
                            readonly property var other: columns.count > 1 ? columns.itemAt(1 - col) : null
                            readonly property real gap: (other?.cardsHeight ?? 0) - parent.cardsHeight - parent.spacing
                            theme: colors
                            readonly property bool isApp: true
                            onControlsHoveredChanged: root.overBody = controlsHovered
                            visible: !full.single && root.cfg.app !== false && root.cfg.items.indexOf("cpu") % full.cols === col && cardRep.count > 0 && gap >= 60
                            Layout.fillWidth: true
                            Layout.preferredHeight: gap
                        }
                        Repeater {
                            id: cardRep
                            model: !root.snap ? [] : full.single ? (root.popupKey ? [root.popupKey] : []) : root.cfg.items.filter((_, i) => i % full.cols === col)
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
    Component { id: gpu; GpuCard { theme: colors; snap: root.snap; hist: root.hist; g: root.pickGpu(root.snap) } }
    Component { id: net; NetCard { theme: colors; snap: root.snap; hist: root.hist; n: Fmt.nic(root.snap, root.nicName) ?? null } }
    Component { id: disk; DiskCard { theme: colors; snap: root.snap; hist: root.hist } }
    Component { id: temps; TempCard { theme: colors; snap: root.snap } }
}
