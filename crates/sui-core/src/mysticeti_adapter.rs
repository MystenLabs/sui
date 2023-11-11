// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use arc_swap::{ArcSwapOption, Guard};
use std::sync::Arc;
use std::time::Duration;

use sui_types::error::{SuiError, SuiResult};
use tap::prelude::*;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, timeout};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_adapter::SubmitToConsensus;
use sui_types::messages_consensus::ConsensusTransaction;
use tracing::warn;

#[derive(Clone)]
pub struct MysticetiClient {
    // channel to transport bcs-serialized bytes of ConsensusTransaction
    sender: mpsc::Sender<(Vec<u8>, oneshot::Sender<()>)>,
}

impl MysticetiClient {
    pub fn new(sender: mpsc::Sender<(Vec<u8>, oneshot::Sender<()>)>) -> MysticetiClient {
        MysticetiClient { sender }
    }

    async fn submit_transaction(&self, transaction: &ConsensusTransaction) -> SuiResult {
        let (sender, receiver) = oneshot::channel();
        let tx_bytes = bcs::to_bytes(&transaction).expect("Serialization should not fail.");
        self.sender
            .send((tx_bytes, sender))
            .await
            .tap_err(|e| warn!("Submit transaction failed with {:?}", e))
            .map_err(|e| SuiError::FailedToSubmitToConsensus(format!("{:?}", e)))?;
        // Give a little bit backpressure if BlockHandler is not able to keep up.
        receiver
            .await
            .tap_err(|e| warn!("Block Handler failed to ack: {:?}", e))
            .map_err(|e| SuiError::ConsensusConnectionBroken(format!("{:?}", e)))
    }
}

/// Basically a wrapper struct that reads from the LOCAL_MYSTICETI_CLIENT variable where the latest
/// MysticetiClient is stored in order to communicate with Mysticeti. The LazyMysticetiClient is considered
/// "lazy" only in the sense that we can't use it directly to submit to consensus unless the underlying
/// local client is set first.
#[derive(Default, Clone)]
pub struct LazyMysticetiClient {
    client: Arc<ArcSwapOption<MysticetiClient>>,
}

impl LazyMysticetiClient {
    pub fn new() -> Self {
        Self {
            client: Arc::new(ArcSwapOption::empty()),
        }
    }

    async fn get(&self) -> Guard<Option<Arc<MysticetiClient>>> {
        let client = self.client.load();
        if client.is_some() {
            return client;
        }

        // We expect this to get called during the SUI process start. After that at least one
        // object will have initialised and won't need to call again.
        const MYSTICETI_START_TIMEOUT: Duration = Duration::from_secs(30);
        const LOAD_RETRY_TIMEOUT: Duration = Duration::from_millis(100);
        if let Ok(client) = timeout(MYSTICETI_START_TIMEOUT, async {
            loop {
                let client = self.client.load();
                if client.is_some() {
                    return client;
                } else {
                    sleep(LOAD_RETRY_TIMEOUT).await;
                }
            }
        })
        .await
        {
            return client;
        }

        panic!(
            "Timed out after {:?} waiting for Mysticeti to start!",
            MYSTICETI_START_TIMEOUT,
        );
    }

    pub fn set(&self, client: MysticetiClient) {
        self.client.store(Some(Arc::new(client)));
    }
}

#[async_trait::async_trait]
impl SubmitToConsensus for LazyMysticetiClient {
    async fn submit_to_consensus(
        &self,
        transaction: &ConsensusTransaction,
        _epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        // The retrieved MysticetiClient can be from the past epoch. Submit would fail after
        // Mysticeti shuts down, so there should be no correctness issue.
        let client = self.get().await;
        client
            .as_ref()
            .expect("Client should always be returned")
            .submit_transaction(transaction)
            .await
            .tap_err(|r| {
                // Will be logged by caller as well.
                warn!("Submit transaction failed with: {:?}", r);
            })?;
        Ok(())
    }
}
