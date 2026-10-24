#!/bin/sh
# Installs systemd, D-Bus and OpenSSH with whichever package manager the
# image has, and creates the users the fleet logs in as.
set -eu

if command -v apt-get >/dev/null; then
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        systemd systemd-sysv dbus dbus-user-session libpam-systemd openssh-server
    rm -rf /var/lib/apt/lists/*
    ssh_service=ssh
elif command -v dnf >/dev/null; then
    dnf install -y systemd dbus openssh-server shadow-utils procps-ng
    dnf clean all
    ssh_service=sshd
elif command -v pacman >/dev/null; then
    pacman -Syu --noconfirm --needed systemd openssh
    pacman -Scc --noconfirm
    ssh_service=sshd
else
    echo "no known package manager" >&2
    exit 1
fi

# `sysorbit` reads the whole journal like an administrator; `viewer` only
# its own entries. Both log in with the key passed at build time, and
# their user managers run from boot (linger) so the user bus is there.
for user in sysorbit viewer; do
    useradd -m -s /bin/bash "$user"
    install -d -m 700 -o "$user" -g "$user" "/home/$user/.ssh"
    printf '%s\n' "$AUTHORIZED_KEY" > "/home/$user/.ssh/authorized_keys"
    chown "$user:$user" "/home/$user/.ssh/authorized_keys"
    chmod 600 "/home/$user/.ssh/authorized_keys"
    install -D /dev/null "/var/lib/systemd/linger/$user"
done
usermod -aG systemd-journal sysorbit

systemctl enable "$ssh_service"
systemctl mask systemd-firstboot.service getty.target console-getty.service
