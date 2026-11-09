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
chk "無 QtWebEngine/localStorage" "! grep -rEn 'WebEngine|localStorage' package/contents"
chk "無網路請求"                  "! grep -rEn 'XMLHttpRequest|fetch\(' package/contents"
chk "main.xml 可解析"             "python3 -c \"import xml.dom.minidom as m; m.parse('package/contents/config/main.xml')\""
chk "設定頁檔案齊全"              "test -f package/contents/config/config.qml && test -f package/contents/ui/configGeneral.qml"
chk "main.qml 讀取的設定都在 main.xml" "for k in \$(grep -o 'Plasmoid.configuration\\.[A-Za-z0-9]*' package/contents/ui/main.qml | sed 's/.*\\.//' | sort -u); do grep -q \"name=\\\"\$k\\\"\" package/contents/config/main.xml || exit 1; done"
chk "無寫死裝置名稱 sensorId"     "! grep -rEn 'disk/(sda|nvme)' package/contents"
# chk "無 nvidia-smi 呼叫"          "! grep -rEn 'nvidia-smi|DataSource|executable' package/contents/ui --include=*.qml --include=*.js | grep -v '^.*//'"
chk "禁止 NVIDIA CLI" \
  "! grep -rEn 'nvidia-smi' package/contents/ui --include=*.qml --include=*.js"

chk "executable 僅允許 SensorManager 使用" \
  "! grep -rEn 'engine:[[:space:]]*\"executable\"' package/contents/ui --include=*.qml | grep -v 'models/SensorManager.qml:'"
chk "install.sh 不使用 sudo 指令" "! grep -v '^#' install.sh | grep -q 'sudo '"
if command -v node >/dev/null; then node tests/format_utils_test.js || fail=1; else echo "NOT TESTED  node 未安裝，略過 FormatUtils 單元測試"; fi
if command -v qmllint >/dev/null; then
  qmllint package/contents/ui/*.qml package/contents/ui/*/*.qml && echo "PASS  qmllint" || echo "WARN  qmllint 有警告（缺 KDE 模組屬正常）"
else
  echo "NOT TESTED  qmllint 未安裝"
fi
exit $fail
