import QtQuick
import org.kde.kirigami as Kirigami

// Same tokens as frontend/style.css; dark or light follows the Plasma color scheme.
QtObject {
    readonly property bool dark: Kirigami.ColorUtils.brightnessForColor(Kirigami.Theme.backgroundColor) === Kirigami.ColorUtils.Dark
    readonly property color card: dark ? "#353537" : "#fbfbfc"
    readonly property color field: dark ? "#444447" : "#efeff2"
    readonly property color line: dark ? "#454548" : "#d9d9de"
    readonly property color text: dark ? "#f2f2f4" : "#1d1d1f"
    readonly property color muted: dark ? "#a4a4aa" : "#6e6e73"
    readonly property color track: dark ? "#4a4a4e" : "#dedee3"
    readonly property color red: dark ? "#ef5350" : "#e5484d"
    readonly property color yellow: dark ? "#f5b731" : "#e8a317"
    readonly property color blue: dark ? "#3d8ef8" : "#2f7cf6"
    readonly property color green: dark ? "#3ccf5d" : "#2fb350"
    readonly property color gray: dark ? "#8e8e93" : "#9a9aa0"
    readonly property color magenta: dark ? "#d65ef0" : "#c84fe0"
    readonly property color cyan: dark ? "#2fd6c8" : "#14b8b0"
}
