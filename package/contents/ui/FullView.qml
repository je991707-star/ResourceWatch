import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.components as PlasmaComponents
import org.kde.kirigami as Kirigami
import "components"
import "models/Strings.js" as Strings

// 桌面 / 彈出視窗完整版：CPU、GPU、RAM、Disk×2、Network 卡片 + 歷史小圖。
Item {
    id: view

    property var manager
    readonly property var m: manager
    readonly property int margin: Kirigami.Units.largeSpacing
    readonly property string loc: Qt.locale().name
    readonly property string version: (Plasmoid.metaData && Plasmoid.metaData.version) ? Plasmoid.metaData.version : ""

    function t(key) { return Strings.t(key, loc); }

    function openSettings() {
        const a = Plasmoid.internalAction("configure");
        if (a) a.trigger();
    }

    Layout.minimumWidth: Kirigami.Units.gridUnit * 26
    Layout.minimumHeight: Kirigami.Units.gridUnit * 20
    Layout.preferredWidth: Kirigami.Units.gridUnit * 34
    Layout.preferredHeight: grid.implicitHeight + margin * 2

    Flickable {
        id: flick
        anchors.fill: parent
        contentWidth: width
        contentHeight: grid.implicitHeight + view.margin * 2
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        GridLayout {
            id: grid
            x: view.margin
            y: view.margin
            width: flick.width - view.margin * 2
            columns: 2
            columnSpacing: Kirigami.Units.largeSpacing
            rowSpacing: Kirigami.Units.largeSpacing

            // ---- 標頭：設定按鈕 ----
            RowLayout {
                Layout.columnSpan: 2
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                PlasmaComponents.ToolButton {
                    icon.name: "configure"
                    text: view.t("settings")
                    display: PlasmaComponents.AbstractButton.IconOnly
                    onClicked: view.openSettings()
                }
            }

            // ---- CPU ----
            SectionCard {
                Layout.columnSpan: 2
                Layout.fillWidth: true
                title: "CPU"
                subtitle: view.m.cpuName
                // CPU 卡片內部使用 GridLayout 排版，兩欄顯示使用率、頻率、溫度、開機時間等資訊。
                GridLayout {
                    Layout.alignment: Qt.AlignTop
                    columns: 2
                    columnSpacing: Kirigami.Units.gridUnit * 2
                    rowSpacing: Kirigami.Units.smallSpacing
                    StatItem { label: view.t("usage"); value: view.m.cpu.displayValue }
                    StatItem { label: view.t("speed"); value: view.m.cpuFreqText }
                    StatItem { label: view.t("temp"); value: view.m.cpuTempText }
                    StatItem { Layout.columnSpan: 2; label: view.t("uptime"); value: view.m.uptimeText }
                }
                // CPU 使用率歷史小圖
                Sparkline {
                    Layout.fillWidth: true
                    Layout.minimumWidth: Kirigami.Units.gridUnit * 6
                    Layout.preferredHeight: Kirigami.Units.gridUnit * 5
                    series: [view.m.history.cpu]
                    colors: [Kirigami.Theme.highlightColor]
                    points: view.m.historyPoints
                    maxValue: 100
                    caption: view.t("cpuUsageChart")
                    maxText: "100%"
                }
            }

            // ---- GPU ----
            SectionCard {
                Layout.columnSpan: 2
                Layout.fillWidth: true
                title: "GPU" + view.m.gpuIndex
                subtitle: view.m.gpuName

                GridLayout {
                    Layout.alignment: Qt.AlignTop
                    columns: 2
                    columnSpacing: Kirigami.Units.gridUnit * 2
                    rowSpacing: Kirigami.Units.smallSpacing
                    StatItem { label: view.t("usage"); value: view.m.gpu.displayValue }
                    StatItem { label: view.t("temp"); value: view.m.gpuTempText }
                    StatItem {
                        Layout.columnSpan: 2
                        label: view.t("gpuMemory")
                        value: view.m.vram.status === "ready" ? view.m.vram.usedText + " / " + view.m.vram.totalText : "--"
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.smallSpacing
                    Sparkline {
                        Layout.fillWidth: true
                        Layout.preferredWidth: 1
                        Layout.minimumWidth: Kirigami.Units.gridUnit * 4
                        Layout.preferredHeight: Kirigami.Units.gridUnit * 5
                        series: [view.m.history.gpu]
                        colors: [Kirigami.Theme.highlightColor]
                        points: view.m.historyPoints
                        maxValue: 100
                        caption: view.t("gpuCompute")
                        maxText: "100%"
                    }
                    Sparkline {
                        Layout.fillWidth: true
                        Layout.preferredWidth: 1
                        Layout.minimumWidth: Kirigami.Units.gridUnit * 4
                        Layout.preferredHeight: Kirigami.Units.gridUnit * 5
                        series: [view.m.history.vram]
                        colors: [Kirigami.Theme.positiveTextColor]
                        points: view.m.historyPoints
                        maxValue: 100
                        caption: view.t("vram")
                        maxText: view.m.vram.totalText === "--" ? "" : view.m.vram.totalText
                    }
                }
            }

            // ---- RAM ----
            SectionCard {
                Layout.columnSpan: 2
                Layout.fillWidth: true
                title: "RAM"
                subtitle: view.m.ram.totalText === "--" ? "" : view.m.ram.totalText

                GridLayout {
                    Layout.alignment: Qt.AlignTop
                    columns: 2
                    columnSpacing: Kirigami.Units.gridUnit * 2
                    rowSpacing: Kirigami.Units.smallSpacing
                    StatItem { label: view.t("usage"); value: view.m.ram.displayValue }
                    StatItem {
                        label: view.t("inUse")
                        value: view.m.ram.status === "ready" ? view.m.ram.usedText + " / " + view.m.ram.totalText : "--"
                    }
                    StatItem {
                        Layout.columnSpan: 2
                        label: view.t("swapInUse")
                        value: view.m.swap.status === "ready" ? view.m.swap.usedText + " / " + view.m.swap.totalText : "--"
                    }
                }
                Sparkline {
                    Layout.fillWidth: true
                    Layout.minimumWidth: Kirigami.Units.gridUnit * 6
                    Layout.preferredHeight: Kirigami.Units.gridUnit * 5
                    series: [view.m.history.ram]
                    colors: [Kirigami.Theme.highlightColor]
                    points: view.m.historyPoints
                    maxValue: 100
                    caption: view.t("ramUsage")
                    maxText: "100%"
                }
            }

            // ---- 硬碟 1 ----
            SectionCard {
                Layout.columnSpan: view.m.disk2Enabled ? 1 : 2
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                title: view.t("disk") + "1"
                subtitle: view.m.disk1 ? view.m.disk1.name + " · " + view.m.disk1.sizeText + " · " + view.m.disk1.model : "--"

                StatItem { Layout.fillWidth: true; label: view.t("read"); value: view.m.disk1Read.displayValue }
                StatItem { Layout.fillWidth: true; label: view.t("write"); value: view.m.disk1Write.displayValue }
            }

            // ---- 硬碟 2（未設定 ID 時隱藏）----
            SectionCard {
                visible: view.m.disk2Enabled
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                title: view.t("disk") + "2"
                subtitle: view.m.disk2 ? view.m.disk2.name + " · " + view.m.disk2.sizeText + " · " + view.m.disk2.model : "--"

                StatItem { Layout.fillWidth: true; label: view.t("read"); value: view.m.disk2Read.displayValue }
                StatItem { Layout.fillWidth: true; label: view.t("write"); value: view.m.disk2Write.displayValue }
            }

            // ---- 網路 ----
            SectionCard {
                Layout.columnSpan: 2
                Layout.fillWidth: true
                title: view.t("network")
                subtitle: view.m.netHasInterface
                          ? "IPv4: " + (view.m.netIp.length > 0 ? view.m.netIp : "--")
                          : view.t("allInterfaces")

                ColumnLayout {
                    Layout.alignment: Qt.AlignTop
                    spacing: Kirigami.Units.smallSpacing
                    StatItem { label: view.t("upload"); value: view.m.netUp.displayValue }
                    StatItem { label: view.t("download"); value: view.m.netDown.displayValue }
                }
                Sparkline {
                    Layout.fillWidth: true
                    Layout.minimumWidth: Kirigami.Units.gridUnit * 6
                    Layout.preferredHeight: Kirigami.Units.gridUnit * 5
                    series: [view.m.history.netDown, view.m.history.netUp]
                    colors: [Kirigami.Theme.highlightColor, Kirigami.Theme.neutralTextColor]
                    points: view.m.historyPoints
                    maxValue: 0
                    caption: view.t("throughput")
                }
            }

            // ---- 頁尾 ----
            PlasmaComponents.Label {
                Layout.columnSpan: 2
                Layout.alignment: Qt.AlignRight
                text: "Resource Watch" + (view.version.length > 0 ? " v" + view.version : "")
                font: Kirigami.Theme.smallFont
                opacity: 0.5
            }
        }
    }
}
