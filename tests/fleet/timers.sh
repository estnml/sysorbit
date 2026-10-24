#!/bin/bash
# Writes a set of timers that is random but the same for every build of
# the container named $1: what runs when, how long it takes, and whether
# it fails. Covers the shapes the app draws: minutely to monthly calendar
# timers, monotonic ones, randomized delays, Persistent=, failures, long
# runs, an inactive timer and a user timer.
set -eu

RANDOM=$(printf %s "$1" | cksum | cut -d' ' -f1)
units=/etc/systemd/system
wants=$units/timers.target.wants
mkdir -p "$wants"

# timer NAME DESCRIPTION SCRIPT [TIMER LINES...]
timer() {
    local name=$1 description=$2 script=$3
    shift 3
    # systemd reads % in ExecStart= as a specifier such as %n.
    script=${script//%/%%}
    cat > "$units/$name.service" <<UNIT
[Unit]
Description=$description

[Service]
Type=oneshot
ExecStart=/bin/bash -c '$script'
UNIT
    {
        printf '[Unit]\nDescription=%s\n\n[Timer]\n' "$description"
        printf '%s\n' "$@"
        printf '\n[Install]\nWantedBy=timers.target\n'
    } > "$units/$name.timer"
    ln -sf "$units/$name.timer" "$wants/$name.timer"
}

hour() { printf %02d $((RANDOM % 24)); }
minute() { printf %02d $((RANDOM % 60)); }
weekday() { local days=(Mon Tue Wed Thu Fri Sat Sun); echo "${days[RANDOM % 7]}"; }

timer heartbeat "Heartbeat" 'echo alive' \
    "OnCalendar=minutely" "AccuracySec=1s"
timer metrics-scrape "Scrape metrics" "echo scraping; sleep $((RANDOM % 15 + 1)); echo done" \
    "OnCalendar=*:0/5"
timer queue-flush "Flush the job queue" 'echo flushing; sleep 2' \
    "OnCalendar=*:0/15" "RandomizedDelaySec=2min"
timer cache-warm "Warm caches" "echo warming; sleep $((RANDOM % 5 + 1))" \
    "OnCalendar=*:$(minute)"
timer log-rotate "Rotate logs" 'echo rotating' \
    "OnCalendar=*-*-* $(hour):$(minute)" "Persistent=true"
timer db-backup "Back up the database" "echo dumping; sleep $((RANDOM % 90 + 30)); echo backup ok" \
    "OnCalendar=*-*-* $(hour):$(minute)" "Persistent=true" "RandomizedDelaySec=30min"
timer weekly-report "Weekly report" 'echo report sent' \
    "OnCalendar=$(weekday) *-*-* $(hour):$(minute)" "Persistent=true"
timer weekday-digest "Weekday digest" 'echo digest' \
    "OnCalendar=Mon..Fri *-*-* 09:30"
timer cert-check "Check certificates" 'echo all certificates valid' \
    "OnCalendar=*-*-01 $(hour):00"
timer flaky-sync "Sync with the upstream mirror" \
    'echo syncing; sleep 3; if (( RANDOM % 3 == 0 )); then echo "mirror unreachable" >&2; exit 1; fi; echo synced' \
    "OnCalendar=*:0/10"
timer broken-export "Export to the old API" 'echo "connection refused" >&2; exit 2' \
    "OnCalendar=*:0/30"
timer boot-warmup "Warm up after boot" 'echo warming up; sleep 5' \
    "OnBootSec=2min"
timer poll-api "Poll the partner API" "echo polling; sleep $((RANDOM % 4 + 1))" \
    "OnBootSec=1min" "OnUnitActiveSec=$((RANDOM % 6 + 4))min"

# Installed but never enabled: an inactive timer.
timer old-cleanup "Old cleanup, retired" 'echo cleaning' "OnCalendar=daily"
rm "$wants/old-cleanup.timer"

# A timer in the user manager of `sysorbit`.
mkdir -p /etc/systemd/user
cat > /etc/systemd/user/notes-backup.service <<'UNIT'
[Unit]
Description=Back up my notes

[Service]
Type=oneshot
ExecStart=/bin/bash -c 'echo notes saved'
UNIT
cat > /etc/systemd/user/notes-backup.timer <<UNIT
[Unit]
Description=Back up my notes

[Timer]
OnCalendar=*:0/20

[Install]
WantedBy=timers.target
UNIT
user_wants=/home/sysorbit/.config/systemd/user/timers.target.wants
mkdir -p "$user_wants"
ln -sf /etc/systemd/user/notes-backup.timer "$user_wants/notes-backup.timer"
chown -R sysorbit:sysorbit /home/sysorbit/.config
