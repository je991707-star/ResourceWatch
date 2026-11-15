'use strict';
// Polls the Rust commands and re-renders the current page. Missing values are null → "--".
const invoke = window.__TAURI__?.core.invoke ?? window.mockInvoke;
const N = 60; // history points (1 per second)
const hist = {};
let snap = null;
let procs = [];
let page = 'overview';
let sort = { key: 'cpu', desc: true };

const $ = (s) => document.querySelector(s);
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
const sum = (a) => (a.length && a.every((v) => v != null) ? a.reduce((x, y) => x + y, 0) : null);
const push = (k, v) => {
  const a = (hist[k] ??= []);
  a.push(v);
  if (a.length > N) a.shift();
  return a;
};

function bytes(b) {
  if (b == null) return '--';
  const u = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let i = 0;
  while (b >= 1024 && i < u.length - 1) { b /= 1024; i++; }
  return `${b.toFixed(i && b < 100 ? 1 : 0)} ${u[i]}`;
}
const rate = (b) => (b == null ? '--' : `${bytes(b)}/s`);
const pct = (v) => (v == null ? '--' : `${v.toFixed(0)}%`);
const temp = (v) => (v == null ? '--' : `${v.toFixed(0)} °C`);
const ratio = (a, b) => (a == null || !b ? null : (a / b) * 100);
function duration(s) {
  if (s == null) return '';
  const d = Math.floor(s / 86400), h = Math.floor((s % 86400) / 3600), m = Math.floor((s % 3600) / 60);
  return `運作 ${d ? `${d} 天 ` : ''}${h} 小時 ${m} 分`;
}

function bar(p, cls) {
  const w = p == null ? 0 : Math.min(Math.max(p, 0), 100);
  return `<svg class="bar" viewBox="0 0 100 6" preserveAspectRatio="none"><rect class="track" width="100" height="6"/><rect class="${cls}" width="${w}" height="6"/></svg>`;
}

// series: [{ data, cls }]; max fixed (percent) or auto from data.
function chart(series, max, tall) {
  max = max ?? Math.max(1, ...series.flatMap((s) => s.data.filter((v) => v != null))) * 1.1;
  const y = (v) => (100 - (Math.min(v, max) / max) * 98).toFixed(1);
  const paths = series.map(({ data, cls }) => {
    const off = N - data.length;
    const runs = [[]];
    data.forEach((v, i) => (v == null ? runs.push([]) : runs.at(-1).push([off + i, y(v)])));
    return runs.filter((r) => r.length).map((r) => {
      const line = r.map(([x, yy], i) => `${i ? 'L' : 'M'}${x} ${yy}`).join('');
      return `<path class="area ${cls}" d="${line}L${r.at(-1)[0]} 100L${r[0][0]} 100Z"/><path class="line ${cls}" d="${line}"/>`;
    }).join('');
  });
  const grid = [25, 50, 75].map((g) => `<line class="grid-line" x1="0" x2="${N - 1}" y1="${g}" y2="${g}"/>`).join('');
  return `<svg class="chart${tall ? ' tall' : ''}" viewBox="0 0 ${N - 1} 100" preserveAspectRatio="none">${grid}${paths.join('')}</svg>`;
}

const stats = (pairs) => `<div class="stats">${pairs.map(([k, v]) => `<div><span>${k}</span><b>${esc(v)}</b></div>`).join('')}</div>`;
const legend = (items) => `<div class="legend">${items.map(([cls, t]) => `<span><i class="${cls}"></i>${esc(t)}</span>`).join('')}</div>`;
const card = (title, body, link) => `<section class="card${link ? ' link' : ''}"${link ? ` data-goto="${link}"` : ''}><h2>${title}</h2>${body}</section>`;
const tile = (title, big, sub, chartHtml, link) => card(title, `<div class="big">${esc(big)}</div><div class="sub">${esc(sub)}</div>${chartHtml}`, link);

function record(s) {
  push('cpu', s.cpu.usage);
  push('mem', ratio(s.mem.used, s.mem.total));
  s.gpus.forEach((g, i) => push(`gpu${i}`, g.usage));
  push('diskR', sum(s.disks.map((d) => d.readBps)));
  push('diskW', sum(s.disks.map((d) => d.writeBps)));
  push('netRx', sum(s.net.map((n) => n.rxBps)));
  push('netTx', sum(s.net.map((n) => n.txBps)));
}
const last = (k) => hist[k]?.at(-1) ?? null;

const pages = {
  overview: ['總覽', (s) => {
    const m = s.mem;
    const tiles = [
      tile('處理器', pct(s.cpu.usage), `${temp(s.cpu.temp)} · ${s.cpu.freqMhz ? (s.cpu.freqMhz / 1000).toFixed(2) + ' GHz' : '--'}`, chart([{ data: hist.cpu, cls: 'c-cpu' }], 100), 'cpu'),
      tile('記憶體', pct(ratio(m.used, m.total)), `${bytes(m.used)} / ${bytes(m.total)}`, chart([{ data: hist.mem, cls: 'c-mem' }], 100), 'mem'),
      ...s.gpus.map((g, i) => tile('顯示卡', pct(g.usage), `${g.name} · ${temp(g.temp)}`, chart([{ data: hist[`gpu${i}`], cls: 'c-gpu' }], 100), 'gpu')),
      tile('磁碟 I/O', rate(sum([last('diskR'), last('diskW')])), `讀 ${rate(last('diskR'))} · 寫 ${rate(last('diskW'))}`, chart([{ data: hist.diskR, cls: 'c-disk' }, { data: hist.diskW, cls: 'c-alt' }]), 'disk'),
      tile('網路', rate(sum([last('netRx'), last('netTx')])), `↓ ${rate(last('netRx'))} · ↑ ${rate(last('netTx'))}`, chart([{ data: hist.netRx, cls: 'c-net' }, { data: hist.netTx, cls: 'c-alt' }]), 'net'),
    ];
    const storage = card('儲存空間', mountTable(s.mounts), 'disk');
    return `<div class="grid">${tiles.join('')}</div>${storage}`;
  }],

  cpu: ['處理器', (s) => {
    const c = s.cpu;
    const cores = c.cores.map((v, i) => `<div><span>CPU ${i}</span><b>${pct(v)}</b>${bar(v, 'c-cpu')}</div>`).join('');
    return card(esc(info?.cpuModel || '使用率'), `${chart([{ data: hist.cpu, cls: 'c-cpu' }], 100, true)}${stats([
      ['使用率', pct(c.usage)], ['平均頻率', c.freqMhz ? `${(c.freqMhz / 1000).toFixed(2)} GHz` : '--'],
      ['溫度', temp(c.temp)], ['執行緒', String(c.cores.length)],
    ])}`) + card('各執行緒', `<div class="cores">${cores}</div>`);
  }],

  mem: ['記憶體', (s) => {
    const m = s.mem;
    return card('使用率', `${chart([{ data: hist.mem, cls: 'c-mem' }], 100, true)}${stats([
      ['已使用', bytes(m.used)], ['可用', bytes(m.available)], ['快取', bytes(m.cached)], ['總量', bytes(m.total)],
    ])}`) + card('交換空間', m.swapTotal
      ? `<div class="big">${pct(ratio(m.swapUsed, m.swapTotal))}</div><div class="sub">${bytes(m.swapUsed)} / ${bytes(m.swapTotal)}</div>${bar(ratio(m.swapUsed, m.swapTotal), 'c-mem')}`
      : '<div class="empty">未啟用</div>');
  }],

  gpu: ['顯示卡', (s) => s.gpus.length
    ? s.gpus.map((g, i) => card(esc(g.name), `${chart([{ data: hist[`gpu${i}`], cls: 'c-gpu' }], 100, true)}${stats([
      ['使用率', pct(g.usage)], ['顯示記憶體', `${bytes(g.memUsed)} / ${bytes(g.memTotal)}`], ['溫度', temp(g.temp)],
    ])}${bar(ratio(g.memUsed, g.memTotal), 'c-gpu')}`)).join('')
    : card('顯示卡', '<div class="empty">未偵測到可讀取的顯示卡</div>')],

  disk: ['儲存', (s) => {
    const rows = s.disks.map((d) => `<tr><td>${esc(d.name)}</td><td class="grow">${esc(d.model || '--')}</td><td class="num">${bytes(d.size)}</td><td class="num">${rate(d.readBps)}</td><td class="num">${rate(d.writeBps)}</td></tr>`).join('');
    return card('磁碟 I/O', `${chart([{ data: hist.diskR, cls: 'c-disk' }, { data: hist.diskW, cls: 'c-alt' }], null, true)}${legend([['c-disk', `讀取 ${rate(last('diskR'))}`], ['c-alt', `寫入 ${rate(last('diskW'))}`]])}`)
      + card('磁碟', rows ? `<table><tr><th>名稱</th><th>型號</th><th class="num">容量</th><th class="num">讀取</th><th class="num">寫入</th></tr>${rows}</table>` : '<div class="empty">沒有實體磁碟</div>')
      + card('檔案系統', mountTable(s.mounts));
  }],

  net: ['網路', (s) => {
    const rows = s.net.map((n) => `<tr><td class="grow">${esc(n.name)}</td><td class="num">${rate(n.rxBps)}</td><td class="num">${rate(n.txBps)}</td></tr>`).join('');
    return card('流量', `${chart([{ data: hist.netRx, cls: 'c-net' }, { data: hist.netTx, cls: 'c-alt' }], null, true)}${legend([['c-net', `下載 ${rate(last('netRx'))}`], ['c-alt', `上傳 ${rate(last('netTx'))}`]])}`)
      + card('介面', rows ? `<table><tr><th>名稱</th><th class="num">下載</th><th class="num">上傳</th></tr>${rows}</table>` : '<div class="empty">沒有實體網路介面</div>');
  }],

  sensors: ['感測器', (s) => {
    const chips = Map.groupBy ? Map.groupBy(s.sensors, (x) => x.chip) : s.sensors.reduce((m, x) => m.set(x.chip, [...(m.get(x.chip) ?? []), x]), new Map());
    if (!chips.size) return card('感測器', '<div class="empty">沒有可讀取的感測器</div>');
    return `<div class="grid">${[...chips].map(([chip, list]) => card(esc(chip), `<table>${list.map((x) =>
      `<tr><td class="grow">${esc(x.label)}</td><td class="num">${x.kind === 'temp' ? temp(x.value) : `${x.value.toFixed(0)} RPM`}</td></tr>`).join('')}</table>`)).join('')}</div>`;
  }],

  procs: ['程序', () => {
    const k = sort.key, dir = sort.desc ? -1 : 1;
    const list = [...procs].sort((a, b) => {
      const x = a[k], y = b[k];
      if (x == null || y == null) return (x == null) - (y == null); // missing CPU% always last
      return (typeof x === 'string' ? x.localeCompare(y) : x - y) * dir || a.pid - b.pid;
    }).slice(0, 200);
    const th = (key, t, cls = '') => `<th class="${cls}" data-sort="${key}">${t}${sort.key === key ? (sort.desc ? ' ▼' : ' ▲') : ''}</th>`;
    const rows = list.map((p) => `<tr><td class="grow">${esc(p.name)}</td><td class="num">${p.pid}</td><td class="num">${p.cpu == null ? '--' : p.cpu.toFixed(1) + '%'}</td><td class="num">${bytes(p.mem)}</td></tr>`).join('');
    return card(`共 ${procs.length} 個程序（顯示前 200）`, `<table><tr>${th('name', '名稱')}${th('pid', 'PID', 'num')}${th('cpu', 'CPU', 'num')}${th('mem', '記憶體', 'num')}</tr>${rows}</table>`);
  }],
};

function mountTable(mounts) {
  if (!mounts.length) return '<div class="empty">沒有掛載的檔案系統</div>';
  return `<table>${mounts.map((m) => `<tr><td class="grow">${esc(m.mount)}<div class="sub">${esc(m.device)}</div></td><td class="barcell">${bar(ratio(m.used, m.total), 'c-disk')}</td><td class="num">${pct(ratio(m.used, m.total))}</td><td class="num">${bytes(m.used)} / ${bytes(m.total)}</td></tr>`).join('')}</table>`;
}

function render() {
  if (!snap) return;
  $('#content').innerHTML = pages[page][1](snap);
  $('#uptime').textContent = duration(snap.uptime);
}

function go(p) {
  page = p;
  $('#title').textContent = pages[p][0];
  document.querySelectorAll('nav button').forEach((b) => b.classList.toggle('active', b.dataset.page === p));
  $('#content').scrollTop = 0;
  if (p === 'procs') pollProcs();
  render();
}

async function tick() {
  try {
    snap = await invoke('snapshot');
    record(snap);
    render();
  } catch (e) {
    console.error(e);
  }
}

async function pollProcs() {
  if (page !== 'procs' || document.hidden) return;
  try {
    procs = await invoke('processes');
    render();
  } catch (e) {
    console.error(e);
  }
}

let info = null;
$('#nav').addEventListener('click', (e) => { const b = e.target.closest('button'); if (b) go(b.dataset.page); });
$('#content').addEventListener('click', (e) => {
  const link = e.target.closest('[data-goto]');
  if (link) return go(link.dataset.goto);
  const th = e.target.closest('th[data-sort]');
  if (!th) return;
  const key = th.dataset.sort;
  sort = sort.key === key ? { key, desc: !sort.desc } : { key, desc: key === 'cpu' || key === 'mem' };
  render();
});

invoke('system_info').then((i) => {
  info = i;
  $('#sysinfo').innerHTML = [i.hostname, i.os, `Linux ${i.kernel}`].filter(Boolean).map(esc).join('<br>');
});
tick();
setInterval(tick, 1000);
setInterval(pollProcs, 2000);
