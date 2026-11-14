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

  // Sidebar glass: needs the compositor blur (get_window_info) and the user's
  // preference; otherwise the sidebar stays opaque.
  const GLASS_KEY = "resourcewatch-glass";
  let glassAvailable = false;
  let glassWanted = true;
  try { glassWanted = localStorage.getItem(GLASS_KEY) !== "off"; } catch { /* storage blocked */ }
  function applyGlass() {
    document.documentElement.dataset.glass = glassAvailable && glassWanted ? "on" : "off";
    const box = $("sidebarGlass");
    box.checked = glassWanted;
    box.disabled = !glassAvailable;
    $("sidebarGlassHint").textContent = glassAvailable
      ? "側欄半透明並模糊桌面背景"
      : "目前桌面環境不支援視窗模糊（需要 KDE Plasma 的模糊特效），側欄維持不透明";
  }
  $("sidebarGlass").addEventListener("change", (event) => {
    glassWanted = event.target.checked;
    try { localStorage.setItem(GLASS_KEY, glassWanted ? "on" : "off"); } catch { /* storage blocked */ }
    applyGlass();
  });
  applyGlass();

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
    row.append(name, rates, model);
    return { row, values };
  }

  // ---------------------------------------------------------------- render
  let state = null;
  let pendingRender = false;
  let lastDiskInventory = null;
  let diskStatNodes = []; // per list: per disk: [readNode, writeNode]
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
    setText("cpuTemp", s.cpuTempText);
    setRing("cpu", s.cpu);
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
    setText("gpuHeading", "GPU" + (s.gpuIndex ?? 0));
    setText("gpuName", labeledGpuName(s.gpuName, s.gpuVendor));
    setText("gpuUsage", metricText(s.gpu));
    setText("gpuTemp", s.gpuTempText);
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
    setBar("ram", s.ram);
    setText("netSubtitle", (s.netHasInterface || s.netIpAvailable) ? `IPv4: ${text(s.netIp)}` : "所有網路介面");
    setText("netUp", metricText(s.netUp));
    setText("netDown", metricText(s.netDown));

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
      for (const list of diskStatNodes) {
        writeText(list[i][0], read);
        writeText(list[i][1], write);
      }
    });

    const h = s.history || {};
    chartSeries("cpu", h.cpu);
    chartSeries("ram", h.ram);
    // Upload and download share one scale so the two lines are comparable.
    let max = 1;
    for (const v of [...(h.netUp || []), ...(h.netDown || [])]) if (Number.isFinite(v) && v > max) max = v;
    chartSeries("netUp", h.netUp, max * 1.12);
    chartSeries("netDown", h.netDown, max * 1.12);
    setText("netMax", formatRate(max));
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
      invoke("get_window_info").then((info) => { glassAvailable = Boolean(info?.glass); applyGlass(); })
        .catch((e) => console.warn("ResourceWatch: window info unavailable", e));
      receiveSnapshot(await invoke("get_snapshot"));
    } catch (e) {
      console.error("ResourceWatch: backend connection failed", e);
      setLiveStatus("後端連線失敗", null);
    }
  }
  switchPage("overview");
  start();
})();
