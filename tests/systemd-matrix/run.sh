#!/usr/bin/env bash
# Starts one container per systemd version for the matrix test:
#   tests/systemd-matrix/run.sh            build and start all
#   tests/systemd-matrix/run.sh stop       remove the containers
#   cargo test systemd_matrix -- --ignored --nocapture
#
# Each container's D-Bus socket directory is bind mounted under
# $RUNTIME_DIR/<name>, where the test connects to it.
set -euo pipefail
cd "$(dirname "$0")"

# name=base image (systemd version in the comment)
TARGETS=(
    "rocky8=rockylinux:8"          # 239
    "ubuntu2204=ubuntu:22.04"      # 249
    "debian12=debian:12"           # 252
    "ubuntu2404=ubuntu:24.04"      # 255
)
RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}/sysorbit-matrix"

if [[ "${1:-}" == "stop" ]]; then
    for target in "${TARGETS[@]}"; do
        docker rm -f "sysorbit-matrix-${target%%=*}" >/dev/null 2>&1 || true
    done
    exit 0
fi

mkdir -p "$RUNTIME_DIR"
for target in "${TARGETS[@]}"; do
    name="${target%%=*}"
    base="${target#*=}"
    image="sysorbit-matrix:$name"
    container="sysorbit-matrix-$name"

    docker build --quiet --build-arg "BASE=$base" --build-arg "HOST_UID=$(id -u)" \
        -t "$image" . >/dev/null
    docker rm -f "$container" >/dev/null 2>&1 || true
    mkdir -p "$RUNTIME_DIR/$name"
    docker run -d --name "$container" \
        --cgroupns=private --cap-add SYS_ADMIN \
        --tmpfs /run --tmpfs /run/lock --tmpfs /tmp \
        -v "$RUNTIME_DIR/$name:/run/dbus" \
        "$image" >/dev/null
    echo "$container: started from $base"
done
