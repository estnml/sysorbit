#!/usr/bin/env bash
# A demo fleet of current Linux distributions, each with sshd and a random
# set of timers, listed in sysorbit's host menu:
#   tests/fleet/run.sh          build and start all
#   tests/fleet/run.sh stop     remove the containers and the menu entries
#
# The hosts go to sysorbit's hosts.d/demo-fleet.toml, never to your own
# hosts.toml. Log in by hand with `ssh -p PORT sysorbit@localhost`.
set -euo pipefail
cd "$(dirname "$0")"

# name=image=port
HOSTS=(
    "debian-13=debian:trixie=2301"
    "ubuntu-24-04=ubuntu:24.04=2302"
    "fedora=fedora:latest=2303"
    "arch=archlinux:latest=2304"
    "rocky-9=rockylinux:9=2305"
)
KEY="${KEY:-$HOME/.ssh/id_ed25519.pub}"

case "$(uname)" in
    Darwin) CONFIG="$HOME/Library/Application Support/sysorbit" ;;
    *) CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}/sysorbit" ;;
esac
DROP_IN="$CONFIG/hosts.d/demo-fleet.toml"

for host in "${HOSTS[@]}"; do
    docker rm -f "sysorbit-fleet-${host%%=*}" >/dev/null 2>&1 || true
done
if [[ "${1:-}" == "stop" ]]; then
    rm -f "$DROP_IN"
    exit 0
fi

mkdir -p "$(dirname "$DROP_IN")"
{
    echo "# Written by tests/fleet/run.sh; removed by \`run.sh stop\`."
    echo "# The containers' host keys change with every build."
} > "$DROP_IN"

for host in "${HOSTS[@]}"; do
    IFS='=' read -r name image port <<<"$host"
    docker build --quiet \
        --build-arg "BASE=$image" --build-arg "NAME=$name" \
        --build-arg "AUTHORIZED_KEY=$(cat "$KEY")" \
        -t "sysorbit-fleet:$name" . >/dev/null
    docker run -d --name "sysorbit-fleet-$name" --hostname "$name" \
        --cgroupns=private --cap-add SYS_ADMIN \
        --tmpfs /run --tmpfs /run/lock --tmpfs /tmp \
        -p "127.0.0.1:$port:22" \
        "sysorbit-fleet:$name" >/dev/null
    cat >> "$DROP_IN" <<TOML

[[host]]
id = "fleet-$name"
name = "Fleet: $name"
destination = "localhost"
user = "sysorbit"
port = $port
options = ["StrictHostKeyChecking=no", "UserKnownHostsFile=/dev/null", "LogLevel=ERROR"]
TOML
    echo "sysorbit-fleet-$name: ssh -p $port sysorbit@localhost  ($image)"
done
