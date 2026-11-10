/* Plasma config HTML editor. The QML cfg_* properties, not this JS, own state.
 * Browser mode is for styling only: no persistence and no KDE connection.
 */
(() => {
  "use strict";
  const specs = {updateInterval: {min:100, max:10000, step:50}};
  specs.historyPoints = {min:10,max:120,step:10};
  specs.gpuIndex = {min:0,max:15,step:1};
  specs.netInterface = {text:true};
  const fields = Object.fromEntries(Object.keys(specs).map(key => [key, document.getElementById(key)]));
  const indicator = document.getElementById("connectionState");
  const note = document.getElementById("previewNote");
  let bridge = null;

  function readValid(key, input) {
    const spec = specs[key];
    if (spec.text) return input.value.trim() || "all";
    const value = input.valueAsNumber;
    if (!Number.isInteger(value) || value < spec.min || value > spec.max ||
        (value - spec.min) % spec.step !== 0) return null;
    return value;
  }

  function displayState(data) {
    if (!data || typeof data !== "object") return;
    Object.keys(fields).forEach(key => {
      if (!Object.prototype.hasOwnProperty.call(data, key)) return;
      const field = fields[key];
      // Preserve the focused text cursor; committed changes use 'change' event.
      if (document.activeElement === field) return;
      const next = data[key];
      if (next !== null && next !== undefined) field.value = String(next);
    });
  }

  Object.entries(fields).forEach(([key, field]) => {
    field.addEventListener("change", () => {
      const value = readValid(key, field);
      if (value === null) { field.reportValidity(); return; }
      if (bridge) bridge.setSetting(key, value);
      // In a browser this only changes the preview; no storage involved.
    });
  });

  const feedback = document.getElementById("refreshFeedback");
  [["refreshIPv4", "ipv4"], ["refreshDisks", "disks"]].forEach(([id, kind]) => {
    document.getElementById(id).addEventListener("click", () => {
      if (!bridge) {
        feedback.textContent = "僅能從 KDE 設定視窗執行裝置刷新。";
        return;
      }
      bridge.refreshDevice(kind, accepted => {
        feedback.textContent = accepted
          ? "已送出重新查詢請求。"
          : "無法送出刷新請求，請查看 KDE 記錄。";
      });
    });
  });

  function connect() {
    if (!window.qt || !window.qt.webChannelTransport) return;
    const script = document.createElement("script");
    script.src = "qrc:///qtwebchannel/qwebchannel.js";
    script.onload = () => {
      new QWebChannel(qt.webChannelTransport, channel => {
        bridge = channel.objects.resourceSettings;
        if (!bridge) return;
        indicator.textContent = "已連線至 KDE";
        indicator.classList.add("connected");
        note.hidden = true;
        const refresh = json => {
          try { displayState(JSON.parse(json)); }
          catch (error) { console.warn("ResourceWatch settings snapshot error", error); }
        };
        refresh(bridge.settingsJson);
        bridge.settingsJsonChanged.connect(refresh);
        bridge.refresh();
      });
    };
    script.onerror = () => { indicator.textContent = "QtWebChannel 載入失敗"; };
    document.head.appendChild(script);
  }
  connect();
})();
