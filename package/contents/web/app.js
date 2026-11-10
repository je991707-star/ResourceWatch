/* ResourceWatch HTML UI — no direct disk, GPU, or network system access.
 * External host may call window.ResourceWatchUI.update(snapshot).
 * All sensor-looking values on initial load are clearly marked DEMO.
 */
(() => {
  "use strict";
  const $ = (id) => document.getElementById(id);
  const MAX_SAMPLES = 45;
  const SVG_NS = "http://www.w3.org/2000/svg";

  // 固定 720px 的設計寬度；保留既有卡片比例與圖表 SVG。
  // 外層 Widget 的尺寸由 main.qml 決定，HTML 僅在容器內等比縮放。
  const DESIGN_WIDTH = 720;
  const DESIGN_MIN_HEIGHT = 821;

  function fitToViewport() {
    const canvas = $("resourceWatch");
    if (!canvas) return;

    // 先恢復自然高度，避免前一次視窗大小留下的 min-height
    // 影響下一次重新測量，導致反覆縮小或底部留白。
    canvas.style.width = DESIGN_WIDTH + "px";
    canvas.style.minHeight = "0";
    canvas.style.height = "auto";
    canvas.style.zoom = "1";

    const naturalHeight = Math.max(DESIGN_MIN_HEIGHT, canvas.scrollHeight);
    const viewportWidth = Math.max(1, window.innerWidth);
    const viewportHeight = Math.max(1, window.innerHeight);
    const scale = Math.max(0.1, Math.min(viewportWidth / DESIGN_WIDTH,
                                          viewportHeight / naturalHeight));

    // 將多出的高度用於卡片間距（card-stack 的 space-between），
    // 使 footer 靠近視窗底緣，不把空白堆積在版本號下方。
    canvas.style.minHeight = (viewportHeight / scale) + "px";
    canvas.style.zoom = String(scale);
    document.documentElement.style.overflow = "hidden";
    document.body.style.overflow = "hidden";
  }

  window.addEventListener("resize", fitToViewport, {passive: true});

  function wave(level, swing, samples = MAX_SAMPLES, phase = 0) {
    return Array.from({length: samples}, (_, i) =>
      Math.max(0, Math.min(100, level + swing * Math.sin(i * .36 + phase)
        + swing * .24 * Math.cos(i * .79 + phase * 2))));
  }

  const demo = {
    version: "0.0.4",
    cpuName: "Intel Xeon CPU E3-1231 v3 @ 3.40GHz",
    cpu: {displayValue: "18.4%", percent: 18.4},
    cpuFreqText: "3.00 GHz", cpuTempText: "46.8 °C", uptimeText: "2:29:50",
    gpuIndex: 0, gpuVendor: "nvidia", gpuName: "Quadro P620",
    gpu: {displayValue: "68%", percent: 68}, gpuTempText: "60 °C",
    vram: {displayValue: "80%", percent: 80, usedText: "1.6 GiB", totalText: "2.0 GiB"},
    // Neither NVENC nor NVDEC is exposed by the current SensorManager.
    encoder: null, decoder: null,
    ram: {displayValue: "41.7%", percent: 41.7, usedText: "6.5 GiB", totalText: "15.5 GiB"},
    ramSubtitle: "15.5 GiB DDR3", ramSpeedText: "1600 MT/s",
    swap: {usedText: "0 B", totalText: "7.8 GiB"},
    disks: [
      {name: "sda", sizeText: "1.8 TiB", model: "TOSHIBA DT01ABA200V"},
      {name: "nvme0n1", sizeText: "476.9 GiB", model: "Micron 2400 MTFDKBA512QFM"}
    ],
    diskRates: {sda:{read:0,write:208400},nvme0n1:{read:0,write:0}},
    netIp: "192.168.50.212", netHasInterface: true,
    netUp: {rawValue: 1363149, displayValue:"1.3 MiB/s"},
    netDown: {rawValue: 14234, displayValue:"13.9 KiB/s"},
    history: {
      cpu: wave(15, 5), gpu: wave(56, 15, MAX_SAMPLES, .5),
      vram: wave(80, 2), ram: wave(41.7, .8),
      netUp: Array.from({length: MAX_SAMPLES}, (_, i) => 660000 + (i===35?600000:0) + 90000 * Math.sin(i*.39)),
      netDown: Array.from({length: MAX_SAMPLES}, (_, i) => 40000 + 15000 * Math.cos(i*.45))
    }
  };

  const clone = obj => JSON.parse(JSON.stringify(obj));
  // 在 QtWebEngine 中絕不顯示虛假的示範數值；瀏覽器預覽才使用 demo。
  const embedded = Boolean(window.qt && window.qt.webChannelTransport);
  const empty = {
    version: "0.0.4", cpuName: "--",
    cpu: {displayValue:"--"}, cpuFreqText:"--", cpuTempText:"--", uptimeText:"--",
    gpuIndex:0, gpuName:"--", gpu:{displayValue:"--"}, gpuTempText:"--",
    vram:{usedText:"--",totalText:"--"}, encoder:null, decoder:null,
    ram:{displayValue:"--",usedText:"--",totalText:"--"}, ramSpeedText:"--",
    swap:{usedText:"--",totalText:"--"},
    disks:[], diskRates:{}, netHasInterface:false, netIp:"--",
    netUp:{displayValue:"--"}, netDown:{displayValue:"--"},
    history:{cpu:[],gpu:[],vram:[],ram:[],netUp:[],netDown:[]}
  };
  let state = clone(embedded ? empty : demo);
  let isDemo = !embedded;
  let connected = false;
  let bridge = null;
  let frame = 0;
  let uptimeAnchorSeconds = null;
  let uptimeAnchorAt = 0;
  let lastObservedUptimeSeconds = null;
  function uptimeFromSeconds(seconds) {
    const total = Math.max(0, Math.floor(seconds));
    const days = Math.floor(total / 86400);
    const hours = Math.floor((total % 86400) / 3600);
    const minutes = Math.floor((total % 3600) / 60);
    const secs = total % 60;
    const hms = [hours, minutes, secs].map(v => String(v).padStart(2, "0")).join(":");
    return days > 0 ? `${days}天 ${hms}` : hms;
  }
  function refreshUptimeText() {
    if (uptimeAnchorSeconds === null) return;
    const elapsed = Math.max(0, (performance.now() - uptimeAnchorAt) / 1000);
    setText("cpuUptime", uptimeFromSeconds(uptimeAnchorSeconds + elapsed));
  }
  // One DOM text change per second. No subprocess and no full chart redraw.
  if (embedded) setInterval(refreshUptimeText, 1000);
  const text = (value, fallback = "--") => value === null || value === undefined || value === "" ? fallback : String(value);
  const metricText = value => text(value && value.displayValue);
  // Avoid scheduling text layout/paint when a sensor value has not changed.
  function setText(id, value) {
    const element = $(id);
    if (!element) return;
    const next = text(value);
    if (element.textContent !== next) element.textContent = next;
  }
  function formatRate(value) {
    if (!Number.isFinite(value) || value < 0) return "--";
    const units = ["B/s", "KiB/s", "MiB/s", "GiB/s"];
    let v = value, unit = 0;
    while (v >= 1024 && unit < units.length - 1) { v /= 1024; unit++; }
    return (unit === 0 ? v.toFixed(0) : v.toFixed(1)) + " " + units[unit];
  }
  function escapeAttr(value) { return String(value).replace(/[&<>"']/g, ch => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[ch])); }
  function makeDiskCard(disk, idx, rates) {
    const card = document.createElement("section");
    card.className = "monitor-card monitor-card--disk";
    card.setAttribute("aria-label", "硬碟 " + (idx + 1));
    const name = text(disk.name);
    const model = text(disk.model);
    const size = text(disk.sizeText);
    const diskRates = rates?.[name] || {};
    card.innerHTML = `<div class="disk-header"><h2>硬碟${idx + 1}</h2><span class="disk-name" title="${escapeAttr(name)}">${escapeAttr(name)}</span></div>
      <div class="disk-model" title="${escapeAttr(model)}">${escapeAttr(model)} · ${escapeAttr(size)}</div>
      <div class="disk-stats"><div class="stat"><span class="stat-label">讀取速度</span><strong class="stat-value">${formatRate(diskRates.read)}</strong></div><div class="stat"><span class="stat-label">寫入速度</span><strong class="stat-value">${formatRate(diskRates.write)}</strong></div></div>`;
    return card;
  }

  function updateGrid(svg) {
    const g = svg.querySelector(".grid-lines");
    if (!g) return;
    // Grid as a vector path, inspectable in browser DevTools.
    let d = "";
    for (let y = 0; y <= 100; y += 20) d += `M0 ${y} H300 `;
    for (let x = 0; x <= 300; x += 30) d += `M${x} 0 V100 `;
    const path = document.createElementNS(SVG_NS, "path");
    path.setAttribute("d", d);
    g.replaceChildren(path);
  }
  document.querySelectorAll(".plot svg").forEach(updateGrid);

  // Resolve chart nodes only once; all charts are static HTML elements.
  const chartNodes = Object.fromEntries(
    Array.from(document.querySelectorAll("[data-chart]"), chart => [
      chart.dataset.chart,
      { root: chart, line: chart.querySelector(".line"), area: chart.querySelector(".area") }
    ])
  );

  function chartSeries(name, values, max = 100) {
    const chart = chartNodes[name];
    if (!chart) return;
    if (!Array.isArray(values)) {
      chart.root.classList.add("is-missing");
      if (chart.line.getAttribute("d")) chart.line.setAttribute("d", "");
      if (chart.area.getAttribute("d")) chart.area.setAttribute("d", "");
      return;
    }
    const last = Math.max(1, values.length - 1);
    const upper = max > 0 ? max : 1;
    const points = [];
    let path = "";
    for (let i = 0; i < values.length; i++) {
      const value = values[i];
      if (!Number.isFinite(value)) continue;
      const x = (i / last * 300).toFixed(2);
      const y = (100 - Math.min(1, Math.max(0, value) / upper) * 100).toFixed(2);
      path += `${path ? " L" : "M"}${x} ${y}`;
      points.push(x);
    }
    if (!points.length) {
      chart.root.classList.add("is-missing");
      if (chart.line.getAttribute("d")) chart.line.setAttribute("d", "");
      if (chart.area.getAttribute("d")) chart.area.setAttribute("d", "");
      return;
    }
    chart.root.classList.remove("is-missing");
    const areaPath = `M${points[0]} 100 ${path.replace(/^M/, "L")} L${points[points.length - 1]} 100 Z`;
    // Attribute assignments invalidate SVG paint; skip identical paths.
    if (chart.line.getAttribute("d") !== path) chart.line.setAttribute("d", path);
    if (chart.area.getAttribute("d") !== areaPath) chart.area.setAttribute("d", areaPath);
  }

  function labeledGpuName(name, vendor) {
    const raw = text(name);
    if (raw === "--") return raw;
    const brand = ({nvidia: "NVIDIA", amd: "AMD", intel: "Intel"})[
      String(vendor || "").toLowerCase()];
    if (!brand || new RegExp(`\\b${brand}\\b`, "i").test(raw)) return raw;
    return `${brand} ${raw}`;
  }

  let lastDiskInventory = null;
  let diskStatNodes = [];
  function render() {
    const s = state;
    setText("cpuName", s.cpuName);
    setText("cpuUsage", metricText(s.cpu));
    setText("cpuChartUsage", metricText(s.cpu));
    setText("cpuSpeed", s.cpuFreqText);
    setText("cpuTemp", s.cpuTempText);
    if (Number.isFinite(s.uptimeSeconds)) {
      // The kernel uptime sensor can update less often than HTML snapshots.
      // Never reset the local clock for repeated identical snapshots.
      if (s.uptimeSeconds !== lastObservedUptimeSeconds) {
        lastObservedUptimeSeconds = s.uptimeSeconds;
        uptimeAnchorSeconds = s.uptimeSeconds;
        uptimeAnchorAt = performance.now();
        refreshUptimeText();
      }
    } else if (uptimeAnchorSeconds === null) {
      setText("cpuUptime", s.uptimeText);
    }
    setText("gpuHeading", "GPU" + (s.gpuIndex ?? 0));
    setText("gpuName", labeledGpuName(s.gpuName, s.gpuVendor));
    setText("gpuUsage", metricText(s.gpu));
    setText("gpuChartUsage", metricText(s.gpu));
    setText("gpuTemp", s.gpuTempText);
    setText("gpuVram", s.vram ? `${text(s.vram.usedText)} / ${text(s.vram.totalText)}` : "--");
    setText("vramGraphMax", s.vram?.totalText);
    setText("encoderLabel", s.encoder?.displayValue ?? "--");
    setText("decoderLabel", s.decoder?.displayValue ?? "--");
    setText("ramSubtitle", s.ramSubtitle ?? s.ram?.totalText);
    setText("ramUsage", metricText(s.ram));
    setText("ramChartUsage", s.ram?.usedText);
    setText("ramSpeed", s.ramSpeedText);
    setText("ramUsed", metricText(s.ram));
    setText("swapUsed", s.swap ? `${text(s.swap.usedText)} / ${text(s.swap.totalText)}` : "--");
    setText("netSubtitle", (s.netHasInterface || s.netIpAvailable)
      ? `IPv4: ${text(s.netIp)}` : "所有網路介面");
    setText("netUp", metricText(s.netUp));
    setText("netDown", metricText(s.netDown));
    setText("versionText", "v" + text(s.version,"0.0.3"));
    const disks = s.disks || [];
    const inventory = JSON.stringify(disks.map(d => [d.name, d.model, d.sizeText]));
    const disksChanged = inventory !== lastDiskInventory;
    if (disksChanged) {
      const cards = disks.map((disk, i) => makeDiskCard(disk, i, s.diskRates));
      $("disksGrid").replaceChildren(...cards);
      // Disk inventory only changes on startup / manual refresh.
      // Reuse the two label references rather than querying each snapshot.
      diskStatNodes = cards.map(card => card.querySelectorAll(".stat-value"));
      lastDiskInventory = inventory;
    } else {
      diskStatNodes.forEach((labels, i) => {
        const rate = s.diskRates?.[disks[i]?.name] || {};
        if (labels.length >= 2) {
          const read = formatRate(rate.read), write = formatRate(rate.write);
          if (labels[0].textContent !== read) labels[0].textContent = read;
          if (labels[1].textContent !== write) labels[1].textContent = write;
        }
      });
    }

    const h = s.history || {};
    chartSeries("cpu", h.cpu);
    chartSeries("gpu", h.gpu);
    chartSeries("vram", h.vram);
    chartSeries("ram", h.ram);
    chartSeries("encode", h.encode || (Number.isFinite(s.encoder?.percent) ? [s.encoder.percent] : null));
    chartSeries("decode", h.decode || (Number.isFinite(s.decoder?.percent) ? [s.decoder.percent] : null));
    const up = Array.isArray(h.netUp) ? h.netUp : [];
    const down = Array.isArray(h.netDown) ? h.netDown : [];
    // Compute the network graph and its scale in one pass, without
    // allocating two filtered arrays on every snapshot.
    const net = [];
    let max = 1;
    for (let i = 0, count = Math.max(up.length, down.length); i < count; i++) {
      const a = up[i], b = down[i];
      if (!Number.isFinite(a) && !Number.isFinite(b)) {
        net.push(null);
        continue;
      }
      const value = Math.max(0, Number.isFinite(a) ? a : 0,
                               Number.isFinite(b) ? b : 0);
      net.push(value);
      if (value > max) max = value;
    }
    chartSeries("network", net.length ? net : null, max * 1.12);
    setText("networkGraphMax", formatRate(max));
    // 硬碟數量變化時重新計算內容高度，避免下一列硬碟被裁掉。
    if (disksChanged) fitToViewport();
  }

  function updateMode() {
    const badge = $("dataMode");
    badge.textContent = isDemo ? "示範資料 · 非實機" :
      (connected ? "即時資料" : "連線中 · 等待 KDE 感測器");
    badge.classList.toggle("is-live",connected);
    badge.hidden = connected;
  }
  // Public integration API for an eventual QML/QtWebChannel bridge.
  // Pass a complete snapshot with the same property names as SensorManager.qml.
  window.ResourceWatchUI = Object.freeze({
    update(snapshot) {
      if (!snapshot || typeof snapshot !== "object") return;
      // The WebChannel JSON has already been parsed into a fresh object.
      // Do not stringify/parse the complete history a second time every tick.
      state = snapshot;
      const firstLive = !connected || isDemo;
      isDemo = false;
      connected = true;
      if (firstLive) {
        $("demoAnimation").checked = false;
        updateMode();
      }
      render();
    },
    showDemo() {
      if (embedded) return; // Plasma Widget 禁止手動切回虛假監控資料
      state = clone(demo);
      isDemo = true;
      updateMode();
      render();
    }
  });

  $("settingsButton").addEventListener("click", () => {
    if (embedded) {
      if (bridge) bridge.openSettings();
      return;
    }
    const panel = $("previewSettings");
    panel.hidden = !panel.hidden;
    $("settingsButton").setAttribute("aria-expanded", String(!panel.hidden));
  });
  document.addEventListener("keydown", event => {
    if (event.key === "Escape") {
      $("previewSettings").hidden = true;
      $("settingsButton").setAttribute("aria-expanded","false");
    }
  });
  $("showGridLines").addEventListener("change", e => {
    $("resourceWatch").classList.toggle("hide-grid", !e.target.checked);
  });
  $("demoAnimation").addEventListener("change", e => {
    if (!embedded && e.target.checked && !isDemo) window.ResourceWatchUI.showDemo();
  });

  if (embedded) {
    $("previewSettings").hidden = true;
    $("settingsButton").title = "KDE 小工具設定";
    $("settingsButton").setAttribute("aria-label", "KDE 小工具設定");
    const clientScript = document.createElement("script");
    clientScript.src = "qrc:///qtwebchannel/qwebchannel.js";
    clientScript.onload = () => {
      if (typeof QWebChannel !== "function") return;
      new QWebChannel(window.qt.webChannelTransport, channel => {
        bridge = channel.objects.resourceWatch;
        if (!bridge) {
          $("dataMode").textContent = "KDE 資料橋接失敗";
          return;
        }
        let lastSnapshotJson = null;
        const receive = json => {
          // The initial snapshot can arrive via both property and signal.
          if (json === lastSnapshotJson) return;
          try {
            const snapshot = JSON.parse(json);
            lastSnapshotJson = json;
            window.ResourceWatchUI.update(snapshot);
          } catch (error) {
            console.warn("Invalid ResourceWatch snapshot", error);
          }
        };
        bridge.snapshotReady.connect(receive);
        receive(bridge.snapshotJson);
      });
    };
    clientScript.onerror = () => { $("dataMode").textContent = "QtWebChannel 載入失敗"; };
    document.head.appendChild(clientScript);
  }

  if (!embedded) setInterval(() => {
    if (!isDemo || !$("demoAnimation").checked) return;
    frame++;
    const h = state.history;
    function push(key, value) { if (!Array.isArray(h[key])) h[key]=[]; h[key].push(value); if (h[key].length>MAX_SAMPLES) h[key].shift(); }
    const cpu = 18 + 6 * Math.sin(frame*.5) + 3 * Math.sin(frame*.17);
    const gpu = 52 + 17 * Math.cos(frame*.25);
    const ram = 41.7 + .9*Math.sin(frame*.12);
    const up = Math.max(0, 850000 + 360000*Math.sin(frame*.4));
    const down = Math.max(0, 24000 + 15000*Math.cos(frame*.31));
    push("cpu",cpu); push("gpu",gpu); push("ram",ram);
    push("vram",80+.5*Math.sin(frame*.07)); push("netUp",up); push("netDown",down);
    state.cpu = {displayValue:cpu.toFixed(1)+"%",percent:cpu};
    state.gpu = {displayValue:gpu.toFixed(1)+"%",percent:gpu};
    state.ram = {...state.ram,displayValue:ram.toFixed(1)+"%",percent:ram};
    state.netUp = {rawValue:up,displayValue:formatRate(up)};
    state.netDown = {rawValue:down,displayValue:formatRate(down)};
    render();
  }, 1500);
  updateMode();
  render();
})();
