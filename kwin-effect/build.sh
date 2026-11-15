#!/usr/bin/env bash
# Builds the effect in the `resourcewatch-kwin` distrobox against the host's
# exact KWin version and installs it to ~/.local/lib64/qt6/plugins.
# KWin refuses plugins built for another version, so rerun after KWin updates.
set -euo pipefail
cd "$(dirname "$0")"
BOX=resourcewatch-kwin
want=$(rpm -q --qf '%{VERSION}-%{RELEASE}' kwin)
ver=${want%%-*}
rel=${want#*-}

distrobox list | grep -q "$BOX" || distrobox create --yes --name "$BOX" \
    --image "registry.fedoraproject.org/fedora:$(rpm -E %fedora)"
distrobox enter "$BOX" -- bash -euc "
    rpm -q cmake extra-cmake-modules gcc-c++ ninja-build kf6-kcoreaddons-devel qt6-qtbase-devel qt6-qtdeclarative-devel >/dev/null ||
        sudo dnf install -y cmake extra-cmake-modules gcc-c++ ninja-build kf6-kcoreaddons-devel qt6-qtbase-devel qt6-qtdeclarative-devel
    if [ \"\$(rpm -q --qf '%{VERSION}-%{RELEASE}' kwin-devel 2>/dev/null)\" != '$want' ]; then
        sudo dnf install -y kwin-devel-$want || {
            # Not in the repos any more: take the exact build from Koji.
            base=https://kojipkgs.fedoraproject.org/packages/kwin/$ver/$rel/x86_64
            sudo dnf install -y --allow-downgrade \$base/kwin-devel-$want.x86_64.rpm \
                \$base/kwin-libs-$want.x86_64.rpm \$base/kwin-common-$want.x86_64.rpm
        }
    fi
    cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=RelWithDebInfo >/dev/null
    cmake --build build
"
dest=$HOME/.local/lib64/qt6/plugins/kwin/effects/plugins
install -Dm755 build/bin/kwin/effects/plugins/resourcewatch_blur.so "$dest/resourcewatch_blur.so"

# KWin only searches ~/.local for plugins when QT_PLUGIN_PATH says so; Plasma
# sources this file at login.
env=$HOME/.config/plasma-workspace/env/resourcewatch-blur.sh
if [ ! -e "$env" ]; then
    mkdir -p "$(dirname "$env")"
    echo 'export QT_PLUGIN_PATH="$HOME/.local/lib64/qt6/plugins${QT_PLUGIN_PATH:+:$QT_PLUGIN_PATH}"' >"$env"
    echo "Installed. Log out and back in once so KWin picks up $env."
else
    echo "Installed for KWin $want. Log out and back in to load the new build."
fi
