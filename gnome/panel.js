// Top-bar items, one PanelMenu.Button each in widget.json's `panel` order (iStat Menus style, as the Plasma
// panel item): CPU/GPU/RAM show the label over the percentage and a narrow area chart; network/disk an icon,
// two rate lines and mirrored bars. Clicking an item opens its card (cards.js).
import St from 'gi://St';
import Clutter from 'gi://Clutter';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as Fmt from './fmt.js';
import * as Chart from './chart.js';
import { vbox, COLORS } from './theme.js';
import { makeCard } from './cards.js';

const SPEC = {
    cpu: { label: 'CPU', chart: ['cpu', COLORS.blue] },
    gpu: { label: 'GPU', chart: ['gpu', COLORS.cyan] },
    mem: { label: 'RAM', chart: ['mem', COLORS.yellow] },
    net: { icon: 'network-wired-symbolic', mirror: ['tx', 'rx', COLORS.magenta, COLORS.blue] },
    disk: { icon: 'drive-harddisk-symbolic', mirror: ['r', 'w', COLORS.magenta, COLORS.blue] },
};

class Item {
    constructor(key, source, uuid, position) {
        this.key = key;
        this._source = source;
        this._spec = SPEC[key];
        this.button = new PanelMenu.Button(0.5, `ResourceWatch ${key}`, false);
        const box = new St.BoxLayout({ style_class: 'rw-item', y_align: Clutter.ActorAlign.CENTER });
        if (this._spec.icon) box.add_child(new St.Icon({ icon_name: this._spec.icon, style_class: 'rw-icon', y_align: Clutter.ActorAlign.CENTER }));
        const lines = vbox({ style_class: this._spec.icon ? 'rw-lines rw-rates' : 'rw-lines', y_align: Clutter.ActorAlign.CENTER });
        this._l1 = new St.Label({ style_class: this._spec.icon ? 'rw-line' : 'rw-line rw-label', x_align: Clutter.ActorAlign.CENTER });
        this._l2 = new St.Label({ style_class: 'rw-line', x_align: Clutter.ActorAlign.CENTER });
        lines.add_child(this._l1);
        lines.add_child(this._l2);
        box.add_child(lines);
        this._chart = new St.DrawingArea({ style_class: this._spec.mirror ? 'rw-mirror' : 'rw-chart', y_align: Clutter.ActorAlign.CENTER });
        this._chart.connect('repaint', (a) => this._paint(a));
        box.add_child(this._chart);
        this.button.add_child(box);

        // the card is built on first open and only updated while the menu is open
        // straight into the menu's box: a non-reactive menu item would be styled as inactive (dimmed)
        this._box = vbox({ style_class: 'rw-card-box', x_expand: true });
        this._err = new St.Label({ style_class: 'rw-error' });
        this._box.add_child(this._err);
        this.button.menu.box.add_child(this._box);
        this.button.menu.connect('open-state-changed', (_m, open) => { if (open) this._updateCard(); });
        Main.panel.addToStatusArea(`${uuid}-${key}`, this.button, position, 'right');
    }

    update() {
        const src = this._source, s = src.snap;
        let lines;
        if (!s) lines = [this._spec.label ?? '--', '--'];
        else if (this.key === 'cpu') lines = ['CPU', Fmt.pct(s.cpu.usage)];
        else if (this.key === 'mem') lines = ['RAM', Fmt.pct(Fmt.ratio(s.mem.used, s.mem.total))];
        else if (this.key === 'gpu') lines = ['GPU', Fmt.pct(src.gpu()?.usage)];
        else if (this.key === 'net') { const n = src.nic(); lines = [`↓${Fmt.short(n?.rxBps)}`, `↑${Fmt.short(n?.txBps)}`]; }
        else lines = [`R ${Fmt.short(Fmt.sum(s.disks, 'readBps'))}`, `W ${Fmt.short(Fmt.sum(s.disks, 'writeBps'))}`];
        this._l1.text = lines[0];
        this._l2.text = lines[1];
        this._chart.queue_repaint();
        if (this.button.menu.isOpen) this._updateCard();
    }

    _updateCard() {
        const src = this._source;
        this._err.text = src.error;
        this._err.visible = !!src.error;
        if (!src.snap) return;
        if (!this._card) {
            this._card = makeCard(this.key, src);
            this._box.add_child(this._card.actor);
        }
        this._card.update(src.snap);
    }

    _paint(area) {
        const cr = area.get_context();
        const [w, h] = area.get_surface_size();
        const hist = this._source.hist;
        if (this._spec.chart) Chart.area(cr, w, h, hist[this._spec.chart[0]] ?? [], this._spec.chart[1], { points: 30, line: 1 });
        else { const [u, d, uc, dc] = this._spec.mirror; Chart.mirror(cr, w, h, hist[u] ?? [], hist[d] ?? [], uc, dc, { points: 30, gap: 0 }); }
        cr.$dispose();
    }

    destroy() { this.button.destroy(); }
}

export class PanelItems {
    constructor(source, uuid) {
        this._source = source;
        this._uuid = uuid;
        this._items = [];
        this._keys = '';
        this._unsub = source.subscribe(() => this._update());
        this._update();
    }

    _update() {
        const keys = (this._source.cfg.panel ?? []).filter((k) => SPEC[k]);
        if (keys.join() !== this._keys) {
            this._items.forEach((i) => i.destroy());
            this._items = keys.map((k, i) => new Item(k, this._source, this._uuid, i));
            this._keys = keys.join();
        }
        this._items.forEach((i) => i.update());
    }

    destroy() {
        this._unsub();
        this._items.forEach((i) => i.destroy());
        this._items = [];
    }
}
