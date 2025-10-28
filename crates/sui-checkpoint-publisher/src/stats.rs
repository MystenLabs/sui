// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicU64, Ordering};
use tracing::info;

pub struct PublisherStats {
    checkpoints: AtomicU64,
    objects: AtomicU64,
    transactions: AtomicU64,
    events: AtomicU64,
    errors: AtomicU64,
}

impl PublisherStats {
    pub fn new() -> Self {
        Self {
            checkpoints: AtomicU64::new(0),
            objects: AtomicU64::new(0),
            transactions: AtomicU64::new(0),
            events: AtomicU64::new(0),
            errors: AtomicU64::new(0),
        }
    }

    pub fn checkpoint_processed(&self) {
        self.checkpoints.fetch_add(1, Ordering::Relaxed);
    }

    pub fn object_published(&self) {
        self.objects.fetch_add(1, Ordering::Relaxed);
    }

    pub fn transaction_published(&self) {
        self.transactions.fetch_add(1, Ordering::Relaxed);
    }

    pub fn event_published(&self) {
        self.events.fetch_add(1, Ordering::Relaxed);
    }

    pub fn error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn report(&self) {
        info!(
            "📊 Stats: {} checkpoints | {} objects | {} txs | {} events | {} errors",
            self.checkpoints.load(Ordering::Relaxed),
            self.objects.load(Ordering::Relaxed),
            self.transactions.load(Ordering::Relaxed),
            self.events.load(Ordering::Relaxed),
            self.errors.load(Ordering::Relaxed),
        );
    }
}
