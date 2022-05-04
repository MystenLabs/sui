// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_server::AuthorityServer;
use bytes::Bytes;
use std::sync::Arc;
use sui_types::{
    error::{SuiError, SuiResult},
    messages::ConfirmationTransaction,
};
use tokio::{sync::broadcast::Receiver, task::JoinHandle};

/// The `ConsensusHandler` receives certificates sequenced by the consensus and updates
/// the authority's database
pub struct ConsensusHandler {
    /// Receive sequenced certificates from consensus.
    rx_consensus: Receiver<Bytes>,
    /// The (global) authority server to update the locks.
    server: Arc<AuthorityServer>,
}

impl ConsensusHandler {
    /// Spawn a new `ConsensusHandler` in a separate tokio task.
    pub fn spawn(
        rx_consensus: Receiver<Bytes>,
        server: Arc<AuthorityServer>,
    ) -> JoinHandle<SuiResult<()>> {
        tokio::spawn(async move {
            Self {
                rx_consensus,
                server,
            }
            .run()
            .await
        })
    }

    /// Main reactor loop receiving certificates from consensus.
    async fn run(&mut self) -> SuiResult<()> {
        while let Ok(bytes) = self.rx_consensus.recv().await {
            // The consensus simply orders bytes, so we first need to deserialize the
            // certificate. If the deserialization fail it is safe to ignore the
            // certificate since all correct authorities will do the same.
            let confirmation = match bincode::deserialize::<CertifiedTransaction>(bytes) {
                Ok(certificate) => ConfirmationTransaction { certificate },
                Err(e) => {
                    log::debug!("Failed to deserialize certificate {e}");
                    continue;
                }
            };

            // Process the certificate to set the locks on the shared objects.
            let result = self
                .server
                .state
                .handle_consensus_certificate(confirmation.certificate)
                .await;
            match &result {
                // Log the errors that are our faults (not the client's).
                Err(SuiError::StorageError(e)) => {
                    log::error!("{e}");

                    // If we have a store error we cannot continue processing other
                    // outputs from consensus. We may otherwise attribute locks to
                    // shared objects that are different from other authorities.
                    return result;
                }
                // Log the errors that are the client's fault (not ours). This is
                // only for debug purposes: all correct authorities will do the same.
                Err(e) => {
                    log::debug!("{e}");
                    continue;
                }
                Ok(()) => (),
            }
        }
        Ok(())
    }
}
