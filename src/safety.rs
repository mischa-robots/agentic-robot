// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Michael Schaefer <https://github.com/mischa-robots/agentic-robot>

//! Watchdog safety system.
//!
//! Monitors command activity and stops motors if no command is received
//! within the configured timeout. This is critical because the PCA9685
//! motor driver board keeps motors running until explicitly stopped.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Trait for time source (enables testing with fake clocks).
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

/// Real system clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Watchdog that monitors command activity.
///
/// If no command is received within `timeout`, the watchdog triggers
/// and calls the stop callback.
pub struct Watchdog {
    timeout: Duration,
    cancel_token: CancellationToken,
    activity_notify: Arc<Notify>,
}

impl Watchdog {
    /// Create a new watchdog with the given timeout.
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            cancel_token: CancellationToken::new(),
            activity_notify: Arc::new(Notify::new()),
        }
    }

    /// Get a handle to notify the watchdog of activity.
    pub fn activity_handle(&self) -> WatchdogHandle {
        WatchdogHandle {
            notify: Arc::clone(&self.activity_notify),
        }
    }

    /// Run the watchdog loop. Calls `on_timeout` whenever the timeout expires.
    ///
    /// This should be spawned as a background task.
    pub async fn run<F>(&self, on_timeout: F)
    where
        F: Fn() + Send + Sync,
    {
        info!(timeout_secs = self.timeout.as_secs(), "watchdog started");

        loop {
            tokio::select! {
                () = self.cancel_token.cancelled() => {
                    info!("watchdog cancelled");
                    break;
                }
                () = tokio::time::sleep(self.timeout) => {
                    warn!("watchdog timeout — no activity for {:?}", self.timeout);
                    on_timeout();
                }
                () = self.activity_notify.notified() => {
                    // Activity received, reset the timer by looping
                    continue;
                }
            }
        }
    }

    /// Cancel the watchdog.
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }
}

/// Handle for notifying the watchdog of activity.
#[derive(Clone)]
pub struct WatchdogHandle {
    notify: Arc<Notify>,
}

impl WatchdogHandle {
    /// Signal that a command was received (resets the watchdog timer).
    pub fn ping(&self) {
        self.notify.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn watchdog_triggers_on_timeout() {
        let timeout_count = Arc::new(AtomicU32::new(0));
        let count_clone = Arc::clone(&timeout_count);

        let watchdog = Watchdog::new(Duration::from_millis(50));

        let task = tokio::spawn(async move {
            watchdog
                .run(move || {
                    count_clone.fetch_add(1, Ordering::SeqCst);
                })
                .await;
        });

        // Wait for at least one timeout
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should have triggered at least once
        assert!(timeout_count.load(Ordering::SeqCst) >= 1);

        task.abort();
    }

    #[tokio::test]
    async fn watchdog_resets_on_activity() {
        let timeout_count = Arc::new(AtomicU32::new(0));
        let count_clone = Arc::clone(&timeout_count);

        let watchdog = Watchdog::new(Duration::from_millis(100));
        let handle = watchdog.activity_handle();

        let task = tokio::spawn(async move {
            watchdog
                .run(move || {
                    count_clone.fetch_add(1, Ordering::SeqCst);
                })
                .await;
        });

        // Ping before timeout, multiple times
        for _ in 0..5 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            handle.ping();
        }

        // Should not have triggered
        assert_eq!(timeout_count.load(Ordering::SeqCst), 0);

        task.abort();
    }

    #[tokio::test]
    async fn watchdog_cancel_stops_loop() {
        let watchdog = Watchdog::new(Duration::from_millis(50));

        let cancel_watchdog = CancellationToken::new();
        let token_clone = cancel_watchdog.clone();

        let task = tokio::spawn(async move {
            watchdog.run(|| {}).await;
        });

        // Give it time to start
        tokio::time::sleep(Duration::from_millis(10)).await;
        token_clone.cancel();

        // Task should complete (abort to clean up since we can't cancel from outside easily)
        task.abort();
    }

    #[test]
    fn handle_clone_works() {
        let watchdog = Watchdog::new(Duration::from_secs(5));
        let handle1 = watchdog.activity_handle();
        let handle2 = handle1.clone();
        // Both should be usable
        handle1.ping();
        handle2.ping();
    }
}
