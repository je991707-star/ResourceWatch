.pragma library

// 純函式：不依賴感測器、不產生副作用。
// 原則：無效值一律回傳 null / "--"，絕不轉成 0。

function isValidNumber(v) {
    return typeof v === "number" && isFinite(v);
}

function isValidRate(v) {
    return isValidNumber(v) && v >= 0;
}

// 僅限制「視覺寬度」用；來源超界由 SensorManager 記錄一次警告。
function clampPercent(v) {
    return isValidNumber(v) ? Math.min(100, Math.max(0, v)) : null;
}

function percentOf(used, total) {
    if (!isValidNumber(used) || !isValidNumber(total) || total <= 0 || used < 0)
        return null;
    return used / total * 100;
}

function formatPercent(p) {
    return isValidNumber(p) ? (Math.round(p * 10) / 10).toString() + "%" : "--";
}

// 優先使用 Sensors 提供的 formattedValue（含正確單位）；不推測未知單位。
function displayText(formatted) {
    return (typeof formatted === "string" && formatted.length > 0) ? formatted : "--";
}

// 有界環形緩衝：回傳「新陣列」（QML 才會偵測到變更），長度不超過 limit。
// 無效值存成 null，圖表在該處斷線，不會畫成 0。
function pushHistory(arr, value, limit) {
    var n = Math.max(2, Math.floor(limit) || 2);
    var out = (arr || []).slice(-(n - 1));
    out.push(isValidNumber(value) ? value : null);
    return out;
}
