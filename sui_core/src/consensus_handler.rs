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

/// The `ConsensusHandler` receives certificates sequenced by the consensus and updates
/// the authority's database
pub struct ConsensusHandler {
    /// The (global) authority state to update the locks of shared objects.
    pub state: Arc<AuthorityState>,
}

impl ConsensusHandler {
    /// Spawn the consensus handler in a new tokio task.
    pub fn spawn(mut handler: Self, address: SocketAddr, buffer_size: usize) {
        tokio::spawn(async move {
            handler.run(address, buffer_size).await;
        });
    }

    /// Process a single sequenced certificate.
    async fn handle_consensus_message(&self, bytes: Bytes) -> SuiResult<()> {
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
                        // shared objects that are different from other authorities.
                        //panic!("{}", e); // Alberto is tempted to panic here
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
