import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.private.mpris as Mpris

// Now playing (MPRIS, the same player Plasma's media controller picks), laid out like the iOS lock-screen
// player: album art with title / artist and the player's icon, elapsed time / progress bar / remaining time
// on one row, then previous / play-pause / next (Font Awesome, stretched to its proportions) centered.
// main.qml sizes it to fill the empty space left in the CPU card's column.
Rectangle {
    id: a
    required property Theme theme
    radius: 16
    color: theme.card
    // the least height that fits the layout; main.qml hides the card when the free space is smaller
    implicitHeight: col.implicitHeight + col.anchors.topMargin + col.anchors.bottomMargin

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
    // glyph, font size, horizontal stretch: Font Awesome's skip arrows are nearly square (1.14:1) and its pause
    // bars thick, so they are stretched to the reference's wide arrows (1.58:1) and thin bars
    readonly property var glyphs: ({
        prev: ["", 18, 1.39], next: ["", 18, 1.39],
        play: ["", 27, 1], pause: ["", 27, 0.68]
    })

    // transport button in a 57 px slot, so the three sit centered with even spacing
    component Btn: Item {
        id: b
        required property string kind
        signal clicked()
        readonly property var g: a.glyphs[kind]
        Layout.preferredWidth: 57
        Layout.preferredHeight: 24
        opacity: enabled ? 1 : 0.3
        Text {
            id: t
            anchors.centerIn: parent
            text: b.g[0]
            font.family: fa.font.family
            font.weight: fa.font.weight
            font.pixelSize: b.g[1]
            color: a.theme.text
            // t.width, not width: inside Scale an unqualified width is the button slot's
            transform: Scale { origin.x: t.width / 2; origin.y: t.height / 2; xScale: b.g[2] }
            scale: ma.pressed ? 0.88 : 1
            Behavior on scale { NumberAnimation { duration: 80 } }
        }
        MouseArea { id: ma; anchors { fill: parent; leftMargin: 10; rightMargin: 10; topMargin: -6; bottomMargin: -6 } hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: b.clicked() }
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
        anchors { fill: parent; topMargin: 14; bottomMargin: 16; leftMargin: 14; rightMargin: 14 }
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            spacing: 10
            // album art; a placeholder icon when there is none
            Rectangle {
                Layout.preferredWidth: 46
                Layout.preferredHeight: 46
                radius: 6
                color: a.theme.field
                Kirigami.ShadowedImage {
                    id: art
                    anchors.fill: parent
                    radius: 6
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
            // title and artist at the same size, centered on the art; the artist only lighter
            ColumnLayout {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignVCenter
                spacing: 1
                Text { text: a.player?.track ?? ""; color: a.theme.text; font.weight: Font.DemiBold; font.pixelSize: 14; elide: Text.ElideRight; Layout.fillWidth: true }
                Text { text: a.player?.artist ?? ""; visible: text !== ""; color: a.theme.muted; font.pixelSize: 14; elide: Text.ElideRight; Layout.fillWidth: true }
            }
            Kirigami.Icon {
                Layout.preferredWidth: 16
                Layout.preferredHeight: 16
                Layout.alignment: Qt.AlignVCenter
                source: a.player?.iconName || "media-default-album"
                opacity: 0.6
            }
        }

        Item { Layout.fillHeight: true; Layout.minimumHeight: 10 }

        // elapsed, bar, remaining on one row
        RowLayout {
            Layout.fillWidth: true
            spacing: 9
            Text { text: a.time(a.position); color: a.theme.muted; font.pixelSize: 11; font.features: { "tnum": 1 } }
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 6
                radius: 3
                color: Qt.alpha(a.theme.text, 0.15)
                Rectangle {
                    height: parent.height
                    radius: 3
                    color: Qt.alpha(a.theme.text, 0.6)
                    width: a.length > 0 ? Math.max(height, parent.width * a.position / a.length) : 0
                }
            }
            Text { text: a.length > 0 ? "−" + a.time(a.length - a.position) : "--:--"; color: a.theme.muted; font.pixelSize: 11; font.features: { "tnum": 1 } }
        }

        Item { Layout.fillHeight: true; Layout.minimumHeight: 14 }

        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            spacing: 0
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
