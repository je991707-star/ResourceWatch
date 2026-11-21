#!/usr/bin/env bash
# 使用者空間安裝／更新／解除安裝 ResourceWatch；不需要 sudo、不分層、不重開機。
# 用法：./install.sh [install|uninstall]
#   install：建置 release 執行檔（有 distrobox 容器 resourcewatch-dev 時在容器內建置），安裝到 ~/.local；
#            有 kpackagetool6（KDE Plasma 6）時一併安裝桌面小工具，有 gnome-extensions（GNOME）時安裝頂欄擴充功能
set -euo pipefail
ID="io.github.local.resourcewatch"
BOX="${RESOURCEWATCH_BOX:-resourcewatch-dev}"
ROOT="$(cd "$(dirname "$0")" && pwd)"
BIN="$HOME/.local/bin/resourcewatch"
# 檔名必須等於視窗的 Wayland app_id（執行檔名），KWin 才找得到圖示
DESKTOP="$HOME/.local/share/applications/resourcewatch.desktop"
OLD_DESKTOP="$HOME/.local/share/applications/$ID.desktop" # 舊版啟動器
ICON="$HOME/.local/share/icons/hicolor/128x128/apps/$ID.png"
WIDGET="$ID.widget"
GUUID="resourcewatch@je991707-star.github.io"
GEXT="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions/$GUUID"

# 整行比對：舊版 id 是新版 id 的前綴
has_applet() { kpackagetool6 --type Plasma/Applet --list 2>/dev/null | grep -qx "$1"; }

# GNOME 頂欄擴充功能：複製 gnome/（不含測試用的 dev/）並啟用
install_gnome() {
    local updating=""
    [ -d "$GEXT" ] && updating=1
    rm -rf "$GEXT"
    mkdir -p "$GEXT"
    (cd "$ROOT/gnome" && tar --exclude=./dev -cf - .) | tar -xf - -C "$GEXT"
    # Wayland 的 Shell 要重新登入才認得新裝的擴充功能，那時 enable 會失敗：直接加進啟用清單，下次登入載入
    if ! gnome-extensions enable "$GUUID" 2>/dev/null; then
        local cur
        cur="$(gsettings get org.gnome.shell enabled-extensions 2>/dev/null || echo "@as []")"
        case "$cur" in
            *"'$GUUID'"*) ;;
            "@as []" | "[]") gsettings set org.gnome.shell enabled-extensions "['$GUUID']" ;;
            *) gsettings set org.gnome.shell enabled-extensions "${cur%]}, '$GUUID']" ;;
        esac
    fi
    if [ -n "$updating" ]; then
        echo "已更新 GNOME 頂欄擴充功能；重新登入後生效（Wayland 無法即時重新載入擴充功能）。"
    else
        echo "已安裝 GNOME 頂欄擴充功能；重新登入後出現在頂欄（可在「擴充功能」App 開關）。"
    fi
}

uninstall_gnome() {
    gnome-extensions disable "$GUUID" 2>/dev/null || true
    rm -rf "$GEXT"
}

build() {
    local cmd="cd '$ROOT/src-tauri' && cargo tauri build --no-bundle"
    # 容器優先：主機（例如 Bazzite）常有 cargo 卻缺 WebKitGTK 等開發函式庫
    # 直接進容器試一次：`distrobox list` 在另一個 enter 剛結束時偶爾列不出容器，會誤退回主機建置
    if command -v distrobox >/dev/null 2>&1 && distrobox enter "$BOX" -- true >/dev/null 2>&1; then
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
        install -Dm755 "$ROOT/target/release/resourcewatch" "$BIN"
        install -Dm644 "$ROOT/src-tauri/icons/128x128.png" "$ICON"
        mkdir -p "$(dirname "$DESKTOP")"
        rm -f "$OLD_DESKTOP"
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
        if command -v kpackagetool6 >/dev/null 2>&1; then
            if has_applet "$WIDGET"; then
                kpackagetool6 --type Plasma/Applet --upgrade "$ROOT/widget" >/dev/null
                echo "已更新桌面小工具；plasmashell 會快取舊版，重新登入或執行 systemctl --user restart plasma-plasmashell 後生效。"
            else
                kpackagetool6 --type Plasma/Applet --install "$ROOT/widget" >/dev/null
                echo "已安裝桌面小工具：右鍵桌面 → 新增或管理小工具 → ResourceWatch"
            fi
            if has_applet "$ID"; then
                echo "偵測到舊版 Plasma Widget（v0.0.5）仍在系統中；不再需要時可移除："
                echo "  kpackagetool6 --type Plasma/Applet --remove $ID"
            fi
        elif ! command -v gnome-extensions >/dev/null 2>&1; then
            echo "沒有 kpackagetool6（非 KDE Plasma 6），略過桌面小工具。"
        fi
        if command -v gnome-extensions >/dev/null 2>&1; then
            install_gnome
        fi
        ;;
    uninstall)
        rm -f "$BIN" "$DESKTOP" "$OLD_DESKTOP" "$ICON"
        if command -v kpackagetool6 >/dev/null 2>&1 && has_applet "$WIDGET"; then
            kpackagetool6 --type Plasma/Applet --remove "$WIDGET" >/dev/null
        fi
        if [ -d "$GEXT" ]; then
            uninstall_gnome
        fi
        # 小工具設定（設定頁寫入）
        rm -rf "${XDG_CONFIG_HOME:-$HOME/.config}/resourcewatch"
        echo "已解除安裝。"
        ;;
    *)
        echo "用法：$0 [install|uninstall]" >&2
        exit 2
        ;;
esac
