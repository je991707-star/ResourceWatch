#!/usr/bin/env bash
# 使用者空間安裝／更新／解除安裝 ResourceWatch（Tauri 版）；不需要 sudo、不分層、不重開機。
# 用法：./install.sh [install|uninstall]
#   install：建置 release 執行檔（有 distrobox 容器 resourcewatch-dev 時在容器內建置），安裝到 ~/.local
set -euo pipefail
ID="io.github.local.resourcewatch"
BOX="${RESOURCEWATCH_BOX:-resourcewatch-dev}"
ROOT="$(cd "$(dirname "$0")" && pwd)"
BIN="$HOME/.local/bin/resourcewatch"
DESKTOP="$HOME/.local/share/applications/$ID.desktop"
ICON="$HOME/.local/share/icons/hicolor/128x128/apps/$ID.png"

build() {
    local cmd="cd '$ROOT/src-tauri' && cargo tauri build --no-bundle"
    # 容器優先：主機（例如 Bazzite）常有 cargo 卻缺 WebKitGTK 等開發函式庫
    if command -v distrobox >/dev/null 2>&1 && distrobox list 2>/dev/null | grep -qw "$BOX"; then
        distrobox enter "$BOX" -- bash -lc "$cmd"
    elif cargo tauri --version >/dev/null 2>&1; then
        bash -c "$cmd"
    else
        echo "找不到 distrobox 容器 $BOX，主機也沒有 cargo-tauri 可用來建置。" >&2
        exit 1
    fi
}

case "${1:-install}" in
    install)
        build
        pkill -x resourcewatch 2>/dev/null || true # 執行中的舊版會佔住執行檔
        install -Dm755 "$ROOT/src-tauri/target/release/resourcewatch" "$BIN"
        install -Dm644 "$ROOT/src-tauri/icons/128x128.png" "$ICON"
        mkdir -p "$(dirname "$DESKTOP")"
        # 同一個 ID 會覆蓋舊版 Plasma Widget 的 plasmawindowed 啟動器
        cat >"$DESKTOP" <<EOF
[Desktop Entry]
Type=Application
Name=ResourceWatch
Name[zh_TW]=ResourceWatch 資源監控
Comment=Linux desktop resource monitor
Comment[zh_TW]=系統資源監控
Exec=$BIN
Icon=$ID
Terminal=false
Categories=System;Monitor;
StartupNotify=true
EOF
        update-desktop-database "$(dirname "$DESKTOP")" 2>/dev/null || true
        echo "已安裝：$BIN"
        if command -v kpackagetool6 >/dev/null 2>&1 &&
            kpackagetool6 --type Plasma/Applet --list 2>/dev/null | grep -q "$ID"; then
            echo "偵測到舊版 Plasma Widget（v0.0.5）仍在系統中；不再需要時可移除："
            echo "  kpackagetool6 --type Plasma/Applet --remove $ID"
        fi
        ;;
    uninstall)
        rm -f "$BIN" "$DESKTOP" "$ICON"
        echo "已解除安裝（設定檔 ~/.config/$ID 保留）。"
        ;;
    *)
        echo "用法：$0 [install|uninstall]" >&2
        exit 2
        ;;
esac
