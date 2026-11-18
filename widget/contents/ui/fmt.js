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
// Sum of a per-disk rate over all disks; null when none reports one.
function sum(list, key) {
    const v = list.map(d => d[key]).filter(v => v != null);
    return v.length ? v.reduce((a, b) => a + b, 0) : null;
}
// First temperature per sensor device, like the dashboard's sensors card.
function temps(s) {
    const seen = {}, out = [];
    for (const x of s.sensors) {
        if (x.kind !== "temp" || seen[x.device]) continue;
        seen[x.device] = true;
        out.push({ device: x.device, value: x.value, crit: x.crit });
    }
    return out;
}
// Compact rate for the panel: "980B", "1.4K", "12M" (per second implied).
function short(b) {
    if (b == null) return "--";
    const u = ["B", "K", "M", "G"];
    let i = 0;
    while (b >= 1000 && i < u.length - 1) { b /= 1024; i++; }
    return (i && b < 10 ? b.toFixed(1) : b.toFixed(0)) + u[i];
}
