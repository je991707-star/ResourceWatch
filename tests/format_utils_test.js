// 純函式單元測試（不需要 Qt）。用法：node tests/format_utils_test.js
const fs = require("fs");
const path = require("path");
const assert = require("assert");

const src = fs.readFileSync(path.join(__dirname, "../package/contents/ui/models/FormatUtils.js"), "utf8")
    .replace(/^\.pragma library\s*$/m, "");
const F = new Function(src + "; return { isValidNumber, isValidRate, clampPercent, percentOf, formatPercent, displayText, pushHistory };")();

// 有效 / 無效數值
assert.strictEqual(F.isValidNumber(0), true);
assert.strictEqual(F.isValidNumber(NaN), false);
assert.strictEqual(F.isValidNumber(Infinity), false);
assert.strictEqual(F.isValidNumber(null), false);
assert.strictEqual(F.isValidNumber(undefined), false);
assert.strictEqual(F.isValidNumber("5"), false);
assert.strictEqual(F.isValidRate(-1), false);
assert.strictEqual(F.isValidRate(0), true);

// 百分比：無效回傳 null，不是 0
assert.strictEqual(F.clampPercent(null), null);
assert.strictEqual(F.clampPercent(135), 100);
assert.strictEqual(F.clampPercent(-3), 0);
assert.strictEqual(F.percentOf(5, 0), null);
assert.strictEqual(F.percentOf(5, null), null);
assert.strictEqual(F.percentOf(-1, 10), null);
assert.strictEqual(F.percentOf(5, 10), 50);
assert.strictEqual(F.formatPercent(30.66), "30.7%");
assert.strictEqual(F.formatPercent(null), "--");

// formattedValue
assert.strictEqual(F.displayText(""), "--");
assert.strictEqual(F.displayText(undefined), "--");
assert.strictEqual(F.displayText("4.50 GHz"), "4.50 GHz");

// 歷史緩衝：有界、回傳新陣列、無效值存 null
let h = [];
for (let i = 0; i < 100; i++) h = F.pushHistory(h, i, 30);
assert.strictEqual(h.length, 30);
assert.strictEqual(h[29], 99);
assert.strictEqual(h[0], 70);
const before = h;
const after = F.pushHistory(h, 1, 30);
assert.notStrictEqual(before, after);
assert.strictEqual(F.pushHistory([], NaN, 30)[0], null);
assert.strictEqual(F.pushHistory(null, 5, 30).length, 1);
assert.strictEqual(F.pushHistory(h, 1, 10).length, 10);   // 縮小上限時會截斷
assert.strictEqual(F.pushHistory([1, 2, 3], 4, 0).length, 2); // 下限保護

console.log("PASS  FormatUtils 單元測試");
