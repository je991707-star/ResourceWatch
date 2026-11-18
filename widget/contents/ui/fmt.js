.pragma library
// Same formatting as frontend/app.js.

function bytes(b) {
    if (b == null) return "--";
    const u = ["B", "KB", "MB", "GB", "TB"];
    let i = 0;
    while (b >= 1024 && i < u.length - 1) { b /= 1024; i++; }
    return b.toFixed(i && b < 100 ? (b < 10 ? 2 : 1) : 0) + " " + u[i];
}
function rate(b) { return b == null ? "--" : bytes(b) + "/s"; }
function pct(v) { return v == null ? "--" : v.toFixed(0) + "%"; }
// shortcut: always °C; the app's °C/°F choice lives in its localStorage, add `unit` to widget.json if wanted
function temp(v) { return v == null ? "--" : v.toFixed(0) + " °C"; }
function ratio(a, b) { return a == null || !b ? null : (a / b) * 100; }

// The network card's interface: the configured one, else the one with the default gateway (as the dashboard).
function nic(s, name) {
    return s.net.find(n => n.name === name) ?? s.net.find(n => n.gateway) ?? s.net.find(n => n.up) ?? s.net[0];
}
// First temperature per sensor device, like the dashboard's sensors card.
function temps(s) {
    const seen = {}, out = [];
    for (const x of s.sensors) {
        if (x.kind !== "temp" || seen[x.device]) continue;
        seen[x.device] = true;
        out.push({ device: x.device, value: x.value });
    }
    return out;
}

const TITLES = { cpu: "處理器", mem: "記憶體", gpu: "顯示卡", net: "網路", disk: "磁碟", temp: "溫度" };

// Plain text lines per card.
function lines(key, s, cfg) {
    switch (key) {
    case "cpu": return ["使用率 " + pct(s.cpu.usage), "溫度 " + temp(s.cpu.temp)];
    case "mem": return ["已用 " + bytes(s.mem.used) + " / " + bytes(s.mem.total) + "（" + pct(ratio(s.mem.used, s.mem.total)) + "）"];
    case "gpu": {
        const g = s.gpus[0];
        return g ? [g.name, "使用率 " + pct(g.usage) + " · " + temp(g.temp)] : ["沒有顯示卡"];
    }
    case "net": {
        const n = nic(s, cfg.nic);
        return n ? [n.name, "↓ " + rate(n.rxBps) + "  ↑ " + rate(n.txBps)] : ["沒有網路介面"];
    }
    case "disk": {
        const r = s.disks.reduce((a, d) => a + (d.readBps ?? 0), 0), w = s.disks.reduce((a, d) => a + (d.writeBps ?? 0), 0);
        return ["讀取 " + rate(r) + "  寫入 " + rate(w)];
    }
    case "temp": {
        const t = temps(s).slice(0, 4);
        return t.length ? t.map(x => x.device + " " + temp(x.value)) : ["沒有溫度感測器"];
    }
    }
    return [];
}
