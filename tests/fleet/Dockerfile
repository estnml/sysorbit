# One host of the demo fleet: a distribution booting systemd as PID 1,
# with sshd and a random set of timers. Built by tests/fleet/run.sh.
ARG BASE
FROM ${BASE}

ARG AUTHORIZED_KEY
COPY install.sh timers.sh /fleet/
RUN AUTHORIZED_KEY="${AUTHORIZED_KEY}" sh /fleet/install.sh

ARG NAME
RUN bash /fleet/timers.sh "${NAME}"

# Docker mounts the (private) cgroup tree read-only; systemd needs to write
# to it. Remounting needs CAP_SYS_ADMIN, but not a privileged container.
STOPSIGNAL SIGRTMIN+3
ENTRYPOINT ["/bin/sh", "-c", "mount -o remount,rw /sys/fs/cgroup && exec /lib/systemd/systemd"]
