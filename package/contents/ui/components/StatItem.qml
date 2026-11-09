import QtQuick
import QtQuick.Layouts
import org.kde.plasma.components as PlasmaComponents
import org.kde.kirigami as Kirigami

// 小標籤 + 大數值。值為 "--" 時淡化顯示（資料不可用，不是 0）。
ColumnLayout {
    id: item
    property string label
    property string value: "--"
    readonly property bool unavailable: value === "--"

    spacing: 0

    PlasmaComponents.Label {
        text: item.label
        font: Kirigami.Theme.smallFont
        opacity: 0.6
    }
    PlasmaComponents.Label {
        text: item.value
        font.pointSize: Kirigami.Theme.defaultFont.pointSize * 1.3
        opacity: item.unavailable ? 0.5 : 1.0
    }
}
