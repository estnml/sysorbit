# A distro image booting systemd as PID 1, with a few timers to look at.
ARG BASE
FROM ${BASE}

RUN if command -v apt-get >/dev/null; then \
        apt-get update && \
        DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends systemd systemd-sysv dbus && \
        rm -rf /var/lib/apt/lists/*; \
    else \
        dnf install -y systemd dbus && dnf clean all; \
    fi

# dbus-daemon drops clients whose UID it cannot resolve; the test connects
# from the host with the host user's UID.
ARG HOST_UID=1000
RUN getent passwd "${HOST_UID}" >/dev/null || useradd -u "${HOST_UID}" -M sysorbit

COPY units/ /etc/systemd/system/
RUN systemctl enable sysorbit-quarter.timer sysorbit-boot.timer && \
    systemctl mask systemd-firstboot.service getty.target console-getty.service

# Docker mounts the (private) cgroup tree read-only; systemd needs to write
# to it. Remounting needs CAP_SYS_ADMIN, but not a privileged container.
STOPSIGNAL SIGRTMIN+3
ENTRYPOINT ["/bin/sh", "-c", "mount -o remount,rw /sys/fs/cgroup && exec /sbin/init"]
