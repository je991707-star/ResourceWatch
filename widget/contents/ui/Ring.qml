import QtQuick

// Usage ring like the dashboard's `ring`, round caps, value 0–100.
Canvas {
    id: c
    property real value: 0
    property color color
    property color track
    property real thickness: 8
    implicitWidth: 72
    implicitHeight: 72
    onValueChanged: requestPaint()
    onColorChanged: requestPaint()
    onPaint: {
        const ctx = getContext("2d");
        ctx.reset();
        const r = Math.min(width, height) / 2 - thickness / 2, cx = width / 2, cy = height / 2, start = -Math.PI / 2;
        ctx.lineWidth = thickness;
        ctx.lineCap = "round";
        ctx.strokeStyle = track;
        ctx.beginPath(); ctx.arc(cx, cy, r, 0, 2 * Math.PI); ctx.stroke();
        if (value > 0) {
            ctx.strokeStyle = color;
            ctx.beginPath(); ctx.arc(cx, cy, r, start, start + Math.min(value, 100) / 100 * 2 * Math.PI); ctx.stroke();
        }
    }
}
