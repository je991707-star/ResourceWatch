// Same formatting as widget/contents/ui/fmt.js and frontend/app.js.

export function bytes(b) {
    if (b == null) return '--';
    const u = ['B', 'KB', 'MB', 'GB', 'TB'];
    let i = 0;
    while (b >= 1024 && i < u.length - 1) { b /= 1024; i++; }
    return `${b.toFixed(i && b < 100 ? (b < 10 ? 2 : 1) : 0)} ${u[i]}`;
}
export const rate = (b) => (b == null ? '--' : `${bytes(b)}/s`);
export const pct = (v) => (v == null ? '--' : `${v.toFixed(0)}%`);
export const temp = (v) => (v == null ? '--' : `${v.toFixed(0)} °C`);
export const ratio = (a, b) => (a == null || !b ? null : (a / b) * 100);

// The configured interface, else the one with the default gateway (as the dashboard).
export const nic = (s, name) => s.net.find((n) => n.name === name) ?? s.net.find((n) => n.gateway) ?? s.net.find((n) => n.up) ?? s.net[0] ?? null;
// The configured GPU index, clamped to the last GPU.
export const gpu = (s, i) => (s.gpus.length ? s.gpus[Math.min(i ?? 0, s.gpus.length - 1)] : null);
// Sum of a per-disk rate over all disks; null when none reports one.
export function sum(list, key) {
    const v = list.map((d) => d[key]).filter((x) => x != null);
    return v.length ? v.reduce((a, b) => a + b, 0) : null;
}
// Compact rate for the top bar: "980B", "1.4K", "12M" (per second implied).
export function short(b) {
    if (b == null) return '--';
    const u = ['B', 'K', 'M', 'G'];
    let i = 0;
    while (b >= 1000 && i < u.length - 1) { b /= 1024; i++; }
    return (i && b < 10 ? b.toFixed(1) : b.toFixed(0)) + u[i];
}
