'use strict';
// Polls the Rust commands and re-renders the current page. Missing values are null → "--".
const invoke = window.__TAURI__?.core.invoke ?? window.mockInvoke;
const N = 60; // line-chart points (1 per sample)
const NB = 120; // samples kept; bar charts show all of them, line charts the last N
const hist = {};
let snap = null;
let info = null;
let procs = [];
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
const DEFAULTS = { theme: 'system', interval: 1000, unit: 'c', nic: '', dev: '' };
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
  pc: 'fa-solid fa-desktop', router: 'fa-solid fa-network-wired', down: 'fa-solid fa-arrow-down', up: 'fa-solid fa-arrow-up', eth: 'fa-solid fa-ethernet', wifi: 'fa-solid fa-wifi',
  folder: 'fa-solid fa-folder', pick: 'fa-solid fa-folder-open', file: 'fa-regular fa-file', refresh: 'fa-solid fa-rotate-right', gpu: 'fa-solid fa-microchip', app: 'fa-regular fa-window-maximize', reset: 'fa-solid fa-rotate-left',
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
const primaryNet = (s) => s.net.find((n) => n.name === prefs.nic) ?? s.net.find((n) => n.gateway) ?? s.net.find((n) => n.up) ?? s.net[0];
const diskName = (d) => (d.model || d.name).replace(/_/g, ' '); // NVMe models use _ for spaces
// Process name → icon data: URL; null while loading or when no app matches (generic glyph then).
const appIcons = {};
const appIcon = (name) => (appIcons[name] ? `<img class="app-ic" src="${esc(appIcons[name])}" alt="">` : icon('app', 'app-ic'));
async function loadIcons(names) {
  const missing = [...new Set(names)].filter((n) => !(n in appIcons));
  if (!missing.length) return;
  missing.forEach((n) => { appIcons[n] = null; });
  try {
    Object.assign(appIcons, await invoke('app_icons', { names: missing }));
    if (page === 'dash') render();
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
  const d = disks[sumDisk], gpu = s.gpus[0]?.usage;
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
    <div class="sect"><table class="zebra">${rows || '<tr><td class="muted">讀取程序中…</td></tr>'}</table></div>${more('apps', '顯示應用程式…')}`);
}

function memCard(s) {
  const m = s.mem, parts = memParts(m);
  const rows = topProcs('mem', 5).map((p) => `<tr><td class="grow">${appIcon(p.name)}${esc(p.name)}</td><td class="num">${bytes(p.mem)}</td></tr>`).join('');
  const spec = `<i class="spacer"></i><span class="spec">${esc(info?.memSpec ?? bytes(m.total))}</span>`;
  return card('記憶體', `${bar(parts.map(([cls, , v]) => [ratio(v, m.total), cls]), true)}
    <div class="sect">${legend([...parts.map(([cls, k, v]) => [cls, k, bytes(v)]), ['c-gray', 'SWAP', `${bytes(m.swapUsed)} / ${bytes(m.swapTotal)}`]])}</div>
    <div class="sect"><table class="zebra"><thead><tr><th>名稱</th><th class="num">記憶體</th></tr></thead>${rows || '<tr><td class="muted">讀取程序中…</td></tr>'}</table></div>${more('apps', '顯示應用程式…')}`, spec);
}

function addrBlock(n) {
  return `<div class="addr">
    <div>${icon('pc')}<p><span>${esc(n.ip ?? '--')}</span><span class="muted sm">${esc(n.mac || '--')}</span></p></div>
    <div>${icon('router')}<p><span>${esc(n.gateway ?? '--')}</span><span class="muted sm">預設閘道</span></p></div></div>`;
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
  const i = Math.min(+pick.gpu || 0, s.gpus.length - 1), g = s.gpus[i];
  return card('顯示卡', `<h3 class="sub">${icon('gpu')}<span class="ell" title="${esc(g.name)}">${esc(g.name)}</span>${flipper('gpu', s.gpus.map((_, k) => String(k)), i)}<i class="spacer"></i><span class="muted sm">${temp(g.temp)}</span></h3>
      <div class="gpu-row">${ring([[[g.usage, 'c-blue']]], 96, pct(g.usage))}<div class="meters">
        ${meter('顯示記憶體', `${bytes(g.memUsed)} / ${bytes(g.memTotal)}`, ratio(g.memUsed, g.memTotal), 'c-yellow')}
        ${meter('編碼器', pct(g.encoder), g.encoder, 'c-green')}
        ${meter('解碼器', pct(g.decoder), g.decoder, 'c-magenta')}</div></div>
      <div class="sect">${cap('使用率', '')}${chart([{ data: hist[`gpu${i}`], cls: 'c-blue' }], 100)}</div>`);
}

function sensorsCard(s) {
  const list = s.sensors.slice(0, 8);
  if (!list.length) return card('感測器', '<div class="empty">沒有可讀取的感測器</div>');
  return card('感測器', `<table>${list.map((x) => `<tr><td class="grow">${esc(x.chip)} · ${esc(x.label)}</td><td class="num">${fmtSensor(x)}</td></tr>`).join('')}</table>${s.sensors.length > 8 ? more('sensors', '顯示更多…') : ''}`);
}
const fmtSensor = (x) => (x.kind === 'temp' ? temp(x.value) : `${x.value.toFixed(0)} rpm`);

function ioCard(s) {
  const d = s.disks.find((x) => x.name === pick.io) ?? s.disks[0];
  if (!d) return card('磁碟 I/O', '<div class="empty">沒有實體磁碟</div>');
  return card('磁碟 I/O', `<h3 class="sub">${icon('storage')}<span class="ell" title="${esc(d.name)}">${esc(diskName(d))}</span>${flipper('io', s.disks.map((x) => x.name), s.disks.indexOf(d))}</h3>
    ${flow(['讀取', `r:${d.name}`], ['寫入', `w:${d.name}`])}`);
}

// ── pages ──
const PAGES = {
  dash: ['儀表板', (s) => `<div class="masonry">${[summaryCard(s), cpuCard(s), memCard(s), netCard(s), disksCard(s), gpuCard(s), ioCard(s), sensorsCard(s)].join('')}</div>`],

  apps: ['應用程式', () => {
    const k = sort.key, dir = sort.desc ? -1 : 1;
    const list = [...procs].sort((a, b) => {
      const x = a[k], y = b[k];
      if (x == null || y == null) return (x == null) - (y == null); // missing CPU% always last
      return (typeof x === 'string' ? x.localeCompare(y) : x - y) * dir || a.pid - b.pid;
    }).slice(0, 300);
    const th = (key, t, cls = '') => `<th class="${cls}" data-sort="${key}">${t}${sort.key === key ? (sort.desc ? ' ▼' : ' ▲') : ''}</th>`;
    const rows = list.map((p) => `<tr><td class="grow">${esc(p.name)}</td><td class="num">${p.pid}</td><td class="num">${p.cpu == null ? '--' : p.cpu.toFixed(1) + '%'}</td><td class="num">${bytes(p.mem)}</td></tr>`).join('');
    return card(`共 ${procs.length} 個程序`, `<table class="zebra"><thead><tr>${th('name', '名稱')}${th('pid', 'PID', 'num')}${th('cpu', 'CPU', 'num')}${th('mem', '記憶體', 'num')}</tr></thead>${rows}</table>`);
  }],

  storage: ['儲存', () => storagePage()],

  settings: ['設定', (s) => {
    const seg = (key, opts) => `<div class="seg">${opts.map(([v, t]) => `<button data-pref="${key}" data-val="${esc(v)}" class="${String(prefs[key]) === String(v) ? 'on' : ''}">${esc(t)}</button>`).join('')}</div>`;
    const row = (t, sub, ctl) => `<div class="set-row"><p>${t}${sub ? `<span>${sub}</span>` : ''}</p>${ctl}</div>`;
    return `<div class="stack">${card('一般', [
      row('主題', '', seg('theme', [['system', '跟隨系統'], ['dark', '深色'], ['light', '淺色']])),
      row('更新間隔', `曲線保留最近 ${N} 筆`, seg('interval', [[500, '0.5 秒'], [1000, '1 秒'], [2000, '2 秒'], [5000, '5 秒']])),
      row('溫度單位', '', seg('unit', [['c', '°C'], ['f', '°F']])),
      row('儀表板網路卡', '自動：有預設閘道的網卡', seg('nic', [['', '自動'], ...s.net.map((n) => [n.name, n.name])])),
      row('開發者模式', '開啟後按 F12 或 Ctrl+Shift+I 開啟檢查器', `<div class="set-ctl">${prefs.dev ? '<button class="btn" data-devtools>開啟檢查器</button>' : ''}${seg('dev', [['', '關閉'], ['1', '開啟']])}</div>`),
    ].join(''))}${card('關於', `<div class="about">ResourceWatch 1.0.0 · MIT License<br>${esc(info?.os || '')} · Linux ${esc(info?.kernel || '')}<br>
      圖示：Font Awesome Free 7.3.1（CC BY 4.0／SIL OFL 1.1／MIT）</div>`)}</div>`;
  }],

  net: ['網路', (s) => {
    if (!s.net.length) return card('網路', '<div class="empty">沒有實體網路介面</div>');
    return `<div class="stack">${s.net.map((n) => {
      const title = `${esc(n.name)}`;
      const dot = `<i class="spacer"></i><i class="dot ${n.up ? 'c-green' : 'c-red'}"></i>`;
      if (!n.up) return `<section class="card iface down"><h2>${title}${dot}</h2></section>`;
      return `<section class="card iface"><h2>${title}${dot}</h2>${addrBlock(n)}<div class="speeds"><div>
          <div class="rate">${icon('down')}<b><i class="dot c-blue"></i>${rate(n.rxBps)}</b><span class="muted">${bytes(n.rxTotal)}</span></div>
          <div class="rate">${icon('up')}<b><i class="dot c-magenta"></i>${rate(n.txBps)}</b><span class="muted">${bytes(n.txTotal)}</span></div></div>
        <div>${cap('速度', '')}${mirror({ data: hist[`tx:${n.name}`], cls: 'c-magenta' }, { data: hist[`rx:${n.name}`], cls: 'c-blue' }, true)}</div></div></section>`;
    }).join('')}</div>`;
  }],

  sensors: ['感測器', (s) => {
    const chips = new Map();
    s.sensors.forEach((x) => chips.set(x.chip, [...(chips.get(x.chip) ?? []), x]));
    if (!chips.size) return card('感測器', '<div class="empty">沒有可讀取的感測器</div>');
    return `<div class="grid">${[...chips].map(([chip, list]) => card(esc(chip), `<table class="zebra">${list.map((x) =>
      `<tr><td class="grow">${esc(x.label)}</td><td class="num">${fmtSensor(x)}</td></tr>`).join('')}</table>`)).join('')}</div>`;
  }],
};

// ── storage analyzer ──
const du = { root: null, levels: [], busy: false, err: null, started: false };
const SIZE_STEPS = [1e8, 2.5e8, 5e8, 1e9, 2.5e9, 5e9, 1e10, 2e10, 5e10];
const sizeClass = (b) => `s${SIZE_STEPS.filter((t) => b >= t).length}`;

function storagePage() {
  const head = `<div class="du-head">${icon('folder', 'folder')}<div class="path"><b>${esc(du.root?.path ?? '尚未選擇資料夾')}</b>
      <span class="muted">${du.busy ? '掃描中…' : du.root ? `項目：${du.root.items.toLocaleString()} · ${bytes(du.root.size)}` : ''}</span></div>
      <button class="btn" data-du="pick"${du.busy ? ' disabled' : ''}>${icon('pick')}選擇資料夾…</button>
      <button class="btn" data-du="rescan"${du.busy || !du.root ? ' disabled' : ''}>${icon('refresh')}</button></div>`;
  if (!du.levels.length) return `${head}${du.err ? `<div class="err">${esc(du.err)}</div>` : ''}`;
  const rows = du.levels.map((lv, i) => {
    const shown = lv.children.filter((c) => c.size > 0).slice(0, 40);
    const rest = lv.children.slice(shown.length).reduce((t, c) => t + c.size, 0);
    const blocks = shown.map((c, j) => `<div class="blk ${sizeClass(c.size)}${lv.sel === j ? ' sel' : ''}" data-grow="${c.size}" data-lvl="${i}" data-idx="${j}" title="${esc(c.name)} · ${bytes(c.size)}"><span>${esc(c.name)}</span></div>`).join('');
    const restBlk = rest > 0 ? `<div class="blk rest" data-grow="${rest}" title="其他 · ${bytes(rest)}"><span>其他</span></div>` : '';
    return `${i ? '<div class="down-arrow">▼</div><div></div>' : ''}<div class="lvl-name">${icon('folder')}<span>${esc(lv.name)}</span></div>
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
  du.busy = true; du.err = null; render();
  try {
    du.root = await invoke('scan_dir', { path });
    du.levels = [];
    await duOpen(du.root.path, du.root.path.split('/').filter(Boolean).at(-1) ?? '/', 0);
  } catch (e) {
    du.err = String(e);
  }
  du.busy = false;
  render();
}

async function duClick(t) {
  const blk = t.closest('.blk[data-lvl]');
  const row = t.closest('tr.dir');
  const btn = t.closest('[data-du]');
  if (btn?.dataset.du === 'rescan') return duScan(du.root.path);
  if (btn?.dataset.du === 'pick') {
    const dir = await invoke('plugin:dialog|open', { options: { directory: true, defaultPath: du.root?.path ?? info?.home } }).catch((e) => { du.err = String(e); return null; });
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
  pollProcs();
  render();
}

// prefs: frontend-only display settings in localStorage
try { prefs = { ...DEFAULTS, ...JSON.parse(store('rw-prefs') || '{}') }; } catch { /* corrupt → defaults */ }
let timer = null;
function setPref(key, val) {
  prefs[key] = key === 'interval' ? +val : val;
  if (key === 'nic') pick.nic = null; // a new default replaces the card's ‹ › choice
  store('rw-prefs', JSON.stringify(prefs));
  applyPrefs();
  render();
}
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

$('#content').addEventListener('click', (e) => {
  const t = e.target;
  if (page === 'storage') return duClick(t);
  const link = t.closest('[data-goto]');
  if (link) return go(link.dataset.goto);
  if (t.closest('[data-devtools]')) return invoke('open_devtools');
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
  const th = t.closest('th[data-sort]');
  if (!th) return;
  const key = th.dataset.sort;
  sort = sort.key === key ? { key, desc: !sort.desc } : { key, desc: key === 'cpu' || key === 'mem' };
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
    if (page === 'storage' || page === 'apps') hostBar(); else render();
  } catch (e) {
    console.error(e);
  }
}

async function pollProcs() {
  if ((page !== 'dash' && page !== 'apps') || document.hidden || paused) return;
  try {
    procs = await invoke('processes');
    loadIcons([...topProcs('cpu', 5), ...topProcs('mem', 5)].map((p) => p.name));
    if (page === 'apps') render();
  } catch (e) {
    console.error(e);
  }
}

applyPrefs();
invoke('system_info').then((i) => { info = i; hostBar(); }).catch(console.error);
tick();
pollProcs();
setInterval(pollProcs, 2000);
