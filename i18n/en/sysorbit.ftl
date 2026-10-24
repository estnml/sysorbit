# sysorbit — English, the source language.
#
# Translators: copy this file to i18n/<language>/sysorbit.ftl and translate
# the values. Keep systemd's own words (OnCalendar=, Persistent=, unit and
# property names) as they are. See TRANSLATING.md.

## Header and status line

scope-system = System
scope-user = User
zoom-hours = 6 hours
zoom-day = Day
zoom-week = Week
zoom-month = Month
refresh = Refresh
refreshing = Refreshing…
reading-timers = Reading timers…
no-timers = No timers in this scope.
no-host = No host
host-this-computer = this computer
host-ssh-config = from ~/.ssh/config
host-add-hint = Add servers in { $path }
no-hosts = No servers yet. Add one in { $path }.
menu-add = + Add server…
menu-edit = Edit
form-title-add = Add server
form-title-edit = Edit server
form-name = Name
form-name-placeholder = Prod web
form-address = Host
form-address-placeholder = 203.0.113.10 or a ~/.ssh/config alias
form-port = Port
form-user = User
form-user-placeholder = as in ~/.ssh/config
form-auth = Log in with
form-auth-ssh = My SSH setup (agent, ~/.ssh/config)
form-auth-key = A key file, such as a PEM file
form-key-placeholder = ~/keys/server.pem
form-browse = Browse…
form-browse-title = Choose a private key
form-test = Test connection
form-testing = Testing…
form-test-timeout = No answer within 20 seconds.
form-test-systemd = systemd { $version }
form-test-connected = connected
form-test-journal = journal readable
form-test-journal-restricted = journal shows only this user's entries; add the user to systemd-journal
form-cancel = Cancel
form-save = Save
form-delete = Delete
form-error-address = Enter a host.
form-error-port = The port is a number from 1 to 65535.
form-error-key = Choose a key file.
status-timers = { $count ->
    [one] 1 timer
   *[other] { $count } timers
} · updated { $time }
status-live = ● live
status-connecting = ○ connecting
status-polling = ○ polling every { $seconds }s, live updates unavailable: { $reason }
status-keys = ←/→ marks · ↑/↓ timers · Enter logs · Esc back
live-closed = connection to systemd closed

## Warnings from reading systemd

warning-old-systemd = systemd { $version } is older than the supported { $minimum }
warning-next-elapse-only = only the next elapse is shown (systemd < 242)
warning-no-past-elapses = past elapses are not shown (systemd < 244)
warning-monotonic = Monotonic times unavailable: { $error }
warning-history = History unavailable: { $error }
warning-journal-restricted = History unavailable: the journal shows only this user's entries. Add the user to the systemd-journal group.
warning-downtime = Downtime unavailable: { $error }
warning-clock = Server clock unavailable: { $error }
# The remote host's clock compared with this machine's.
host-clock-synced = server clock in sync
host-clock = server clock { $direction ->
    [ahead] { $span } ahead
   *[behind] { $span } behind
}
host-zone = server time ({ $zone })
warning-unreadable = { $unit } unreadable: { $error }

## Dates and durations

weekday-short-mon = Mon
weekday-short-tue = Tue
weekday-short-wed = Wed
weekday-short-thu = Thu
weekday-short-fri = Fri
weekday-short-sat = Sat
weekday-short-sun = Sun
month-short-1 = Jan
month-short-2 = Feb
month-short-3 = Mar
month-short-4 = Apr
month-short-5 = May
month-short-6 = Jun
month-short-7 = Jul
month-short-8 = Aug
month-short-9 = Sep
month-short-10 = Oct
month-short-11 = Nov
month-short-12 = Dec
# A moment in a tooltip or label, e.g. "Fri 23 Oct 14:05".
moment = { $weekday } { $day } { $month } { $time }
relative-now = now
relative-future = in { $span }
relative-past = { $span } ago
# Compact durations, e.g. "1h 40min". Two units at most are shown.
short-week = { $n }w
short-day = { $n }d
short-hour = { $n }h
short-minute = { $n }min
short-second = { $n }s
short-millisecond = { $n }ms
short-zero = 0
# Durations in sentences, e.g. "1 day 2 hours".
span-week = { $n ->
    [one] 1 week
   *[other] { $n } weeks
}
span-day = { $n ->
    [one] 1 day
   *[other] { $n } days
}
span-hour = { $n ->
    [one] 1 hour
   *[other] { $n } hours
}
span-minute = { $n ->
    [one] 1 minute
   *[other] { $n } minutes
}
span-second = { $n ->
    [one] 1 second
   *[other] { $n } seconds
}
span-millisecond = { $n ->
    [one] 1 millisecond
   *[other] { $n } milliseconds
}
span-zero = 0 seconds
# $category is the number's ordinal plural category in this language.
ordinal = { $category ->
    [one] { $n }st
    [two] { $n }nd
    [few] { $n }rd
   *[other] { $n }th
}
list-separator = {", "}
list-and = { $rest } and { $last }

## Track subtitles in the music box

schedule-daily = daily { $time }
schedule-hourly = hourly
schedule-every-minute = every minute
schedule-activation = activation +{ $offset }
schedule-boot = boot +{ $offset }
schedule-startup = startup +{ $offset }
schedule-every = every { $offset }
schedule-after-finish = { $offset } after finish
schedule-random = +{ $delay } random
schedule-persistent = Persistent
schedule-wake = Wake
unit-file-state = { $state ->
    [enabled] enabled
    [enabled-runtime] enabled
    [disabled] disabled
    [static] static
    [masked] masked
    [masked-runtime] masked
    [transient] transient
    [generated] generated
    [indirect] indirect
    [linked] linked
    [linked-runtime] linked
    [alias] alias
   *[other] { $state }
}

## Music box marks and legend

now-clock = now { $time }
mark-ran = Ran · { $moment }
mark-took = took { $span }
mark-still-running = still running
outcome-success = success
outcome-running = running
outcome-failed = failed: { $result }
mark-planned = Planned · { $moment }
mark-random-picked = calendar { $moment }, +{ $span } random delay
mark-random-window = randomly within { $span } after { $moment }
mark-wakes = wakes the machine from sleep
mark-ghost = timer inactive; it would fire here if enabled
mark-skipped = Skipped · { $moment }
mark-skipped-down = machine was down
mark-skipped-no-run = no run in the journal
mark-catch-up = Persistent catch-up
mark-tether = Monotonic countdown
downtime-powered-off = Powered off
downtime-suspended = Suspended
legend-ran = ran
legend-failed = failed
legend-planned = planned
legend-delay = RandomizedDelaySec
legend-wake = WakeSystem
legend-skipped = skipped
legend-catch-up = Persistent catch-up
legend-down = machine down

## Drawer

tab-overview = Overview
tab-runs = Runs
tab-logs = Logs
no-runs = No runs in the loaded range.
run-logs-link = logs →
run-heading = Run · { $moment }
no-invocation = systemd did not record an invocation ID for this run.
logs-failed = Could not read the logs: { $error }
logs-empty = This run wrote nothing to the journal.
logs-loading = Loading logs…

## Overview tab

card-schedule = Schedule
card-next-run = Next run
card-recent-runs = Recent runs
service-running = The service is running right now.
service-idle = The service is not running.
badge-active = active
badge-inactive = inactive
badge-persistent = persistent
badge-wake = wakes system
help-active = The timer is started and triggers its service when it elapses.
help-inactive = The timer is stopped; it will not trigger until it is started.
help-persistent = An elapse missed while the machine was off or asleep runs right after it comes back.
help-wake = Wakes the machine from suspend when the timer elapses.
help-unit-file = { $state ->
    [enabled] Started automatically at boot, or at login for user timers.
    [enabled-runtime] Started automatically at boot, or at login for user timers.
    [disabled] Installed, but only runs when started by hand.
    [static] Has no [Install] section, so it cannot be enabled; another unit pulls it in.
    [masked] systemd refuses to start it.
    [masked-runtime] systemd refuses to start it.
    [transient] Created at runtime, e.g. by systemd-run; gone after a reboot.
    [generated] Created by a generator from other configuration.
    [indirect] Enabled through another unit's Also=.
    [linked] The unit file lives outside the search path.
    [linked-runtime] The unit file lives outside the search path.
    [alias] Another name for a different unit.
   *[other] The unit file state systemd reports.
}
no-triggers = No triggers.
next-occurrences = Next: { $moments }
running-since = Running now, since { $time }.
timer-inactive-note = The timer is inactive and will not fire until it is started.
no-upcoming = systemd has no upcoming elapse for this timer.
accuracy-note = May run up to { $span } late (AccuracySec={ $value }): systemd batches wakeups inside this window.
delay-note = Waits a random 0 to { $span } after each elapse (RandomizedDelaySec={ $value }).
bar-last = last { $moment } · { $relative }
bar-next = next { $moment } · { $relative }
bar-not-scheduled = not scheduled
stat-succeeded = { $count } succeeded
stat-failed = { $count } failed
stat-average = avg { $span }
stat-p95 = p95 { $span }
durations-not-logged = durations not logged
duration-not-logged = duration not logged
chart-summary = last { $count } · longest { $span }
chart-run = { $moment } · { $span } · { $outcome }

## Triggers in plain words
##
## These build sentences such as "On the 1st of every month at 00:00".
## Pieces are joined by the messages further down, so their order can
## change per language. Numbers and times arrive already formatted.

explain-unknown = No plain form for this expression.
weekdays-mon = Mondays
weekdays-tue = Tuesdays
weekdays-wed = Wednesdays
weekdays-thu = Thursdays
weekdays-fri = Fridays
weekdays-sat = Saturdays
weekdays-sun = Sundays
weekday-mon = Monday
weekday-tue = Tuesday
weekday-wed = Wednesday
weekday-thu = Thursday
weekday-fri = Friday
weekday-sat = Saturday
weekday-sun = Sunday
month-1 = January
month-2 = February
month-3 = March
month-4 = April
month-5 = May
month-6 = June
month-7 = July
month-8 = August
month-9 = September
month-10 = October
month-11 = November
month-12 = December
nth-1 = first
nth-2 = second
nth-3 = third
nth-4 = fourth
nth-last = last

# Times of day. $times is a list such as "06:00 and 18:00".
explain-at = at { $times }
explain-every-second = every second
explain-every-n-seconds = every { $n } seconds
explain-every-minute-at-second = every minute at second { $second }
explain-fields = when the hour is { $hours }, the minute { $minutes } and the second { $seconds }
explain-any = any
explain-every-from-to = every { $step } from { $from } to { $to }
# A minute past the hour as shown in the phrases below, e.g. ":05".
minute-mark = :{ $minute }
explain-every-minute = every minute
explain-every-hour = every hour
explain-every-hour-at = every hour at { $minute }
explain-every-n-minutes = every { $n } minutes
explain-every-n-minutes-from = every { $n } minutes from { $minute }
explain-every-minute-between = every minute from { $from } to { $to } past the hour
explain-every-n-minutes-between = every { $n } minutes from { $from } to { $to } past the hour
explain-past-the-hour = at { $minutes } past the hour
explain-every-hour-range = every hour from { $from } to { $to }
explain-every-n-hours = every { $n } hours
explain-every-n-hours-at = every { $n } hours at { $minute }
explain-every-n-hours-range = every { $n } hours from { $from } to { $to }
explain-past-hours = at { $minute } past hours { $hours }
explain-between = between { $from } and { $to }
explain-during-hours = during hours { $hours }
# A repeating time limited to some hours: "every 10 minutes between 09:00 and 17:59".
explain-repeat-window = { $repeat } { $window }

# Days.
explain-on-weekdays = on weekdays
explain-on-weekends = on weekends
explain-weekday-range = from { $from } to { $to }
explain-on-days = on { $days }
explain-nth-weekday = on the { $nth } { $weekday } of { $months }
explain-every-month = every month
# Months a day belongs to: "January and July" in "the 1st of January and July".
explain-months-of = { $count ->
   *[other] { $months }
}
explain-in-months = { $count ->
   *[other] in { $months }
}
explain-month-range = from { $from } to { $to }
explain-date = on { $day } { $month }
explain-day-of = { $day } of { $months }
explain-day-one = on the { $day }
explain-day-range = on the { $from } to the { $to }
explain-odd-days = on odd days
explain-even-days = on even days
explain-every-n-days = every { $n } days from the { $from }
explain-day-list = on the { $days }
explain-last-day = on the last day
explain-nth-last-day = on the { $nth }-to-last day
explain-last-n-days = on the last { $n } days
explain-days-from-end = on the { $days } days counted from the end
explain-in-year = in { $year }
explain-year-range = from { $from } to { $to }
explain-every-n-years = every { $n } years from { $from }
explain-in-years = in { $years }

# Whole sentences. $time is an "at …" phrase, $repeat a repeating one,
# $date the day phrases joined by spaces.
explain-daily = every day { $time }
explain-dated = { $date } { $time }
explain-repeat-dated = { $repeat }, { $date }
explain-zone = { $sentence } ({ $zone } time)

# Monotonic triggers.
explain-after-activation = { $span } after the timer was started
explain-after-boot = { $span } after boot
explain-after-startup = { $span } after the service manager started (for user timers: login)
explain-after-unit-active = Every { $span }, counted from the last time the service started
explain-after-unit-inactive = { $span } after the service last finished, again after every run

