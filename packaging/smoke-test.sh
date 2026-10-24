#!/usr/bin/env bash
# Run after package installation, with Xvfb, xauth, D-Bus, gio and xdotool available.
set -euo pipefail
desktop-file-validate /usr/share/applications/io.github.estnml.sysorbit.desktop
test -r /usr/share/icons/hicolor/scalable/apps/sysorbit.svg
test -r /usr/share/icons/hicolor/256x256/apps/sysorbit.png
export ICED_BACKEND=tiny-skia
export XDG_RUNTIME_DIR
XDG_RUNTIME_DIR=$(mktemp -d)
trap 'rm -rf "$XDG_RUNTIME_DIR"' EXIT
timeout 20s xvfb-run -a dbus-run-session -- bash -eu -c '
    trap '\''pkill -x sysorbit || true'\'' EXIT
    gio launch /usr/share/applications/io.github.estnml.sysorbit.desktop
    for attempt in {1..30}; do
        if xdotool search --onlyvisible --class "^io.github.estnml.sysorbit$"; then
            sleep 2
            pgrep -x sysorbit >/dev/null
            echo "Desktop launcher opened a visible Sysorbit window."
            exit 0
        fi
        sleep 0.25
    done
    echo "Sysorbit did not open a visible window." >&2
    exit 1
'
