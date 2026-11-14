/* ResourceWatch Tauri frontend — display only.
 *
 * All values come from the Rust backend through Tauri IPC:
 *   commands: get_snapshot, get_settings, get_settings_info, get_window_info, update_settings, reset_settings, refresh_devices
 *   events:   snapshot, settings-changed
 * This file never reads hardware, files, or the network.
 *
 * Markup binds to data by attribute, so one value can appear on several pages:
 *   data-bind="key"  text   data-ring="key"  ring gauge (percent)
 *   data-bar="key"   bar    data-chart="key" history line/area   data-disk-list  disk rows
 */
(() => {
  "use strict";
  const $ = (id) => document.getElementById(id);
  const SVG_NS = "http://www.w3.org/2000/svg";
  const tauri = window.__TAURI__;
  const invoke = tauri?.core?.invoke;
  const listen = tauri?.event?.listen;
  const all = (selector) => Array.from(document.querySelectorAll(selector));
  const groupBy = (attr) => {
    const map = {};
    for (const el of all(`[data-${attr}]`)) (map[el.dataset[attr]] ||= []).push(el);
    return map;
  };

  // ---------------------------------------------------------------- theme
  // Display preference only, kept in this browser profile (not in Rust Settings).
  const THEME_KEY = "resourcewatch-theme";
  function applyTheme(theme) {
    document.documentElement.dataset.theme = theme;
  }
  let savedTheme = null;
  try { savedTheme = localStorage.getItem(THEME_KEY); } catch { /* storage blocked */ }
  applyTheme(savedTheme === "light" || savedTheme === "dark" ? savedTheme
    : (matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark"));
  $("themeToggle").addEventListener("click", () => {
    const next = document.documentElement.dataset.theme === "light" ? "dark" : "light";
    applyTheme(next);
    try { localStorage.setItem(THEME_KEY, next); } catch { /* storage blocked */ }
  });

  // Sidebar glass = Settings.sidebarGlass. The backend reads it once when it
  // creates the window (transparent + compositor blur), so a change applies on
  // the next start; get_window_info.glass says whether this window has it.
  let glassActive = false;
  let glassSetting = true;
  let glassAtStart = null;
  function applyGlass() {
    document.documentElement.dataset.glass = glassActive && glassSetting ? "on" : "off";
    $("sidebarGlass").checked = glassSetting;
    let hint;
    if (glassSetting !== glassAtStart && glassAtStart !== null) {
      hint = glassSetting ? "重新啟動後生效" : "重新啟動後改用不透明視窗（較省 CPU，NVIDIA 上縮放視窗較順）";
    } else if (!glassSetting) hint = "側欄與視窗不透明（較省 CPU，NVIDIA 上縮放視窗較順）";
    else if (glassActive) hint = "側欄半透明並模糊桌面背景（較耗 CPU；NVIDIA 上縮放視窗時內容會落後，關閉可改善）";
    else hint = "目前桌面環境不支援視窗模糊（需要 Wayland 上的 KDE Plasma 模糊特效），側欄維持不透明";
    $("sidebarGlassHint").textContent = hint;
  }
  $("sidebarGlass").addEventListener("change", async (event) => {
    try {
      displaySettings(await invoke("update_settings", { patch: { sidebarGlass: event.target.checked } }));
    } catch (e) {
      console.warn("ResourceWatch: saving sidebarGlass failed", e);
      applyGlass(); // back to the stored value
    }
  });

  // Sidebar collapse: user preference, forced icon-only below 760 px.
  const COLLAPSE_KEY = "resourcewatch-sidebar-collapsed";
  const narrow = matchMedia("(max-width: 760px)");
  let collapsedWanted = false;
  try { collapsedWanted = localStorage.getItem(COLLAPSE_KEY) === "1"; } catch { /* storage blocked */ }
  function applyCollapse() {
    const collapsed = narrow.matches || collapsedWanted;
    $("appShell").classList.toggle("is-collapsed", collapsed);
    const toggle = $("sidebarToggle");
    toggle.setAttribute("aria-expanded", String(!collapsed));
    toggle.title = toggle.ariaLabel = collapsed ? "展開側欄" : "收合側欄";
  }
  $("sidebarToggle").addEventListener("click", () => {
    collapsedWanted = !collapsedWanted;
    try { localStorage.setItem(COLLAPSE_KEY, collapsedWanted ? "1" : "0"); } catch { /* storage blocked */ }
    applyCollapse();
  });
  narrow.addEventListener("change", applyCollapse);
  applyCollapse();

  // ---------------------------------------------------------------- pages
  // live: page shows monitoring data, so snapshots are rendered while it is visible.
  const PAGES = {
    overview: { host: "overviewPage", nav: "navOverview", title: "儀表板", live: true },
    storage: { host: "storagePage", nav: "navStorage", title: "儲存裝置", live: true },
    network: { host: "networkPage", nav: "navNetwork", title: "網路", live: true },
    sensors: { host: "sensorsPage", nav: "navSensors", title: "感測器", live: true },
    apps: { host: "appsPage", nav: "navApps", title: "應用程式" },
    widgets: { host: "widgetsPage", nav: "navWidgets", title: "桌面小工具" },
    settings: { host: "settingsPage", nav: "navSettings", title: "設定" },
  };
  let activePage = "overview";
  const isLive = () => Boolean(PAGES[activePage].live);

  function switchPage(page) {
    if (!PAGES[page]) return;
    activePage = page;
    for (const [key, def] of Object.entries(PAGES)) {
      const selected = key === page;
      $(def.host).hidden = !selected;
      const button = $(def.nav);
      button.classList.toggle("is-active", selected);
      if (selected) button.setAttribute("aria-current", "page");
      else button.removeAttribute("aria-current");
    }
    setText("pageTitle", PAGES[page].title);
    // Snapshots are not rendered while no monitoring page is visible; catch up now.
    if (isLive() && pendingRender) render();
    watchProcesses();
  }
  for (const [key, def] of Object.entries(PAGES)) {
    $(def.nav).addEventListener("click", () => switchPage(key));
  }
  for (const link of all("[data-goto]")) {
    link.addEventListener("click", () => switchPage(link.dataset.goto));
  }

  // ---------------------------------------------------------------- helpers
  const text = (value, fallback = "--") =>
    value === null || value === undefined || value === "" ? fallback : String(value);
  const metricText = (m) => text(m && m.displayValue);
  const pairText = (m) => (m ? `${text(m.usedText)} / ${text(m.totalText)}` : "--");

  // Avoid layout/paint when a value has not changed.
  function writeText(element, value) {
    const next = text(value);
    if (element.textContent !== next) element.textContent = next;
  }
  const bound = groupBy("bind");
  function setText(key, value) {
    for (const element of bound[key] || []) writeText(element, value);
  }

  const percentOf = (m) => (m && Number.isFinite(m.percent) ? Math.min(100, Math.max(0, m.percent)) : 0);
  const rings = groupBy("ring");
  function setRing(key, metric) {
    const percent = percentOf(metric);
    const dash = `${percent.toFixed(1)} 100`;
    // A round line cap draws a dot even at 0 length; hide the arc instead.
    const opacity = percent > 0 ? "" : "0";
    for (const el of rings[key] || []) {
      if (el.style.strokeDasharray !== dash) el.style.strokeDasharray = dash;
      if (el.style.opacity !== opacity) el.style.opacity = opacity;
    }
  }
  const bars = groupBy("bar");
  function setBar(key, metric) {
    // CSSOM is allowed by the CSP (style-src 'self'); inline style attributes are not.
    const width = percentOf(metric).toFixed(1) + "%";
    for (const el of bars[key] || []) if (el.style.width !== width) el.style.width = width;
  }

  function formatRate(value) {
    if (!Number.isFinite(value) || value < 0) return "--";
    const units = ["B/s", "KiB/s", "MiB/s", "GiB/s"];
    let v = value, unit = 0;
    while (v >= 1024 && unit < units.length - 1) { v /= 1024; unit++; }
    return (unit === 0 ? v.toFixed(0) : v.toFixed(1)) + " " + units[unit];
  }

  function uptimeFromSeconds(seconds) {
    const total = Math.max(0, Math.floor(seconds));
    const days = Math.floor(total / 86400);
    const hms = [Math.floor((total % 86400) / 3600), Math.floor((total % 3600) / 60), total % 60]
      .map((v) => String(v).padStart(2, "0")).join(":");
    return days > 0 ? `${days}天 ${hms}` : hms;
  }

  function labeledGpuName(name, vendor) {
    const raw = text(name);
    if (raw === "--") return raw;
    const brand = ({ nvidia: "NVIDIA", amd: "AMD", intel: "Intel" })[String(vendor || "").toLowerCase()];
    if (!brand || new RegExp(`\\b${brand}\\b`, "i").test(raw)) return raw;
    return `${brand} ${raw}`;
  }

  // ---------------------------------------------------------------- charts
  for (const g of all(".plot .grid-lines")) {
    let d = "";
    for (let y = 0; y <= 100; y += 25) d += `M0 ${y} H300 `;
    for (let x = 0; x <= 300; x += 50) d += `M${x} 0 V100 `;
    const path = document.createElementNS(SVG_NS, "path");
    path.setAttribute("d", d);
    g.replaceChildren(path);
  }

  const chartNodes = Object.fromEntries(Object.entries(groupBy("chart")).map(([key, roots]) => [
    key, roots.map((root) => ({ root, line: root.querySelector(".line"), area: root.querySelector(".area") })),
  ]));

  // Missing samples (null) break the line instead of being drawn as 0.
  function chartPaths(values, max) {
    if (!Array.isArray(values)) return null;
    const last = Math.max(1, values.length - 1);
    const upper = max > 0 ? max : 1;
    const xs = [];
    let line = "";
    for (let i = 0; i < values.length; i++) {
      const value = values[i];
      if (!Number.isFinite(value)) continue;
      const x = (i / last * 300).toFixed(2);
      const y = (100 - Math.min(1, Math.max(0, value) / upper) * 100).toFixed(2);
      line += `${line ? " L" : "M"}${x} ${y}`;
      xs.push(x);
    }
    if (!xs.length) return null;
    return { line, area: `M${xs[0]} 100 ${line.replace(/^M/, "L")} L${xs[xs.length - 1]} 100 Z` };
  }

  function chartSeries(name, values, max = 100) {
    const paths = chartPaths(values, max);
    for (const chart of chartNodes[name] || []) {
      chart.root.classList.toggle("is-missing", !paths);
      const line = paths ? paths.line : "", area = paths ? paths.area : "";
      if ((chart.line.getAttribute("d") || "") !== line) chart.line.setAttribute("d", line);
      if ((chart.area.getAttribute("d") || "") !== area) chart.area.setAttribute("d", area);
    }
  }

  // ---------------------------------------------------------------- per-core bars
  const coreBarHosts = all("[data-core-bars]");
  function renderCoreBars(values) {
    const list = Array.isArray(values) ? values : [];
    for (const host of coreBarHosts) {
      if (host.children.length !== list.length) {
        // Rebuilt only when the CPU count changes (CPU hotplug).
        host.replaceChildren(...list.map(() => document.createElement("span")));
      }
      list.forEach((v, i) => {
        const bar = host.children[i];
        const ok = Number.isFinite(v);
        const height = ok ? Math.min(100, Math.max(0, v)).toFixed(1) + "%" : "";
        bar.classList.toggle("is-missing", !ok);
        if (bar.style.height !== height) bar.style.height = height;
        bar.title = `CPU ${i}：${ok ? v.toFixed(1) + "%" : "--"}`;
      });
    }
  }

  // ---------------------------------------------------------------- temperatures and sensors
  // °C / °F is a display preference (localStorage); the backend always sends °C.
  const UNIT_KEY = "resourcewatch-temp-unit";
  let tempUnit = "c";
  try { tempUnit = localStorage.getItem(UNIT_KEY) === "f" ? "f" : "c"; } catch { /* storage blocked */ }
  const formatTemp = (c) => !Number.isFinite(c) ? "--"
    : tempUnit === "f" ? `${(c * 9 / 5 + 32).toFixed(0)} °F` : `${c.toFixed(1)} °C`;
  // Backend texts look like "48.6 °C"; convert them only when °F is chosen.
  function tempText(value) {
    const m = /^(-?\d+(?:\.\d+)?) °C$/.exec(String(value ?? ""));
    return m && tempUnit === "f" ? formatTemp(Number(m[1])) : value;
  }
  const unitButtons = all("[data-temp-unit]");
  function applyUnit() {
    for (const b of unitButtons) b.setAttribute("aria-pressed", String(b.dataset.tempUnit === tempUnit));
  }
  for (const b of unitButtons) {
    b.addEventListener("click", () => {
      tempUnit = b.dataset.tempUnit;
      try { localStorage.setItem(UNIT_KEY, tempUnit); } catch { /* storage blocked */ }
      applyUnit();
      render();
    });
  }
  applyUnit();

  const sensorLists = all("[data-sensor-list]");
  const sensorEmptyNotes = all("[data-sensor-empty]");
  let sensorInventory = null;
  let sensorNodes = []; // per list: value <dd> per shown sensor
  function renderSensors(sensors) {
    const list = Array.isArray(sensors) ? sensors : [];
    const inventory = JSON.stringify(list.map((x) => [x.chip, x.label, x.kind]));
    if (inventory !== sensorInventory) {
      // Rebuilt only on startup / hardware refresh.
      sensorNodes = sensorLists.map((host) => {
        // "all" groups consecutive sensors of one hwmon chip under a heading; short lists inline the chip.
        const grouped = host.dataset.sensorList === "all";
        const limit = grouped ? Infinity : Number(host.dataset.sensorList);
        const children = [];
        const values = [];
        let chip = null;
        for (const x of list.slice(0, limit)) {
          if (grouped && x.chip !== chip) {
            chip = x.chip;
            const head = document.createElement("div");
            head.className = "sensor-group";
            const dt = document.createElement("dt");
            dt.textContent = text(chip);
            head.append(dt);
            children.push(head);
          }
          const row = document.createElement("div");
          const dt = document.createElement("dt");
          dt.textContent = dt.title = grouped ? text(x.label) : `${text(x.chip)} · ${text(x.label)}`;
          const dd = document.createElement("dd");
          row.append(dt, dd);
          children.push(row);
          values.push(dd);
        }
        host.replaceChildren(...children);
        return values;
      });
      for (const note of sensorEmptyNotes) note.hidden = list.length > 0;
      sensorInventory = inventory;
    }
    for (const nodes of sensorNodes) {
      nodes.forEach((dd, i) => {
        const x = list[i];
        writeText(dd, x.kind === "fan" ? (Number.isFinite(x.value) ? `${Math.round(x.value)} RPM` : "--") : formatTemp(x.value));
      });
    }
  }

  // ---------------------------------------------------------------- disks
  const diskLists = all("[data-disk-list]");
  const diskEmptyNotes = all("[data-disk-empty]");

  function makeDiskRow(disk) {
    const row = document.createElement("div");
    row.className = "disk-row";
    const name = document.createElement("span");
    name.className = "disk-name";
    name.textContent = name.title = text(disk.name);
    const model = document.createElement("span");
    model.className = "disk-model";
    model.textContent = model.title = [disk.model, disk.sizeText, disk.transport?.toUpperCase()].map((v) => text(v)).join(" · ");
    const rates = document.createElement("span");
    rates.className = "disk-rates";
    const values = [["disk-read", "讀取"], ["disk-write", "寫入"]].map(([color, label]) => {
      const v = document.createElement("span");
      v.dataset.color = color;
      v.title = label;
      v.textContent = "--";
      rates.append(v);
      return v;
    });
    // Capacity of the filesystems mounted from this disk.
    const usage = document.createElement("span");
    usage.className = "disk-usage";
    const bar = document.createElement("span");
    bar.className = "bar";
    const fill = document.createElement("span");
    fill.dataset.color = "ram";
    bar.append(fill);
    const usageText = document.createElement("span");
    usageText.textContent = "--";
    usage.append(bar, usageText);
    row.append(name, rates, model, usage);
    return { row, values: [...values, fill, usageText] };
  }

  const diskStepButtons = all("[data-disk-step]");
  for (const b of diskStepButtons) {
    b.addEventListener("click", () => {
      const n = Array.isArray(state?.disks) ? state.disks.length : 0;
      if (!n) return;
      summaryDisk = (summaryDisk + Number(b.dataset.diskStep) + n) % n;
      render();
    });
  }

  // ---------------------------------------------------------------- render
  let state = null;
  let pendingRender = false;
  let lastDiskInventory = null;
  let diskStatNodes = []; // per list: per disk: [read, write, usageFill, usageText]
  let summaryDisk = 0; // disk shown in the summary card (‹ › buttons)
  let uptimeAnchorSeconds = null;
  let uptimeAnchorAt = 0;
  let lastObservedUptimeSeconds = null;

  function refreshUptimeText() {
    if (uptimeAnchorSeconds === null) return;
    const elapsed = Math.max(0, (performance.now() - uptimeAnchorAt) / 1000);
    setText("uptime", uptimeFromSeconds(uptimeAnchorSeconds + elapsed));
  }
  // One text update per second from a local monotonic clock; no IPC, no process.
  setInterval(() => { if (isLive()) refreshUptimeText(); }, 1000);

  function render() {
    pendingRender = false;
    const s = state;
    if (!s) return;
    setText("hostname", s.hostname);
    setText("machineModel", s.machineModel);
    setText("cpuName", s.cpuName);
    setText("cpuUsage", metricText(s.cpu));
    setText("cpuSpeed", s.cpuFreqText);
    setText("cpuCores", s.cpuCores);
    setText("cpuThreads", s.cpuThreads);
    setText("cpuTemp", tempText(s.cpuTempText));
    renderSensors(s.sensors);
    setRing("cpu", s.cpu);
    renderCoreBars(s.cpuPerCore);
    if (Number.isFinite(s.uptimeSeconds)) {
      if (s.uptimeSeconds !== lastObservedUptimeSeconds) {
        lastObservedUptimeSeconds = s.uptimeSeconds;
        uptimeAnchorSeconds = s.uptimeSeconds;
        uptimeAnchorAt = performance.now();
      }
      refreshUptimeText();
    } else {
      uptimeAnchorSeconds = null;
      setText("uptime", s.uptimeText);
    }
    fillSettingSelect("gpuIndex", (s.gpus || []).map((g) => [String(g.index), `GPU${g.index} · ${labeledGpuName(g.name, g.vendor)}`]));
    setText("gpuName", labeledGpuName(s.gpuName, s.gpuVendor));
    setText("gpuUsage", metricText(s.gpu));
    setText("gpuTemp", tempText(s.gpuTempText));
    setText("gpuVram", pairText(s.vram));
    setText("encoder", metricText(s.encoder));
    setText("decoder", metricText(s.decoder));
    setRing("gpu", s.gpu);
    setBar("vram", s.vram);
    setText("ramTotal", s.ramSubtitle ?? s.ram?.totalText);
    setText("ramUsage", metricText(s.ram));
    setText("ramUsed", s.ram?.usedText);
    setText("ramSpeed", s.ramSpeedText);
    setText("swapUsed", pairText(s.swap));
    setRing("ram", s.ram);
    const parts = s.ramBreakdown || {};
    for (const [key, metric] of [["ramApp", parts.app], ["ramCache", parts.cache], ["ramFree", parts.free]]) {
      setText(key, metric?.usedText);
      setBar(key, metric);
    }
    setText("memPressure", Number.isFinite(s.memPressure) ? s.memPressure.toFixed(2) + "%" : "--");
    setText("netInterfaceName", s.netInterfaceName);
    setText("netIp", s.netIp);
    setText("netMac", s.netMac);
    setText("netGateway", s.netGateway);
    fillSettingSelect("netInterface", [["all", "所有實體介面"], ...(s.netInterfaces || []).map((i) => [i, i])]);
    setText("netUp", metricText(s.netUp));
    setText("netDown", metricText(s.netDown));

    const battery = s.battery;
    $("batteryCard").hidden = !battery;
    if (battery) {
      const percent = Number.isFinite(battery.percent) ? battery.percent : null;
      setText("batteryPercent", percent === null ? null : `${Math.round(percent)}%`);
      setBar("battery", { percent });
      const status = { charging: "充電中", discharging: "使用電池", full: "已充滿", notCharging: "未充電" }[battery.status] || "未知";
      setText("batteryStatus", status);
      const prefix = { charging: "充滿還需 ", discharging: "剩餘 " }[battery.status];
      setText("batteryTime", prefix && battery.timeText !== "--" ? prefix + battery.timeText : "--");
      setText("batteryHealth", Number.isFinite(battery.health) ? `${battery.health.toFixed(0)}%` : null);
      setText("batteryCycles", battery.cycles);
      setText("batteryNow", battery.nowText);
      setText("batteryFull", battery.fullText);
      setText("batteryDesign", battery.designText);
    }

    const disks = Array.isArray(s.disks) ? s.disks : [];
    const inventory = JSON.stringify(disks.map((d) => [d.name, d.model, d.sizeText, d.transport]));
    if (inventory !== lastDiskInventory) {
      // Inventory changes only on startup / manual refresh.
      diskStatNodes = diskLists.map((list) => {
        const built = disks.map(makeDiskRow);
        list.replaceChildren(...built.map((b) => b.row));
        return built.map((b) => b.values);
      });
      for (const note of diskEmptyNotes) note.hidden = disks.length > 0;
      lastDiskInventory = inventory;
    }
    disks.forEach((disk, i) => {
      const rate = s.diskRates?.[disk.name] || {};
      const read = formatRate(rate.read), write = formatRate(rate.write);
      const usage = s.diskUsage?.[disk.name];
      const usageText = usage ? `${text(usage.usedText)} / ${text(usage.totalText)}` : "未掛載";
      const width = percentOf(usage).toFixed(1) + "%";
      for (const list of diskStatNodes) {
        writeText(list[i][0], read);
        writeText(list[i][1], write);
        if (list[i][2].style.width !== width) list[i][2].style.width = width;
        writeText(list[i][3], usageText);
      }
    });
    summaryDisk = disks.length ? Math.min(summaryDisk, disks.length - 1) : 0;
    const shown = disks[summaryDisk];
    const shownUsage = shown && s.diskUsage?.[shown.name];
    setText("sumDiskName", shown ? shown.name : "未偵測到硬碟");
    setText("sumDiskUsed", shownUsage?.usedText);
    setText("sumDiskTotal", shownUsage?.totalText);
    setText("sumDiskPercent", shownUsage?.displayValue);
    for (const b of diskStepButtons) b.hidden = disks.length < 2;

    const h = s.history || {};
    chartSeries("cpu", h.cpu);
    // Pressure is usually near 0: scale to the recent peak (at least 10%) so changes are visible.
    let pressureMax = 10;
    for (const v of h.memPressure || []) if (Number.isFinite(v) && v > pressureMax) pressureMax = v;
    chartSeries("memPressure", h.memPressure, pressureMax);
    // Paired series share one scale so the two lines are comparable.
    const pair = (a, b, maxKey) => {
      let max = 1;
      for (const v of [...(h[a] || []), ...(h[b] || [])]) if (Number.isFinite(v) && v > max) max = v;
      chartSeries(a, h[a], max * 1.12);
      chartSeries(b, h[b], max * 1.12);
      setText(maxKey, formatRate(max));
    };
    pair("netUp", "netDown", "netMax");
    pair("diskRead", "diskWrite", "diskIoMax");
    const last = (series) => (Array.isArray(series) && series.length ? series[series.length - 1] : null);
    setText("diskReadNow", formatRate(last(h.diskRead)));
    setText("diskWriteNow", formatRate(last(h.diskWrite)));
  }

  function setLiveStatus(label, kind) {
    const el = $("appLiveStatus");
    writeText(el, label);
    el.classList.toggle("is-live", kind === "live");
  }

  function receiveSnapshot(snapshot) {
    if (!snapshot || typeof snapshot !== "object") return;
    state = snapshot;
    const mock = snapshot.dataSource === "mock";
    $("mockBanner").hidden = !mock;
    setLiveStatus(mock ? "模擬資料 · 非實機" : "即時監控", mock ? "mock" : "live");
    setText("version", "v" + text(snapshot.version));
    if (isLive()) render();
    else pendingRender = true;
  }

  // ---------------------------------------------------------------- setting pickers on cards
  // <select data-setting-select="key">: options come from the snapshot, the
  // value from the current settings; choosing writes through update_settings.
  let currentSettings = {};
  const settingSelects = all("[data-setting-select]");
  function syncSettingSelects() {
    for (const sel of settingSelects) {
      const want = String(currentSettings[sel.dataset.settingSelect] ?? "");
      // A saved value missing from the list (e.g. an unplugged interface) stays selectable.
      if (want && ![...sel.options].some((o) => o.value === want)) sel.append(new Option(want, want));
      if (document.activeElement !== sel && sel.value !== want) sel.value = want;
    }
  }
  function fillSettingSelect(key, options) {
    const signature = JSON.stringify(options);
    for (const sel of settingSelects) {
      if (sel.dataset.settingSelect !== key || sel.dataset.options === signature) continue;
      sel.replaceChildren(...options.map(([value, label]) => new Option(label, value)));
      sel.dataset.options = signature;
    }
    syncSettingSelects();
  }
  for (const sel of settingSelects) {
    sel.addEventListener("change", async () => {
      const key = sel.dataset.settingSelect;
      const value = SPECS[key]?.text ? sel.value : Number(sel.value);
      if (!invoke) return;
      try {
        displaySettings(await invoke("update_settings", { patch: { [key]: value } }));
      } catch (e) {
        console.warn("ResourceWatch: setting rejected", e);
        syncSettingSelects(); // back to the stored value
      }
    });
  }

  // ---------------------------------------------------------------- settings
  const SPECS = {
    updateInterval: { min: 100, max: 10000, step: 50 },
    historyPoints: { min: 10, max: 120, step: 10 },
    gpuIndex: { min: 0, max: 15, step: 1 },
    netInterface: { text: true },
  };
  const fields = Object.fromEntries(Object.keys(SPECS).map((k) => [k, $(k)]));

  function setFieldError(key, message) {
    // Not setText(): that renders an empty value as "--", which would show a
    // bogus error under every valid field.
    const el = $(key + "Error");
    const next = message || "";
    if (el && el.textContent !== next) el.textContent = next;
    if (message) fields[key].setAttribute("aria-invalid", "true");
    else fields[key].removeAttribute("aria-invalid");
  }

  function displaySettings(settings) {
    if (!settings || typeof settings !== "object") return;
    currentSettings = { ...currentSettings, ...settings };
    document.documentElement.dataset.motion = currentSettings.updateInterval >= 1000 ? "on" : "off";
    if (typeof settings.sidebarGlass === "boolean") {
      glassSetting = settings.sidebarGlass;
      if (glassAtStart === null) glassAtStart = glassSetting;
      applyGlass();
    }
    syncSettingSelects();
    for (const [key, field] of Object.entries(fields)) {
      if (!(key in settings) || document.activeElement === field) continue;
      field.value = String(settings[key]);
      setFieldError(key, "");
    }
  }

  // Client-side check is only for quick feedback; Rust validates again.
  function readField(key) {
    const spec = SPECS[key], field = fields[key];
    if (spec.text) return { value: field.value.trim() || "all" };
    const v = field.valueAsNumber;
    if (!Number.isInteger(v) || v < spec.min || v > spec.max || (v - spec.min) % spec.step !== 0) {
      return { error: `必須介於 ${spec.min}–${spec.max}，步進 ${spec.step}` };
    }
    return { value: v };
  }

  function errorMessage(error) {
    if (error && typeof error === "object" && error.message) return error.message;
    return text(error, "未知錯誤");
  }

  for (const key of Object.keys(fields)) {
    fields[key].addEventListener("change", async () => {
      const { value, error } = readField(key);
      if (error) { setFieldError(key, error); return; }
      if (!invoke) { setFieldError(key, "未連線到應用程式後端"); return; }
      try {
        const settings = await invoke("update_settings", { patch: { [key]: value } });
        setFieldError(key, "");
        fields[key].value = String(settings[key]);
      } catch (e) {
        // invalidSetting → that field; settingsWriteFailed (field null) → the
        // edited field, so the error stays next to what the user changed.
        setFieldError(e?.field || key, errorMessage(e));
      }
    });
  }

  const feedback = $("refreshFeedback");
  function showFeedback(message, isError) {
    feedback.textContent = message;
    feedback.classList.toggle("is-error", Boolean(isError));
  }

  document.querySelectorAll("[data-refresh]").forEach((button) => {
    button.addEventListener("click", async () => {
      if (!invoke) { showFeedback("未連線到應用程式後端。", true); return; }
      button.disabled = true;
      try {
        receiveSnapshot(await invoke("refresh_devices", { kind: button.dataset.refresh }));
        showFeedback(`已重新查詢（${button.textContent.trim()}）。`, false);
      } catch (e) {
        showFeedback("刷新失敗：" + errorMessage(e), true);
      } finally {
        button.disabled = false;
      }
    });
  });

  $("resetSettings").addEventListener("click", async () => {
    if (!invoke) return;
    try {
      const settings = await invoke("reset_settings");
      if (document.activeElement) document.activeElement.blur();
      displaySettings(settings);
      showFeedback("已恢復預設值並儲存。", false);
    } catch (e) {
      showFeedback("恢復預設值失敗：" + errorMessage(e), true);
    }
  });

  // Where settings are stored and any problem found while loading them.
  async function showSettingsInfo() {
    try {
      const info = await invoke("get_settings_info");
      writeText($("settingsPath"), info.path ? `設定檔：${info.path}` : "設定目錄無法使用，設定只保存在記憶體中。");
      const warning = $("settingsWarning");
      warning.hidden = !info.loadWarning;
      warning.textContent = info.loadWarning || "";
    } catch (e) {
      console.warn("ResourceWatch: settings info unavailable", e);
    }
  }

  // ---------------------------------------------------------------- startup
  async function start() {
    if (!invoke || !listen) {
      // Opened outside Tauri (e.g. a browser for styling): no data at all.
      setLiveStatus("未連線到應用程式後端", null);
      return;
    }
    try {
      await listen("snapshot", (event) => receiveSnapshot(event.payload));
      await listen("settings-changed", (event) => displaySettings(event.payload));
      displaySettings(await invoke("get_settings"));
      showSettingsInfo();
      invoke("get_window_info").then((info) => { glassActive = Boolean(info?.glass); applyGlass(); })
        .catch((e) => console.warn("ResourceWatch: window info unavailable", e));
      receiveSnapshot(await invoke("get_snapshot"));
    } catch (e) {
      console.error("ResourceWatch: backend connection failed", e);
      setLiveStatus("後端連線失敗", null);
    }
  }
  // ---------------------------------------------------------------- processes
  // Polled (get_processes) only while the dashboard or the applications page is visible.
  const PROC_INTERVAL = 2000;
  const procLists = all("[data-proc-list]");
  const procTable = document.querySelector("[data-proc-table]");
  const procSortButtons = all("[data-proc-sort]");
  let procSort = "cpu";
  let procs = [];
  let procPolling = false;
  const procVisible = () => !document.hidden && (activePage === "overview" || activePage === "apps");
  const cpuText = (c) => (Number.isFinite(c) ? `${c.toFixed(1)}%` : "--");
  const memText = (b) => (b >= 2 ** 30 ? `${(b / 2 ** 30).toFixed(1)} GiB` : `${Math.round(b / 2 ** 20)} MiB`);
  const procValue = { cpu: (p) => cpuText(p.cpu), memory: (p) => memText(p.memory) };
  const sortedBy = (key) => [...procs].sort((a, b) => (b[key] ?? -1) - (a[key] ?? -1));

  function renderProcesses() {
    for (const host of procLists) {
      const key = host.dataset.procList;
      host.replaceChildren(...sortedBy(key).slice(0, 5).map((p) => {
        const row = document.createElement("div");
        const dt = document.createElement("dt");
        dt.textContent = dt.title = p.name;
        const dd = document.createElement("dd");
        dd.textContent = procValue[key](p);
        row.append(dt, dd);
        return row;
      }));
    }
    setText("procCount", `${procs.length} 個程序`);
    if (activePage !== "apps") return;
    procTable.replaceChildren(...sortedBy(procSort).map((p) => {
      const tr = document.createElement("tr");
      for (const value of [p.name, p.pid, cpuText(p.cpu), memText(p.memory)]) {
        const td = document.createElement("td");
        td.textContent = value;
        tr.append(td);
      }
      tr.firstChild.title = p.name;
      return tr;
    }));
  }
  function applyProcSort() {
    for (const b of procSortButtons) {
      if (b.dataset.procSort === procSort) b.parentElement.setAttribute("aria-sort", "descending");
      else b.parentElement.removeAttribute("aria-sort");
    }
  }
  for (const b of procSortButtons) {
    b.addEventListener("click", () => { procSort = b.dataset.procSort; applyProcSort(); renderProcesses(); });
  }
  applyProcSort();

  async function watchProcesses() {
    if (procPolling || !invoke) return;
    procPolling = true;
    // The first scan has no CPU share yet, so the second one follows quickly.
    let delay = 500;
    while (procVisible()) {
      try {
        procs = await invoke("get_processes");
        renderProcesses();
      } catch (e) {
        console.warn("ResourceWatch: process list unavailable", e);
      }
      await new Promise((resolve) => setTimeout(resolve, delay));
      delay = PROC_INTERVAL;
    }
    procPolling = false;
  }
  document.addEventListener("visibilitychange", watchProcesses);

  switchPage("overview");
  start();
})();
