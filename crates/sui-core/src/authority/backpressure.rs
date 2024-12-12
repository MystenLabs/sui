// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use mysten_metrics::monitored_scope;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::watch;
use tracing::{debug, info};

use crate::checkpoints::CheckpointStore;

#[derive(Debug, Default, Copy, Clone)]
struct Watermarks {
    executed: CheckpointSequenceNumber,
    certified: CheckpointSequenceNumber,
}

impl Watermarks {
    // we can only permit backpressure if the certified checkpoint is ahead of the executed
    // checkpoint. Otherwise, backpressure might prevent construction of the next checkpoint,
    // because it could stop consensus commits from being processed.
    fn should_suppress_backpressure(&self) -> bool {
        self.certified <= self.executed
    }
}

pub struct BackpressureManager {
    // Holds the executed and certified checkpoint watermarks.
    // Because we never execute an uncertified checkpoint, the executed watermark is always
    // less than or equal to the certified watermark.
    //
    // If the watermarks are equal, we must not apply backpressure to consensus handler,
    // because we could be waiting on the next consensus commit in order to build and eventually
    // certify the next checkpoint.
    watermarks_sender: watch::Sender<Watermarks>,

    // used by the WritebackCache to notify us when it has too many pending transactions in memory.
    backpressure_sender: watch::Sender<bool>,
}

pub struct BackpressureSubscriber {
    mgr: Arc<BackpressureManager>,
}

impl BackpressureManager {
    pub fn new_for_tests() -> Arc<Self> {
        Self::new_from_watermarks(Default::default())
    }

    fn new_from_watermarks(watermarks: Watermarks) -> Arc<Self> {
        let (watermarks_sender, _) = watch::channel(watermarks);
        let (backpressure_sender, _) = watch::channel(false);
        Arc::new(Self {
            watermarks_sender,
            backpressure_sender,
        })
    }

    pub fn new_from_checkpoint_store(store: &CheckpointStore) -> Arc<Self> {
        let executed = store
            .get_highest_executed_checkpoint_seq_number()
            .expect("read cannot fail")
            .unwrap_or_default();
        let certified = store
            .get_highest_synced_checkpoint_seq_number()
            .expect("read cannot fail")
            .unwrap_or_default();
        info!(
            ?executed,
            ?certified,
            "initializing backpressure manager from checkpoint store"
        );
        Self::new_from_watermarks(Watermarks {
            executed,
            certified,
        })
    }

    pub fn update_highest_certified_checkpoint(&self, seq: CheckpointSequenceNumber) {
        self.watermarks_sender.send_if_modified(|watermarks| {
            if seq > watermarks.certified {
                watermarks.certified = seq;
                debug!(?watermarks, "updating highest certified checkpoint");
                true
            } else {
                false
            }
        });
    }

    pub fn update_highest_executed_checkpoint(&self, seq: CheckpointSequenceNumber) {
        self.watermarks_sender.send_if_modified(|watermarks| {
            if seq > watermarks.executed {
                debug_assert_eq!(seq, watermarks.executed + 1);
                watermarks.executed = seq;
                debug!(?watermarks, "updating highest executed checkpoint");
                true
            } else {
                false
            }
        });
    }

    // Returns true if the backpressure state was changed.
    pub fn set_backpressure(&self, backpressure: bool) -> bool {
        self.backpressure_sender.send_if_modified(|bp| {
            if *bp != backpressure {
                debug!(?backpressure, "setting backpressure");
                *bp = backpressure;
                true
            } else {
                false
            }
        })
    }

    pub fn subscribe(self: &Arc<Self>) -> BackpressureSubscriber {
        BackpressureSubscriber { mgr: self.clone() }
    }
}

impl BackpressureSubscriber {
    pub fn is_backpressure_active(&self) -> bool {
        *self.mgr.backpressure_sender.borrow()
    }

    /// If there is no backpressure returns immediately.
    /// Otherwise, wait until backpressure is lifted or suppressed.
    pub async fn await_no_backpressure(&self) {
        let mut watermarks_rx = self.mgr.watermarks_sender.subscribe();
        if watermarks_rx
            .borrow_and_update()
            .should_suppress_backpressure()
        {
            return;
        }

        let mut backpressure_rx = self.mgr.backpressure_sender.subscribe();
        if !*backpressure_rx.borrow_and_update() {
            return;
        }

        info!("waiting for backpressure to be lifted");
        let _scope = monitored_scope("await_backpressure");

        loop {
            tokio::select! {
                _ = backpressure_rx.changed() => {
                    let backpressure = *backpressure_rx.borrow_and_update();
                    debug!(?backpressure, "backpressure updated");
                    if !backpressure {
                        info!("backpressure lifted");
                        return;
                    }
                }
                _ = watermarks_rx.changed() => {
                    let watermarks = watermarks_rx.borrow_and_update();
                    debug!(?watermarks, "watermarks updated");
                    if watermarks.should_suppress_backpressure() {
                        info!("backpressure suppressed");
                        return;
                    }
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use futures::FutureExt;
    use parking_lot::Mutex;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn test_no_backpressure() {
        let manager = Arc::new(BackpressureManager::new_for_tests());

        manager.update_highest_certified_checkpoint(1);
        manager.set_backpressure(false);

        let subscriber = manager.subscribe();

        subscriber.await_no_backpressure().now_or_never().unwrap();
    }

    #[tokio::test]
    async fn test_backpressure_suppressed() {
        let manager = Arc::new(BackpressureManager::new_for_tests());

        // watermarks start at 0, 0
        manager.set_backpressure(true);

        let subscriber = manager.subscribe();

        // backpressure should be suppressed because of watermarks.
        subscriber.await_no_backpressure().now_or_never().unwrap();
    }

    async fn await_with_timeout<R>(f: impl std::future::Future<Output = R>) {
        tokio::time::timeout(Duration::from_secs(1), f)
            .await
            .unwrap();
    }

    #[derive(Clone)]
    struct Log {
        log: Arc<Mutex<Vec<String>>>,
        manager: Arc<BackpressureManager>,
    }

    impl Log {
        fn new(manager: Arc<BackpressureManager>) -> Self {
            Self {
                log: Arc::new(Mutex::new(Vec::new())),
                manager,
            }
        }

        fn set_backpressure(&self, backpressure: bool) {
            self.log
                .lock()
                .push(format!("set backpressure {}", backpressure));
            self.manager.set_backpressure(backpressure);
        }

        fn update_executed(&self, executed: u64) {
            self.log
                .lock()
                .push(format!("update executed {}", executed));
            self.manager.update_highest_executed_checkpoint(executed);
        }

        fn push<S: Into<String>>(&self, msg: S) {
            self.log.lock().push(msg.into());
        }

        fn get(&self) -> Vec<String> {
            self.log.lock().clone()
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_clear_backpressure() {
        let manager = BackpressureManager::new_for_tests();

        // backpressure is in effect, and not suppressed by watermarks.
        manager.update_highest_certified_checkpoint(1);
        manager.set_backpressure(true);

        let log = Log::new(manager.clone());

        let waiter = tokio::spawn({
            let subscriber = manager.subscribe();
            let log = log.clone();
            log.push("await");
            async move {
                subscriber.await_no_backpressure().await;
                log.push("await_finished");
            }
        });

        // clear the backpressure
        log.set_backpressure(false);

        await_with_timeout(waiter).await;

        assert_eq!(
            log.get(),
            vec![
                "await".to_string(),
                "set backpressure false".to_string(),
                "await_finished".to_string(),
            ]
        );
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn test_backpressure_becomes_suppressed() {
        let manager = BackpressureManager::new_for_tests();

        // backpressure is in effect, and not suppressed by watermarks.
        manager.update_highest_certified_checkpoint(1);
        manager.set_backpressure(true);

        let log = Log::new(manager.clone());

        let waiter = tokio::spawn({
            let subscriber = manager.subscribe();
            let log = log.clone();
            log.push("await");
            async move {
                subscriber.await_no_backpressure().await;
                log.push("await_finished");
            }
        });

        // once executed checkpoint catches up to certified checkpoint,
        // backpressure should be suppressed.
        log.update_executed(1);

        await_with_timeout(waiter).await;

        assert_eq!(
            log.get(),
            vec![
                "await".to_string(),
                "update executed 1".to_string(),
                "await_finished".to_string(),
            ]
        );
    }
}
