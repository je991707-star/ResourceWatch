#!/usr/bin/env bash
# Test the extension in an isolated, headless GNOME Shell (virtual monitor, no window on the host) inside the
# distrobox container resourcewatch-gnome (Fedora with gnome-shell and mutter-devkit).
# Isolation: its own HOME / XDG_CONFIG_HOME / XDG_DATA_HOME / XDG_RUNTIME_DIR under $TEST and its own D-Bus
# session (dbus-run-session), so it never reads or writes the host's dconf, extensions or Wayland sockets.
#
#   nested.sh start        copy the extension in and run the shell (foreground; run it in the background)
#   nested.sh eval '<js>'  org.gnome.Shell.Eval in the test shell (unsafe mode is on via devtools@resourcewatch)
#   nested.sh shot <png>   screenshot of the virtual monitor
#   nested.sh stop         stop the test shell
# RW_COLOR_SCHEME=prefer-light|prefer-dark sets the test shell's style on start.
set -euo pipefail
BOX="${RESOURCEWATCH_GNOME_BOX:-resourcewatch-gnome}"
SRC="$(cd "$(dirname "$0")/.." && pwd)"
REAL_HOME="${RW_REAL_HOME:-$HOME}"
TEST="$REAL_HOME/.local/share/resourcewatch-gnome-test"
UUID="resourcewatch@je991707-star.github.io"
MONITOR="${RW_MONITOR:-1600x900}"

# run inside the container
if [ ! -e /run/.containerenv ]; then
    exec distrobox enter "$BOX" -- env RW_REAL_HOME="$HOME" RW_MONITOR="$MONITOR" RW_COLOR_SCHEME="${RW_COLOR_SCHEME:-}" bash "$SRC/dev/nested.sh" "$@"
fi

export HOME="$TEST/home" XDG_CONFIG_HOME="$TEST/home/.config" XDG_DATA_HOME="$TEST/home/.local/share" \
    XDG_CACHE_HOME="$TEST/home/.cache" XDG_STATE_HOME="$TEST/home/.local/state" XDG_RUNTIME_DIR="$TEST/run"
unset WAYLAND_DISPLAY DISPLAY DBUS_SESSION_BUS_ADDRESS DBUS_SYSTEM_BUS_ADDRESS

bus() { DBUS_SESSION_BUS_ADDRESS="$(cat "$TEST/bus")" "$@"; }

case "${1:-}" in
    start)
        mkdir -p "$XDG_CONFIG_HOME/resourcewatch" "$XDG_DATA_HOME/gnome-shell/extensions" "$HOME/.local/bin" "$XDG_RUNTIME_DIR"
        chmod 700 "$XDG_RUNTIME_DIR"
        ext="$XDG_DATA_HOME/gnome-shell/extensions"
        rm -rf "${ext:?}/$UUID" "$ext/devtools@resourcewatch"
        mkdir -p "$ext/$UUID"
        (cd "$SRC" && tar --exclude=./dev -cf - .) | tar -xf - -C "$ext/$UUID"
        cp -r "$SRC/dev/devtools@resourcewatch" "$ext/"
        ln -sf "$REAL_HOME/.local/bin/resourcewatch" "$HOME/.local/bin/resourcewatch"
        [ -f "$REAL_HOME/.config/resourcewatch/widget.json" ] && cp "$REAL_HOME/.config/resourcewatch/widget.json" "$XDG_CONFIG_HOME/resourcewatch/"
        # a private, empty bus stands in for the system bus: the shell needs one to start, and the host's
        # (logind, NetworkManager, …) must stay out of reach
        rm -f "$XDG_RUNTIME_DIR/system_bus"
        dbus-daemon --session --nofork --address="unix:path=$XDG_RUNTIME_DIR/system_bus" &
        export DBUS_SYSTEM_BUS_ADDRESS="unix:path=$XDG_RUNTIME_DIR/system_bus"
        for _ in $(seq 50); do [ -S "$XDG_RUNTIME_DIR/system_bus" ] && break; sleep 0.1; done
        exec dbus-run-session -- bash -c '
            echo "$DBUS_SESSION_BUS_ADDRESS" > "'"$TEST"'/bus"
            gsettings set org.gnome.shell disable-user-extensions false
            gsettings set org.gnome.shell enabled-extensions "[\"'"$UUID"'\", \"devtools@resourcewatch\"]"
            gsettings set org.gnome.shell welcome-dialog-last-shown-version "999"
            [ -n "${RW_COLOR_SCHEME:-}" ] && gsettings set org.gnome.desktop.interface color-scheme "$RW_COLOR_SCHEME"
            exec gnome-shell --headless --virtual-monitor "'"$MONITOR"'" --no-x11
        '
        ;;
    eval)
        bus gdbus call --session -d org.gnome.Shell -o /org/gnome/Shell -m org.gnome.Shell.Eval "$2"
        ;;
    shot)
        bus gdbus call --session -d org.gnome.Shell -o /org/gnome/Shell/Screenshot -m org.gnome.Shell.Screenshot.Screenshot false false "$(realpath -m "$2")"
        ;;
    stop)
        pkill -f "gnome-shell --headless --virtual-monitor" || true
        pkill -f "dbus-daemon --session --nofork --address=unix:path=$XDG_RUNTIME_DIR/system_bus" || true
        ;;
    *)
        sed -n '2,12p' "$0"
        exit 1
        ;;
esac
