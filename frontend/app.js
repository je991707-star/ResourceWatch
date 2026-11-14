/* ResourceWatch Tauri frontend — display only.
 *
 * All values come from the Rust backend through Tauri IPC:
 *   commands: get_snapshot, get_settings, update_settings, reset_settings, refresh_devices
 *   events:   snapshot, settings-changed
 * This file never reads hardware, files, or the network.
 */
(() => {
  "use strict";
  const $ = (id) => document.getElementById(id);
  const SVG_NS = "http://www.w3.org/2000/svg";
  const tauri = window.__TAURI__;
  const invoke = tauri?.core?.invoke;
  const listen = tauri?.event?.listen;

  // ---------------------------------------------------------------- pages
  const PAGES = {
    overview: { host: "overviewPage", nav: "navOverview", title: "總覽", subtitle: "CPU、GPU、記憶體、磁碟與網路狀態" },
    widgets: { host: "widgetsPage", nav: "navWidgets", title: "桌面小工具", subtitle: "預留入口，Plasma 桌面小工具尚未實作" },
    settings: { host: "settingsPage", nav: "navSettings", title: "設定", subtitle: "更新間隔、監控裝置與手動刷新" },
  };
  let activePage = "overview";

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
    setText("appPageTitle", PAGES[page].title);
    setText("appPageSubtitle", PAGES[page].subtitle);
    // Snapshots are not rendered while the overview is hidden; catch up now.
    if (page === "overview" && pendingRender) render();
  }
  for (const [key, def] of Object.entries(PAGES)) {
    $(def.nav).addEventListener("click", () => switchPage(key));
  }

  // ---------------------------------------------------------------- helpers
  const text = (value, fallback = "--") =>
    value === null || value === undefined || value === "" ? fallback : String(value);
  const metricText = (m) => text(m && m.displayValue);

  // Avoid layout/paint when a value has not changed.
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
  document.querySelectorAll(".plot svg").forEach((svg) => {
    const g = svg.querySelector(".grid-lines");
    if (!g) return;
    let d = "";
    for (let y = 0; y <= 100; y += 20) d += `M0 ${y} H300 `;
    for (let x = 0; x <= 300; x += 30) d += `M${x} 0 V100 `;
    const path = document.createElementNS(SVG_NS, "path");
    path.setAttribute("d", d);
    g.replaceChildren(path);
  });

  const chartNodes = Object.fromEntries(
    Array.from(document.querySelectorAll("[data-chart]"), (chart) => [
      chart.dataset.chart,
      { root: chart, line: chart.querySelector(".line"), area: chart.querySelector(".area") },
    ])
  );

  function clearChart(chart) {
    chart.root.classList.add("is-missing");
    if (chart.line.getAttribute("d")) chart.line.setAttribute("d", "");
    if (chart.area.getAttribute("d")) chart.area.setAttribute("d", "");
  }

  // Missing samples (null) break the line instead of being drawn as 0.
  function chartSeries(name, values, max = 100) {
    const chart = chartNodes[name];
    if (!chart) return;
    if (!Array.isArray(values)) { clearChart(chart); return; }
    const last = Math.max(1, values.length - 1);
    const upper = max > 0 ? max : 1;
    const xs = [];
    let path = "";
    for (let i = 0; i < values.length; i++) {
      const value = values[i];
      if (!Number.isFinite(value)) continue;
      const x = (i / last * 300).toFixed(2);
      const y = (100 - Math.min(1, Math.max(0, value) / upper) * 100).toFixed(2);
      path += `${path ? " L" : "M"}${x} ${y}`;
      xs.push(x);
    }
    if (!xs.length) { clearChart(chart); return; }
    chart.root.classList.remove("is-missing");
    const areaPath = `M${xs[0]} 100 ${path.replace(/^M/, "L")} L${xs[xs.length - 1]} 100 Z`;
    if (chart.line.getAttribute("d") !== path) chart.line.setAttribute("d", path);
    if (chart.area.getAttribute("d") !== areaPath) chart.area.setAttribute("d", areaPath);
  }

  // ---------------------------------------------------------------- disks
  function makeDiskCard(disk, idx) {
    const card = document.createElement("section");
    card.className = "monitor-card monitor-card--disk";
    card.setAttribute("aria-label", "硬碟 " + (idx + 1));
    const header = document.createElement("div");
    header.className = "disk-header";
    const h2 = document.createElement("h2");
    h2.textContent = "硬碟" + (idx + 1);
    const name = document.createElement("span");
    name.className = "disk-name";
    name.textContent = name.title = text(disk.name);
    header.append(h2, name);
    const model = document.createElement("div");
    model.className = "disk-model";
    model.textContent = model.title = `${text(disk.model)} · ${text(disk.sizeText)}`;
    const stats = document.createElement("div");
    stats.className = "disk-stats";
    const values = ["讀取速度", "寫入速度"].map((label) => {
      const stat = document.createElement("div");
      stat.className = "stat";
      const l = document.createElement("span");
      l.className = "stat-label";
      l.textContent = label;
      const v = document.createElement("strong");
      v.className = "stat-value";
      v.textContent = "--";
      stat.append(l, v);
      stats.append(stat);
      return v;
    });
    card.append(header, model, stats);
    return { card, values };
  }

  // ---------------------------------------------------------------- render
  let state = null;
  let pendingRender = false;
  let lastDiskInventory = null;
  let diskStatNodes = [];
  let uptimeAnchorSeconds = null;
  let uptimeAnchorAt = 0;
  let lastObservedUptimeSeconds = null;

  function refreshUptimeText() {
    if (uptimeAnchorSeconds === null) return;
    const elapsed = Math.max(0, (performance.now() - uptimeAnchorAt) / 1000);
    setText("cpuUptime", uptimeFromSeconds(uptimeAnchorSeconds + elapsed));
  }
  // One text update per second from a local monotonic clock; no IPC, no process.
  setInterval(() => { if (activePage === "overview") refreshUptimeText(); }, 1000);

  function render() {
    pendingRender = false;
    const s = state;
    if (!s) return;
    setText("cpuName", s.cpuName);
    setText("cpuUsage", metricText(s.cpu));
    setText("cpuChartUsage", metricText(s.cpu));
    setText("cpuSpeed", s.cpuFreqText);
    setText("cpuTemp", s.cpuTempText);
    if (Number.isFinite(s.uptimeSeconds)) {
      if (s.uptimeSeconds !== lastObservedUptimeSeconds) {
        lastObservedUptimeSeconds = s.uptimeSeconds;
        uptimeAnchorSeconds = s.uptimeSeconds;
        uptimeAnchorAt = performance.now();
      }
      refreshUptimeText();
    } else {
      uptimeAnchorSeconds = null;
      setText("cpuUptime", s.uptimeText);
    }
    setText("gpuHeading", "GPU" + (s.gpuIndex ?? 0));
    setText("gpuName", labeledGpuName(s.gpuName, s.gpuVendor));
    setText("gpuUsage", metricText(s.gpu));
    setText("gpuChartUsage", metricText(s.gpu));
    setText("gpuTemp", s.gpuTempText);
    setText("gpuVram", s.vram ? `${text(s.vram.usedText)} / ${text(s.vram.totalText)}` : "--");
    setText("vramGraphMax", s.vram?.totalText);
    setText("encoderLabel", metricText(s.encoder));
    setText("decoderLabel", metricText(s.decoder));
    setText("ramSubtitle", s.ramSubtitle ?? s.ram?.totalText);
    setText("ramUsage", metricText(s.ram));
    setText("ramChartUsage", s.ram?.usedText);
    setText("ramSpeed", s.ramSpeedText);
    setText("swapUsed", s.swap ? `${text(s.swap.usedText)} / ${text(s.swap.totalText)}` : "--");
    setText("netSubtitle", (s.netHasInterface || s.netIpAvailable) ? `IPv4: ${text(s.netIp)}` : "所有網路介面");
    setText("netUp", metricText(s.netUp));
    setText("netDown", metricText(s.netDown));

    const disks = Array.isArray(s.disks) ? s.disks : [];
    const inventory = JSON.stringify(disks.map((d) => [d.name, d.model, d.sizeText]));
    if (inventory !== lastDiskInventory) {
      // Inventory changes only on startup / manual refresh.
      const built = disks.map(makeDiskCard);
      $("disksGrid").replaceChildren(...built.map((b) => b.card));
      diskStatNodes = built.map((b) => b.values);
      lastDiskInventory = inventory;
    }
    diskStatNodes.forEach((nodes, i) => {
      const rate = s.diskRates?.[disks[i]?.name] || {};
      const read = formatRate(rate.read), write = formatRate(rate.write);
      if (nodes[0].textContent !== read) nodes[0].textContent = read;
      if (nodes[1].textContent !== write) nodes[1].textContent = write;
    });

    const h = s.history || {};
    chartSeries("cpu", h.cpu);
    chartSeries("gpu", h.gpu);
    chartSeries("vram", h.vram);
    chartSeries("ram", h.ram);
    chartSeries("encode", h.encode);
    chartSeries("decode", h.decode);
    const up = Array.isArray(h.netUp) ? h.netUp : [];
    const down = Array.isArray(h.netDown) ? h.netDown : [];
    const net = [];
    let max = 1;
    for (let i = 0, n = Math.max(up.length, down.length); i < n; i++) {
      const a = up[i], b = down[i];
      if (!Number.isFinite(a) && !Number.isFinite(b)) { net.push(null); continue; }
      const value = Math.max(0, Number.isFinite(a) ? a : 0, Number.isFinite(b) ? b : 0);
      net.push(value);
      if (value > max) max = value;
    }
    chartSeries("network", net.length ? net : null, max * 1.12);
    setText("networkGraphMax", formatRate(max));
  }

  function setLiveStatus(label, kind) {
    const el = $("appLiveStatus");
    setText("appLiveStatus", label);
    el.classList.toggle("is-live", kind === "live");
    el.classList.toggle("is-mock", kind === "mock");
  }

  function receiveSnapshot(snapshot) {
    if (!snapshot || typeof snapshot !== "object") return;
    state = snapshot;
    const mock = snapshot.dataSource === "mock";
    $("mockBanner").hidden = !mock;
    setLiveStatus(mock ? "模擬資料 · 非實機" : "即時監控", mock ? "mock" : "live");
    setText("appVersion", "v" + text(snapshot.version));
    if (activePage === "overview") render();
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
      setText("settingsPath", info.path ? `設定檔：${info.path}` : "設定目錄無法使用，設定只保存在記憶體中。");
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
      receiveSnapshot(await invoke("get_snapshot"));
    } catch (e) {
      console.error("ResourceWatch: backend connection failed", e);
      setLiveStatus("後端連線失敗", null);
    }
  }
  switchPage("overview");
  start();
})();
