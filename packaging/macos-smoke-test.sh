#!/usr/bin/env bash
# Opens the built app the way a person would and checks that it keeps
# running, then quits it. Run after packaging/macos.sh.
set -euo pipefail
cd "$(dirname "$0")/.."

app=target/packages/sysorbit.app
lipo -archs "$app/Contents/MacOS/sysorbit" | grep -q arm64
lipo -archs "$app/Contents/MacOS/sysorbit" | grep -q x86_64
plutil -extract CFBundleIdentifier raw "$app/Contents/Info.plist" | grep -qx io.github.estnml.sysorbit

open "$app"
for attempt in {1..30}; do
    if pgrep -x sysorbit >/dev/null; then
        sleep 3
        pgrep -x sysorbit >/dev/null
        echo "sysorbit.app opened and kept running."
        pkill -x sysorbit
        exit 0
    fi
    sleep 0.5
done
echo "sysorbit.app did not start." >&2
exit 1
