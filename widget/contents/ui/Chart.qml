import QtQuick

// Area chart like the dashboard's `chart`: the last 60 values, newest at the right edge, null breaks the line.
Canvas {
    id: c
    property var values: []
    property real max: 100
    property color color
    property color grid
    implicitHeight: 56
    onValuesChanged: requestPaint()
    onWidthChanged: requestPaint()
    onColorChanged: requestPaint()
    onPaint: {
        const ctx = getContext("2d");
        ctx.reset();
        const n = 60, step = width / (n - 1), y = v => height - 1 - Math.min(v / max, 1) * (height - 2);
        ctx.strokeStyle = grid;
        ctx.lineWidth = 1;
        ctx.beginPath(); ctx.moveTo(0, height - 0.5); ctx.lineTo(width, height - 0.5); ctx.stroke();
        ctx.setLineDash([2, 3]);
        ctx.beginPath(); ctx.moveTo(0, 0.5); ctx.lineTo(width, 0.5); ctx.moveTo(0, height / 2); ctx.lineTo(width, height / 2); ctx.stroke();
        ctx.setLineDash([]);
        const off = n - values.length;
        // contiguous runs of non-null points
        let run = [];
        const flush = () => {
            if (run.length > 1) {
                ctx.beginPath();
                run.forEach(([x, v], i) => i ? ctx.lineTo(x, y(v)) : ctx.moveTo(x, y(v)));
                ctx.strokeStyle = color; ctx.lineWidth = 1.5; ctx.stroke();
                ctx.lineTo(run[run.length - 1][0], height); ctx.lineTo(run[0][0], height); ctx.closePath();
                ctx.globalAlpha = 0.25; ctx.fillStyle = color; ctx.fill(); ctx.globalAlpha = 1;
            }
            run = [];
        };
        values.forEach((v, i) => v == null ? flush() : run.push([(off + i) * step, v]));
        flush();
    }
}
