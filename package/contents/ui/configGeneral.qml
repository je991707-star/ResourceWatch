import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.kde.kcmutils as KCM
import "models/Strings.js" as Strings

KCM.SimpleKCM {
    id: page

    readonly property string loc: Qt.locale().name
    function t(key) { return Strings.t(key, loc); }

    // Plasma 會自動把 main.xml 的欄位對應到 cfg_<name>
    property alias cfg_updateInterval: intervalBox.value
    property alias cfg_historyPoints: historyBox.value
    property alias cfg_gpuIndex: gpuBox.value
    property alias cfg_disk1Id: disk1Id.text
    property alias cfg_disk1Label: disk1Label.text
    property alias cfg_disk2Id: disk2Id.text
    property alias cfg_disk2Label: disk2Label.text
    property alias cfg_netInterface: netInterface.text

    Kirigami.FormLayout {
        QQC2.SpinBox {
            id: intervalBox
            Kirigami.FormData.label: page.t("cfgInterval")
            from: 500; to: 10000; stepSize: 500
        }
        QQC2.SpinBox {
            id: historyBox
            Kirigami.FormData.label: page.t("cfgHistory")
            from: 10; to: 120; stepSize: 10
        }
        QQC2.SpinBox {
            id: gpuBox
            Kirigami.FormData.label: page.t("cfgGpu")
            from: 0; to: 15
        }

        Item { Kirigami.FormData.isSection: true }

        QQC2.TextField {
            id: disk1Id
            Kirigami.FormData.label: page.t("cfgDisk1Id")
            Layout.minimumWidth: Kirigami.Units.gridUnit * 16
        }
        QQC2.TextField {
            id: disk1Label
            Kirigami.FormData.label: page.t("cfgDisk1Label")
            Layout.minimumWidth: Kirigami.Units.gridUnit * 16
        }
        QQC2.TextField {
            id: disk2Id
            Kirigami.FormData.label: page.t("cfgDisk2Id")
            Layout.minimumWidth: Kirigami.Units.gridUnit * 16
        }
        QQC2.TextField {
            id: disk2Label
            Kirigami.FormData.label: page.t("cfgDisk2Label")
            Layout.minimumWidth: Kirigami.Units.gridUnit * 16
        }
        QQC2.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            wrapMode: Text.WordWrap
            font: Kirigami.Theme.smallFont
            opacity: 0.7
            text: page.t("cfgHintDisk")
        }

        Item { Kirigami.FormData.isSection: true }

        QQC2.TextField {
            id: netInterface
            Kirigami.FormData.label: page.t("cfgNet")
            Layout.minimumWidth: Kirigami.Units.gridUnit * 16
        }
        QQC2.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            wrapMode: Text.WordWrap
            font: Kirigami.Theme.smallFont
            opacity: 0.7
            text: page.t("cfgHintNet")
        }
    }
}
