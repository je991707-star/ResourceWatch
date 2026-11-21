// ResourceWatch in the GNOME top bar: one button per item of widget.json's `panel`, each opening its card.
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import { Source } from './data.js';
import { PanelItems } from './panel.js';

export default class ResourceWatchExtension extends Extension {
    enable() {
        this._source = new Source();
        this._panel = new PanelItems(this._source, this.uuid);
    }

    disable() {
        this._panel?.destroy();
        this._source?.destroy();
        this._panel = null;
        this._source = null;
    }
}
