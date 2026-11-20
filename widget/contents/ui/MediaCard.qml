import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.private.mpris as Mpris

// Now playing (MPRIS, the same player Plasma's media controller picks): album art, title, artist, progress
// bar and time. main.qml sizes it to fill the empty space left in the CPU card's column.
Rectangle {
    id: a
    required property Theme theme
    radius: 16
    color: theme.card

    Mpris.Mpris2Model { id: mpris }
    readonly property var player: mpris.currentPlayer
    readonly property bool playing: player?.playbackStatus === Mpris.PlaybackStatus.Playing
    // microseconds; length is 0 for streams
    readonly property real length: player?.length ?? 0
    readonly property real position: player?.position ?? 0

    // the player only reports position on seeks, so ask for it while playing
    Timer { interval: 1000; repeat: true; running: a.playing && a.visible; onTriggered: a.player.updatePosition() }

    function time(us) {
        const s = Math.floor(us / 1e6);
        return Math.floor(s / 60) + ":" + String(s % 60).padStart(2, "0");
    }

    Text { visible: !a.player; anchors.centerIn: parent; text: "沒有播放中的媒體"; color: a.theme.muted; font.pixelSize: 13 }

    RowLayout {
        visible: !!a.player
        anchors { fill: parent; margins: 14 }
        spacing: 12
        // album art; the player's icon when there is none
        Rectangle {
            readonly property real size: Math.min(a.height - 28, 96)
            Layout.preferredWidth: size
            Layout.preferredHeight: size
            radius: 8
            color: a.theme.field
            Kirigami.ShadowedImage {
                id: art
                anchors.fill: parent
                radius: 8
                source: a.player?.artUrl ?? ""
                fillMode: Image.PreserveAspectCrop
                sourceSize: Qt.size(192, 192)
                asynchronous: true
            }
            Kirigami.Icon {
                anchors.centerIn: parent
                width: parent.size * 0.5
                height: width
                visible: art.status !== Image.Ready
                source: a.player?.iconName || "media-default-album"
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 4
            Text { text: a.player?.track || a.player?.identity || ""; color: a.theme.text; font.bold: true; font.pixelSize: 14; elide: Text.ElideRight; Layout.fillWidth: true }
            Text { text: a.player?.artist ?? ""; visible: text !== ""; color: a.theme.muted; font.pixelSize: 12; elide: Text.ElideRight; Layout.fillWidth: true }
            Rectangle {
                Layout.fillWidth: true
                Layout.topMargin: 6
                Layout.preferredHeight: 4
                radius: 2
                color: a.theme.track
                Rectangle {
                    height: parent.height
                    radius: 2
                    color: a.theme.blue
                    width: a.length > 0 ? parent.width * Math.min(1, a.position / a.length) : 0
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Text { text: a.time(a.position); color: a.theme.muted; font.pixelSize: 11 }
                Item { Layout.fillWidth: true }
                Text { text: a.length > 0 ? a.time(a.length) : "--:--"; color: a.theme.muted; font.pixelSize: 11 }
            }
        }
    }
}
