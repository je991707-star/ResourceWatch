#!/usr/bin/env bash
# 使用者空間安裝 / 更新 / 解除安裝；不需要 sudo，不重啟任何系統服務。
set -euo pipefail
ID="io.github.local.resourcewatch"
cd "$(dirname "$0")"

if ! command -v kpackagetool6 >/dev/null 2>&1; then
    echo "找不到 kpackagetool6。請確認系統為 Plasma 6（Bazzite KDE 通常已內建）。" >&2
    exit 1
fi

case "${1:-install}" in
    uninstall)
        kpackagetool6 --type Plasma/Applet --remove "$ID"
        rm -f "$HOME/.local/share/applications/$ID.desktop"
        ;;
    install|upgrade)
        # 單一 HTML 原始碼：package/contents/web/；直接納入 Plasmoid。
        # 不再複製專案根目錄的 web/，避免修改內容被舊副本覆蓋。
        for asset in index.html styles.css app.js; do
            if [[ ! -f "package/contents/web/$asset" ]]; then
                echo "缺少 HTML 資源：package/contents/web/$asset" >&2
                exit 1
            fi
        done
        if kpackagetool6 --type Plasma/Applet --list 2>/dev/null | grep -q "$ID"; then
            kpackagetool6 --type Plasma/Applet --upgrade ./package
        else
            kpackagetool6 --type Plasma/Applet --install ./package
        fi
        # Desktop application entry uses plasmawindowed as a window host;
        # Plasma desktop widgets still use the same installed plasmoid.
        install -Dm644 "packaging/$ID.desktop" "$HOME/.local/share/applications/$ID.desktop"
        echo "已安裝 ResourceWatch 應用程式啟動器。"
        echo "完成。可用以下指令開獨立視窗（不影響桌面）："
        echo "  plasmawindowed $ID"
        ;;
    *)
        echo "用法：$0 [install|upgrade|uninstall]" >&2; exit 2 ;;
esac
