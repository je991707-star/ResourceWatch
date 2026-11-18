import QtQuick
import QtQuick.Layouts
import "fmt.js" as Fmt

Card {
    id: c
    property var snap
    readonly property var list: Fmt.temps(snap).slice(0, 8)
    title: "溫度"
    Text { visible: !c.list.length; text: "沒有溫度感測器"; color: c.theme.muted }
    ColumnLayout {
        spacing: 6
        Layout.fillWidth: true
        Repeater {
            model: c.list
            RowLayout {
                required property var modelData
                readonly property bool hot: modelData.crit != null && modelData.value >= modelData.crit
                Layout.fillWidth: true
                Text { text: modelData.device; color: c.theme.muted; font.pixelSize: 13; elide: Text.ElideRight; Layout.fillWidth: true }
                Text { text: Fmt.temp(modelData.value); color: hot ? c.theme.red : c.theme.text; font.bold: hot; font.pixelSize: 13 }
            }
        }
    }
}
