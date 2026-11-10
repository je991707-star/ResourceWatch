"use strict";
const fs = require("fs");
const vm = require("vm");
const assert = require("assert");
const qml = fs.readFileSync("package/contents/ui/models/SensorManager.qml", "utf8");
// Extract the actual QML JavaScript functions, not a copy of the algorithm.
function readFunction(name) {
  const start = qml.indexOf("    function " + name + "(");
  assert(start >= 0, `${name} missing`);
  const open = qml.indexOf("{", start);
  let depth = 0;
  for (let i = open; i < qml.length; i++) {
    if (qml[i] === "{") depth++;
    else if (qml[i] === "}" && --depth === 0) return qml.slice(start, i + 1);
  }
  throw new Error(`unterminated ${name}`);
}
let now = 1000;
const hits = {gpu:0, disk:0, history:0, html:0};
const timer = {interval:0, restart(){this.restartCount=(this.restartCount||0)+1;}};
const context = {
  Date:{now:()=>now}, Object, Math, updateInterval:500,
  _schedulerStarted:true, _schedulerTimer:timer,
  _nextDue:{gpu:1000,disk:1000,history:1000,html:1000},
  graceElapsed:false, _graceDeadline:6000,
  requestGpuProbe(){hits.gpu++;}, requestDiskStats(){hits.disk++;},
  _sample(){hits.history++;}, snapshotDue(){hits.html++;}
};
vm.createContext(context);
for (const name of ["_rescheduleAll", "_armScheduler", "_runScheduled"])
  vm.runInContext(readFunction(name), context);
context._runScheduled();
assert.deepStrictEqual(hits, {gpu:1,disk:1,history:1,html:1});
assert.strictEqual(timer.interval, 500);
now = 1200;
context._runScheduled();
assert.deepStrictEqual(hits, {gpu:1,disk:1,history:1,html:1});
now = 1500;
context._runScheduled();
assert.deepStrictEqual(hits, {gpu:2,disk:2,history:2,html:2});
// One input changes the deadlines for all four scheduled jobs.
context.updateInterval = 100;
context._rescheduleAll();
for (const job of Object.keys(hits))
  assert.strictEqual(context._nextDue[job], 1600);
now = 1600;
context._runScheduled();
assert.deepStrictEqual(hits, {gpu:3,disk:3,history:3,html:3});
now = 10000; // wake from sleep: never replay missed ticks
context._runScheduled();
assert.deepStrictEqual(hits, {gpu:4,disk:4,history:4,html:4});
assert.strictEqual(context.graceElapsed, true);
for (const job of Object.keys(hits))
  assert.strictEqual(context._nextDue[job], 10100);
console.log("PASS  統一排程器：一個間隔控制全部群組、重新排程、睡眠恢復不補採樣");
