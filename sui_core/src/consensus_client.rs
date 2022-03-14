// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use bytes::Bytes;
use std::net::SocketAddr;
use std::sync::Arc;
use sui_network::transport;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::ConfirmationTransaction;
use sui_types::serialize::{deserialize_message, SerializedMessage};

/// The `ConsensusClient` receives certificates sequenced by the consensus and updates
/// the authority's database. The client assumes that the messages it receives have
/// already been authenticated (ie. they really come from a trusted consensus node) and
/// integrity-validated (ie. no corrupted messages).
pub struct ConsensusClient {
    /// The (global) authority state to update the locks of shared objects.
    state: Arc<AuthorityState>,
    /// The index of the latest consensus message we processed.
    latest_consensus_index: u64,
}

impl ConsensusClient {
    /// Create a new consensus handler with the input authority state.
    pub fn new(state: Arc<AuthorityState>) -> SuiResult<Self> {
        Ok(Self {
            state,
            // TODO: Read this field from the store.
            latest_consensus_index: 0,
        })
    }

    /// Spawn the consensus client in a new tokio task.
    pub fn spawn(mut handler: Self, address: SocketAddr, buffer_size: usize) {
        tokio::spawn(async move {
            let _ = handler.synchronize().await;
            handler.run(address, buffer_size).await;
        });
    }

    /// Synchronize with the consensus in case we missed part of its output sequence.
    /// It is safety-critical that we process the consensus outputs in the right order.
    async fn synchronize(&mut self) -> SuiResult<()> {
        // TODO: Implement the synchronizer.
        unimplemented!();
    }

    /// Process a single sequenced certificate.
    async fn handle_consensus_message(&mut self, bytes: Bytes) -> SuiResult<()> {
        // Check that the latest consensus index is as expected; otherwise synchronize.
        // TODO: Get the current consensus index from consensus.
        let consensus_index = 0;
        if self.latest_consensus_index != consensus_index {
            self.synchronize().await?;
            return Ok(());
        }

        // Update the latest consensus index. The authority state will atomically
        // update it in the storage when processing the certificate.
        self.latest_consensus_index += 1;

        // The consensus simply orders bytes, so we first need to deserialize the
        // certificate. If the deserialization fail it is safe to ignore the
        // certificate since all correct authorities will do the same.
        let confirmation = match deserialize_message(&*bytes) {
            Ok(SerializedMessage::Cert(certificate)) => ConfirmationTransaction {
                certificate: *certificate,
            },
            Ok(_) => {
                log::debug!("{}", SuiError::UnexpectedMessage);
                return Err(SuiError::UnexpectedMessage);
            }
            Err(e) => {
                log::debug!("Failed to deserialize certificate {}", e);
                return Err(SuiError::InvalidDecoding);
            }
        };

        // Process the certificate to set the locks on the shared objects.
        let certificate = confirmation.certificate;
        self.state.handle_consensus_certificate(&certificate).await
    }

    /// Main loop connecting to the consensus. This mainly acts as a light client.
    async fn run(&mut self, address: SocketAddr, buffer_size: usize) {
        // TODO: Do not try to reconnect immediately after the connection fails, use some
        // sort of back off. We may also move this logic to `sui-network::transport` to
        // expose a 'stream client' or something like that.
        'main: loop {
            // Subscribe to the consensus' output.
            let mut connection = match transport::connect(address.to_string(), buffer_size).await {
                Ok(stream) => stream,
                Err(e) => {
                    log::warn!("Failed to subscribe to consensus output: {}", e);
                    continue 'main;
                }
            };

            // Listen to sequenced certificates and process them.
            loop {
                let bytes = match connection.read_data().await {
                    Some(Ok(data)) => Bytes::from(data),
                    Some(Err(e)) => {
                        log::warn!("Failed to receive data from consensus: {}", e);
                        continue 'main;
                    }
                    None => {
                        log::debug!("Connection dropped by consensus");
                        continue 'main;
                    }
                };

                match self.handle_consensus_message(bytes).await {
                    // Log the errors that are our faults (not the client's).
                    Err(SuiError::StorageError(e)) => {
                        log::error!("{}", e);

                        // If we have a store error we cannot continue processing other
                        // outputs from consensus. We may otherwise attribute locks to
                        // shared objects that are different from other authorities. It
                        // is however safe to ask for that certificate again and re-process
                        // it (the core is idempotent).
                        self.latest_consensus_index -= 1;
                    }
                    // Log the errors that are the client's fault (not ours). This is
                    // only for debug purposes: all correct authorities will do the same.
                    Err(e) => log::debug!("{}", e),
                    Ok(()) => (),
                }
            }
        }
    }
}
