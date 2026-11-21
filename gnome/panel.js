// Top-bar items (skeleton: CPU text only, filled in by the next phase).
import St from 'gi://St';
import Clutter from 'gi://Clutter';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as Fmt from './fmt.js';

export class PanelItems {
    constructor(source, uuid) {
        this._source = source;
        this._button = new PanelMenu.Button(0.5, 'ResourceWatch', false);
        this._label = new St.Label({ text: '…', y_align: Clutter.ActorAlign.CENTER });
        this._button.add_child(this._label);
        Main.panel.addToStatusArea(`${uuid}-cpu`, this._button, 0, 'right');
        this._unsub = source.subscribe((s) => this._update(s));
    }

    _update(s) {
        this._label.text = s.error ? '!' : `CPU ${Fmt.pct(s.snap?.cpu.usage)}`;
    }

    destroy() {
        this._unsub();
        this._button.destroy();
    }
}
