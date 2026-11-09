.pragma library

// 依系統語系（zh*）顯示繁體中文，其他語系顯示英文。
var en = {
    usage: "Usage", speed: "Speed", temp: "Temp", uptime: "Uptime",
    cpuUsageChart: "CPU usage", gpuCompute: "GPU usage", vram: "VRAM",
    ramUsage: "RAM usage", inUse: "In use", swapInUse: "Swap in use", gpuMemory: "GPU memory",
    disk: "Disk", allDisks: "All disks", read: "Read speed", write: "Write speed",
    network: "Network", upload: "Upload", download: "Download",
    throughput: "Throughput", allInterfaces: "All interfaces", settings: "Settings",
    cfgInterval: "Update interval (ms):", cfgHistory: "History points:", cfgGpu: "GPU index:",
    // cfgDisk1Id: "Disk 1 sensor ID:", cfgDisk1Label: "Disk 1 label:",
    // cfgDisk2Id: "Disk 2 sensor ID:", cfgDisk2Label: "Disk 2 label:",
    cfgNet: "Network interface:",
    // cfgHintDisk: "Leave Disk 1 ID empty to use the all-disks total; leave Disk 2 ID empty to hide it. Find IDs with: kstatsviewer --list | grep -E 'disk/.*/read'",
    cfgHintNet: "Use 'all' for the total of every interface, or an interface name such as enp4s0 (see: ip -br a). A named interface also shows its IPv4 address."
};

var zh = {
    usage: "使用率", speed: "速度", temp: "溫度", uptime: "運作時間",
    cpuUsageChart: "CPU使用率", gpuCompute: "GPU使用率", vram: "VRAM",
    ramUsage: "RAM使用率", inUse: "使用中", swapInUse: "SWAP使用中", gpuMemory: "GPU記憶體",
    disk: "硬碟", allDisks: "所有硬碟", read: "讀取速度", write: "寫入速度",
    network: "網路", upload: "上傳", download: "下載",
    throughput: "傳輸速度", allInterfaces: "所有網路介面", settings: "設定",
    cfgInterval: "更新間隔（毫秒）：", cfgHistory: "歷史圖點數：", cfgGpu: "GPU 編號：",
    // cfgDisk1Id: "硬碟 1 感測器 ID：", cfgDisk1Label: "硬碟 1 名稱：",
    // cfgDisk2Id: "硬碟 2 感測器 ID：", cfgDisk2Label: "硬碟 2 名稱：",
    cfgNet: "網路介面：",
    // cfgHintDisk: "硬碟 1 的 ID 留空＝使用所有硬碟的合計；硬碟 2 的 ID 留空＝隱藏該卡片。查詢 ID：kstatsviewer --list | grep -E 'disk/.*/read'",
    cfgHintNet: "填 all＝所有介面合計；或填介面名稱如 enp4s0（用 ip -br a 查詢）。指定介面時會一併顯示 IPv4 位址。"
};

function t(key, localeName) {
    var table = (localeName && localeName.indexOf("zh") === 0) ? zh : en;
    if (table[key] !== undefined) return table[key];
    return en[key] !== undefined ? en[key] : key;
}
