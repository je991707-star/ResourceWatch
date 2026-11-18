import QtQuick

// Stacked bar like the dashboard's `bar`: segments [{ value, color }] over `total`, clipped to a rounded track.
Canvas {
    id: c
    property var segments: []
    property real total: 1
    property color color
    implicitHeight: 8
    onSegmentsChanged: requestPaint()
    onColorChanged: requestPaint()
    onWidthChanged: requestPaint()
    onPaint: {
        const ctx = getContext("2d");
        ctx.reset();
        ctx.beginPath();
        ctx.roundedRect(0, 0, width, height, height / 2, height / 2);
        ctx.clip();
        ctx.fillStyle = color;
        ctx.fillRect(0, 0, width, height);
        let x = 0;
        for (const s of segments) {
            const w = total > 0 ? Math.max(0, s.value ?? 0) / total * width : 0;
            ctx.fillStyle = s.color;
            ctx.fillRect(x, 0, w, height);
            x += w;
        }
    }
}
