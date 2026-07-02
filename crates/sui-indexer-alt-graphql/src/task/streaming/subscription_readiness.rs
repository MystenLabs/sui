// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::OnceLock;

use anyhow::Context;
use tokio::sync::watch;

use crate::task::watermark::KV_PACKAGES_PIPELINE;
use crate::task::watermark::Watermarks;

/// Coordinates readiness of streaming subscriptions at service startup.
///
/// When the service begins streaming at checkpoint C, any package published at
/// checkpoints before C lives only in the database. A subscriber reading from the
/// streaming tip needs those earlier packages resolvable, so subscriptions must wait
/// until `kv_packages` has caught up to at least C - 1 before starting.
pub(crate) struct SubscriptionReadiness {
    /// Sequence number of the first checkpoint received from the live upstream gRPC stream
    /// (the broadcast tip at startup). Not from the kv-rpc resume fallback.
    first_live_checkpoint: OnceLock<u64>,
    watermarks_rx: watch::Receiver<Arc<Watermarks>>,
}

impl SubscriptionReadiness {
    pub(crate) fn new(watermarks_rx: watch::Receiver<Arc<Watermarks>>) -> Arc<Self> {
        Arc::new(Self {
            first_live_checkpoint: OnceLock::new(),
            watermarks_rx,
        })
    }

    /// Record the first checkpoint received from the live upstream stream. Idempotent — only
    /// the first call has effect.
    pub(crate) fn record_first_live_checkpoint(&self, checkpoint_seq: u64) {
        let _ = self.first_live_checkpoint.set(checkpoint_seq);
    }

    /// The first live checkpoint, once recorded. Guaranteed to be `Some` after
    /// `wait_for_ready()` returns `Ok`. Only the staging-gated subscription setup reads it.
    #[cfg(feature = "staging")]
    pub(crate) fn first_live_checkpoint(&self) -> Option<u64> {
        self.first_live_checkpoint.get().copied()
    }

    /// Wait until the service is ready to serve subscriptions: the first live checkpoint has
    /// been streamed AND `kv_packages` has indexed everything before it.
    pub(crate) async fn wait_for_ready(&self) -> anyhow::Result<()> {
        let mut watermarks_rx = self.watermarks_rx.clone();
        watermarks_rx
            .wait_for(|w| {
                let Some(&first_cp) = self.first_live_checkpoint.get() else {
                    return false;
                };
                let target = first_cp.saturating_sub(1);
                w.per_pipeline()
                    .get(KV_PACKAGES_PIPELINE)
                    .is_some_and(|p| p.hi().checkpoint() >= target)
            })
            .await
            .ok()
            .context("Watermark task shut down before subscriptions became ready")?;
        Ok(())
    }
}
