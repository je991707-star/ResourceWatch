import QtQuick
import org.kde.kirigami as Kirigami

// 百分比長條。percent 為 null / NaN 時只畫空軌道，不假裝 0%。
Rectangle {
    id: bar
    property var percent: null
    readonly property bool valid: typeof percent === "number" && isFinite(percent)
    readonly property real fraction: valid ? Math.min(100, Math.max(0, percent)) / 100 : 0

    implicitHeight: Kirigami.Units.smallSpacing * 2
    implicitWidth: Kirigami.Units.gridUnit * 6
    radius: height / 2
    color: Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.15)

    Rectangle {
        width: parent.width * bar.fraction
        height: parent.height
        radius: parent.radius
        visible: bar.valid
        color: bar.fraction >= 0.9 ? Kirigami.Theme.negativeTextColor
             : bar.fraction >= 0.7 ? Kirigami.Theme.neutralTextColor
             : Kirigami.Theme.highlightColor
    }
}
