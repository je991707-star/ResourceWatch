// Test-only (gnome/dev/nested.sh): lets the test script drive the shell over D-Bus.
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';

export default class Devtools extends Extension {
    enable() { global.context.unsafe_mode = true; }
    disable() { global.context.unsafe_mode = false; }
}
