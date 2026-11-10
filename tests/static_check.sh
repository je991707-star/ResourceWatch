#!/usr/bin/env bash
# 靜態檢查：只代表檔案結構與基本規則，不代表實機 / 感測器 / Wayland 已驗證。
set -u
cd "$(dirname "$0")/.."
fail=0
chk() { if eval "$2"; then echo "PASS  $1"; else echo "FAIL  $1"; fail=1; fi; }

chk "metadata.json 可解析"        "python3 -m json.tool package/metadata.json >/dev/null"
chk "package ID 穩定"             "grep -q '\"io.github.local.resourcewatch\"' package/metadata.json"
chk "KPackageStructure"           "grep -q '\"Plasma/Applet\"' package/metadata.json"
chk "無 Qt5 式版本 import"        "! grep -rEn 'import (QtQuick|org\.kde)[A-Za-z.]* [0-9]' package/contents"
chk "QtWebEngine 主視圖已整合" "grep -q 'import QtWebEngine' package/contents/ui/HtmlView.qml && grep -q 'fullRepresentation: HtmlView' package/contents/ui/main.qml"
chk "HTML 資源已隨套件附帶" "test -f package/contents/web/index.html && test -f package/contents/web/styles.css && test -f package/contents/web/app.js"
chk "HTML 只有一份正式來源" "test ! -e web && test -f package/contents/web/index.html && test -f package/contents/web/styles.css && test -f package/contents/web/app.js"
chk "安裝腳本不複製舊 web/" "! grep -E '^[[:space:]]*cp[[:space:]].*web/' install.sh"
chk "CPU/GPU/RAM 圖表即時使用率 ID" "grep -q cpuChartUsage package/contents/web/index.html && grep -q gpuChartUsage package/contents/web/index.html && grep -q ramChartUsage package/contents/web/index.html"
chk "JS 更新 CPU/GPU/RAM 圖表標題" "grep -q cpuChartUsage package/contents/web/app.js && grep -q gpuChartUsage package/contents/web/app.js && grep -q ramChartUsage package/contents/web/app.js"
chk "QtWebChannel 單向感測器快照" "grep -q 'snapshotReady' package/contents/ui/HtmlView.qml && grep -q 'qtwebchannel/qwebchannel.js' package/contents/web/app.js"
chk "無網路請求"                  "! grep -rEn 'XMLHttpRequest|fetch\(' package/contents"
chk "main.xml 可解析"             "python3 -c \"import xml.dom.minidom as m; m.parse('package/contents/config/main.xml')\""
chk "設定頁檔案齊全"              "test -f package/contents/config/config.qml && test -f package/contents/ui/configGeneral.qml"
chk "main.qml 讀取的設定都在 main.xml" "for k in \$(grep -o 'Plasmoid.configuration\\.[A-Za-z0-9]*' package/contents/ui/main.qml | sed 's/.*\\.//' | sort -u); do grep -q \"name=\\\"\$k\\\"\" package/contents/config/main.xml || exit 1; done"
chk "無寫死裝置名稱 sensorId"     "! grep -rEn 'disk/(sda|nvme)' package/contents"
# chk "無 nvidia-smi 呼叫"          "! grep -rEn 'nvidia-smi|DataSource|executable' package/contents/ui --include=*.qml --include=*.js | grep -v '^.*//'"
chk "NVIDIA 查詢僅在隔離 GPU 後端" \
  "! grep -rEn 'nvidia-smi' package/contents/ui --include=*.qml --include=*.js"
chk "GPU 後端隨套件安裝" \
  "test -f package/contents/scripts/gpu_backend.py && grep -q 'gpu_backend.py' package/contents/ui/models/SensorManager.qml"
chk "NVENC/NVDEC 快照與歷史資料" \
  "grep -q 'encoder: metric(m.encoder)' package/contents/ui/HtmlView.qml && grep -q 'encode: historyArray' package/contents/ui/HtmlView.qml"
chk "CPU 名稱與 RAM 速度資訊探針" \
  "test -f package/contents/scripts/hardware_info.py && grep -q 'hardware_info.py' package/contents/ui/models/SensorManager.qml && grep -q 'ramSpeedText: m.ramSpeedText' package/contents/ui/HtmlView.qml"

chk "executable 僅允許 SensorManager 使用" \
  "! grep -rEn 'engine:[[:space:]]*\"executable\"' package/contents/ui --include=*.qml | grep -v 'models/SensorManager.qml:'"
chk "install.sh 不使用 sudo 指令" "! grep -v '^#' install.sh | grep -q 'sudo '"
if command -v node >/dev/null; then node tests/format_utils_test.js || fail=1; else echo "NOT TESTED  node 未安裝，略過 FormatUtils 單元測試"; fi
if command -v python3 >/dev/null; then python3 -m unittest discover -s tests -p 'gpu_backend_test.py' || fail=1; fi
if command -v python3 >/dev/null; then python3 -m unittest discover -s tests -p 'hardware_info_test.py' || fail=1; fi
if command -v qmllint >/dev/null; then
  qmllint package/contents/ui/*.qml package/contents/ui/*/*.qml && echo "PASS  qmllint" || echo "WARN  qmllint 有警告（缺 KDE 模組屬正常）"
else
  echo "NOT TESTED  qmllint 未安裝"
fi
exit $fail
