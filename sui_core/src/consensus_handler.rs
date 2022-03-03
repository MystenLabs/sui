// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use bytes::Bytes;
use std::sync::Arc;
use sui_types::error::SuiError;
use sui_types::messages::ConfirmationTransaction;
use tokio::sync::broadcast::Receiver;
use tokio::task::JoinHandle;

/// The `ConsensusHandler` receives certificates sequenced by the consensus and updates
/// the authority's database
pub struct ConsensusHandler {
    /// Receive sequenced certificates from consensus.
    rx_consensus: Receiver<Bytes>,
    /// The (global) authority state to update the locks.
    state: Arc<AuthorityState>,
}

impl ConsensusHandler {
    /// Spawn a new `ConsensusHandler` in a separate tokio task.
    pub fn spawn(rx_consensus: Receiver<Bytes>, state: Arc<AuthorityState>) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                rx_consensus,
                state,
            }
            .run()
            .await;
        })
    }

    /// Main reactor loop receiving certificates from consensus.
    async fn run(&mut self) {
        while let Ok(bytes) = self.rx_consensus.recv().await {
            // The consensus simply orders bytes, so we first need to deserialize the
            // certificate.
            let confirmation: ConfirmationTransaction = match bincode::deserialize(&bytes) {
                Ok(x) => x,
                Err(e) => {
                    log::debug!("Failed to deserialize certificate {}", e);
                    continue;
                }
            };

            // Process the certificate to set the locks on the shared objects.
            let certificate = &confirmation.certificate;
            match self.state.handle_consensus_certificate(certificate).await {
                // Log the errors that are our faults (not the client's).
                Err(SuiError::StorageError(e)) => {
                    log::error!("{}", e);
                    continue;
                }
                Err(e) => {
                    log::debug!("{}", e);
                    continue;
                }
                Ok(()) => (),
            }
        }
    }
}
