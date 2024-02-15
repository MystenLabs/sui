// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use arc_swap::{ArcSwapOption, Guard};
use consensus_core::TransactionClient;
use sui_types::{
    error::{SuiError, SuiResult},
    messages_consensus::ConsensusTransaction,
};
use tap::prelude::*;
use tokio::time::{sleep, timeout};
use tracing::warn;

use crate::{
    authority::authority_per_epoch_store::AuthorityPerEpochStore,
    consensus_adapter::SubmitToConsensus,
};

/// Basically a wrapper struct that reads from the LOCAL_MYSTICETI_CLIENT variable where the latest
/// MysticetiClient is stored in order to communicate with Mysticeti. The LazyMysticetiClient is considered
/// "lazy" only in the sense that we can't use it directly to submit to consensus unless the underlying
/// local client is set first.
#[derive(Default, Clone)]
pub struct LazyMysticetiClient {
    client: Arc<ArcSwapOption<TransactionClient>>,
}

impl LazyMysticetiClient {
    pub fn new() -> Self {
        Self {
            client: Arc::new(ArcSwapOption::empty()),
        }
    }

    async fn get(&self) -> Guard<Option<Arc<TransactionClient>>> {
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

    pub fn set(&self, client: Arc<TransactionClient>) {
        self.client.store(Some(client));
    }
}

#[async_trait::async_trait]
impl SubmitToConsensus for LazyMysticetiClient {
    async fn submit_to_consensus(
        &self,
        transaction: &ConsensusTransaction,
        _epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        // TODO(mysticeti): confirm comment is still true
        // The retrieved TransactionClient can be from the past epoch. Submit would fail after
        // Mysticeti shuts down, so there should be no correctness issue.
        let client = self.get().await;
        let tx_bytes = bcs::to_bytes(&transaction).expect("Serialization should not fail.");
        client
            .as_ref()
            .expect("Client should always be returned")
            .submit(tx_bytes)
            .await
            .tap_err(|r| {
                // Will be logged by caller as well.
                warn!("Submit transaction failed with: {:?}", r);
            })
            .map_err(|err| SuiError::FailedToSubmitToConsensus(err.to_string()))?;
        Ok(())
    }
}
