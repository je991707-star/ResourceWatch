// The cards in the top-bar popups, laid out like the Plasma widget's cards (widget/contents/ui/*Card.qml):
// title row (title, muted extra), muted subtitle, then the card's body. Text uses the popup's own color, so the
// cards follow GNOME's light and dark styles; grid lines and bar tracks are that color at low alpha.
import St from 'gi://St';
import Clutter from 'gi://Clutter';
import * as Fmt from './fmt.js';
import * as Chart from './chart.js';
import { vbox, COLORS } from './theme.js';

const MUTED = 170; // muted text: the popup's text color at about 2/3 opacity

const label = (style, props = {}) => new St.Label({ style_class: style, ...props });
const muted = (style, props) => { const l = label(style, props); l.opacity = MUTED; return l; };
const hbox = (props = {}) => new St.BoxLayout({ ...props });
const dot = (color) => new St.Widget({ style_class: 'rw-dot', style: `background-color: ${color};`, y_align: Clutter.ActorAlign.CENTER });

// foreground color of an actor's style as [r, g, b] in 0–1
function fg(actor) {
    const c = actor.get_theme_node().get_foreground_color();
    return [c.red / 255, c.green / 255, c.blue / 255];
}

// St.DrawingArea that calls draw(cr, w, h, fg) on repaint
function canvas(style, draw) {
    const a = new St.DrawingArea({ style_class: style, x_expand: true });
    a.connect('repaint', () => {
        const cr = a.get_context();
        const [w, h] = a.get_surface_size();
        draw(cr, w, h, fg(a));
        cr.$dispose();
    });
    return a;
}

function roundRect(cr, x, y, w, h, r) {
    r = Math.min(r, w / 2, h / 2);
    cr.newSubPath();
    cr.arc(x + w - r, y + r, r, -Math.PI / 2, 0);
    cr.arc(x + w - r, y + h - r, r, 0, Math.PI / 2);
    cr.arc(x + r, y + h - r, r, Math.PI / 2, Math.PI);
    cr.arc(x + r, y + r, r, Math.PI, 1.5 * Math.PI);
    cr.closePath();
}

// stacked bar: segments [{ value, color }] over total, clipped to a rounded track
function stackBar(style, get) {
    return canvas(style, (cr, w, h, [r, g, b]) => {
        const { total, segments } = get();
        roundRect(cr, 0, 0, w, h, h / 2);
        cr.clip();
        cr.setSourceRGBA(r, g, b, 0.15);
        cr.paint();
        let x = 0;
        for (const s of segments) {
            const sw = total > 0 ? (Math.max(0, s.value ?? 0) / total) * w : 0;
            cr.setSourceRGBA(...Chart.rgb(s.color), 1);
            cr.rectangle(x, 0, sw, h);
            cr.fill();
            x += sw;
        }
    });
}

// area chart with the dashboard's grid: solid baseline, dashed top and middle
function chart(get, color) {
    return canvas('rw-card-chart', (cr, w, h, [r, g, b]) => {
        cr.setSourceRGBA(r, g, b, 0.15);
        cr.setLineWidth(1);
        cr.moveTo(0, h - 0.5); cr.lineTo(w, h - 0.5); cr.stroke();
        cr.setDash([2, 3], 0);
        cr.moveTo(0, 0.5); cr.lineTo(w, 0.5);
        cr.moveTo(0, Math.round(h / 2) + 0.5); cr.lineTo(w, Math.round(h / 2) + 0.5);
        cr.stroke();
        cr.setDash([], 0);
        Chart.area(cr, w, h, get(), color);
    });
}

// big number with a colored dot, muted caption under it
function big(dotColor) {
    const box = vbox({ style_class: 'rw-big', x_expand: true });
    const row = hbox({ style_class: 'rw-big-row' });
    if (dotColor) row.add_child(dot(dotColor));
    const value = label('rw-big-value');
    row.add_child(value);
    const caption = muted('rw-small');
    box.add_child(row);
    box.add_child(caption);
    return { actor: box, set(v, c) { value.text = v; caption.text = c; } };
}

// label (muted, dot optional) on the left, value on the right
function legend(dotColor) {
    const row = hbox({ style_class: 'rw-legend', x_expand: true });
    if (dotColor) row.add_child(dot(dotColor));
    const name = muted('rw-small', { x_expand: true });
    const value = label('rw-small');
    row.add_child(name);
    row.add_child(value);
    return { actor: row, set(n, v) { name.text = n; value.text = v; } };
}

// card frame: title row and subtitle, body added by the caller
function frame(title) {
    const box = vbox({ style_class: 'rw-card' });
    const head = hbox({ style_class: 'rw-card-head' });
    const t = label('rw-card-title', { text: title, x_expand: true });
    const extra = muted('rw-small', { y_align: Clutter.ActorAlign.CENTER });
    head.add_child(t);
    head.add_child(extra);
    const sub = muted('rw-card-sub');
    // title and subtitle 4 px apart (the card's 10 px spacing minus Card.qml's -6): St mis-measures negative margins
    const top = vbox({ style_class: 'rw-card-top' });
    top.add_child(head);
    top.add_child(sub);
    box.add_child(top);
    return {
        box, add: (a) => box.add_child(a),
        head(e, s) { extra.text = e ?? ''; sub.text = s ?? ''; sub.visible = !!s; },
    };
}

const pair = (a, b) => { const row = hbox({ style_class: 'rw-pair', x_expand: true }); row.add_child(a.actor); row.add_child(b.actor); return row; };

function cpuCard(src) {
    const f = frame('處理器');
    const use = big(COLORS.blue), freq = big(null);
    f.add(pair(use, freq));
    const row = hbox({ style_class: 'rw-pair' });
    const cores = canvas('rw-cores', (cr, w, h) => {
        const v = src.snap?.cpu.cores ?? [];
        if (!v.length) return;
        const gap = 3, bw = Math.max(2, (w - gap * (v.length - 1)) / v.length);
        // clip first (it consumes the path): each bar runs 3 px past the bottom so only its top corners are round
        cr.rectangle(0, 0, w, h);
        cr.clip();
        cr.setSourceRGBA(...Chart.rgb(COLORS.blue), 1);
        v.forEach((u, i) => {
            const bh = (Math.max(u ?? 0, 3) / 100) * h;
            roundRect(cr, i * (bw + gap), h - bh, bw, bh + 3, 3);
        });
        cr.fill();
    });
    const curve = chart(() => src.hist.cpu ?? [], COLORS.blue);
    curve.add_style_class_name('rw-cpu-chart');
    row.add_child(cores);
    row.add_child(curve);
    f.add(row);
    return {
        actor: f.box,
        update(s) {
            f.head(Fmt.temp(s.cpu.temp), src.cpuModel);
            use.set(Fmt.pct(s.cpu.usage), '使用率');
            freq.set(s.cpu.freqMhz == null ? '--' : `${(s.cpu.freqMhz / 1000).toFixed(2)} GHz`, '平均頻率');
            cores.queue_repaint();
            curve.queue_repaint();
        },
    };
}

function memCard(src) {
    const f = frame('記憶體');
    const b = big(null);
    f.add(b.actor);
    const bar = stackBar('rw-bar', () => {
        const m = src.snap.mem, free = Math.max(0, m.total - m.used - m.cached);
        return { total: m.total, segments: [{ value: m.used, color: COLORS.yellow }, { value: m.cached, color: COLORS.blue }, { value: free, color: COLORS.green }] };
    });
    f.add(bar);
    const rows = vbox({ style_class: 'rw-legends' });
    const apps = legend(COLORS.yellow), cache = legend(COLORS.blue), avail = legend(COLORS.green), swap = legend(COLORS.gray);
    [apps, cache, avail, swap].forEach((l) => rows.add_child(l.actor));
    f.add(rows);
    return {
        actor: f.box,
        update(s) {
            const m = s.mem, free = Math.max(0, m.total - m.used - m.cached);
            f.head(Fmt.bytes(m.total), '');
            b.set(Fmt.pct(Fmt.ratio(m.used, m.total)), `${Fmt.bytes(m.used)} 已使用`);
            apps.set('應用程式', Fmt.bytes(m.used));
            cache.set('快取', Fmt.bytes(m.cached));
            avail.set('可用', Fmt.bytes(free));
            swap.set('SWAP', `${Fmt.bytes(m.swapUsed)} / ${Fmt.bytes(m.swapTotal)}`);
            swap.actor.visible = m.swapTotal > 0;
            bar.queue_repaint();
        },
    };
}

function gpuCard(src) {
    const f = frame('顯示卡');
    const none = muted('rw-small', { text: '沒有顯示卡' });
    f.add(none);
    const row = hbox({ style_class: 'rw-gpu-row' });
    const ringBox = new St.Widget({ layout_manager: new Clutter.BinLayout(), style_class: 'rw-ring' });
    const ring = canvas('rw-ring', (cr, w, h, [r, g, b]) => {
        const t = 8, rad = Math.min(w, h) / 2 - t / 2, start = -Math.PI / 2, v = src.gpu()?.usage ?? 0;
        cr.setLineWidth(t);
        cr.setLineCap(1); // ROUND
        cr.setSourceRGBA(r, g, b, 0.15);
        cr.arc(w / 2, h / 2, rad, 0, 2 * Math.PI);
        cr.stroke();
        if (v > 0) {
            cr.setSourceRGBA(...Chart.rgb(COLORS.cyan), 1);
            cr.arc(w / 2, h / 2, rad, start, start + (Math.min(v, 100) / 100) * 2 * Math.PI);
            cr.stroke();
        }
    });
    const ringText = label('rw-ring-text', { x_align: Clutter.ActorAlign.CENTER, y_align: Clutter.ActorAlign.CENTER });
    ringBox.add_child(ring);
    ringBox.add_child(ringText);
    row.add_child(ringBox);
    const meters = vbox({ style_class: 'rw-meters', x_expand: true, y_align: Clutter.ActorAlign.CENTER });
    const meter = () => {
        const box = vbox({ style_class: 'rw-meter' });
        const l = legend(null);
        let val = null;
        const bar = stackBar('rw-bar-thin', () => ({ total: 100, segments: [{ value: val, color: COLORS.cyan }] }));
        box.add_child(l.actor);
        box.add_child(bar);
        meters.add_child(box);
        return (name, v, text) => { l.set(name, text); val = v; bar.queue_repaint(); };
    };
    const vram = meter(), enc = meter(), dec = meter();
    row.add_child(meters);
    f.add(row);
    const curve = chart(() => src.hist.gpu ?? [], COLORS.cyan);
    f.add(curve);
    return {
        actor: f.box,
        update() {
            const g = src.gpu();
            none.visible = !g;
            row.visible = curve.visible = !!g;
            f.head(g ? Fmt.temp(g.temp) : '', g?.name ?? '');
            if (!g) return;
            ringText.text = Fmt.pct(g.usage);
            vram('顯示記憶體', Fmt.ratio(g.memUsed, g.memTotal), g.memTotal ? `${Fmt.bytes(g.memUsed)} / ${Fmt.bytes(g.memTotal)}` : '--');
            enc('編碼器', g.encoder, Fmt.pct(g.encoder));
            dec('解碼器', g.decoder, Fmt.pct(g.decoder));
            ring.queue_repaint();
            curve.queue_repaint();
        },
    };
}

function mirrorChart(get) {
    return canvas('rw-card-chart', (cr, w, h) => { const [u, d] = get(); Chart.mirror(cr, w, h, u, d, COLORS.magenta, COLORS.blue); });
}

function netCard(src) {
    const f = frame('網路');
    const none = muted('rw-small', { text: '沒有網路介面' });
    f.add(none);
    const down = big(COLORS.blue), up = big(COLORS.magenta);
    const rates = pair(down, up);
    f.add(rates);
    const bars = mirrorChart(() => [src.hist.tx ?? [], src.hist.rx ?? []]);
    f.add(bars);
    return {
        actor: f.box,
        update() {
            const n = src.nic();
            none.visible = !n;
            rates.visible = bars.visible = !!n;
            f.head(n?.name ?? '', n ? `本機 IPv4 ${n.ip ? `${n.ip}/${n.prefix}` : '--'}` : '');
            if (!n) return;
            down.set(Fmt.rate(n.rxBps), '下載');
            up.set(Fmt.rate(n.txBps), '上傳');
            bars.queue_repaint();
        },
    };
}

function diskCard(src) {
    const f = frame('磁碟');
    const read = big(COLORS.magenta), write = big(COLORS.blue);
    f.add(pair(read, write));
    const bars = mirrorChart(() => [src.hist.r ?? [], src.hist.w ?? []]);
    f.add(bars);
    const list = vbox({ style_class: 'rw-legends' });
    f.add(list);
    let names = '';
    let rows = [];
    return {
        actor: f.box,
        update(s) {
            f.head('', '');
            read.set(Fmt.rate(Fmt.sum(s.disks, 'readBps')), '讀取');
            write.set(Fmt.rate(Fmt.sum(s.disks, 'writeBps')), '寫入');
            bars.queue_repaint();
            // mounted disks only; unmounted ones have no usage
            const disks = s.disks.filter((d) => d.total);
            if (disks.map((d) => d.name).join() !== names) {
                list.destroy_all_children();
                rows = disks.map(() => {
                    const box = vbox({ style_class: 'rw-meter' });
                    const l = legend(null);
                    const state = { used: 0, total: 1 };
                    const bar = stackBar('rw-bar-thin', () => ({ total: state.total, segments: [{ value: state.used, color: COLORS.yellow }] }));
                    box.add_child(l.actor);
                    box.add_child(bar);
                    list.add_child(box);
                    return { l, bar, state };
                });
                names = disks.map((d) => d.name).join();
            }
            disks.forEach((d, i) => {
                const r = rows[i];
                r.l.set((d.model || d.name).replace(/_/g, ' '), `${Fmt.bytes(d.used)} / ${Fmt.bytes(d.total)}`);
                r.state.used = d.used;
                r.state.total = d.total;
                r.bar.queue_repaint();
            });
        },
    };
}

const CARDS = { cpu: cpuCard, mem: memCard, gpu: gpuCard, net: netCard, disk: diskCard };

// { actor, update(snapshot) } for a top-bar item key
export const makeCard = (key, src) => CARDS[key](src);
