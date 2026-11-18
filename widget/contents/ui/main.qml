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
    property var cfg: ({ items: ["cpu", "mem", "net"], interval: 2, nic: null })
    property var snap: null
    property string error: ""
    // 60 points per series, oldest first; drawn by the cards
    property var hist: ({})

    Theme { id: colors }

    Plasmoid.backgroundHints: PlasmaCore.Types.NoBackground
    preferredRepresentation: fullRepresentation

    function record(s) {
        const h = Object.assign({}, hist); // a new object so bindings on `hist` update
        const push = (key, v) => { h[key] = (h[key] ?? []).concat([v]).slice(-60); };
        push("cpu", s.cpu.usage);
        if (s.gpus.length) push("gpu", s.gpus[0].usage);
        const n = Fmt.nic(s, cfg.nic);
        if (n) { push("rx", n.rxBps); push("tx", n.txBps); }
        push("r", s.disks.reduce((a, d) => a + (d.readBps ?? 0), 0));
        push("w", s.disks.reduce((a, d) => a + (d.writeBps ?? 0), 0));
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

    fullRepresentation: ColumnLayout {
        Layout.preferredWidth: 300
        Layout.minimumWidth: 220
        spacing: 12

        Card {
            theme: colors
            title: "ResourceWatch"
            visible: root.error !== "" || !root.snap
            Text { text: root.error || "讀取中…"; color: root.error ? colors.red : colors.muted; wrapMode: Text.Wrap; Layout.fillWidth: true }
        }

        Repeater {
            model: root.snap ? root.cfg.items : []
            Card {
                required property string modelData
                theme: colors
                title: Fmt.TITLES[modelData]
                Repeater {
                    model: Fmt.lines(modelData, root.snap, root.cfg)
                    Text { required property string modelData; text: modelData; color: colors.text; elide: Text.ElideRight; Layout.fillWidth: true }
                }
            }
        }
    }
}
