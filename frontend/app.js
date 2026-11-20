'use strict';
// Polls the Rust commands and re-renders the current page. Missing values are null → "--".
const invoke = window.__TAURI__?.core.invoke ?? window.mockInvoke;
const N = 60; // line-chart points (1 per sample)
const NB = 120; // samples kept; bar charts show all of them, line charts the last N
const hist = {};
let snap = null;
let info = null;
let procs = [];
let conns = null; // socket list from `connections`, polled only on the network page
let tcp = null; // { states, retrans: % over the last poll, prev counters }
let page = 'dash';
let sort = { key: 'cpu', desc: true };
let sumDisk = 0; // SUMMARY disk flip index
// Dashboard ‹ › choices by key (interface name, GPU index, disk name); null → the card's default.
const pick = { nic: null, gpu: null, io: null };
// Network card totals restart from a saved baseline per interface; kernel counters reset on reboot, so a
// baseline above the current counter is stale and ignored.
let netBase = {};
try { netBase = JSON.parse(store('rw-netbase') || '{}'); } catch { /* corrupt → none */ }
const sinceBase = (n, k) => { const b = netBase[n.name]?.[k] ?? 0; return n[k] >= b ? n[k] - b : n[k]; };
const DEFAULTS = { theme: 'system', interval: 1000, unit: 'c', nic: '', gpu: '0', dev: '' };
let prefs = { ...DEFAULTS };

const $ = (s) => document.querySelector(s);
const esc = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
const store = (k, v) => { try { if (v === undefined) return localStorage.getItem(k); localStorage.setItem(k, v); } catch { return null; } };
const push = (k, v) => {
  const a = (hist[k] ??= []);
  a.push(v);
  if (a.length > NB) a.shift();
};
const last = (k) => hist[k]?.at(-1) ?? null;

// ── formatting ──
function bytes(b) {
  if (b == null) return '--';
  const u = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0;
  while (b >= 1024 && i < u.length - 1) { b /= 1024; i++; }
  return `${b.toFixed(i && b < 100 ? (b < 10 ? 2 : 1) : 0)} ${u[i]}`;
}
const rate = (b) => (b == null ? '--' : `${bytes(b)}/s`);
const pct = (v) => (v == null ? '--' : `${v.toFixed(0)}%`);
const temp = (v) => (v == null ? '--' : prefs.unit === 'f' ? `${(v * 1.8 + 32).toFixed(0)} °F` : `${v.toFixed(0)} °C`);
const ratio = (a, b) => (a == null || !b ? null : (a / b) * 100);
function duration(s) {
  if (s == null) return '--';
  const d = Math.floor(s / 86400), h = Math.floor((s % 86400) / 3600), m = Math.floor((s % 3600) / 60);
  return `${d ? `${d} 天 ` : ''}${h} 小時 ${m} 分`;
}

// ── icons: Font Awesome Free (frontend/vendor/fontawesome); `gpu` is also the CPU card's chip ──
const ICON = {
  dash: 'fa-solid fa-gauge-high', apps: 'fa-solid fa-layer-group', storage: 'fa-solid fa-hard-drive', net: 'fa-solid fa-globe',
  sensors: 'fa-solid fa-temperature-half', moon: 'fa-solid fa-moon', sun: 'fa-solid fa-sun', auto: 'fa-solid fa-circle-half-stroke',
  pc: 'fa-solid fa-desktop', router: 'fa-solid fa-network-wired', down: 'fa-solid fa-arrow-down', up: 'fa-solid fa-arrow-up', eth: 'fa-solid fa-ethernet', wifi: 'fa-solid fa-wifi', dns: 'fa-solid fa-server',
  folder: 'fa-solid fa-folder', pick: 'fa-solid fa-folder-open', file: 'fa-regular fa-file', refresh: 'fa-solid fa-rotate-right', cancel: 'fa-solid fa-xmark', gpu: 'fa-solid fa-microchip', app: 'fa-regular fa-window-maximize', reset: 'fa-solid fa-rotate-left',
  display: 'fa-solid fa-display', board: 'fa-solid fa-server', leaf: 'fa-solid fa-leaf', open: 'fa-solid fa-chevron-down', closed: 'fa-solid fa-chevron-right',
  mem: 'fa-solid fa-memory',
};
const icon = (name, cls = '') => `<i class="i ${ICON[name]} ${cls}"></i>`;
// os-release ID → Font Awesome brand, used when the distro ships no logo file
const BRAND = { fedora: 'fedora', ubuntu: 'ubuntu', debian: 'debian', rhel: 'redhat', centos: 'centos', opensuse: 'suse', 'opensuse-leap': 'suse', 'opensuse-tumbleweed': 'suse', sles: 'suse' };

// ── drawing (SVG attributes only: the CSP forbids inline style) ──
function bar(segs, thick) {
  let x = 0;
  const rects = segs.map(([p, cls]) => {
    const w = p == null ? 0 : Math.min(Math.max(p, 0), 100 - x);
    const r = `<rect class="${cls}" x="${x}" width="${w}" height="12"/>`;
    x += w;
    return r;
  }).join('');
  return `<svg class="bar${thick ? ' thick' : ''}" viewBox="0 0 100 12" preserveAspectRatio="none"><rect class="track" width="100" height="12"/>${rects}</svg>`;
}

// series: [{ data, cls }]; max fixed (percent) or auto from data. Line on top of a translucent area.
function chart(series, max, tall) {
  series = series.map((s) => ({ ...s, data: (s.data ?? []).slice(-N) }));
  max = max ?? Math.max(1, ...series.flatMap((s) => (s.data ?? []).filter((v) => v != null))) * 1.15;
  const y = (v) => (99 - (Math.min(v, max) / max) * 96).toFixed(1); // 0 sits just above the edge so its line stays visible
  const paths = series.map(({ data = [], cls }) => {
    const off = N - data.length;
    const runs = [[]];
    data.forEach((v, i) => (v == null ? runs.push([]) : runs.at(-1).push([off + i, y(v)])));
    return runs.filter((r) => r.length).map((r) => {
      const line = r.map(([x, yy], i) => `${i ? 'L' : 'M'}${x} ${yy}`).join('');
      const area = `<path class="area ${cls}" d="${line}L${r.at(-1)[0]} 100L${r[0][0]} 100Z"/>`;
      return `${area}<path class="line ${cls}" d="${line}"/>`; // the line keeps an all-zero series visible
    }).join('');
  });
  // top (max) long dashes, middle (half) short dashes, bottom (zero) solid; y() maps max/half/0 to 3/51/99
  const grid = [[3, 'top'], [51, 'mid'], [99, 'base']].map(([g, c]) => `<line class="grid-line ${c}" x1="0" x2="${N - 1}" y1="${g}" y2="${g}"/>`).join('');
  return `<svg class="chart${tall ? ' tall' : ''}" viewBox="0 0 ${N - 1} 100" preserveAspectRatio="none">${grid}${paths.join('')}</svg>`;
}

// Mirrored bars: `up` grows upward from the midline, `down` downward; each half has its own auto scale
// so a quiet direction stays readable next to a busy one. HTML bars (heights via data-h → CSSOM in render)
// keep their rounded outer ends, which a stretched SVG would distort. Zero samples stay as stubs on the midline.
function mirror(up, down, tall) {
  const half = ({ data = [], cls }, top) => {
    const max = Math.max(1, ...data.filter((v) => v != null));
    const bars = data.map((v) => (v == null ? '<i></i>' : `<i class="${cls}" data-h="${Math.max((v / max) * 100, 4).toFixed(1)}"></i>`));
    return `<div class="half${top ? ' up' : ''}">${'<i></i>'.repeat(NB - data.length)}${bars.join('')}</div>`;
  };
  return `<div class="mirror${tall ? ' tall' : ''}">${half(up, true)}${half(down, false)}</div>`;
}

// rings: outer first, each a list of [percent, cls] segments.
function ring(rings, size, text) {
  const sw = 8, gap = 3.5;
  const out = rings.map((segs, k) => {
    const r = 50 - sw / 2 - k * (sw + gap);
    let start = 0;
    const arcs = segs.map(([p, cls]) => {
      const v = Math.min(Math.max(p ?? 0, 0), 99.9 - start);
      const c = v > 0 ? `<circle class="${cls}" cx="50" cy="50" r="${r}" pathLength="100" stroke-width="${sw}" stroke-dasharray="${v} 100" stroke-dashoffset="${-start}" transform="rotate(-90 50 50)"/>` : '';
      start += v;
      return c;
    }).reverse().join(''); // earlier segments on top, so each round end cap overlaps the next segment
    return `<circle class="track" cx="50" cy="50" r="${r}" stroke-width="${sw}"/>${arcs}`;
  }).join('');
  return `<svg class="ring" viewBox="0 0 100 100" width="${size}" height="${size}">${out}${text ? `<text x="50" y="50">${esc(text)}</text>` : ''}</svg>`;
}

const card = (title, body, extra = '') => `<section class="card">${title ? `<h2>${title}${extra}</h2>` : ''}${body}</section>`;
const kv = (pairs) => `<div class="kv">${pairs.map(([k, v]) => `<div><span>${k}</span><b>${esc(v)}</b></div>`).join('')}</div>`;
const legend = (items) => `<div class="legend">${items.map(([cls, k, v]) => `<div><span><i class="dot ${cls}"></i>${k}</span><b>${esc(v)}</b></div>`).join('')}</div>`;
const cap = (a, b) => `<div class="cap"><span>${a}</span><span>${b}</span></div>`;
const more = (pg, t) => `<a class="more" data-goto="${pg}">${t}</a>`;

function memParts(m) {
  const app = Math.max(m.total - m.free - m.cached, 0);
  return [['c-yellow', '應用程式', app], ['c-blue', '快取', m.cached], ['c-green', '可用', m.free]];
}
// dashboard GPU: the card's ‹ › choice, else the settings default (index; clamped when a GPU disappears)
const gpuIndex = (s) => Math.max(0, Math.min(+(pick.gpu ?? prefs.gpu) || 0, s.gpus.length - 1));
const primaryNet = (s) => s.net.find((n) => n.name === prefs.nic) ?? s.net.find((n) => n.gateway) ?? s.net.find((n) => n.up) ?? s.net[0];
const diskName = (d) => (d.model || d.name).replace(/_/g, ' '); // NVMe models use _ for spaces
// Process name / desktop id → { icon: data: URL | null, name: .desktop Name | null }; null while loading
// or when no app matches (generic glyph then).
const appIcons = {};
const appIcon = (name) => (appIcons[name]?.icon ? `<img class="app-ic" src="${esc(appIcons[name].icon)}" alt="">` : icon('app', 'app-ic'));
async function loadIcons(names) {
  const missing = [...new Set(names)].filter((n) => !(n in appIcons));
  if (!missing.length) return;
  missing.forEach((n) => { appIcons[n] = null; });
  try {
    Object.assign(appIcons, await invoke('app_icons', { names: missing }));
    if (page === 'dash' || page === 'apps') render();
  } catch (e) {
    console.error(e);
  }
}
const topProcs = (key, n) => [...procs].filter((p) => p[key] != null).sort((a, b) => b[key] - a[key]).slice(0, n);

function record(s) {
  push('cpu', s.cpu.usage);
  s.gpus.forEach((g, i) => push(`gpu${i}`, g.usage));
  s.disks.forEach((d) => { push(`r:${d.name}`, d.readBps); push(`w:${d.name}`, d.writeBps); });
  s.net.forEach((n) => { push(`rx:${n.name}`, n.rxBps); push(`tx:${n.name}`, n.txBps); });
}

// ── dashboard cards ──
function summaryCard(s) {
  const m = s.mem, disks = s.disks.filter((x) => x.total);
  sumDisk = Math.min(sumDisk, Math.max(disks.length - 1, 0));
  const d = disks[sumDisk], gpu = s.gpus[gpuIndex(s)]?.usage;
  const parts = memParts(m);
  const gauge = ring([
    parts.slice(0, 2).map(([cls, , v]) => [ratio(v, m.total), cls]),
    [[s.cpu.usage, 'c-magenta']],
    [[gpu, 'c-cyan']],
  ], 140);
  const right = `${legend(parts.map(([cls, k, v]) => [cls, k, bytes(v)]))}
    <div class="sect"><h3>${esc(info?.cpuModel?.replace(/\s*@.*$/, '') || '處理器')}</h3>${kv([['核心', info?.cpuCores ?? '--'], ['執行緒', info?.cpuThreads ?? '--']])}</div>
    ${d ? `<div class="sect"><h3>${icon('storage')}<span class="ell" title="${esc(diskName(d))}">${esc(diskName(d))}</span><i class="spacer"></i>
      <button class="arrow" data-disk="-1"${sumDisk ? '' : ' disabled'}>‹</button><button class="arrow" data-disk="1"${sumDisk < disks.length - 1 ? '' : ' disabled'}>›</button></h3>
      ${kv([['已使用', bytes(d.used)], ['總量', bytes(d.total)], ['可用', bytes(d.total - d.used)]])}</div>` : ''}`;
  return card('摘要', `<div class="summary"><div class="gauge">${gauge}<ul>
      <li><i class="dot c-yellow"></i><span>記憶體</span><b>${bytes(m.total)}</b></li>
      <li><i class="dot c-magenta"></i><span>處理器</span><b>${pct(s.cpu.usage)}</b></li>
      <li><i class="dot c-cyan"></i><span>顯示卡</span><b>${pct(gpu)}</b></li></ul></div><div>${right}</div></div>`);
}

function cpuCard(s) {
  const cores = `<div class="cores">${s.cpu.cores.map((v) => `<i data-h="${Math.max(v ?? 0, 3).toFixed(1)}"></i>`).join('')}</div>`;
  const rows = topProcs('cpu', 5).map((p) => `<tr><td class="grow">${appIcon(p.name)}${esc(p.name)}</td><td class="num">${p.cpu.toFixed(1)}%</td></tr>`).join('');
  return card('處理器', `<h3 class="sub">${icon('gpu')}<span class="ell" title="${esc(info?.cpuModel || '')}">${esc(info?.cpuModel || '--')}</span></h3>
    <div class="cpu-row"><div>${cap('核心', '')}${cores}</div><div>${cap(pct(s.cpu.usage), `${temp(s.cpu.temp)} · ${s.cpu.freqMhz ? (s.cpu.freqMhz / 1000).toFixed(2) + ' GHz' : '--'}`)}${chart([{ data: hist.cpu, cls: 'c-blue' }], 100)}</div></div>
    <div class="sect"><table class="zebra procs">${rows || '<tr><td class="muted">讀取程序中…</td></tr>'}</table></div>${more('apps', '顯示應用程式…')}`);
}

function memCard(s) {
  const m = s.mem, parts = memParts(m);
  const rows = topProcs('mem', 5).map((p) => `<tr><td class="grow">${appIcon(p.name)}${esc(p.name)}</td><td class="num">${bytes(p.mem)}</td></tr>`).join('');
  const spec = `<i class="spacer"></i><span class="spec">${esc(info?.memSpec ?? bytes(m.total))}</span>`;
  return card('記憶體', `${bar(parts.map(([cls, , v]) => [ratio(v, m.total), cls]), true)}
    <div class="sect">${legend([...parts.map(([cls, k, v]) => [cls, k, bytes(v)]), ['c-gray', 'SWAP', `${bytes(m.swapUsed)} / ${bytes(m.swapTotal)}`]])}</div>
    <div class="sect"><table class="zebra procs"><thead><tr><th>名稱</th><th class="num">記憶體</th></tr></thead>${rows || '<tr><td class="muted">讀取程序中…</td></tr>'}</table></div>${more('apps', '顯示應用程式…')}`, spec);
}

// Titled label/value list; a row's third item is a class for the value.
const lv = (title, items) => `<div>${cap(title, '')}<table class="lv">${items.map(([k, v, cls]) =>
  `<tr><th>${k}</th><td class="${cls ?? ''}" title="${esc(v)}">${esc(v)}</td></tr>`).join('')}</table></div>`;

// Cumulative rx/tx packet counters; errors (red) and drops (yellow) shown with their share of the packets
// in that direction, since a bare count says nothing about whether it matters.
function nicDetails(n) {
  const share = (v, total) => { const p = (v / Math.max(total, 1)) * 100; return p < 0.01 ? '<0.01%' : `${p.toFixed(2)}%`; };
  const c = (v, total, cls) => (cls && !v ? '<td class="num muted">無</td>'
    : `<td class="num ${cls ?? ''}">${v.toLocaleString()}${cls ? ` <span class="muted">(${share(v, total)})</span>` : ''}</td>`);
  const row = (k, tip, rx, tx, cls) => `<tr><th><abbr title="${tip}">${k}</abbr></th>${c(rx, n.rxPackets, cls)}${c(tx, n.txPackets, cls)}</tr>`;
  return `<div>${cap('封包統計（開機以來累計）', '')}<table class="lv"><thead><tr><th></th>
      <th class="num"><span class="rx">↓</span> 接收</th><th class="num"><span class="tx">↑</span> 傳送</th></tr></thead><tbody>
    ${row('封包', '成功收送的封包數', n.rxPackets, n.txPackets)}
    ${row('錯誤', '因線路或硬體問題（如 CRC 錯誤）而損毀的封包；持續增加代表網路線、接頭或網卡可能有問題', n.rxErrors, n.txErrors, 'bad')}
    ${row('丟棄', '已收到但被網卡或核心捨棄的封包，常見原因為不支援的協定或緩衝區已滿；少量通常不影響連線', n.rxDropped, n.txDropped, 'warn')}
  </tbody></table></div>`;
}

// Wi-Fi channel number from the center frequency (MHz).
const wifiChannel = (f) => (f === 2484 ? 14 : f < 2500 ? (f - 2407) / 5 : f < 5925 ? (f - 5000) / 5 : (f - 5950) / 5);
const wifiBand = (f) => (f < 2500 ? '2.4 GHz' : f < 5925 ? '5 GHz' : '6 GHz');

function wifiDetails(w) {
  const mb = (v) => (v == null ? '--' : `${v}`);
  return lv('Wi-Fi', [
    ['SSID', w.ssid || '（隱藏）'],
    // NetworkManager's mapping: -100 dBm → 0%, -50 dBm → 100%
    ['訊號', w.signal == null ? '--' : `${w.signal} dBm · ${Math.max(0, Math.min(100, 2 * (w.signal + 100)))}%`],
    ['頻段', w.freq ? [wifiBand(w.freq), `頻道 ${wifiChannel(Math.round(w.freq))}`, w.width && `${w.width} MHz`].filter(Boolean).join(' · ') : '--'],
    ['速率', `↓ ${mb(w.rxRate)} ↑ ${mb(w.txRate)} Mb/s`], ['標準', w.generation ? `Wi-Fi ${w.generation}` : '--'], ['BSSID', w.bssid],
  ]);
}

// System-wide TCP socket states in plain words (raw state names in the tooltips), plus the retransmitted
// share of sent segments rated good / fair / poor.
function tcpStats() {
  if (!tcp) return '';
  const st = tcp.states, n = (...ks) => ks.reduce((a, k) => a + (st[k] ?? 0), 0);
  const detail = (...ks) => ks.map((k) => `${k.toUpperCase()} ${st[k] ?? 0}`).join('\n');
  const closing = ['time_wait', 'close_wait', 'fin_wait1', 'fin_wait2', 'last_ack', 'closing', 'close'];
  const r = tcp.retrans, grade = r == null ? '' : r < 1 ? 'good' : r < 3 ? 'fair' : 'poor';
  const fig = (v, k, tip, cls = '') => `<div title="${esc(tip)}"><b class="${cls}">${v}</b><span>${k}</span></div>`;
  return `<div class="stats">${[
    fig(n('established'), '使用中', '已建立、可以傳輸資料的連線（ESTABLISHED）'),
    fig(n('listen'), '等待連入', '開著連接埠、等待其他電腦連進來的服務（LISTEN）'),
    fig(n('syn_sent', 'syn_recv'), '建立中', `正在握手、尚未完成的連線\n${detail('syn_sent', 'syn_recv')}`),
    fig(n(...closing), '關閉中', `已結束、正在收尾的連線；TIME_WAIT 多屬正常\n${detail(...closing)}`),
    fig(r == null ? '--' : `${r.toFixed(2)}%`, `重傳率${{ good: ' · 良好', fair: ' · 普通', poor: ' · 不佳' }[grade] ?? ''}`,
      '兩次更新之間需要重送的封包比例；低於 1% 良好，持續高於 3% 代表網路不穩或壅塞', `q-${grade}`),
  ].join('')}</div>`;
}

const HTTPS_LIKE = { 443: 'HTTPS', 80: 'HTTP', 22: 'SSH', 53: 'DNS', 993: 'IMAP', 587: 'SMTP', 3478: 'STUN' };
// "1.2.3.4:443" / "[::1]:443" → [host, port]; v4-mapped v6 hosts are shown as plain IPv4.
const splitAddr = (a) => { const i = a.lastIndexOf(':'); return [a.slice(0, i).replace(/^\[|\]$/g, '').replace(/^::ffff:/, ''), +a.slice(i + 1)]; };
const isLan = (h) => /^(10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.|127\.|169\.254\.|f[cd]|fe80:|::1$)/i.test(h);

// Remote peers as "where to": host count by LAN / internet and the services by port.
function peers(remotes) {
  if (!remotes.length) return '--';
  const hosts = new Map(), ports = new Set();
  for (const a of remotes) { const [h, p] = splitAddr(a); hosts.set(h, isLan(h)); ports.add(HTTPS_LIKE[p] ?? `埠 ${p}`); }
  const lan = [...hosts.values()].filter(Boolean).length, wan = hosts.size - lan;
  const where = [wan && `網際網路 ${wan} 台`, lan && `區域網路 ${lan} 台`].filter(Boolean).join('、');
  const svc = [...ports].slice(0, 2).join('、') + (ports.size > 2 ? ` 等 ${ports.size} 種` : '');
  return `${where} · ${svc}`;
}

// One row per process: who it talks to, how many live connections, and which ports it listens on.
function connCard() {
  if (!conns) return card('連線', '<div class="empty">讀取連線中…</div>');
  const by = new Map();
  for (const c of conns) {
    const g = by.get(c.pid) ?? { name: c.name, pid: c.pid, open: 0, listen: new Map(), remotes: new Set() };
    if (c.state === 'listen') { const p = splitAddr(c.local)[1]; g.listen.set(p, [...(g.listen.get(p) ?? []), c.proto.toUpperCase()]); }
    else if (c.remote) { g.open++; g.remotes.add(c.remote); }
    by.set(c.pid, g);
  }
  const groups = [...by.values()].sort((a, b) => b.open - a.open || b.listen.size - a.listen.size || a.name.localeCompare(b.name));
  const rows = groups.map((g) => {
    const r = [...g.remotes], l = [...g.listen.keys()].sort((a, b) => a - b);
    return `<tr><td class="grow" title="${esc(`${g.name}（PID ${g.pid}）`)}">${appIcon(g.name)}${esc(g.name)}</td>
      <td class="grow${r.length ? '' : ' muted'}" title="${esc(r.join('\n'))}">${esc(peers(r))}</td>
      <td class="num${g.open ? '' : ' muted'}">${g.open || '--'}</td><td class="muted" title="${esc(l.map((p) => `${p}（${g.listen.get(p).join('／')}）`).join('\n'))}">${esc(l.join('、') || '--')}</td></tr>`;
  }).join('');
  const note = '<i class="spacer"></i><span class="spec" title="其他使用者與系統服務的 socket 需要管理員權限；上方數字為全系統 TCP">程序清單僅含目前使用者</span>';
  return card('連線', `${tcpStats()}<div class="sect"><table class="zebra procs conns"><thead><tr><th>應用程式</th><th>連線對象</th><th class="num">使用中</th><th>等待連入（埠）</th></tr></thead>
    <tbody>${rows || '<tr><td class="muted">沒有連線</td></tr>'}</tbody></table></div>`, note);
}

function addrBlock(n, dns) {
  return `<div class="addr">
    <div>${icon('pc')}<p><span>${esc(n.ip ? `${n.ip}/${n.prefix}` : '--')}</span><span class="muted sm">${esc(n.mac || '--')}</span></p></div>
    <div>${icon('router')}<p><span>${esc(n.gateway ?? '--')}</span><span class="muted sm">預設閘道</span></p></div>
    <div>${icon('dns')}<p><span title="${esc(dns.join('\n'))}">${esc(dns.join(', ') || '--')}</span><span class="muted sm">DNS</span></p></div>
    <div>${icon('net')}<p><span title="${esc(n.ipv6.join('\n'))}">${esc(n.ipv6[0] ?? '--')}</span><span class="muted sm">IPv6${n.ipv6.length > 1 ? ` · 另 ${n.ipv6.length - 1} 個` : ''}</span></p></div></div>`;
}

const peak = (k) => Math.max(0, ...(hist[k] ?? []).filter((v) => v != null));
// Big current rates (color dot, label, window peak) above mirrored bars; `up`/`down` are [label, hist key].
function flow([upL, upK], [downL, downK]) {
  const col = (l, k, cls) => `<div><b><i class="dot ${cls}"></i>${rate(last(k))}</b><span>${l}</span><span class="peak">峰值 ${rate(peak(k))}</span></div>`;
  return `<div class="rates">${col(upL, upK, 'c-magenta')}${col(downL, downK, 'c-blue')}</div>
    ${mirror({ data: hist[upK], cls: 'c-magenta' }, { data: hist[downK], cls: 'c-blue' })}`;
}

// ‹ › stepping through `keys` from index i; a click stores the neighbour's key in pick[kind].
const flipper = (kind, keys, i) => [[-1, '‹'], [1, '›']].map(([d, t]) =>
  `<button class="arrow" data-pick="${kind}" data-val="${esc(keys[i + d] ?? '')}"${keys[i + d] != null ? '' : ' disabled'}>${t}</button>`).join('');

function netCard(s) {
  const n = s.net.find((x) => x.name === pick.nic) ?? primaryNet(s);
  if (!n) return card('網路', '<div class="empty">沒有實體網路介面</div>');
  return card('網路', `<h3 class="sub">${icon(n.name.startsWith('wl') ? 'wifi' : 'eth')}${esc(n.name)}${flipper('nic', s.net.map((x) => x.name), s.net.indexOf(n))}<i class="spacer"></i>
      <span class="muted sm">累計 ↑ ${bytes(sinceBase(n, 'txTotal'))} · ↓ ${bytes(sinceBase(n, 'rxTotal'))}</span><button class="arrow reset" data-netreset="${esc(n.name)}" title="重設累計">${icon('reset')}</button></h3>
    ${flow(['上傳', `tx:${n.name}`], ['下載', `rx:${n.name}`])}`);
}

function disksCard(s) {
  const rows = s.disks.map((d) => `<tr title="${esc(d.name)}${d.mounts.length ? ' · ' + esc(d.mounts.join(', ')) : ''}"><td class="grow">${icon('storage')} ${esc(diskName(d))}</td>
    <td class="barcell">${bar([[ratio(d.used, d.total), 'c-yellow']])}</td>
    <td class="num">${d.total ? `${bytes(d.used)} / ${bytes(d.total)}` : `未掛載 · ${bytes(d.size)}`}</td></tr>`).join('');
  return card('磁碟', rows ? `<table>${rows}</table>` : '<div class="empty">沒有實體磁碟</div>');
}

function gpuCard(s) {
  if (!s.gpus.length) return card('顯示卡', '<div class="empty">未偵測到可讀取的顯示卡</div>');
  const meter = (k, v, p, cls) => `<div><span>${k}</span><b>${esc(v)}</b>${bar([[p, cls]])}</div>`;
  const i = gpuIndex(s), g = s.gpus[i];
  return card('顯示卡', `<h3 class="sub">${icon('gpu')}<span class="ell" title="${esc(g.name)}">${esc(g.name)}</span>${flipper('gpu', s.gpus.map((_, k) => String(k)), i)}<i class="spacer"></i><span class="muted sm">${temp(g.temp)}</span></h3>
      <div class="gpu-row">${ring([[[g.usage, 'c-blue']]], 96, pct(g.usage))}<div class="meters">
        ${meter('顯示記憶體', `${bytes(g.memUsed)} / ${bytes(g.memTotal)}`, ratio(g.memUsed, g.memTotal), 'c-yellow')}
        ${meter('編碼器', pct(g.encoder), g.encoder, 'c-green')}
        ${meter('解碼器', pct(g.decoder), g.decoder, 'c-magenta')}</div></div>
      <div class="sect">${cap('使用率', '')}${chart([{ data: hist[`gpu${i}`], cls: 'c-blue' }], 100)}</div>`);
}

// root actions via pkexec (backend `elevate`): the one running, and the last error
const elev = { busy: null, err: null };
async function elevate(act) {
  Object.assign(elev, { busy: act, err: null });
  render();
  try { await invoke('elevate', { action: act }); } catch (e) { elev.err = String(e); }
  elev.busy = null;
  render();
}

// sensors page card body: named readings in sections per kind; unnamed ones (temp3, in5…) and 0 rpm fans
// (usually empty headers) fold into "其他讀數", whose open state survives the per-tick re-render
const KIND_NAME = { temp: '溫度', fan: '風扇', percent: '風扇', volt: '電壓', power: '功耗' };
const rawOpen = new Set();
const unnamed = (x) => /^(temp|in|power)\d+$/.test(x.label) || (x.kind === 'fan' && x.value === 0);
function sensorBody(dev, list) {
  const table = (xs) => `<table class="zebra">${xs.map((x) => `<tr><td class="grow">${esc(sensorLabel(x.label))}</td>${sensorTd(x)}</tr>`).join('')}</table>`;
  const named = list.filter((x) => !unnamed(x));
  const main = named.length ? named : list; // nothing named (ACPI zones): show them all
  const rest = named.length ? list.filter(unnamed) : [];
  const kinds = [...new Set(main.map((x) => KIND_NAME[x.kind]))];
  const body = kinds.length > 1 ? kinds.map((k) => cap(k, '') + table(main.filter((x) => KIND_NAME[x.kind] === k))).join('') : table(main);
  return body + (rest.length ? `<details class="raw"${rawOpen.has(dev) ? ' open' : ''}><summary data-raw="${esc(dev)}">其他讀數（${rest.length}）</summary>${table(rest)}</details>` : '');
}

// Columns split by card count (6 → 3:3 or 2:2:2), unlike CSS columns which balance by height. Column count
// follows .masonry: up to 3 columns of at least 330 px with 24 px gaps, from #content's inner width.
function evenCols(cards) {
  const c = $('#content'), cs = getComputedStyle(c);
  const w = c.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
  const n = Math.max(1, Math.min(3, Math.floor((w + 24) / (330 + 24)), cards.length));
  const base = Math.floor(cards.length / n), extra = cards.length % n;
  let at = 0;
  const cols = Array.from({ length: n }, (_, k) => cards.slice(at, at += base + (k < extra ? 1 : 0)));
  return `<div class="cols">${cols.map((col) => `<div class="col">${col.join('')}</div>`).join('')}</div>`;
}

// shortcut: groups by display name, so two identical disk models share a card; key by hwmon path if that matters
function sensorGroups(s) {
  const devs = new Map();
  s.sensors.forEach((x) => devs.set(x.device, [...(devs.get(x.device) ?? []), x]));
  return devs;
}

// one row per device: its first temperature (package / Composite / GPU core; the backend lists temps first), else its first reading
function sensorsCard(s) {
  const devs = [...sensorGroups(s)];
  if (!devs.length) return card('感測器', '<div class="empty">沒有可讀取的感測器</div>');
  const rows = devs.slice(0, 8).map(([dev, list]) => {
    const x = list.find((y) => y.kind === 'temp') ?? list[0];
    return `<tr title="${esc(sensorLabel(x.label))}"><td class="grow">${icon(sensorIcon(x.chip))} ${esc(dev)}</td>${sensorTd(x)}</tr>`;
  });
  return card('感測器', `<table>${rows.join('')}</table>${s.sensors.length > rows.length ? more('sensors', '顯示更多…') : ''}`);
}
// hwmon label → readable name; unknown labels pass through
const SENSOR_LABEL = [
  [/^Package id 0$/, '處理器封裝'], [/^Package id (\d+)$/, '處理器封裝 $1'], [/^Core (\d+)$/, '核心 $1'],
  [/^T(ctl|die)$/, '處理器'], [/^Tccd(\d+)$/, 'CCD $1'], [/^Composite$/, '整體'], [/^Sensor (\d+)$/, '感測點 $1'],
  [/^edge$/, '核心'], [/^junction$/, '熱點'], [/^mem$/, '顯示記憶體'],
  [/^temp(\d+)$/, '溫度 $1'], [/^fan(\d+)$/, '風扇 $1'], [/^in(\d+)$/, '電壓 $1'], [/^power(\d+)$/, '功耗 $1'],
  [/^package-(\d+)$/, '封裝功耗'], [/^core$/, '核心功耗'], [/^dram$/, '記憶體功耗'], [/^uncore$/, '內顯功耗'],
];
const sensorLabel = (l) => { const r = SENSOR_LABEL.find(([re]) => re.test(l)); return r ? l.replace(r[0], r[1]) : l; };
// chip → card icon: CPU chip, disk, video card, else mainboard
const sensorIcon = (chip) => (/^(coretemp|k10temp|zenpower|cpu_thermal|intel-rapl)$/.test(chip) ? 'gpu'
  : /^(nvme|drivetemp)$/.test(chip) ? 'storage' : /^(amdgpu|radeon|nouveau|i915|xe|nvidia)$/.test(chip) ? 'display' : 'board');
const fmtSensor = (x) => (x.kind === 'temp' ? temp(x.value) : x.kind === 'percent' ? `${x.value.toFixed(0)}%`
  : x.kind === 'power' ? `${x.value.toFixed(1)} W` : x.kind === 'volt' ? `${x.value.toFixed(2)} V` : `${x.value.toFixed(0)} rpm`);
// value cell; red at or above the chip's critical temperature
const sensorTd = (x) => `<td class="num${x.crit != null && x.value >= x.crit ? ' hot' : ''}"${x.crit != null ? ` title="臨界 ${temp(x.crit)}"` : ''}>${fmtSensor(x)}</td>`;

function ioCard(s) {
  const d = s.disks.find((x) => x.name === pick.io) ?? s.disks[0];
  if (!d) return card('磁碟 I/O', '<div class="empty">沒有實體磁碟</div>');
  return card('磁碟 I/O', `<h3 class="sub">${icon('storage')}<span class="ell" title="${esc(d.name)}">${esc(diskName(d))}</span>${flipper('io', s.disks.map((x) => x.name), s.disks.indexOf(d))}</h3>
    ${flow(['讀取', `r:${d.name}`], ['寫入', `w:${d.name}`])}`);
}

// ── apps page: sections by cgroup kind, one expandable group per cgroup unit ──
const appOpen = new Set(); // expanded group keys; survives the 2 s re-render
const APP_SECTIONS = [['app', '應用程式'], ['bg', '背景處理程序'], ['sys', '系統處理程序']];
const stopped = (p) => p.state === 'T' || p.state === 't';
// Sum over the group; null when no member has a value (other users' disk I/O is unreadable).
const sumOf = (list, k) => (list.some((p) => p[k] != null) ? list.reduce((a, p) => a + (p[k] ?? 0), 0) : null);
function procGroups(kind) {
  const by = new Map();
  for (const p of procs) {
    if (p.kind !== kind) continue;
    const key = p.unit ?? `pid:${p.pid}`;
    if (!by.has(key)) by.set(key, []);
    by.get(key).push(p);
  }
  return [...by].map(([key, list]) => {
    const head = list.reduce((a, p) => (p.pid < a.pid ? p : a));
    const ic = head.app ?? head.name;
    const top = list.reduce((a, p) => ((p.gpu ?? -1) > (a.gpu ?? -1) ? p : a));
    const gpu = sumOf(list, 'gpu');
    return {
      key, list, ic, pid: head.pid,
      name: key === 'kernel' ? '核心執行緒' : appIcons[head.app]?.name ?? head.name,
      cpu: sumOf(list, 'cpu'), mem: sumOf(list, 'mem'), disk: sumOf(list, 'disk'),
      gpu: gpu == null ? null : Math.min(gpu, 100), gpuEngine: top.gpuEngine,
      // a group is paused / low-priority only when every member is
      state: list.length === 1 ? head.state : list.every(stopped) ? 'T' : 'S',
      nice: Math.min(...list.map((p) => p.nice)),
    };
  });
}
function bySort(a, b) {
  const k = sort.key, x = a[k], y = b[k];
  if (x == null || y == null) return (x == null) - (y == null) || a.pid - b.pid; // missing values always last
  return (typeof x === 'string' ? x.localeCompare(y) : x - y) * (sort.desc ? -1 : 1) || a.pid - b.pid;
}
const procStatus = (p) => `${stopped(p) ? '已暫停' : p.state === 'Z' ? '殭屍' : ''}${p.nice > 0 ? `<span title="低優先權（nice ${p.nice}）">${icon('leaf', 'leaf')}</span>` : ''}`;
const mbs = (b) => (b == null ? '--' : `${(b / 1048576).toFixed(1)} MB/秒`);
const procCells = (p) => `<td>${procStatus(p)}</td><td class="num">${p.cpu == null ? '--' : p.cpu.toFixed(1) + '%'}</td><td class="num">${bytes(p.mem)}</td>
  <td class="num">${mbs(p.disk)}</td><td class="num">${p.gpu == null ? '--' : p.gpu.toFixed(1) + '%'}</td><td>${esc(p.gpuEngine ?? '')}</td>`;
function appsPage() {
  const s = snap;
  const total = {
    cpu: s?.cpu.usage, mem: s && ratio(s.mem.used, s.mem.total),
    disk: s?.disks.some((d) => d.busy != null) ? Math.max(...s.disks.map((d) => d.busy ?? 0)) : null,
    gpu: s?.gpus.some((g) => g.usage != null) ? Math.max(...s.gpus.map((g) => g.usage ?? 0)) : null,
  };
  const th = (key, t, cls = '') => `<th class="${cls}"${key ? ` data-sort="${key}"` : ''}>${key in total ? `<b>${pct(total[key])}</b>` : ''}${t}${sort.key === key ? (sort.desc ? ' ▼' : ' ▲') : ''}</th>`;
  const head = `<thead><tr>${th('name', '名稱', 'grow')}${th('', '')}${th('cpu', 'CPU', 'num')}${th('mem', '記憶體', 'num')}${th('disk', '磁碟', 'num')}${th('gpu', 'GPU', 'num')}${th('', 'GPU 引擎')}</tr></thead>`;
  const body = APP_SECTIONS.map(([kind, title]) => {
    const groups = procGroups(kind).sort(bySort);
    if (!groups.length) return '';
    const rows = groups.map((g) => {
      const many = g.list.length > 1, open = many && appOpen.has(g.key);
      const name = `${many ? `<span class="tw">${icon(open ? 'open' : 'closed')}</span>` : '<span class="tw"></span>'}${appIcon(g.ic)}${esc(g.name)}${many ? ` (${g.list.length})` : ''}`;
      const kids = open ? g.list.sort(bySort).map((p) => `<tr class="kid" title="PID ${p.pid}"><td class="grow">${appIcon(p.name)}${esc(p.name)}</td>${procCells(p)}</tr>`).join('') : '';
      return `<tr${many ? ` data-group="${esc(g.key)}"` : ''} title="${many ? esc(g.key) : `PID ${g.pid}`}"><td class="grow">${name}</td>${procCells(g)}</tr>${kids}`;
    }).join('');
    return `<tbody class="sec"><tr><td colspan="7">${title} (${groups.length})</td></tr></tbody><tbody>${rows}</tbody>`;
  }).join('');
  return card(`共 ${procs.length} 個程序`, `<table class="zebra apps">${head}${body || '<tbody><tr><td class="muted">讀取程序中…</td></tr></tbody>'}</table>`);
}

// ── pages ──
// Desktop widget (Plasma): its config lives in widget.json, read by `resourcewatch --widget`.
// what the panel can show, default order (same as widget.rs PANEL_ITEMS)
const PANEL_ITEMS = ['cpu', 'gpu', 'mem', 'net', 'disk'];
const WIDGET_ITEMS = { cpu: '處理器', mem: '記憶體', gpu: '顯示卡', net: '網路', disk: '磁碟', temp: '溫度' };
let wcfg = null, wErr = '';
// the opacity slider's 0–100% (5% steps) maps onto 0–40% real transparency, i.e. opacity 100–60 in 2% steps
const W_MAX_TRANS = 40;
// while the slider is held the settings page is not re-rendered (that would replace the slider mid-drag)
let wSliding = false;
// panel chip being dragged in the settings: { key, x, el, moved }; `moved` suppresses the click that follows
let wDrag = null;
// settings dropdown (the .pick native-select button); data-<attr>-sel, not data-<attr>, so clicking it is not a click action
const dropdown = (attr, key, cur, opts) => {
  const sel = opts.find(([v]) => String(v) === String(cur ?? '')) ?? opts[0];
  return `<label class="btn pick"><span class="ell">${esc(sel[1])}</span>${icon('open')}<select data-${attr}-sel="${key}">${opts.map(([v, t]) =>
    `<option value="${esc(v)}"${v === sel[0] ? ' selected' : ''}>${esc(t)}</option>`).join('')}</select></label>`;
};
const nicOpts = (s) => [['', '自動'], ...s.net.map((n) => [n.name, n.name])];
const gpuOpts = (s) => s.gpus.map((g, i) => [String(i), g.name || `GPU ${i}`]);

function widgetCard(s, row) {
  if (!wcfg) return card('桌面小工具', `<div class="empty">${esc(wErr || '讀取設定中…')}</div>`);
  const seg = (key, opts) => `<div class="seg">${opts.map(([v, t]) => `<button data-w="${key}" data-val="${esc(v)}" class="${String(wcfg[key] ?? '') === String(v) ? 'on' : ''}">${esc(t)}</button>`).join('')}</div>`;
  // shown cards in their order, then the hidden ones; the order itself is changed by long-pressing cards on the widget
  const chips = [...wcfg.items, ...Object.keys(WIDGET_ITEMS).filter((k) => !wcfg.items.includes(k))];
  return card('桌面小工具', [
    `<p class="hint">右鍵桌面 → 新增或管理小工具 → ResourceWatch</p>`,
    row('顯示項目', '點擊切換顯示／隱藏，在小工具上長按卡片調整位置', `<div class="seg">${chips.map((k) =>
      `<button data-w="item" data-val="${k}" class="${wcfg.items.includes(k) ? 'on' : ''}">${WIDGET_ITEMS[k]}</button>`).join('')}</div>`),
    row('更新間隔', '每次取樣約需 1 秒', seg('interval', [[2, '2 秒'], [5, '5 秒'], [10, '10 秒']])),
    row('網路卡', '自動：有預設閘道的網卡', dropdown('w', 'nic', wcfg.nic, nicOpts(s))),
    s.gpus.length ? row('顯示卡', '', dropdown('w', 'gpu', wcfg.gpu ?? 0, gpuOpts(s))) : '',
    row('ResourceWatch 格子', '處理器上方的圖示與名稱', seg('app', [[true, '顯示'], [false, '隱藏']])),
    row('卡片透明度', '只影響卡片底色，文字與圖表不透明', (() => {
      const t = Math.round((100 - (wcfg.opacity ?? 100)) / (W_MAX_TRANS / 100) / 5) * 5;
      return `<div class="set-ctl"><input type="range" class="wrange" data-wrange min="0" max="100" step="5" value="${t}"><span class="wrange-val">${t}%</span></div>`;
    })()),
    wErr ? `<p class="hint err">${esc(wErr)}</p>` : '',
  ].join('')) + card('面板小工具', [
    `<p class="hint">把 ResourceWatch 小工具加到面板時顯示的項目（與桌面卡片分開設定），點面板上的項目可展開該卡片</p>`,
    row('顯示項目', '點擊切換顯示／隱藏，拖動調整順序', `<div class="seg wchips">${panelChips().map((k) =>
      `<button data-w="panel" data-val="${k}" data-wchip="${k}" class="${wcfg.panel?.includes(k) ? 'on' : ''}">${WIDGET_ITEMS[k]}</button>`).join('')}</div>`),
    row('網路卡', '自動：有預設閘道的網卡', dropdown('w', 'panelNic', wcfg.panelNic, nicOpts(s))),
    s.gpus.length ? row('顯示卡', '', dropdown('w', 'panelGpu', wcfg.panelGpu ?? 0, gpuOpts(s))) : '',
  ].join(''));
}
// shown items in panel order, then the hidden ones
const panelChips = () => { const p = wcfg.panel ?? PANEL_ITEMS; return [...p, ...PANEL_ITEMS.filter((k) => !p.includes(k))]; };
async function setWidget(act, val) {
  const c = { ...wcfg, items: [...wcfg.items] };
  if (act === 'item') c.items = c.items.includes(val) ? c.items.filter((k) => k !== val) : [...c.items, val];
  else if (act === 'interval') c.interval = +val;
  else if (act === 'nic' || act === 'panelNic') c[act] = val || null;
  else if (act === 'gpu' || act === 'panelGpu') c[act] = +val;
  else if (act === 'app') c.app = val === 'true';
  else if (act === 'panel') { const p = c.panel ?? [...PANEL_ITEMS]; c.panel = p.includes(val) ? p.filter((k) => k !== val) : [...p, val]; }
  else if (act === 'panelOrder') c.panel = val;
  else if (act === 'opacity') c.opacity = +val;
  try { wcfg = await invoke('set_widget_config', { cfg: c }); wErr = ''; } catch (e) { wErr = `儲存失敗：${e}`; }
  render();
}

const PAGES = {
  dash: ['儀表板', (s) => `<div class="masonry">${[summaryCard(s), cpuCard(s), memCard(s), netCard(s), disksCard(s), gpuCard(s), ioCard(s), sensorsCard(s)].join('')}</div>`],

  apps: ['應用程式', () => appsPage()],

  storage: ['儲存', () => storagePage()],

  settings: ['設定', (s) => {
    const seg = (key, opts) => `<div class="seg">${opts.map(([v, t]) => `<button data-pref="${key}" data-val="${esc(v)}" class="${String(prefs[key]) === String(v) ? 'on' : ''}">${esc(t)}</button>`).join('')}</div>`;
    const row = (t, sub, ctl) => `<div class="set-row"><p>${t}${sub ? `<span>${sub}</span>` : ''}</p>${ctl}</div>`;
    return `<div class="stack">${card('一般', [
      row('主題', '', seg('theme', [['system', '跟隨系統'], ['dark', '深色'], ['light', '淺色']])),
      row('更新間隔', `曲線保留最近 ${N} 筆`, seg('interval', [[500, '0.5 秒'], [1000, '1 秒'], [2000, '2 秒'], [5000, '5 秒']])),
      row('溫度單位', '', seg('unit', [['c', '°C'], ['f', '°F']])),
      row('儀表板網路卡', '自動：有預設閘道的網卡', dropdown('pref', 'nic', prefs.nic, nicOpts(s))),
      s.gpus.length ? row('儀表板顯示卡', '', dropdown('pref', 'gpu', prefs.gpu, gpuOpts(s))) : '',
      row('開發者模式', '開啟後按 F12 或 Ctrl+Shift+I 開啟檢查器', `<div class="set-ctl">${prefs.dev ? '<button class="btn" data-devtools>開啟檢查器</button>' : ''}${seg('dev', [['', '關閉'], ['1', '開啟']])}</div>`),
    ].join(''))}${widgetCard(s, row)}${card('關於', `<div class="about">ResourceWatch 0.4.1 · MIT License<br>${esc(info?.os || '')} · Linux ${esc(info?.kernel || '')}<br>
      圖示：Font Awesome Free 7.3.1（CC BY 4.0／SIL OFL 1.1／MIT）</div>`)}</div>`;
  }],

  net: ['網路', (s) => {
    if (!s.net.length) return `<div class="stack">${card('網路', '<div class="empty">沒有實體網路介面</div>')}${connCard()}</div>`;
    return `<div class="stack">${s.net.map((n) => {
      const sub = [n.model, n.driver && `${{ true: '開源驅動', false: '專有驅動' }[n.driverOpen] ?? '驅動程式'} ${n.driver}`, n.mtu && `MTU ${n.mtu}`].filter(Boolean).join(' · ');
      const title = `<span class="nic-title"><span>${esc(n.name)}</span>${sub ? `<span class="spec ell" title="${esc(sub)}">${esc(sub)}</span>` : ''}</span>`;
      const link = n.wifi ? `<span class="spec">${esc(n.wifi.ssid)}${n.wifi.signal != null ? ` · ${n.wifi.signal} dBm` : ''}</span>` : n.speed ? `<span class="spec">${n.speed >= 1000 ? `${n.speed / 1000} Gbps` : `${n.speed} Mbps`}${{ full: ' 全雙工', half: ' 半雙工' }[n.duplex] ?? ''}</span>` : '';
      const dot = `<i class="spacer"></i>${n.up ? link : ''}<i class="dot ${n.up ? 'c-green' : 'c-red'}"></i>`;
      // share of link capacity; speed is Mb/s, rates are bytes/s
      const use = (bps) => (n.speed && bps != null ? ` · 頻寬 ${pct((bps * 8) / (n.speed * 1e4))}` : '');
      const r = (dir, cls, l, bps, total) => `<div class="rate ${cls}">${icon(dir)}<b>${rate(bps)}</b><span class="muted">${l} · 累計 ${bytes(total)}${use(bps)}</span></div>`;
      if (!n.up) return `<section class="card iface down"><h2>${title}${dot}</h2></section>`;
      return `<section class="card iface"><h2>${title}${dot}</h2><div class="speeds">${addrBlock(n, s.dns)}<div><div class="rates-row">
          ${r('down', 'rx', '下載', n.rxBps, n.rxTotal)}${r('up', 'tx', '上傳', n.txBps, n.txTotal)}</div>
        ${mirror({ data: hist[`tx:${n.name}`], cls: 'c-magenta' }, { data: hist[`rx:${n.name}`], cls: 'c-blue' }, true)}</div></div><div class="details">${n.wifi ? wifiDetails(n.wifi) : ''}${nicDetails(n)}</div></section>`;
    }).join('')}${connCard()}</div>`;
  }],

  sensors: ['感測器', (s) => {
    const devs = sensorGroups(s);
    // hint for sensors that need a kernel module the user has to load (root)
    const has = (re) => s.sensors.some((x) => re.test(x.chip));
    // no mainboard chip (it87/nct6775), or SATA disks without drivetemp
    const needModules = !has(/^(it8|nct)/) || (!has(/^drivetemp$/) && s.disks.some((d) => d.name.startsWith('sd')));
    const btns = [needModules && ['modules', '更多感測器'], s.raplLocked && ['rapl', 'CPU功耗']].filter(Boolean);
    const hint = (btns.length ? `<p class="hint"><span>載入需要管理員權限的感測器</span>${btns.map(([act, label]) =>
      `<button class="btn" data-elevate="${act}"${elev.busy ? ' disabled' : ''}>${elev.busy === act ? '等待授權…' : label}</button>`).join('')}</p>` : '')
      + (elev.err ? `<p class="hint err">${esc(elev.err)}</p>` : '');
    if (!devs.size) return card('感測器', '<div class="empty">沒有可讀取的感測器</div>') + hint;
    const cards = (xs) => xs.map(([dev, list]) => card(`${icon(sensorIcon(list[0].chip))}<span class="ell" title="${esc(dev)}">${esc(dev)}</span>`, sensorBody(dev, list)));
    // all disks share one "磁碟" card (a section per disk) so they stay together inside the masonry
    const isDisk = ([, list]) => sensorIcon(list[0].chip) === 'storage';
    const all = [...devs], disks = all.filter(isDisk);
    const diskCard = disks.length ? card(`${icon('storage')}磁碟`, disks.map(([dev, list], i) =>
      `<div${i ? ' class="sect"' : ''}><h3 class="sub">${icon('storage')}<span class="ell" title="${esc(dev)}">${esc(dev)}</span></h3>${sensorBody(dev, list)}</div>`).join('')) : '';
    return hint + evenCols([...cards(all.filter((d) => !isDisk(d))), ...(diskCard ? [diskCard] : [])]);
  }],
};

// ── storage analyzer ──
const du = { path: null, root: null, levels: [], busy: false, progress: 0, err: null, started: false };
const SIZE_STEPS = [1e8, 2.5e8, 5e8, 1e9, 2.5e9, 5e9, 1e10, 2e10, 5e10];
const sizeClass = (b) => `s${SIZE_STEPS.filter((t) => b >= t).length}`;

function storagePage() {
  // A scanned mount point gets its disk's name and the filesystem's capacity.
  const mnt = snap.mounts.find((m) => m.mount === du.path);
  const dsk = mnt && snap.disks.find((d) => d.mounts.includes(mnt.mount));
  const title = dsk ? diskName(dsk) : du.path ?? '尚未選擇資料夾';
  const stats = du.busy ? [`掃描中… 已掃描 ${du.progress.toLocaleString()} 項`]
    : du.root ? [`${du.root.items.toLocaleString()} 項`, `已用 ${bytes(du.root.size)}`, ...(mnt ? [`總容量 ${bytes(mnt.total)}`, `可用 ${bytes(mnt.total - mnt.used)}`] : [])] : [];
  const head = `<div class="du-head">${icon(dsk ? 'storage' : 'folder', 'folder')}<div class="path"><b title="${esc(title)}">${esc(title)}</b>
      ${dsk ? `<span title="${esc(mnt.mount)}">${esc(mnt.mount)}</span>` : ''}<p class="muted">${stats.map((t) => `<i>${t}</i>`).join(' · ')}</p></div>
      <div class="du-actions"><label class="btn pick">${icon('storage')}<span class="ell">${esc(dsk ? diskName(dsk) : '選擇磁碟…')}</span><select data-du="disk"${du.busy ? ' disabled' : ''}><option value="" disabled${dsk ? '' : ' selected'}>選擇磁碟…</option>${snap.disks.flatMap((d) => d.mounts
        .filter((m) => !/^\/(boot|efi)(\/|$)/.test(m)) // boot partitions aren't worth analyzing
        .map((m) => `<option value="${esc(m)}"${m === mnt?.mount ? ' selected' : ''}>${esc(diskName(d))} · ${esc(m)}</option>`)).join('')}</select></label>
      <button class="btn" data-du="pick"${du.busy ? ' disabled' : ''}>${icon('pick')}選擇資料夾…</button>
      ${du.busy ? `<button class="btn" data-du="cancel" title="取消掃描">${icon('cancel')}</button>`
        : `<button class="btn" data-du="rescan" title="重新掃描"${du.path ? '' : ' disabled'}>${icon('refresh')}</button>`}</div></div>`;
  if (!du.levels.length) return `${head}${du.err ? `<div class="err">${esc(du.err)}</div>` : ''}`;
  const rows = du.levels.map((lv, i) => {
    const shown = lv.children.filter((c) => c.size > 0).slice(0, 40);
    const rest = lv.children.slice(shown.length).reduce((t, c) => t + c.size, 0);
    const blocks = shown.map((c, j) => `<div class="blk ${sizeClass(c.size)}${lv.sel === j ? ' sel' : ''}" data-grow="${c.size}" data-lvl="${i}" data-idx="${j}" title="${esc(c.name)} · ${bytes(c.size)}"><span>${esc(c.name)}</span></div>`).join('');
    const restBlk = rest > 0 ? `<div class="blk rest" data-grow="${rest}" title="其他 · ${bytes(rest)}"><span>其他</span></div>` : '';
    return `${i ? '<div class="down-arrow">▼</div><div></div>' : ''}<div class="lvl-name">${icon(i === 0 && dsk ? 'storage' : 'folder')}<span>${esc(lv.name)}</span>${i === 0 && mnt ? `<small class="muted">總容量 ${bytes(mnt.total)}</small>` : ''}</div>
      <div class="strip">${blocks}${restBlk || (blocks ? '' : '<div class="empty">空資料夾</div>')}</div>`;
  }).join('');
  const cur = du.levels.at(-1);
  const files = cur.children.slice(0, 200).map((c) => `<tr class="${c.dir ? 'dir' : ''}" data-path="${esc(c.path)}" data-name="${esc(c.name)}">
      <td class="grow">${icon(c.dir ? 'folder' : 'file')}${esc(c.name)}</td><td class="num">${bytes(c.size)}</td></tr>`).join('');
  const scale = `<div class="scale"><div>${SIZE_STEPS.concat(0).map((_, i) => `<i class="s${i}"></i>`).join('')}</div><p><span>100 MB</span><span>1 GB</span><span>50 GB</span></p></div>`;
  return `${head}${du.err ? `<div class="err">${esc(du.err)}</div>` : ''}<div class="du">
    <section class="card"><div class="levels">${rows}</div>${scale}</section>
    <section class="card"><table class="zebra files"><thead><tr><th>名稱</th><th class="num">大小</th></tr></thead>${files || '<tr><td class="muted">空資料夾</td></tr>'}</table></section></div>`;
}

async function duOpen(path, name, depth) {
  const children = await invoke('dir_children', { path });
  du.levels.length = depth;
  du.levels.push({ path, name, children, sel: null });
}

async function duScan(path) {
  // Drop the old result right away so a long scan never shows the previous disk.
  Object.assign(du, { path, root: null, levels: [], busy: true, progress: 0, err: null });
  render();
  const name = path.split('/').filter(Boolean).at(-1) ?? '/';
  // While the backend walks, draw the first level from the sizes found so far.
  const poll = setInterval(async () => {
    const p = await invoke('scan_progress');
    if (!du.busy) return;
    du.progress = p.items;
    du.levels = [{ path, name, children: p.top, sel: null }];
    if (page === 'storage') render();
  }, 500);
  try {
    du.root = await invoke('scan_dir', { path });
    du.levels = [];
    await duOpen(du.root.path, du.root.path.split('/').filter(Boolean).at(-1) ?? '/', 0);
  } catch (e) {
    du.err = String(e);
    du.levels = []; // partial sizes: the cache behind them is still the previous scan's
  }
  clearInterval(poll);
  du.busy = false;
  render();
}

async function duClick(t) {
  const blk = t.closest('.blk[data-lvl]');
  const row = t.closest('tr.dir');
  const btn = t.closest('[data-du]');
  if (btn?.dataset.du === 'cancel') return invoke('scan_cancel');
  if (du.busy) return; // partial results: nothing to open until the scan finishes
  if (btn?.dataset.du === 'rescan') return duScan(du.path);
  if (btn?.dataset.du === 'pick') {
    const dir = await invoke('plugin:dialog|open', { options: { directory: true, defaultPath: du.path ?? info?.home } }).catch((e) => { du.err = String(e); return null; });
    if (dir) duScan(dir); else render();
    return;
  }
  if (blk) {
    const i = +blk.dataset.lvl, j = +blk.dataset.idx, c = du.levels[i].children[j];
    du.levels[i].sel = j;
    du.levels.length = i + 1;
    if (c.dir) await duOpen(c.path, c.name, i + 1);
    render();
  } else if (row) {
    const lv = du.levels.at(-1);
    lv.sel = lv.children.findIndex((c) => c.path === row.dataset.path);
    await duOpen(row.dataset.path, row.dataset.name, du.levels.length);
    render();
  }
}

// ── shell ──
function hostBar() {
  const logo = info?.logo ? `<img class="logo" src="${esc(info.logo)}" alt="">` : `<i class="logo fa-brands fa-${BRAND[info?.osId] ?? 'linux'}"></i>`;
  $('#host').innerHTML = `${logo}
    <div class="hcol"><b>${esc(info?.hostname || '--')}</b><span>運作時間：<em>${duration(snap?.uptime)}</em></span></div>
    <div class="hcol"><b>${esc(info?.os || '--')}</b><span>${esc(info?.desktop || '')}</span></div>`;
}

function render() {
  if (snap) {
    $('#content').innerHTML = PAGES[page][1](snap);
    // Proportional widths through CSSOM: the CSP blocks style attributes in HTML.
    const blocks = [...document.querySelectorAll('[data-grow]')];
    blocks.forEach((el) => { el.style.flexGrow = el.dataset.grow; });
    document.querySelectorAll('[data-h]').forEach((el) => { el.style.height = `${el.dataset.h}%`; });
    blocks.forEach((el) => { if (el.clientWidth < 40) el.textContent = ''; }); // too narrow for a readable name (title still has it)
  }
  hostBar();
}

function go(p) {
  page = p;
  document.querySelectorAll('.nav-item[data-page]').forEach((b) => b.classList.toggle('active', b.dataset.page === p));
  $('#content').scrollTop = 0;
  if (p === 'storage' && !du.started && info) { du.started = true; duScan(info.home); }
  // the desktop widget rewrites the order after a drag, so re-read before showing the settings
  if (p === 'settings') invoke('get_widget_config').then((c) => { wcfg = c; render(); }).catch(() => {});
  pollProcs();
  render();
}

// prefs: frontend-only display settings in localStorage
try { prefs = { ...DEFAULTS, ...JSON.parse(store('rw-prefs') || '{}') }; } catch { /* corrupt → defaults */ }
let timer = null;
function setPref(key, val) {
  prefs[key] = key === 'interval' ? +val : val;
  if (key === 'nic' || key === 'gpu') pick[key] = null; // a new default replaces the card's ‹ › choice
  store('rw-prefs', JSON.stringify(prefs));
  applyPrefs();
  render();
}
// evenCols picks the column count from the width, so the sensors page re-lays out on resize
addEventListener('resize', () => { if (page === 'sensors') render(); });
const media = matchMedia('(prefers-color-scheme: dark)');
function applyPrefs() {
  document.documentElement.dataset.theme = prefs.theme === 'system' ? (media.matches ? 'dark' : 'light') : prefs.theme;
  const [ic, label] = { system: ['auto', '跟隨系統'], dark: ['moon', '深色'], light: ['sun', '淺色'] }[prefs.theme] ?? ['auto', ''];
  $('#theme').innerHTML = icon(ic);
  $('#theme').title = `主題：${label}`;
  clearInterval(timer);
  timer = setInterval(tick, [500, 1000, 2000, 5000].includes(prefs.interval) ? prefs.interval : 1000);
  invoke('set_dev_mode', { on: !!prefs.dev }).catch(() => {});
  setPaused(paused);
}
media.addEventListener('change', applyPrefs);
$('#theme').addEventListener('click', () => setPref('theme', { system: 'dark', dark: 'light', light: 'system' }[prefs.theme] ?? 'system'));
document.querySelector('.nav-foot .nav-item').addEventListener('click', () => go('settings'));

$('#nav').classList.toggle('collapsed', store('rw-collapsed') === '1');
$('#collapse').addEventListener('click', () => store('rw-collapsed', $('#nav').classList.toggle('collapsed') ? '1' : '0'));

$('#pages').innerHTML = ['dash', 'apps', 'storage', 'net', 'sensors'].map((k) =>
  `<button class="nav-item${k === page ? ' active' : ''}" data-page="${k}" title="${PAGES[k][0]}">${icon(k)}<span class="label">${PAGES[k][0]}</span></button>`).join('');
$('#pages').addEventListener('click', (e) => { const b = e.target.closest('[data-page]'); if (b) go(b.dataset.page); });

$('#content').addEventListener('change', (e) => {
  if (e.target.dataset.du === 'disk' && e.target.value) duScan(e.target.value);
  if (e.target.dataset.prefSel) setPref(e.target.dataset.prefSel, e.target.value);
  if (e.target.dataset.wSel) setWidget(e.target.dataset.wSel, e.target.value);
  if (e.target.matches('[data-wrange]')) { wSliding = false; setWidget('opacity', 100 - (+e.target.value * W_MAX_TRANS) / 100); }
});
$('#content').addEventListener('pointerdown', (e) => {
  if (e.target.matches('[data-wrange]')) wSliding = true;
  const chip = e.target.closest('[data-wchip]');
  if (chip && e.button === 0) { wDrag = { key: chip.dataset.wchip, x: e.clientX, el: chip, moved: false }; wSliding = true; chip.setPointerCapture(e.pointerId); }
});
// Panel chips are reordered with pointer events (no HTML5 drag and drop: the webview's drop handling is taken
// by Tauri): the chip follows the pointer horizontally, the chip under the pointer is marked, and on release the
// dragged one moves to that chip's place; hidden chips keep their relative order after the shown ones.
$('#content').addEventListener('pointermove', (e) => {
  if (!wDrag?.el || !(e.buttons & 1)) return; // after release wDrag only remembers `moved` for the click
  const dx = e.clientX - wDrag.x;
  if (!wDrag.moved && Math.abs(dx) < 4) return;
  wDrag.moved = true;
  wDrag.el.classList.add('dragging');
  wDrag.el.style.transform = `translateX(${dx}px)`;
  const over = document.elementsFromPoint(e.clientX, e.clientY).find((x) => x.dataset?.wchip && x !== wDrag.el);
  document.querySelectorAll('[data-wchip].drop').forEach((x) => x !== over && x.classList.remove('drop'));
  over?.classList.add('drop');
});
$('#content').addEventListener('pointerup', (e) => {
  wSliding = false;
  if (!wDrag?.el) return;
  if (!wDrag.moved) { wDrag = null; return; } // a plain click: the click handler toggles
  const over = document.elementsFromPoint(e.clientX, e.clientY).find((x) => x.dataset?.wchip && x !== wDrag.el);
  const key = wDrag.key;
  wDrag = { moved: true }; // the click that follows this release is not a toggle
  const order = panelChips();
  if (over) {
    const from = order.indexOf(key), to = order.indexOf(over.dataset.wchip);
    order.splice(from, 1);
    order.splice(to, 0, key);
    const shown = new Set(wcfg.panel ?? PANEL_ITEMS);
    setWidget('panelOrder', order.filter((k) => shown.has(k)));
  } else render();
});
$('#content').addEventListener('input', (e) => {
  if (e.target.matches('[data-wrange]')) { wSliding = true; e.target.nextElementSibling.textContent = `${e.target.value}%`; }
});
$('#content').addEventListener('click', (e) => {
  const t = e.target;
  if (page === 'storage') return duClick(t);
  const link = t.closest('[data-goto]');
  if (link) return go(link.dataset.goto);
  if (t.closest('[data-devtools]')) return invoke('open_devtools');
  const raw = t.closest('summary[data-raw]');
  if (raw) { e.preventDefault(); const d = raw.dataset.raw; if (!rawOpen.delete(d)) rawOpen.add(d); return render(); }
  const el = t.closest('[data-elevate]');
  if (el) return elev.busy || elevate(el.dataset.elevate);
  const w = t.closest('[data-w]');
  if (w && w.dataset.wchip && wDrag?.moved) { wDrag = null; return; } // the end of a chip drag, not a toggle
  if (w) return setWidget(w.dataset.w, w.dataset.val);
  const pref = t.closest('[data-pref]');
  if (pref) return setPref(pref.dataset.pref, pref.dataset.val);
  const flip = t.closest('[data-disk]');
  if (flip) { sumDisk += +flip.dataset.disk; return render(); }
  const nr = t.closest('[data-netreset]');
  if (nr) {
    const n = snap?.net.find((x) => x.name === nr.dataset.netreset);
    if (n) { netBase[n.name] = { rxTotal: n.rxTotal, txTotal: n.txTotal }; store('rw-netbase', JSON.stringify(netBase)); }
    return render();
  }
  const pk = t.closest('[data-pick]');
  if (pk) { pick[pk.dataset.pick] = pk.dataset.val; return render(); }
  const grp = t.closest('[data-group]');
  if (grp) { const k = grp.dataset.group; if (!appOpen.delete(k)) appOpen.add(k); return render(); }
  const th = t.closest('th[data-sort]');
  if (!th) return;
  const key = th.dataset.sort;
  sort = sort.key === key ? { key, desc: !sort.desc } : { key, desc: key !== 'name' };
  render();
});

// Developer mode: pausing stops the per-tick re-render so the inspector's element tree stays put.
let paused = false;
function setPaused(on) {
  paused = on && !!prefs.dev;
  $('#pause').hidden = !prefs.dev;
  $('#pause').innerHTML = `<i class="fa-solid ${paused ? 'fa-play' : 'fa-pause'}"></i>`;
  $('#pause').title = paused ? '繼續更新（F8）' : '暫停更新（F8）';
}
$('#pause').addEventListener('click', () => setPaused(!paused));
document.addEventListener('keydown', (e) => {
  if (!prefs.dev) return;
  if (e.key === 'F12' || (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === 'i')) { e.preventDefault(); invoke('open_devtools'); }
  if (e.key === 'F8') { e.preventDefault(); setPaused(!paused); }
});

async function tick() {
  if (paused) return;
  try {
    snap = await invoke('snapshot');
    record(snap);
    // The analyzer and the process table only change on their own events.
    // an open dropdown would close if the page were redrawn under it
    if (page === 'storage' || page === 'apps' || wSliding || document.activeElement?.matches('#content select')) hostBar(); else render();
  } catch (e) {
    console.error(e);
  }
}

async function pollProcs() {
  if (document.hidden || paused) return;
  if (page === 'net') {
    try {
      const r = await invoke('connections');
      conns = r.list;
      const p = tcp?.prev, dOut = p && r.outSegs - p.outSegs;
      tcp = { states: r.tcpStates, prev: r, retrans: dOut > 0 ? (100 * (r.retransSegs - p.retransSegs)) / dOut : dOut === 0 ? 0 : null };
      loadIcons(conns.map((c) => c.name));
      render();
    } catch (e) {
      console.error(e);
    }
    return;
  }
  if (page !== 'dash' && page !== 'apps') return;
  try {
    procs = await invoke('processes');
    loadIcons(page === 'apps' ? procs.flatMap((p) => (p.app ? [p.app, p.name] : [p.name])) : [...topProcs('cpu', 5), ...topProcs('mem', 5)].map((p) => p.name));
    if (page === 'apps') render();
  } catch (e) {
    console.error(e);
  }
}

applyPrefs();
invoke('system_info').then((i) => { info = i; hostBar(); }).catch(console.error);
invoke('get_widget_config').then((c) => { wcfg = c; }).catch((e) => { wErr = `無法讀取小工具設定：${e}`; });
tick();
pollProcs();
setInterval(pollProcs, 2000);
