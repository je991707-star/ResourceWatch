import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.private.mpris as Mpris

// Now playing (MPRIS, the same player Plasma's media controller picks), laid out like a lock-screen player:
// album art with title / artist and the player's icon, a full-width progress bar with elapsed and remaining
// time, then centered previous / play-pause / next buttons (Font Awesome).
// main.qml sizes it to fill the empty space left in the CPU card's column.
Rectangle {
    id: a
    required property Theme theme
    radius: 16
    color: theme.card
    // the least height that fits the layout; main.qml hides the card when the free space is smaller
    implicitHeight: col.implicitHeight + 32

    Mpris.Mpris2Model { id: mpris }
    readonly property var player: mpris.currentPlayer
    // a player with no track (e.g. a KDE Connect phone that is idle) counts as nothing playing
    readonly property bool active: !!player && (player.track ?? "") !== ""
    readonly property bool playing: player?.playbackStatus === Mpris.PlaybackStatus.Playing
    // microseconds; length is 0 for streams
    readonly property real length: player?.length ?? 0
    readonly property real position: Math.min(player?.position ?? 0, length > 0 ? length : Infinity)
    // pointer over the buttons: main.qml stops the desktop from taking the press as a widget move
    readonly property bool controlsHovered: controlsHover.hovered

    // Font Awesome Free solid glyphs (contents/fonts, SIL OFL 1.1) for the transport buttons
    FontLoader { id: fa; source: Qt.resolvedUrl("../fonts/fa-solid-900.woff2") }
    readonly property var glyphs: ({ prev: "\uf04a", next: "\uf04e", play: "\uf04b", pause: "\uf04c" })

    // transport button: backward / forward / play / pause
    component Btn: Text {
        id: b
        required property string kind
        signal clicked()
        text: a.glyphs[kind]
        font.family: fa.font.family
        font.weight: fa.font.weight
        font.pixelSize: kind === "prev" || kind === "next" ? 15 : 19
        color: a.theme.text
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        // fixed box: play and pause (different glyph widths) do not shift their neighbours, and the height is
        // the glyph's, without the line spacing that would add space under the row
        Layout.preferredWidth: 24
        Layout.preferredHeight: font.pixelSize
        Layout.alignment: Qt.AlignVCenter
        opacity: enabled ? 1 : 0.3
        scale: ma.pressed ? 0.88 : 1
        Behavior on scale { NumberAnimation { duration: 80 } }
        MouseArea { id: ma; anchors { fill: parent; margins: -8 } hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: b.clicked() }
    }

    // the player only reports position on seeks, so ask for it while playing
    Timer { interval: 1000; repeat: true; running: a.playing && a.visible; onTriggered: a.player.updatePosition() }

    function time(us) {
        const s = Math.floor(us / 1e6);
        return Math.floor(s / 60) + ":" + String(s % 60).padStart(2, "0");
    }

    ColumnLayout {
        visible: !a.active
        anchors.centerIn: parent
        spacing: 8
        Kirigami.Icon { source: "media-default-album"; Layout.preferredWidth: 32; Layout.preferredHeight: 32; Layout.alignment: Qt.AlignHCenter; opacity: 0.5 }
        Text { text: "沒有播放中的媒體"; color: a.theme.muted; font.pixelSize: 13; Layout.alignment: Qt.AlignHCenter }
    }

    ColumnLayout {
        id: col
        visible: a.active
        anchors { fill: parent; margins: 16 }
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            spacing: 12
            // album art; a placeholder icon when there is none
            Rectangle {
                Layout.preferredWidth: 44
                Layout.preferredHeight: 44
                radius: 8
                color: a.theme.field
                Kirigami.ShadowedImage {
                    id: art
                    anchors.fill: parent
                    radius: 8
                    color: "transparent" // default white would cover the placeholder
                    source: a.player?.artUrl ?? ""
                    fillMode: Image.PreserveAspectCrop
                    sourceSize: Qt.size(128, 128)
                    asynchronous: true
                }
                Kirigami.Icon {
                    anchors.centerIn: parent
                    width: 22
                    height: 22
                    visible: art.status !== Image.Ready
                    source: "media-default-album"
                    opacity: 0.6
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 2
                Text { text: a.player?.track ?? ""; color: a.theme.text; font.bold: true; font.pixelSize: 14; elide: Text.ElideRight; Layout.fillWidth: true }
                Text { text: a.player?.artist ?? ""; visible: text !== ""; color: a.theme.muted; font.pixelSize: 12; elide: Text.ElideRight; Layout.fillWidth: true }
            }
            Kirigami.Icon {
                Layout.preferredWidth: 16
                Layout.preferredHeight: 16
                Layout.alignment: Qt.AlignTop
                source: a.player?.iconName || "media-default-album"
                opacity: 0.7
            }
        }

        Item { Layout.fillHeight: true; Layout.minimumHeight: 12 }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 3
            radius: 1.5
            color: Qt.alpha(a.theme.text, 0.2)
            Rectangle {
                height: parent.height
                radius: 1.5
                color: Qt.alpha(a.theme.text, 0.85)
                width: a.length > 0 ? Math.max(height, parent.width * a.position / a.length) : 0
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: 5
            Text { text: a.time(a.position); color: a.theme.muted; font.pixelSize: 11; font.features: { "tnum": 1 } }
            Item { Layout.fillWidth: true }
            Text { text: a.length > 0 ? "-" + a.time(a.length - a.position) : "--:--"; color: a.theme.muted; font.pixelSize: 11; font.features: { "tnum": 1 } }
        }

        Item { Layout.fillHeight: true; Layout.minimumHeight: 12 }

        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            spacing: 48
            HoverHandler { id: controlsHover }
            Btn { kind: "prev"; enabled: !!a.player?.canGoPrevious; onClicked: a.player.Previous() }
            Btn {
                kind: a.playing ? "pause" : "play"
                enabled: !!(a.playing ? a.player?.canPause : a.player?.canPlay)
                onClicked: a.player.PlayPause()
            }
            Btn { kind: "next"; enabled: !!a.player?.canGoNext; onClicked: a.player.Next() }
        }
    }
}
