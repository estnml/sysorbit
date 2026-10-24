//! Ticks on wall clock boundaries, so the comb and the clock label turn
//! together with the system clock.
//!
//! A `timerfd` on `CLOCK_REALTIME` with an absolute deadline lets the kernel
//! wake the app exactly at each boundary: no polling, a timer that expired
//! during suspend fires on resume, and `TFD_TIMER_CANCEL_ON_SET` reports
//! clock changes (NTP steps, manual adjustments) so the timer can be re-armed.

use std::io;
use std::os::fd::OwnedFd;
use std::time::Duration;

use iced::Subscription;
use iced::futures::{SinkExt, Stream};
use jiff::Timestamp;
use rustix::io::Errno;
use rustix::time::{
    Itimerspec, TimerfdClockId, TimerfdFlags, TimerfdTimerFlags, Timespec, timerfd_create,
    timerfd_settime,
};
use tokio::io::Interest;
use tokio::io::unix::AsyncFd;

use super::Message;

/// Sends [`Message::Tick`] whenever the wall clock crosses a multiple of
/// `period` since the Unix epoch.
pub fn subscription(period: Duration) -> Subscription<Message> {
    Subscription::run_with(period, ticks)
}

fn ticks(period: &Duration) -> impl Stream<Item = Message> + use<> {
    let period = *period;
    iced::stream::channel(1, async move |mut output| {
        if let Err(error) = watch(period, &mut output).await {
            // Without a timerfd the clock still turns, just not on the boundary.
            eprintln!("sysorbit: aligned clock unavailable: {error}");
            let mut interval = tokio::time::interval(period);
            loop {
                interval.tick().await;
                let _ = output.send(Message::Tick).await;
            }
        }
    })
}

async fn watch(
    period: Duration,
    output: &mut iced::futures::channel::mpsc::Sender<Message>,
) -> io::Result<()> {
    let timer = timerfd_create(
        TimerfdClockId::Realtime,
        TimerfdFlags::NONBLOCK | TimerfdFlags::CLOEXEC,
    )?;
    arm(&timer, period)?;
    // SAFETY: the `OwnedFd` moves into the `AsyncFd`, so the descriptor stays
    // open and unchanged for as long as it is registered.
    let timer = unsafe { AsyncFd::register_with_interest(timer, Interest::READABLE) }?;

    loop {
        let mut guard = timer.readable().await?;
        // Expiration count; how many boundaries passed does not matter.
        let mut expirations = [0u8; 8];
        match guard.try_io(|timer| Ok(rustix::io::read(timer.get_ref(), &mut expirations)?)) {
            Ok(Ok(_)) => {}
            Ok(Err(error)) if error.raw_os_error() == Some(Errno::CANCELED.raw_os_error()) => {
                arm(timer.get_ref(), period)?;
            }
            Ok(Err(error)) => return Err(error),
            Err(_would_block) => continue,
        }
        let _ = output.send(Message::Tick).await;
    }
}

/// Arms `timer` for the next boundary and every `period` after it. Arming
/// also clears a pending clock change cancellation.
fn arm(timer: &OwnedFd, period: Duration) -> io::Result<()> {
    let step = period.as_nanos().max(1) as i128;
    let now = Timestamp::now().as_nanosecond();
    let next = (now.div_euclid(step) + 1) * step;
    let spec = |nanos: i128| Timespec {
        tv_sec: (nanos / 1_000_000_000) as _,
        tv_nsec: (nanos % 1_000_000_000) as _,
    };
    timerfd_settime(
        timer,
        TimerfdTimerFlags::ABSTIME | TimerfdTimerFlags::CANCEL_ON_SET,
        &Itimerspec {
            it_interval: spec(step),
            it_value: spec(next),
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use iced::futures::StreamExt;
    use iced::futures::channel::mpsc;

    use super::*;

    #[tokio::test]
    async fn ticks_on_the_second() {
        let (mut sender, mut ticks) = mpsc::channel(4);
        tokio::spawn(async move { watch(Duration::from_secs(1), &mut sender).await });

        for _ in 0..2 {
            let tick = tokio::time::timeout(Duration::from_secs(3), ticks.next()).await;
            assert!(matches!(tick, Ok(Some(Message::Tick))));
            let late = Timestamp::now().subsec_millisecond();
            assert!(late < 50, "tick came {late} ms after the boundary");
        }
    }
}
