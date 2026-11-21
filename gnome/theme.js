// Colors from widget/contents/ui/Theme.qml (dark; GNOME's top bar is dark in both styles) and small St helpers.
import St from 'gi://St';
import Clutter from 'gi://Clutter';

export const COLORS = {
    blue: '#3d8ef8', yellow: '#f5b731', cyan: '#2fd6c8', magenta: '#d65ef0', green: '#3ccf5d', gray: '#8e8e93',
};

// Vertical St.BoxLayout: `orientation` replaced `vertical` in GNOME 48; 46/47 only have `vertical`.
export function vbox(props = {}) {
    const box = new St.BoxLayout(props);
    if ('orientation' in box) box.orientation = Clutter.Orientation.VERTICAL;
    else box.vertical = true;
    return box;
}
