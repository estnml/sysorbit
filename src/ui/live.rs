//! Turns systemd's change signals into a debounced iced subscription.

use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use iced::Subscription;
use iced::futures::{SinkExt, Stream, StreamExt};
use tokio::time::{Instant, timeout_at};

use super::Message;
use crate::diagnostics::{self, Event as DiagnosticEvent, Phase, Redacted};
use crate::host::Host;
use crate::i18n::fl;
use crate::model::{Scope, Snapshot};
use crate::systemd::watch;

/// Quiet time after the last signal before refreshing.
const DEBOUNCE: Duration = Duration::from_millis(300);
/// Refresh at least this often while signals keep coming.
const MAX_DELAY: Duration = Duration::from_secs(2);
/// Pause before reconnecting a watch that failed.
const RETRY: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub enum Event {
    Connected,
    Changed,
    Lost(String),
}

#[derive(Debug, Clone, Default)]
pub enum State {
    #[default]
    Connecting,
    Live,
    Lost(String),
}

/// What to watch. A new timer, a timer turning active or a scope change
/// alters the hash, which makes iced replace the running subscription.
#[derive(Clone)]
struct Watch {
    host: Arc<dyn Host>,
    scope: Scope,
    units: Arc<watch::Watched>,
}

impl Hash for Watch {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.host.name().hash(state);
        self.scope.hash(state);
        self.units.hash(state);
    }
}

pub fn subscription(host: &Arc<dyn Host>, snapshot: &Snapshot) -> Subscription<Message> {
    let mut units = watch::Watched::default();
    for timer in &snapshot.timers {
        units.timers.push(timer.id.clone());
        if timer.active {
            units.active.push(timer.id.clone());
            units.active.push(timer.service.clone());
        }
    }
    units.timers.sort();
    units.active.sort();
    units.active.dedup();

    let watch = Watch {
        host: host.clone(),
        scope: snapshot.scope,
        units: Arc::new(units),
    };
    Subscription::run_with(watch, events).map(Message::Live)
}

fn events(watch: &Watch) -> impl Stream<Item = Event> + use<> {
    let watch = watch.clone();
    iced::stream::channel(8, async move |mut output| {
        loop {
            diagnostics::host_event(
                DiagnosticEvent::Reconnecting,
                Redacted::host(watch.host.name()),
            );
            let connected = async {
                let bus = watch.host.bus(watch.scope).await?;
                watch::changes(&bus, &watch.units).await
            };
            let mut changes = match connected.await {
                Ok(changes) => changes,
                Err(error) => {
                    diagnostics::event(DiagnosticEvent::Lost);
                    let _ = output.send(Event::Lost(error.to_string())).await;
                    tokio::time::sleep(RETRY).await;
                    continue;
                }
            };
            diagnostics::event(DiagnosticEvent::Connected);
            let _ = output.send(Event::Connected).await;

            while changes.next().await.is_some() {
                // Trailing debounce: wait for a quiet moment, but not forever.
                let deadline = Instant::now() + MAX_DELAY;
                diagnostics::measure(Phase::LiveDebounce, async {
                    loop {
                        let quiet = (Instant::now() + DEBOUNCE).min(deadline);
                        match timeout_at(quiet, changes.next()).await {
                            Ok(Some(())) => continue,
                            Ok(None) | Err(_) => break,
                        }
                    }
                })
                .await;
                let _ = output.send(Event::Changed).await;
            }

            diagnostics::event(DiagnosticEvent::Lost);
            let _ = output.send(Event::Lost(fl!("live-closed"))).await;
            tokio::time::sleep(RETRY).await;
        }
    })
}
