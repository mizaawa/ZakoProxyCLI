use super::{refresh_saved, require_authenticated};
use crate::Shared;
use futures_util::{StreamExt, stream};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
use tokio::{
    sync::{Mutex as AsyncMutex, OwnedMutexGuard},
    time::{self, MissedTickBehavior},
};

/// Only one user of a site's potentially single-use refresh token at a time.
#[derive(Default)]
pub(crate) struct SessionLocks(Mutex<HashMap<String, Weak<AsyncMutex<()>>>>);

impl SessionLocks {
    pub(crate) async fn lock(&self, id: &str) -> OwnedMutexGuard<()> {
        let gate = {
            let mut gates = self.0.lock().unwrap();
            if let Some(gate) = gates.get(id).and_then(Weak::upgrade) {
                gate
            } else {
                gates.retain(|_, gate| gate.strong_count() != 0);
                let gate = Arc::new(AsyncMutex::new(()));
                gates.insert(id.to_owned(), Arc::downgrade(&gate));
                gate
            }
        };
        gate.lock_owned().await
    }
}

fn balance_timer() -> time::Interval {
    let mut timer = time::interval(Duration::from_secs(5 * 60));
    timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    timer
}

/// One immediate pass on process startup, then one every five minutes. No browser or admin session required.
pub async fn run_balance_refresh(state: Shared) {
    let mut timer = balance_timer();
    loop {
        timer.tick().await;
        let ids: Vec<_> = match state.store.config() {
            Ok(config) => config
                .providers
                .into_iter()
                .filter(|p| require_authenticated(p).is_ok())
                .map(|p| p.id)
                .collect(),
            Err(_) => {
                tracing::warn!("Could not read saved providers for the scheduled balance refresh");
                continue;
            }
        };
        stream::iter(ids).for_each_concurrent(3, |id| {
            let state = &state;
            async move {
                match refresh_saved(state, &id).await {
                    Ok(provider) if provider.last_error.is_none() => (),
                    // The manual endpoint already exposes a sanitized, persisted error. Don't log secrets or upstream bodies.
                    _ => tracing::warn!(provider_id = %id, "Scheduled balance refresh did not succeed"),
                }
            }
        }).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;

    #[tokio::test(start_paused = true)]
    async fn balance_clock_runs_immediately_then_every_five_minutes_without_catchup_bursts() {
        let mut timer = balance_timer();
        timer.tick().await;
        time::advance(Duration::from_secs(299)).await;
        assert!(timer.tick().now_or_never().is_none());
        time::advance(Duration::from_secs(1)).await;
        assert!(timer.tick().now_or_never().is_some());
        time::advance(Duration::from_secs(901)).await;
        timer.tick().await;
        assert!(timer.tick().now_or_never().is_none());
    }

    #[tokio::test]
    async fn single_use_credentials_are_serialized_per_provider_not_globally() {
        let locks = SessionLocks::default();
        let held = locks.lock("site-a").await;
        let blocked = locks.lock("site-a");
        tokio::pin!(blocked);
        assert!(blocked.as_mut().now_or_never().is_none());
        assert!(locks.lock("site-b").now_or_never().is_some());
        drop(held);
        drop(blocked.await);
    }
}
