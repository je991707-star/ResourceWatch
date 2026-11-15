'use strict';
// Polls the Rust commands and re-renders the current page. Missing values are null → "--".
const invoke = window.__TAURI__?.core.invoke ?? window.mockInvoke;
const N = 60; // history points (1 per second)
const hist = {};
let snap = null;
let info = null;
let procs = [];
let page = 'dash';
let sort = { key: 'cpu', desc: true };
let sumDisk = 0; // SUMMARY storage flip index

const $ = (s) => document.querySelector(s);
const esc = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
const sum = (a) => (a.length && a.every((v) => v != null) ? a.reduce((x, y) => x + y, 0) : null);
const store = (k, v) => { try { if (v === undefined) return localStorage.getItem(k); localStorage.setItem(k, v); } catch { return null; } };
const push = (k, v) => {
  const a = (hist[k] ??= []);
  a.push(v);
  if (a.length > N) a.shift();
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
const temp = (v) => (v == null ? '--' : `${v.toFixed(0)} °C`);
const ratio = (a, b) => (a == null || !b ? null : (a / b) * 100);
function duration(s) {
  if (s == null) return '--';
  const d = Math.floor(s / 86400), h = Math.floor((s % 86400) / 3600), m = Math.floor((s % 3600) / 60);
  return `${d ? `${d} 天 ` : ''}${h} 小時 ${m} 分`;
}

// ── icons (24×24 line art) ──
const ICON = {
  dash: '<rect x="3.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="3.5" y="13.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="13.5" width="7" height="7" rx="1.5"/>',
  apps: '<path d="M12 3 4.5 20M12 3l7.5 17M7.5 14h9M3 20h3M18 20h3"/>',
  storage: '<path d="M3 13.5 5.8 5.5h12.4l2.8 8v5H3zM3 13.5h18M6.5 16h1M9.5 16h1"/>',
  net: '<circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17M12 3.5c2.5 2.5 3.5 5.5 3.5 8.5s-1 6-3.5 8.5c-2.5-2.5-3.5-5.5-3.5-8.5s1-6 3.5-8.5z"/>',
  sensors: '<path d="M10 14.5V5a2 2 0 0 1 4 0v9.5a4 4 0 1 1-4 0zM12 11v6"/>',
  collapse: '<rect x="3.5" y="4.5" width="17" height="15" rx="2"/><path d="M9 4.5v15M15.5 10l-2 2 2 2"/>',
  moon: '<path d="M19.5 14.5A8 8 0 0 1 9.5 4.5a8 8 0 1 0 10 10z"/>',
  sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M5.3 18.7l1.4-1.4M17.3 6.7l1.4-1.4"/>',
  auto: '<circle cx="12" cy="12" r="8.5"/><path d="M12 3.5v17a8.5 8.5 0 0 0 0-17z" fill="currentColor"/>',
  laptop: '<rect x="5" y="4" width="14" height="10" rx="1"/><path d="M2.5 17.5h19l-1.5 1.5H4z"/>',
  pc: '<rect x="3.5" y="5" width="17" height="11" rx="1"/><path d="M8 19.5h8M12 16v3.5"/>',
  router: '<path d="M3.5 13.5h17v6h-17zM6 16.5h1M9 16.5h1M7 13.5V6M17 13.5V6M12 13.5V8"/>',
  down: '<path d="M18 6 6 18M6 9v9h9M4 21h16"/>',
  up: '<path d="M6 18 18 6M9 6h9v9M4 21h16"/>',
  folder: '<path d="M3 7.5a1.5 1.5 0 0 1 1.5-1.5h4.5l2 2h8.5a1.5 1.5 0 0 1 1.5 1.5v8.5a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z"/>',
  file: '<path d="M6 3.5h8l4 4v13H6zM14 3.5v4h4"/>',
  refresh: '<path d="M19.5 12a7.5 7.5 0 1 1-2.2-5.3M19.5 4v4h-4"/>',
};
const icon = (name, cls = '') => `<svg class="i ${cls}" viewBox="0 0 24 24">${ICON[name]}</svg>`;

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

// series: [{ data, cls }]; max fixed (percent) or auto from data. `mode`: '' line+area, 'solid' filled.
function chart(series, max, tall) {
  max = max ?? Math.max(1, ...series.flatMap((s) => (s.data ?? []).filter((v) => v != null))) * 1.15;
  const y = (v) => (100 - (Math.min(v, max) / max) * 97).toFixed(1);
  const paths = series.map(({ data = [], cls, solid }) => {
    const off = N - data.length;
    const runs = [[]];
    data.forEach((v, i) => (v == null ? runs.push([]) : runs.at(-1).push([off + i, y(v)])));
    return runs.filter((r) => r.length).map((r) => {
      const line = r.map(([x, yy], i) => `${i ? 'L' : 'M'}${x} ${yy}`).join('');
      const area = `<path class="${solid ? 'solid' : 'area'} ${cls}" d="${line}L${r.at(-1)[0]} 100L${r[0][0]} 100Z"/>`;
      return solid ? area : `${area}<path class="line ${cls}" d="${line}"/>`;
    }).join('');
  });
  const grid = [33, 66].map((g) => `<line class="grid-line" x1="0" x2="${N - 1}" y1="${g}" y2="${g}"/>`).join('');
  return `<svg class="chart${tall ? ' tall' : ''}" viewBox="0 0 ${N - 1} 100" preserveAspectRatio="none">${grid}${paths.join('')}</svg>`;
}

// rings: outer first, each a list of [percent, cls] segments.
function ring(rings, size, text) {
  const sw = 8, gap = 3.5;
  const out = rings.map((segs, k) => {
    const r = 50 - sw / 2 - k * (sw + gap);
    let start = 0;
    const flat = segs.length > 1 ? ' flat' : '';
    const arcs = segs.map(([p, cls]) => {
      const v = Math.min(Math.max(p ?? 0, 0), 99.9 - start);
      const c = v > 0 ? `<circle class="${cls}${flat}" cx="50" cy="50" r="${r}" pathLength="100" stroke-width="${sw}" stroke-dasharray="${v} 100" stroke-dashoffset="${-start}" transform="rotate(-90 50 50)"/>` : '';
      start += v;
      return c;
    }).join('');
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
const primaryNet = (s) => s.net.find((n) => n.gateway) ?? s.net.find((n) => n.up) ?? s.net[0];
const mainMounts = (s) => [...s.mounts].sort((a, b) => b.total - a.total);
const topProcs = (key, n) => [...procs].filter((p) => p[key] != null).sort((a, b) => b[key] - a[key]).slice(0, n);

function record(s) {
  push('cpu', s.cpu.usage);
  push('pressure', s.mem.pressure);
  s.gpus.forEach((g, i) => push(`gpu${i}`, g.usage));
  push('diskR', sum(s.disks.map((d) => d.readBps)));
  push('diskW', sum(s.disks.map((d) => d.writeBps)));
  s.net.forEach((n) => { push(`rx:${n.name}`, n.rxBps); push(`tx:${n.name}`, n.txBps); });
}

// ── dashboard cards ──
function summaryCard(s) {
  const m = s.mem, mounts = mainMounts(s);
  sumDisk = Math.min(sumDisk, Math.max(mounts.length - 1, 0));
  const d = mounts[sumDisk];
  const parts = memParts(m);
  const gauge = ring([
    parts.slice(0, 2).map(([cls, , v]) => [ratio(v, m.total), cls]),
    [[s.cpu.usage, 'c-magenta']],
    [[d && ratio(d.used, d.total), 'c-cyan']],
  ], 140);
  const right = `${legend(parts.map(([cls, k, v]) => [cls, k, bytes(v)]))}
    <div class="sect"><h3>${esc(info?.cpuModel || '處理器')}</h3>${kv([['核心', info?.cpuCores ?? '--'], ['執行緒', info?.cpuThreads ?? '--']])}</div>
    ${d ? `<div class="sect"><h3>${esc(d.mount)}<i class="spacer"></i>
      <button class="arrow" data-disk="-1"${sumDisk ? '' : ' disabled'}>‹</button><button class="arrow" data-disk="1"${sumDisk < mounts.length - 1 ? '' : ' disabled'}>›</button></h3>
      ${kv([['已使用', bytes(d.used)], ['總量', bytes(d.total)], ['可用', bytes(d.total - d.used)]])}</div>` : ''}`;
  return card('摘要', `<div class="summary"><div class="gauge">${gauge}<ul>
      <li><i class="dot c-yellow"></i>${bytes(m.total)}</li>
      <li><i class="dot c-magenta"></i>${pct(s.cpu.usage)}</li>
      <li><i class="dot c-cyan"></i>${pct(d && ratio(d.used, d.total))}</li></ul></div><div>${right}</div></div>`);
}

function cpuCard(s) {
  const n = s.cpu.cores.length || 1;
  const cores = `<svg class="cores" viewBox="0 0 ${n * 4} 100" preserveAspectRatio="none">${s.cpu.cores.map((v, i) =>
    `<rect class="c-blue" x="${i * 4}" y="${100 - Math.max(v ?? 0, 2)}" width="3" height="${Math.max(v ?? 0, 2)}"/>`).join('')}</svg>`;
  const rows = topProcs('cpu', 5).map((p) => `<tr><td class="grow">${esc(p.name)}</td><td class="num">${p.cpu.toFixed(1)}%</td></tr>`).join('');
  return card('處理器', `<div class="cpu-row"><div>${cap('核心', '')}${cores}</div><div>${cap(pct(s.cpu.usage), `${temp(s.cpu.temp)} · ${s.cpu.freqMhz ? (s.cpu.freqMhz / 1000).toFixed(2) + ' GHz' : '--'}`)}${chart([{ data: hist.cpu, cls: 'c-blue' }], 100)}</div></div>
    <table>${rows || '<tr><td class="muted">讀取程序中…</td></tr>'}</table>${more('apps', '顯示應用程式…')}`);
}

function memCard(s) {
  const m = s.mem, parts = memParts(m);
  const peak = Math.max(10, ...(hist.pressure ?? []).filter((v) => v != null));
  const rows = topProcs('mem', 5).map((p) => `<tr><td class="grow">${esc(p.name)}</td><td class="num">${bytes(p.mem)}</td></tr>`).join('');
  return card('記憶體', `${bar(parts.map(([cls, , v]) => [ratio(v, m.total), cls]), true)}
    <div class="sect">${legend([...parts.map(([cls, k, v]) => [cls, k, bytes(v)]), ['c-gray', '交換', `${bytes(m.swapUsed)} / ${bytes(m.swapTotal)}`]])}</div>
    <div class="sect">${cap('壓力', m.pressure == null ? '--' : `${m.pressure.toFixed(1)}%`)}${chart([{ data: hist.pressure, cls: 'c-orange', solid: true }], peak)}</div>
    <div class="sect"><table><tr><th>名稱</th><th class="num">記憶體</th></tr>${rows || '<tr><td class="muted">讀取程序中…</td></tr>'}</table></div>${more('apps', '顯示應用程式…')}`);
}

function addrBlock(n) {
  return `<div class="addr">
    <div>${icon('pc')}<p><span>${esc(n.ip ?? '--')}</span><span class="muted sm">${esc(n.mac || '--')}</span></p></div>
    <div>${icon('router')}<p><span>${esc(n.gateway ?? '--')}</span><span class="muted sm">預設閘道</span></p></div></div>`;
}

function netCard(s) {
  const n = primaryNet(s);
  if (!n) return card('網路', '<div class="empty">沒有實體網路介面</div>');
  return card('網路', `${addrBlock(n)}${cap('速度', esc(n.name))}${chart([{ data: hist[`rx:${n.name}`], cls: 'c-green' }, { data: hist[`tx:${n.name}`], cls: 'c-blue' }])}
    <div class="foot"><span><i class="dot c-green"></i>↓ ${bytes(n.rxTotal)}（${rate(n.rxBps)}）</span><span><i class="dot c-blue"></i>↑ ${bytes(n.txTotal)}（${rate(n.txBps)}）</span></div>`);
}

function disksCard(s) {
  const rows = mainMounts(s).map((m) => `<tr><td class="grow">${esc(m.mount)}</td><td class="barcell">${bar([[ratio(m.used, m.total), 'c-yellow']])}</td><td class="num">${bytes(m.used)} / ${bytes(m.total)}</td></tr>`).join('');
  return card('磁碟', rows ? `<table>${rows}</table>` : '<div class="empty">沒有掛載的磁碟</div>');
}

function gpuCard(s) {
  if (!s.gpus.length) return card('顯示卡', '<div class="empty">未偵測到可讀取的顯示卡</div>');
  return s.gpus.map((g, i) => card('顯示卡', `<div class="cpu-row">${ring([[[g.usage, 'c-blue']]], 90, pct(g.usage))}<div>
      <b>${esc(g.name)}</b><div class="muted">溫度 ${temp(g.temp)}</div>
      ${cap('顯示記憶體', `${bytes(g.memUsed)} / ${bytes(g.memTotal)}`)}${bar([[ratio(g.memUsed, g.memTotal), 'c-yellow']], true)}</div></div>
      ${chart([{ data: hist[`gpu${i}`], cls: 'c-blue' }], 100)}`)).join('');
}

function sensorsCard(s) {
  const list = s.sensors.slice(0, 8);
  if (!list.length) return card('感測器', '<div class="empty">沒有可讀取的感測器</div>');
  return card('感測器', `<table>${list.map((x) => `<tr><td class="grow">${esc(x.chip)} · ${esc(x.label)}</td><td class="num">${fmtSensor(x)}</td></tr>`).join('')}</table>${s.sensors.length > 8 ? more('sensors', '顯示更多…') : ''}`);
}
const fmtSensor = (x) => (x.kind === 'temp' ? temp(x.value) : `${x.value.toFixed(0)} rpm`);

function ioCard() {
  return card('磁碟 I/O', `${cap('速度', '')}${chart([{ data: hist.diskR, cls: 'c-red' }, { data: hist.diskW, cls: 'c-blue' }])}
    <div class="foot"><span><i class="dot c-red"></i>讀取 ${rate(last('diskR'))}</span><span><i class="dot c-blue"></i>寫入 ${rate(last('diskW'))}</span></div>`);
}

// ── pages ──
const PAGES = {
  dash: ['儀表板', (s) => `<div class="masonry">${[summaryCard(s), cpuCard(s), memCard(s), netCard(s), disksCard(s), gpuCard(s), ioCard(), sensorsCard(s)].join('')}</div>`],

  apps: ['應用程式', () => {
    const k = sort.key, dir = sort.desc ? -1 : 1;
    const list = [...procs].sort((a, b) => {
      const x = a[k], y = b[k];
      if (x == null || y == null) return (x == null) - (y == null); // missing CPU% always last
      return (typeof x === 'string' ? x.localeCompare(y) : x - y) * dir || a.pid - b.pid;
    }).slice(0, 300);
    const th = (key, t, cls = '') => `<th class="${cls}" data-sort="${key}">${t}${sort.key === key ? (sort.desc ? ' ▼' : ' ▲') : ''}</th>`;
    const rows = list.map((p) => `<tr><td class="grow">${esc(p.name)}</td><td class="num">${p.pid}</td><td class="num">${p.cpu == null ? '--' : p.cpu.toFixed(1) + '%'}</td><td class="num">${bytes(p.mem)}</td></tr>`).join('');
    return card(`共 ${procs.length} 個程序`, `<table class="zebra"><tr>${th('name', '名稱')}${th('pid', 'PID', 'num')}${th('cpu', 'CPU', 'num')}${th('mem', '記憶體', 'num')}</tr>${rows}</table>`);
  }],

  storage: ['儲存', () => storagePage()],

  net: ['網路', (s) => {
    if (!s.net.length) return card('網路', '<div class="empty">沒有實體網路介面</div>');
    return `<div class="stack">${s.net.map((n) => {
      const title = `${esc(n.name)}`;
      const dot = `<i class="spacer"></i><i class="dot ${n.up ? 'c-green' : 'c-red'}"></i>`;
      if (!n.up) return `<section class="card iface down"><h2>${title}${dot}</h2></section>`;
      return `<section class="card iface"><h2>${title}${dot}</h2>${addrBlock(n)}<div class="speeds"><div>
          <div class="rate">${icon('down')}<b><i class="dot c-green"></i>${rate(n.rxBps)}</b><span class="muted">${bytes(n.rxTotal)}</span></div>
          <div class="rate">${icon('up')}<b><i class="dot c-blue"></i>${rate(n.txBps)}</b><span class="muted">${bytes(n.txTotal)}</span></div></div>
        <div>${cap('速度', '')}${chart([{ data: hist[`rx:${n.name}`], cls: 'c-green' }, { data: hist[`tx:${n.name}`], cls: 'c-blue' }], null, true)}</div></div></section>`;
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
      <button class="btn" data-du="pick"${du.busy ? ' disabled' : ''}>${icon('folder')}選擇資料夾…</button>
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
    <section class="card"><table class="zebra files"><tr><th>名稱</th><th class="num">大小</th></tr>${files || '<tr><td class="muted">空資料夾</td></tr>'}</table></section></div>`;
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
  $('#host').innerHTML = `${icon('laptop', 'laptop')}
    <div class="hcol"><b>${esc(info?.hostname || '--')}</b><span>運作時間：<em>${duration(snap?.uptime)}</em></span></div>
    <div class="hcol"><b>${esc(info?.model || info?.os || '--')}</b><span>${esc(info?.os || '')}</span><span>${esc(info?.cpuModel || '')}</span></div>`;
}

function render() {
  if (snap) {
    $('#content').innerHTML = PAGES[page][1](snap);
    // Proportional widths through CSSOM: the CSP blocks style attributes in HTML.
    const blocks = [...document.querySelectorAll('[data-grow]')];
    blocks.forEach((el) => { el.style.flexGrow = el.dataset.grow; });
    blocks.forEach((el) => { if (el.clientWidth < 40) el.textContent = ''; }); // too narrow for a readable name (title still has it)
  }
  hostBar();
}

function go(p) {
  page = p;
  document.querySelectorAll('#pages .nav-item').forEach((b) => b.classList.toggle('active', b.dataset.page === p));
  $('#content').scrollTop = 0;
  if (p === 'storage' && !du.started && info) { du.started = true; duScan(info.home); }
  pollProcs();
  render();
}

// theme: system → dark → light
const media = matchMedia('(prefers-color-scheme: dark)');
let theme = store('rw-theme') || 'system';
function applyTheme() {
  document.documentElement.dataset.theme = theme === 'system' ? (media.matches ? 'dark' : 'light') : theme;
  const [ic, label] = { system: ['auto', '跟隨系統'], dark: ['moon', '深色'], light: ['sun', '淺色'] }[theme];
  $('#theme').innerHTML = `${icon(ic)}<span class="label">${label}</span>`;
}
media.addEventListener('change', applyTheme);
$('#theme').addEventListener('click', () => {
  theme = { system: 'dark', dark: 'light', light: 'system' }[theme];
  store('rw-theme', theme);
  applyTheme();
});

$('#collapse').innerHTML = icon('collapse');
$('#nav').classList.toggle('collapsed', store('rw-collapsed') === '1');
$('#collapse').addEventListener('click', () => store('rw-collapsed', $('#nav').classList.toggle('collapsed') ? '1' : '0'));

$('#pages').innerHTML = Object.entries(PAGES).map(([k, [t]]) =>
  `<button class="nav-item${k === page ? ' active' : ''}" data-page="${k}" title="${t}">${icon({ dash: 'dash', apps: 'apps', storage: 'storage', net: 'net', sensors: 'sensors' }[k])}<span class="label">${t}</span></button>`).join('');
$('#pages').addEventListener('click', (e) => { const b = e.target.closest('[data-page]'); if (b) go(b.dataset.page); });

$('#content').addEventListener('click', (e) => {
  const t = e.target;
  if (page === 'storage') return duClick(t);
  const link = t.closest('[data-goto]');
  if (link) return go(link.dataset.goto);
  const flip = t.closest('[data-disk]');
  if (flip) { sumDisk += +flip.dataset.disk; return render(); }
  const th = t.closest('th[data-sort]');
  if (!th) return;
  const key = th.dataset.sort;
  sort = sort.key === key ? { key, desc: !sort.desc } : { key, desc: key === 'cpu' || key === 'mem' };
  render();
});

async function tick() {
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
  if ((page !== 'dash' && page !== 'apps') || document.hidden) return;
  try {
    procs = await invoke('processes');
    if (page === 'apps') render();
  } catch (e) {
    console.error(e);
  }
}

applyTheme();
invoke('system_info').then((i) => { info = i; hostBar(); }).catch(console.error);
tick();
pollProcs();
setInterval(tick, 1000);
setInterval(pollProcs, 2000);
