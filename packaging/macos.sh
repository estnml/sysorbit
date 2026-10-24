#!/usr/bin/env bash
# Builds a universal (Apple Silicon and Intel) sysorbit.app and disk image on
# macOS. With the Apple credentials in the environment the app is signed with
# its Developer ID, notarized and stapled, and so is the disk image; without
# them both stay unsigned and the image's name says so.
#
# Credentials, as the packages workflow passes them from repository secrets:
#   APPLE_CERTIFICATE           Developer ID Application certificate, .p12, base64
#   APPLE_CERTIFICATE_PASSWORD  its password
#   APPLE_API_KEY               App Store Connect API key ID
#   APPLE_API_ISSUER            its issuer ID
#   APPLE_API_KEY_P8            the key's .p8 file contents
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
signed=false
if [[ -n "${APPLE_CERTIFICATE:-}" && -n "${APPLE_API_KEY_P8:-}" ]]; then
    signed=true
fi

for target in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release --locked --target "$target"
done
mkdir -p target/release
lipo -create -output target/release/sysorbit \
    target/aarch64-apple-darwin/release/sysorbit \
    target/x86_64-apple-darwin/release/sysorbit

# cargo-packager reads its settings only from Cargo.toml. This checkout is
# thrown away, so the macOS icon on Apple's grid, and with credentials the
# signing identity, are written into it here rather than kept for Linux too.
python3 - "$signed" <<'PY'
import re, sys
path = "Cargo.toml"
text = open(path).read()
text = re.sub(r'(?m)^icons = \[.*\]$',
              'icons = ["assets/sysorbit-macos-512.png", "assets/sysorbit-macos-1024.png"]',
              text, count=1)
if sys.argv[1] == "true":
    # The certificate is alone in the packager's own keychain, so its kind
    # names it without the team's name and ID.
    text = text.replace("[package.metadata.packager.macos]\n",
                        '[package.metadata.packager.macos]\nsigning-identity = "Developer ID Application"\n', 1)
open(path, "w").write(text)
PY

if $signed; then
    key_dir=$(mktemp -d)
    trap 'rm -rf "$key_dir"' EXIT
    export APPLE_API_KEY_PATH="$key_dir/AuthKey_$APPLE_API_KEY.p8"
    printf '%s' "$APPLE_API_KEY_P8" > "$APPLE_API_KEY_PATH"
fi

rm -rf target/packages
cargo packager --release --formats app,dmg

suffix=$($signed && echo universal || echo universal-unsigned)
dmg="target/packages/sysorbit-$version-macos-$suffix.dmg"
mv target/packages/*.dmg "$dmg"

if $signed; then
    # The app inside is notarized already; the image is checked on its own
    # when it is opened from a download.
    xcrun notarytool submit "$dmg" --wait \
        --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER"
    xcrun stapler staple "$dmg"
    spctl --assess --type execute --verbose target/packages/sysorbit.app
    spctl --assess --type open --context context:primary-signature --verbose "$dmg"
fi
echo "Created $dmg"
