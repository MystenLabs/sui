// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use tokio::sync::watch;

pub struct CacheReadyCoordinator {
    latest: Arc<AtomicU64>,
    tx: tokio::sync::watch::Sender<u64>,
    rx: watch::Receiver<u64>,
}

// Signals handlers when a checkpoint's objects have been added to the package cache.
#[allow(clippy::new_without_default)]
impl CacheReadyCoordinator {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(0);
        Self {
            latest: Arc::new(AtomicU64::new(0)),
            tx,
            rx,
        }
    }

    pub fn mark_ready(&self, checkpoint: u64) {
        let prev = self.latest.swap(checkpoint, Ordering::SeqCst);
        if checkpoint > prev {
            let _ = self.tx.send_replace(checkpoint);
        } else {
            // Should never happen since concurrency is set to 1.
            panic!("Package cache coordinator saw checkpoints out of order.");
        }
    }

    pub async fn wait(&self, checkpoint: u64) {
        if self.latest.load(Ordering::SeqCst) >= checkpoint {
            return;
        }
        let mut rx = self.rx.clone();
        while rx.changed().await.is_ok() && *rx.borrow() < checkpoint {}
    }
}
