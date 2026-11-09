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
