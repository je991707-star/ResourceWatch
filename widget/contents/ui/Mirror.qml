import QtQuick

// Symmetric bars like the dashboard's `mirror`: `up` above the middle line, `down` below, each scaled to its own recent peak.
Canvas {
    id: c
    property var up: []
    property var down: []
    property color upColor
    property color downColor
    implicitHeight: 56
    onUpChanged: requestPaint()
    onWidthChanged: requestPaint()
    onDownChanged: requestPaint()
    onUpColorChanged: requestPaint()
    onPaint: {
        const ctx = getContext("2d");
        ctx.reset();
        const n = 60, slot = width / n, bw = Math.max(slot - 1, 1), mid = height / 2;
        const draw = (data, col, dir) => {
            const peak = Math.max(1, ...data.map(v => v ?? 0));
            ctx.fillStyle = col;
            data.forEach((v, i) => {
                const h = Math.max(((v ?? 0) / peak) * (mid - 1), 1); // 0 still shows a stub on the middle line
                const x = (n - data.length + i) * slot;
                ctx.fillRect(x, dir < 0 ? mid - h - 0.5 : mid + 0.5, bw, h);
            });
        };
        draw(up, upColor, -1);
        draw(down, downColor, 1);
    }
}
