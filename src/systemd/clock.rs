//! A host's own clock and time zone, so that a remote host's timeline
//! follows its clock rather than this machine's.

use jiff::Timestamp;
use zbus::Connection;
use zbus::proxy::CacheProperties;

use super::dbus::TimedateProxy;
use super::{Error, run};
use crate::host::{Host, Invocation};
use crate::model::HostClock;

/// Reads the host's clock through systemd-timedated, falling back to
/// `date` where timedated is missing. The offset is measured against the
/// middle of the round trip, so it is off by at most half of it.
pub async fn read(host: &dyn Host, bus: &Connection) -> Result<HostClock, Error> {
    match timedated(bus).await {
        Ok(clock) => Ok(clock),
        Err(_) => date(host).await,
    }
}

async fn timedated(bus: &Connection) -> zbus::Result<HostClock> {
    // The time changes on every read; a cached value would be stale.
    let timedate = TimedateProxy::builder(bus)
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    let zone = timedate
        .timezone()
        .await
        .ok()
        .filter(|zone| !zone.is_empty());
    let sent = Timestamp::now();
    let time = timedate.time_usec().await?;
    let received = Timestamp::now();
    let at = Timestamp::from_microsecond(time as i64)
        .map_err(|error| zbus::Error::Failure(error.to_string()))?;
    Ok(measure(at, sent, received, zone))
}

async fn date(host: &dyn Host) -> Result<HostClock, Error> {
    let sent = Timestamp::now();
    let stdout = run(host, Invocation::new("date").arg("+%s%N")).await?;
    let received = Timestamp::now();
    let nanos: i128 = stdout.trim().parse().map_err(Error::parse)?;
    let at = Timestamp::from_nanosecond(nanos).map_err(Error::parse)?;
    Ok(measure(at, sent, received, None))
}

fn measure(at: Timestamp, sent: Timestamp, received: Timestamp, zone: Option<String>) -> HostClock {
    let round_trip = received.duration_since(sent);
    let middle = sent + round_trip / 2;
    HostClock {
        offset: at.duration_since(middle),
        uncertainty: round_trip / 2,
        zone,
    }
}

#[cfg(test)]
mod tests {
    use jiff::SignedDuration;

    use super::*;

    #[test]
    fn measures_against_the_middle_of_the_round_trip() {
        let sent = Timestamp::from_second(1_800_000_000).unwrap();
        let received = sent + SignedDuration::from_millis(200);
        let at = sent + SignedDuration::from_millis(3100);
        let clock = measure(at, sent, received, None);
        assert_eq!(clock.offset, SignedDuration::from_millis(3000));
        assert_eq!(clock.uncertainty, SignedDuration::from_millis(100));
    }
}
