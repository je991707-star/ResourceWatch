import QtQuick
import org.kde.plasma.components as PlasmaComponents
import org.kde.kirigami as Kirigami

// 歷史小圖。只讀取傳入的陣列：number 畫線、null 斷線（不畫成 0）。
// maxValue > 0 為固定刻度（百分比用 100）；<= 0 為依資料自動縮放（速率用）。
Item {
    id: root

    property var series: []        // [[v, v, null, ...], ...]，最舊在前
    property var colors: []        // 對應每條線的顏色（請傳 Kirigami.Theme 顏色）
    property real maxValue: 100
    property int points: 30        // 圖寬代表的點數，需與歷史緩衝上限一致
    property string caption: ""
    property string maxText: ""

    implicitWidth: Kirigami.Units.gridUnit * 8
    implicitHeight: Kirigami.Units.gridUnit * 4

    readonly property color base: Kirigami.Theme.textColor

    function scaleMax() {
        if (maxValue > 0) return maxValue;
        let m = 0;
        for (let i = 0; i < series.length; i++) {
            const s = series[i] || [];
            for (let j = 0; j < s.length; j++) {
                if (typeof s[j] === "number" && isFinite(s[j]) && s[j] > m) m = s[j];
            }
        }
        return m > 0 ? m * 1.1 : 1;
    }

    Rectangle {
        anchors.fill: parent
        radius: Kirigami.Units.smallSpacing
        color: Qt.rgba(root.base.r, root.base.g, root.base.b, 0.08)
        border.width: 1
        border.color: Qt.rgba(root.base.r, root.base.g, root.base.b, 0.15)
    }

    Canvas {
        id: canvas
        anchors.fill: parent
        anchors.margins: 1

        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()

        onPaint: {
            const ctx = getContext("2d");
            const w = width;
            const h = height;
            ctx.clearRect(0, 0, w, h);
            if (w <= 0 || h <= 0) return;

            const n = Math.max(2, root.points);
            const step = w / (n - 1);
            const max = root.scaleMax();

            for (let i = 0; i < root.series.length; i++) {
                const s = root.series[i] || [];
                const c = (root.colors[i] !== undefined) ? root.colors[i] : Kirigami.Theme.highlightColor;
                const offset = n - s.length;       // 資料靠右對齊

                const runs = [];
                let cur = [];
                for (let k = 0; k < s.length; k++) {
                    const v = s[k];
                    if (typeof v === "number" && isFinite(v)) {
                        cur.push({
                            x: (offset + k) * step,
                            y: h - Math.max(0, Math.min(1, v / max)) * (h - 2) - 1
                        });
                    } else if (cur.length > 0) {
                        runs.push(cur);
                        cur = [];
                    }
                }
                if (cur.length > 0) runs.push(cur);

                for (let r = 0; r < runs.length; r++) {
                    const run = runs[r];
                    if (run.length < 2) continue;

                    ctx.beginPath();
                    ctx.moveTo(run[0].x, h);
                    for (let p = 0; p < run.length; p++) ctx.lineTo(run[p].x, run[p].y);
                    ctx.lineTo(run[run.length - 1].x, h);
                    ctx.closePath();
                    ctx.fillStyle = Qt.rgba(c.r, c.g, c.b, 0.25);
                    ctx.fill();

                    ctx.beginPath();
                    ctx.moveTo(run[0].x, run[0].y);
                    for (let q = 1; q < run.length; q++) ctx.lineTo(run[q].x, run[q].y);
                    ctx.lineWidth = 1.5;
                    ctx.strokeStyle = c;
                    ctx.stroke();
                }
            }
        }
    }

    onSeriesChanged: canvas.requestPaint()
    onColorsChanged: canvas.requestPaint()
    onMaxValueChanged: canvas.requestPaint()
    onPointsChanged: canvas.requestPaint()

    PlasmaComponents.Label {
        anchors { left: parent.left; top: parent.top; margins: Kirigami.Units.smallSpacing }
        text: root.caption
        font: Kirigami.Theme.smallFont
        opacity: 0.6
    }
    PlasmaComponents.Label {
        anchors { right: parent.right; top: parent.top; margins: Kirigami.Units.smallSpacing }
        text: root.maxText
        font: Kirigami.Theme.smallFont
        opacity: 0.6
    }
}
