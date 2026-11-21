// Cairo drawing for St.DrawingArea: the mini charts in the top bar and the charts in the cards.

// '#rrggbb' → [r, g, b] in 0–1
export function rgb(hex) {
    const n = parseInt(hex.slice(1), 16);
    return [(n >> 16 & 255) / 255, (n >> 8 & 255) / 255, (n & 255) / 255];
}

// Area chart of `values` (oldest first, newest at the right edge, null breaks the line) from 0 to `max`;
// `points` slots across the width so the chart fills up from the right as samples arrive.
export function area(cr, w, h, values, color, { max = 100, points = 60, line = 1.5, fill = 0.25 } = {}) {
    const [r, g, b] = rgb(color);
    const step = w / (points - 1);
    const x0 = w - (values.length - 1) * step;
    const y = (v) => h - 1 - (Math.min(v, max) / max) * (h - 2);
    let run = [];
    const flush = () => {
        if (!run.length) return;
        cr.moveTo(run[0][0], h);
        for (const [x, yy] of run) cr.lineTo(x, yy);
        cr.lineTo(run[run.length - 1][0], h);
        cr.closePath();
        cr.setSourceRGBA(r, g, b, fill);
        cr.fill();
        cr.moveTo(run[0][0], run[0][1]);
        for (const [x, yy] of run) cr.lineTo(x, yy);
        cr.setSourceRGBA(r, g, b, 1);
        cr.setLineWidth(line);
        cr.stroke();
        run = [];
    };
    values.forEach((v, i) => (v == null ? flush() : run.push([x0 + i * step, y(v)])));
    flush();
}

// Mirrored bars: `up` above the middle line, `down` below, each scaled to its own recent maximum; a zero is
// a short stub on the middle line so the baseline stays visible.
export function mirror(cr, w, h, up, down, upColor, downColor, { points = 60, gap = 1 } = {}) {
    const mid = h / 2;
    const bw = Math.max(1, w / points - gap);
    const side = (vals, color, dir) => {
        const max = Math.max(1, ...vals.filter((v) => v != null));
        const [r, g, b] = rgb(color);
        cr.setSourceRGBA(r, g, b, 1);
        const x0 = w - vals.length * (bw + gap);
        vals.forEach((v, i) => {
            if (v == null) return;
            const len = Math.max(1, (v / max) * (mid - 1));
            cr.rectangle(x0 + i * (bw + gap), dir < 0 ? mid - len : mid, bw, len);
        });
        cr.fill();
    };
    side(up, upColor, -1);
    side(down, downColor, 1);
}
