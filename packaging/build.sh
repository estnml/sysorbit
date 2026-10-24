#!/usr/bin/env bash
# Build inside the Debian baseline image; one binary feeds all three packages.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "$(uname -m)" != x86_64 ]]; then
    echo 'Initial Linux packages target x86_64.' >&2
    exit 1
fi
export CARGO_TARGET_DIR="$PWD/target"
desktop-file-validate packaging/io.github.estnml.sysorbit.desktop
cargo build --release --locked
cargo packager --release --formats deb
# 0.21.0's ldd parser emits invalid [WEAK] requirements for Rust's optional
# glibc symbols. Use the explicit runtime requirements in Cargo.toml instead.
cargo generate-rpm --auto-req disabled --output target/packages
python3 packaging/prepare-aur.py
