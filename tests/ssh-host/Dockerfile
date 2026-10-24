# A Linux host reached over SSH: systemd as PID 1, sshd, and the timers of
# the systemd matrix. Built from tests/ by tests/ssh-host/run.sh.
FROM debian:12

RUN apt-get update && \
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        systemd systemd-sysv dbus dbus-user-session libpam-systemd openssh-server && \
    rm -rf /var/lib/apt/lists/*

# Users logging in with the key passed at build time, whose user managers
# run from boot (linger) so the user bus is there: `sysorbit` reads the
# whole journal like an administrator, `viewer` only its own entries.
ARG AUTHORIZED_KEY
RUN for user in sysorbit viewer; do \
        useradd -m -s /bin/bash "$user" && \
        install -d -m 700 -o "$user" -g "$user" "/home/$user/.ssh" && \
        echo "${AUTHORIZED_KEY}" > "/home/$user/.ssh/authorized_keys" && \
        chown "$user:$user" "/home/$user/.ssh/authorized_keys" && \
        chmod 600 "/home/$user/.ssh/authorized_keys" && \
        install -D /dev/null "/var/lib/systemd/linger/$user"; \
    done && \
    usermod -aG systemd-journal sysorbit

COPY systemd-matrix/units/ /etc/systemd/system/
RUN systemctl enable ssh sysorbit-quarter.timer sysorbit-boot.timer && \
    systemctl mask systemd-firstboot.service getty.target console-getty.service

STOPSIGNAL SIGRTMIN+3
ENTRYPOINT ["/bin/sh", "-c", "mount -o remount,rw /sys/fs/cgroup && exec /sbin/init"]
