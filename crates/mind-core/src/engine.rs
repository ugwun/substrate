use crate::{
    protocol::{HealthState, StatusSnapshot},
    PROTOCOL_VERSION,
};
use chrono::{DateTime, Utc};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{sync::watch, time::MissedTickBehavior};

pub struct MindEngine {
    tick_rate_hz: u32,
    tick: AtomicU64,
    clients: AtomicU32,
    shutting_down: AtomicBool,
    started_at: DateTime<Utc>,
    started_instant: Instant,
}

impl MindEngine {
    pub fn new(tick_rate_hz: u32) -> Arc<Self> {
        Arc::new(Self {
            tick_rate_hz,
            tick: AtomicU64::new(0),
            clients: AtomicU32::new(0),
            shutting_down: AtomicBool::new(false),
            started_at: Utc::now(),
            started_instant: Instant::now(),
        })
    }

    pub async fn run(self: Arc<Self>, mut shutdown: watch::Receiver<bool>) {
        let period = Duration::from_secs_f64(1.0 / f64::from(self.tick_rate_hz));
        let mut interval = tokio::time::interval(period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.tick.fetch_add(1, Ordering::Relaxed);
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        self.shutting_down.store(true, Ordering::Release);
                        break;
                    }
                }
            }
        }
    }

    pub fn status(&self) -> StatusSnapshot {
        StatusSnapshot {
            protocol_version: PROTOCOL_VERSION,
            daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
            pid: std::process::id(),
            health: if self.shutting_down.load(Ordering::Acquire) {
                HealthState::ShuttingDown
            } else {
                HealthState::Ready
            },
            started_at: self.started_at,
            uptime_ms: self.started_instant.elapsed().as_millis() as u64,
            tick: self.tick.load(Ordering::Relaxed),
            tick_rate_hz: self.tick_rate_hz,
            connected_clients: self.clients.load(Ordering::Relaxed),
        }
    }

    pub fn client_connected(&self) {
        self.clients.fetch_add(1, Ordering::Relaxed);
    }

    pub fn client_disconnected(&self) {
        self.clients.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn begin_shutdown(&self) {
        self.shutting_down.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn advances_ticks_and_stops() {
        let engine = MindEngine::new(20);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let task = tokio::spawn(engine.clone().run(shutdown_rx));
        tokio::task::yield_now().await;
        for _ in 0..3 {
            tokio::time::advance(Duration::from_millis(50)).await;
            tokio::task::yield_now().await;
        }
        assert!(engine.status().tick >= 2);
        shutdown_tx.send(true).unwrap();
        task.await.unwrap();
        assert_eq!(engine.status().health, HealthState::ShuttingDown);
    }
}
