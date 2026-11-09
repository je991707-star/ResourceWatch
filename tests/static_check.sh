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
chk "install.sh 不使用 sudo 指令" "! grep -v '^#' install.sh | grep -q 'sudo '"
if command -v qmllint >/dev/null; then
  qmllint package/contents/ui/*.qml package/contents/ui/*/*.qml && echo "PASS  qmllint" || echo "WARN  qmllint 有警告（缺 KDE 模組屬正常）"
else
  echo "NOT TESTED  qmllint 未安裝"
fi
exit $fail
