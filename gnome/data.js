// Runs `resourcewatch --widget` every `interval` seconds (asynchronously: the shell's main loop must never
// block) and keeps the latest snapshot and 60-point histories; watches widget.json so settings changed in the
// app apply at once. Listeners get called after every sample or config change.

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import * as Fmt from './fmt.js';

const DEFAULT = { panel: ['cpu', 'gpu', 'mem', 'net', 'disk'], interval: 2, panelNic: null, panelGpu: 0 };
const POINTS = 60;

export class Source {
    constructor() {
        this.cfg = { ...DEFAULT };
        this.snap = null;
        this.cpuModel = '';
        this.error = '';
        this.hist = {};
        this._listeners = new Set();
        this._cancel = new Gio.Cancellable();
        this._timer = 0;
        this._busy = false;
        this._cfgFile = Gio.File.new_for_path(GLib.build_filenamev([GLib.get_user_config_dir(), 'resourcewatch', 'widget.json']));
        this._monitor = this._cfgFile.monitor_file(Gio.FileMonitorFlags.NONE, null);
        this._monitorId = this._monitor.connect('changed', (_m, _f, _o, ev) => {
            if (ev === Gio.FileMonitorEvent.CHANGES_DONE_HINT || ev === Gio.FileMonitorEvent.CREATED) this._readConfig();
        });
        this._readConfig();
        this._run();
        this._restartTimer();
    }

    subscribe(fn) { this._listeners.add(fn); return () => this._listeners.delete(fn); }
    _emit() { for (const fn of this._listeners) fn(this); }

    _restartTimer() {
        if (this._timer) GLib.source_remove(this._timer);
        this._timer = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, this.cfg.interval ?? 2, () => { this._run(); return GLib.SOURCE_CONTINUE; });
    }

    // ~/.local/bin is not always on gnome-shell's PATH
    _exe() {
        const local = GLib.build_filenamev([GLib.get_home_dir(), '.local', 'bin', 'resourcewatch']);
        return GLib.file_test(local, GLib.FileTest.IS_EXECUTABLE) ? local : GLib.find_program_in_path('resourcewatch');
    }

    _run() {
        if (this._busy) return; // the previous run (about a second of sampling) is still going
        const exe = this._exe();
        if (!exe) { this._fail('找不到 resourcewatch，請先執行 install.sh'); return; }
        let proc;
        try {
            proc = Gio.Subprocess.new([exe, '--widget'], Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_PIPE);
        } catch (e) {
            this._fail(`resourcewatch 執行失敗：${e.message}`);
            return;
        }
        this._busy = true;
        proc.communicate_utf8_async(null, this._cancel, (p, res) => {
            this._busy = false;
            let out, err;
            try {
                [, out, err] = p.communicate_utf8_finish(res);
            } catch (e) {
                if (!e.matches(Gio.IOErrorEnum, Gio.IOErrorEnum.CANCELLED)) this._fail(`resourcewatch 執行失敗：${e.message}`);
                return;
            }
            if (!p.get_successful()) { this._fail(`resourcewatch 執行失敗：${(err || '').trim() || p.get_exit_status()}`); return; }
            let data;
            try { data = JSON.parse(out); } catch { this._fail('無法解析 resourcewatch 輸出'); return; }
            this._applyConfig(data.config);
            this.snap = data.snapshot;
            this.cpuModel = data.cpuModel ?? '';
            this.error = '';
            this._record(data.snapshot);
            this._emit();
        });
    }

    _fail(msg) { this.error = msg; this._emit(); }

    _readConfig() {
        this._cfgFile.load_contents_async(this._cancel, (f, res) => {
            try {
                const [, bytes] = f.load_contents_finish(res);
                this._applyConfig(JSON.parse(new TextDecoder().decode(bytes)));
                this._emit();
            } catch { /* missing file (the --widget output brings the defaults) or half-written: next change */ }
        });
    }

    _applyConfig(c) {
        if (!c) return;
        const interval = this.cfg.interval;
        this.cfg = { ...DEFAULT, ...c };
        if (this.cfg.interval !== interval) this._restartTimer();
    }

    nic(s = this.snap) { return s ? Fmt.nic(s, this.cfg.panelNic) : null; }
    gpu(s = this.snap) { return s ? Fmt.gpu(s, this.cfg.panelGpu) : null; }

    _record(s) {
        const push = (k, v) => { const a = (this.hist[k] ??= []); a.push(v); if (a.length > POINTS) a.shift(); };
        push('cpu', s.cpu.usage);
        push('mem', Fmt.ratio(s.mem.used, s.mem.total));
        push('gpu', this.gpu(s)?.usage ?? null);
        const n = this.nic(s);
        push('rx', n?.rxBps ?? null);
        push('tx', n?.txBps ?? null);
        push('r', Fmt.sum(s.disks, 'readBps'));
        push('w', Fmt.sum(s.disks, 'writeBps'));
    }

    destroy() {
        this._cancel.cancel();
        if (this._timer) GLib.source_remove(this._timer);
        this._timer = 0;
        this._monitor.disconnect(this._monitorId);
        this._monitor.cancel();
        this._listeners.clear();
    }
}
