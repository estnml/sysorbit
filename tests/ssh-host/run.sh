#!/usr/bin/env bash
# Starts a container to reach over SSH on localhost:2222 as `sysorbit` (in
# systemd-journal) or `viewer` (not), with your ~/.ssh/id_ed25519.pub
# authorized:
#   tests/ssh-host/run.sh        build and start
#   tests/ssh-host/run.sh stop   remove the container
#   ssh -p 2222 sysorbit@localhost
set -euo pipefail
cd "$(dirname "$0")/.."

CONTAINER=sysorbit-ssh-host
PORT="${PORT:-2222}"
KEY="${KEY:-$HOME/.ssh/id_ed25519.pub}"

docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
if [[ "${1:-}" == "stop" ]]; then
    exit 0
fi

docker build --quiet --build-arg "AUTHORIZED_KEY=$(cat "$KEY")" \
    -f ssh-host/Dockerfile -t sysorbit-ssh-host . >/dev/null
docker run -d --name "$CONTAINER" \
    --cgroupns=private --cap-add SYS_ADMIN \
    --tmpfs /run --tmpfs /run/lock --tmpfs /tmp \
    -p "127.0.0.1:$PORT:22" \
    sysorbit-ssh-host >/dev/null
echo "$CONTAINER: ssh -p $PORT sysorbit@localhost"
