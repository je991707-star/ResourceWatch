import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.private.mpris as Mpris

// Now playing (MPRIS, the same player Plasma's media controller picks), laid out like a lock-screen player:
// small album art with title / artist and the player's icon, a full-width progress bar with elapsed and
// remaining time, then centered previous / play-pause / next buttons.
// main.qml sizes it to fill the empty space left in the CPU card's column.
Rectangle {
    id: a
    required property Theme theme
    radius: 16
    color: theme.card
    // the least height that fits the layout; main.qml hides the card when the free space is smaller
    implicitHeight: col.implicitHeight + 28

    Mpris.Mpris2Model { id: mpris }
    readonly property var player: mpris.currentPlayer
    readonly property bool playing: player?.playbackStatus === Mpris.PlaybackStatus.Playing
    // microseconds; length is 0 for streams
    readonly property real length: player?.length ?? 0
    readonly property real position: Math.min(player?.position ?? 0, length > 0 ? length : Infinity)
    // pointer over the buttons: main.qml stops the desktop from taking the press as a widget move
    readonly property bool controlsHovered: controlsHover.hovered

    component Ctl: Kirigami.Icon {
        id: ic
        signal clicked()
        property real size: 20
        Layout.preferredWidth: size
        Layout.preferredHeight: size
        isMask: true
        color: a.theme.text
        opacity: !enabled ? 0.3 : ma.containsMouse ? 1 : 0.8
        MouseArea { id: ma; anchors { fill: parent; margins: -6 } hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: ic.clicked() }
    }

    // the player only reports position on seeks, so ask for it while playing
    Timer { interval: 1000; repeat: true; running: a.playing && a.visible; onTriggered: a.player.updatePosition() }

    function time(us) {
        const s = Math.floor(us / 1e6);
        return Math.floor(s / 60) + ":" + String(s % 60).padStart(2, "0");
    }

    Text { visible: !a.player; anchors.centerIn: parent; text: "沒有播放中的媒體"; color: a.theme.muted; font.pixelSize: 13 }

    ColumnLayout {
        id: col
        visible: !!a.player
        anchors { fill: parent; margins: 14 }
        spacing: 6

        RowLayout {
            Layout.fillWidth: true
            spacing: 10
            // album art; the player's icon when there is none
            Rectangle {
                Layout.preferredWidth: 38
                Layout.preferredHeight: 38
                radius: 6
                color: a.theme.field
                Kirigami.ShadowedImage {
                    id: art
                    anchors.fill: parent
                    radius: 6
                    color: "transparent" // default white would cover the placeholder
                    source: a.player?.artUrl ?? ""
                    fillMode: Image.PreserveAspectCrop
                    sourceSize: Qt.size(96, 96)
                    asynchronous: true
                }
                Kirigami.Icon {
                    anchors.centerIn: parent
                    width: 20
                    height: 20
                    visible: art.status !== Image.Ready
                    source: "media-default-album"
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 1
                Text { text: a.player?.track || a.player?.identity || ""; color: a.theme.text; font.bold: true; font.pixelSize: 13; elide: Text.ElideRight; Layout.fillWidth: true }
                Text { text: a.player?.artist ?? ""; visible: text !== ""; color: a.theme.muted; font.pixelSize: 12; elide: Text.ElideRight; Layout.fillWidth: true }
            }
            Kirigami.Icon {
                Layout.preferredWidth: 18
                Layout.preferredHeight: 18
                Layout.alignment: Qt.AlignTop
                source: a.player?.iconName || "media-default-album"
                opacity: 0.8
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.topMargin: 4
            Layout.preferredHeight: 4
            radius: 2
            color: a.theme.track
            Rectangle {
                height: parent.height
                radius: 2
                color: a.theme.text
                width: a.length > 0 ? parent.width * a.position / a.length : 0
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: -2
            Text { text: a.time(a.position); color: a.theme.muted; font.pixelSize: 10 }
            Item { Layout.fillWidth: true }
            Text { text: a.length > 0 ? "-" + a.time(a.length - a.position) : "--:--"; color: a.theme.muted; font.pixelSize: 10 }
        }

        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            spacing: 32
            HoverHandler { id: controlsHover }
            Ctl { source: "media-skip-backward"; enabled: !!a.player?.canGoPrevious; onClicked: a.player.Previous() }
            Ctl {
                size: 26
                source: a.playing ? "media-playback-pause" : "media-playback-start"
                enabled: !!(a.playing ? a.player?.canPause : a.player?.canPlay)
                onClicked: a.player.PlayPause()
            }
            Ctl { source: "media-skip-forward"; enabled: !!a.player?.canGoNext; onClicked: a.player.Next() }
        }
    }
}
