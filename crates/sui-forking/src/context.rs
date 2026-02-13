// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use rand::rngs::OsRng;
use sui_rpc_api::subscription::SubscriptionServiceHandle;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::storage::ReadStore;
use tokio::sync::RwLock;
use tokio::sync::mpsc;

use crate::store::ForkingStore;
use simulacrum::Simulacrum;
use sui_types::digests::ChainIdentifier;

#[derive(Clone)]
pub(crate) struct Context {
    pub simulacrum: Arc<RwLock<Simulacrum<OsRng, ForkingStore>>>,
    pub subscription_service_handle: SubscriptionServiceHandle,
    pub checkpoint_sender: mpsc::Sender<Checkpoint>,
    pub chain_id: ChainIdentifier,
}

impl Context {
    /// Publish a checkpoint to the subscription service by its sequence number. This is used for
    /// the subscription service for checkpoints.
    pub async fn publish_checkpoint_by_sequence_number(
        &self,
        checkpoint_sequence_number: u64,
    ) -> anyhow::Result<()> {
        let checkpoint = {
            let sim = self.simulacrum.read().await;
            let store = sim.store_static();

            let verified_checkpoint = store
                .get_checkpoint_by_sequence_number(checkpoint_sequence_number, true)
                .with_context(|| {
                    format!("missing checkpoint summary at sequence {checkpoint_sequence_number}")
                })?;
            let checkpoint_contents = store
                .get_checkpoint_contents_by_digest(&verified_checkpoint.content_digest, true)
                .with_context(|| {
                    format!("missing checkpoint contents for sequence {checkpoint_sequence_number}")
                })?;

            store.get_checkpoint_data(verified_checkpoint.clone(), checkpoint_contents)?
        };

        self.checkpoint_sender
            .send(checkpoint)
            .await
            .context("failed to enqueue checkpoint to subscription service")
    }
}
