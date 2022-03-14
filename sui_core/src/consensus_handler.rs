// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use async_trait::async_trait;
use bytes::Bytes;
use futures::SinkExt;
use futures::StreamExt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use sui_network::network::{NetworkServer, NetworkClient};
use sui_network::transport::{MessageHandler, RwChannel, SpawnedServer};
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{ConfirmationTransaction, TransactionInfoResponse};
use sui_types::serialize::{deserialize_message, serialize_message, SerializedMessage};
use network_utils::transport;

/// The `ConsensusHandler` receives certificates sequenced by the consensus and updates
/// the authority's database
pub struct ConsensusHandler {
    /// The network address of to request sequenced certificates from the consensus.
    pub address: SocketAddr,
    /// The network buffer size. 
    pub buffer_size: usize,
    /// The (global) authority state to update the locks of shared objects.
    pub state: Arc<AuthorityState>,
}

impl ConsensusHandler {
    /// Create a new consensus handler instance.
    pub fn new(address: SocketAddr, buffer_size: usize, state: Arc<AuthorityState>) -> Self {
        Self {
            address, 
            buffer_size,
            state
        }
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
        self.state.handle_consensus_certificate(certificate).await
    }

    async fn run(&mut self) {
        'main: loop {
            // Subscribe to the consensus' output.
            let connection = match transport::connect(self.address.to_string(), self.buffer_size) {
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

/*
/// The `ConsensusHandler` receives certificates sequenced by the consensus and updates
/// the authority's database
pub struct ConsensusHandler {
    /// Receive sequenced certificates from consensus.
    server: NetworkServer,
    /// The (global) authority state to update the locks of shared objects.
    state: Arc<AuthorityState>,
}

impl ConsensusHandler {
    /// Create a new consensus handler instance.
    pub fn new(address: SocketAddr, buffer_size: usize, state: Arc<AuthorityState>) -> Self {
        Self {
            server: NetworkServer::new(address.ip().to_string(), address.port(), buffer_size),
            state,
        }
    }

    /// Spawn the consensus handler in a new task.
    pub async fn spawn(self) -> Result<SpawnedServer, std::io::Error> {
        let address = format!("{}:{}", self.server.base_address, self.server.base_port);
        let buffer_size = self.server.buffer_size;
        sui_network::transport::spawn_server(&address, self, buffer_size).await
    }

    async fn handle_one_message(&self, bytes: Bytes) -> SuiResult<TransactionInfoResponse> {
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
        let result = self.state.handle_consensus_certificate(certificate).await;
        match &result {
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
            Ok(_) => (),
        }

        // Make a reply for the end user.
        result
    }
}

#[async_trait]
impl<'a, A> MessageHandler<A> for ConsensusHandler
where
    A: 'static + RwChannel<'a> + Unpin + Send,
{
    async fn handle_messages(&self, mut channel: A) -> () {
        loop {
            // Read the consensus' output sequence.
            let buffer = match channel.stream().next().await {
                Some(Ok(buffer)) => buffer,
                Some(Err(err)) => {
                    // We expect some EOF or disconnect error at the end.
                    log::error!("Error while reading TCP stream: {}", err);
                    break;
                }
                None => break,
            };

            // Handle the message (update the state).
            let reply = match self.handle_one_message(Bytes::from(buffer)).await {
                Ok(x) => SerializedMessage::TransactionResp(Box::new(x)),
                Err(e) => SerializedMessage::Error(Box::new(e)),
            };

            // Reply to the consensus. The consensus will then decide what to do with this
            // reply; it can either forward it to the client or simply use it as ack for its
            // internal cleanup operations.
            let bytes = serialize_message(&reply);
            if let Err(error) = channel.sink().send(bytes.into()).await {
                log::error!("Failed to send query response: {}", error);
            }
        }
    }
}
*/