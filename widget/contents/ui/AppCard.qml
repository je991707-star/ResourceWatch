import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// App icon and name; main.qml sizes it to fill the empty space left in the CPU card's column.
Rectangle {
    id: a
    required property Theme theme
    radius: 16
    color: theme.card
    RowLayout {
        anchors.centerIn: parent
        spacing: 12
        Kirigami.Icon {
            source: "io.github.local.resourcewatch"
            readonly property real size: Math.min(48, a.height - 24)
            Layout.preferredWidth: size
            Layout.preferredHeight: size
        }
        Text { text: "ResourceWatch"; color: a.theme.text; font.bold: true; font.pixelSize: 18 }
    }
}
