// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use std::io;
use sui_network::{
    network::NetworkServer,
    transport::{spawn_server, MessageHandler, RwChannel, SpawnedServer},
};
use sui_types::{error::*, messages::*, serialize::*};

use crate::authority_batch::BatchManager;
use futures::{SinkExt, StreamExt};

use std::time::Duration;
use tracing::*;

use async_trait::async_trait;

#[cfg(test)]
#[path = "unit_tests/server_tests.rs"]
mod server_tests;

pub struct AuthorityServer {
    server: NetworkServer,
    pub state: AuthorityState,
}

impl AuthorityServer {
    pub fn new(
        base_address: String,
        base_port: u16,
        buffer_size: usize,
        state: AuthorityState,
    ) -> Self {
        Self {
            server: NetworkServer::new(base_address, base_port, buffer_size),
            state,
        }
    }

    /// Create a batch subsystem, register it with the authority state, and
    /// launch a task that manages it. Return the join handle of this task.
    pub async fn spawn_batch_subsystem(
        &mut self,
        min_batch_size: u64,
        max_delay: Duration,
    ) -> Result<tokio::task::JoinHandle<()>, SuiError> {
        // Start the batching subsystem, and register the handles with the authority.
        let (tx_sender, manager, (batch_sender, _batch_receiver)) =
            BatchManager::new(self.state.db(), 1000);

        let _batch_join_handle = manager
            .start_service(
                self.state.name,
                self.state.secret.clone(),
                min_batch_size,
                max_delay,
            )
            .await?;
        self.state.set_batch_sender(tx_sender, batch_sender)?;

        Ok(_batch_join_handle)
    }

    pub async fn spawn(self) -> Result<SpawnedServer, io::Error> {
        let address = format!("{}:{}", self.server.base_address, self.server.base_port);
        let buffer_size = self.server.buffer_size;

        // Launch server for the appropriate protocol.
        spawn_server(&address, self, buffer_size).await
    }

    async fn handle_one_message<'a>(&'a self, buffer: &'a [u8]) -> Option<Vec<u8>> {
        let result = deserialize_message(buffer);
        let reply = match result {
            Err(_) => Err(SuiError::InvalidDecoding),
            Ok(result) => {
                match result {
                    SerializedMessage::Transaction(message) => self
                        .state
                        .handle_transaction(*message)
                        .await
                        .map(|info| Some(serialize_transaction_info(&info))),
                    SerializedMessage::Cert(message) => {
                        let confirmation_transaction = ConfirmationTransaction {
                            certificate: message.as_ref().clone(),
                        };
                        match self
                            .state
                            .handle_confirmation_transaction(confirmation_transaction)
                            .await
                        {
                            Ok(info) => {
                                // Response
                                Ok(Some(serialize_transaction_info(&info)))
                            }
                            Err(error) => Err(error),
                        }
                    }
                    SerializedMessage::AccountInfoReq(message) => self
                        .state
                        .handle_account_info_request(*message)
                        .await
                        .map(|info| Some(serialize_account_info_response(&info))),
                    SerializedMessage::ObjectInfoReq(message) => self
                        .state
                        .handle_object_info_request(*message)
                        .await
                        .map(|info| Some(serialize_object_info_response(&info))),
                    SerializedMessage::TransactionInfoReq(message) => self
                        .state
                        .handle_transaction_info_request(*message)
                        .await
                        .map(|info| Some(serialize_transaction_info(&info))),
                    _ => Err(SuiError::UnexpectedMessage),
                }
            }
        };

        self.server.increment_packets_processed();

        if self.server.packets_processed() % 5000 == 0 {
            info!(
                "{}:{} has processed {} packets",
                self.server.base_address,
                self.server.base_port,
                self.server.packets_processed()
            );
        }

        match reply {
            Ok(x) => x,
            Err(error) => {
                warn!("User query failed: {}", error);
                self.server.increment_user_errors();
                Some(serialize_error(&error))
            }
        }
    }
}

#[async_trait]
impl<'a, A> MessageHandler<A> for AuthorityServer
where
    A: 'static + RwChannel<'a> + Unpin + Send,
{
    async fn handle_messages(&self, mut channel: A) -> () {
        loop {
            let buffer = match channel.stream().next().await {
                Some(Ok(buffer)) => buffer,
                Some(Err(err)) => {
                    // We expect some EOF or disconnect error at the end.
                    error!("Error while reading TCP stream: {}", err);
                    break;
                }
                None => {
                    break;
                }
            };

            if let Some(reply) = self.handle_one_message(&buffer[..]).await {
                let status = channel.sink().send(reply.into()).await;
                if let Err(error) = status {
                    error!("Failed to send query response: {}", error);
                }
            };
        }
    }
}
