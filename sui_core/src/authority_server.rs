// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use std::io;
use sui_network::{
    network::NetworkServer,
    transport::{spawn_server, MessageHandler, RwChannel, SpawnedServer},
};
use sui_types::{batch::UpdateItem, error::*, messages::*, serialize::*};

use crate::authority_batch::BatchManager;
use futures::{SinkExt, StreamExt};

use std::time::Duration;
use tracing::*;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::broadcast::error::RecvError;

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

    async fn handle_batch_streaming<'a, 'b, A>(
        &'a self,
        request: BatchInfoRequest,
        channel: &mut A,
    ) -> Result<(), SuiError>
    where
        A: RwChannel<'b>,
    {
        // Register a subscriber to not miss any updates
        let mut subscriber = self.state.subscribe()?;
        let message_end = request.end;

        // Get the historical data requested
        let (mut items, should_subscribe) = self.state.handle_batch_info_request(request).await?;

        let mut last_seq_sent = 0;
        while let Some(item) = items.pop_front() {
            // Remember the last transaction sequence number sent
            if let UpdateItem::Transaction((seq, _)) = &item {
                last_seq_sent = *seq;
            }

            // Send all items back to the client
            let item = serialize_batch_item(&BatchInfoResponseItem(item));
            channel
                .sink()
                .send(Bytes::from(item))
                .await
                .map_err(|_| SuiError::CannotSendClientMessageError)?;
        }

        // No need to send live events.
        if !should_subscribe {
            return Ok(());
        }

        // Now we read from the live updates.
        loop {
            match subscriber.recv().await {
                Ok(item) => {
                    let seq = match &item {
                        UpdateItem::Transaction((seq, _)) => *seq,
                        UpdateItem::Batch(signed_batch) => signed_batch.batch.next_sequence_number,
                    };

                    // Do not re-send transactions already sent from the database
                    if seq <= last_seq_sent {
                        continue;
                    }

                    let response = BatchInfoResponseItem(item);

                    // Send back the item from the subscription
                    let resp = serialize_batch_item(&response);
                    channel
                        .sink()
                        .send(Bytes::from(resp))
                        .await
                        .map_err(|_| SuiError::CannotSendClientMessageError)?;

                    // We always stop sending at batch boundaries, so that we try to always
                    // start with a batch and end with a batch to allow signature verification.
                    if let BatchInfoResponseItem(UpdateItem::Batch(signed_batch)) = &response {
                        if message_end < signed_batch.batch.next_sequence_number {
                            break;
                        }
                    }
                }
                Err(RecvError::Closed) => {
                    // The service closed the channel, so we tell the client.
                    return Err(SuiError::SubscriptionServiceClosed);
                }
                Err(RecvError::Lagged(number_skipped)) => {
                    // We tell the client they are too slow to consume, and
                    // stop.
                    return Err(SuiError::SubscriptionItemsDropedError(number_skipped));
                }
            }
        }

        Ok(())
    }

    async fn handle_one_message<'a, 'b, A>(
        &'a self,
        buffer: &'a [u8],
        channel: &mut A,
    ) -> Option<Vec<u8>>
    where
        A: RwChannel<'b>,
    {
        let result = deserialize_message(buffer);
        let reply = match result {
            Err(_) => Err(SuiError::InvalidDecoding),
            Ok(result) => {
                match result {
                    SerializedMessage::Transaction(message) => {
                        let tx_digest = message.digest();
                        // No allocations: it's a 'static str!
                        let tx_kind = message.data.kind_as_str();
                        self.state
                            .handle_transaction(*message)
                            .instrument(tracing::debug_span!("process_tx", ?tx_digest, tx_kind))
                            .await
                            .map(|info| Some(serialize_transaction_info(&info)))
                    }
                    SerializedMessage::Cert(message) => {
                        let confirmation_transaction = ConfirmationTransaction {
                            certificate: message.as_ref().clone(),
                        };
                        let tx_kind = message.transaction.data.kind_as_str();
                        match self
                            .state
                            .handle_confirmation_transaction(confirmation_transaction)
                            .instrument(tracing::debug_span!("process_cert",
                                                             tx_digest =? message.transaction.digest(),
                                                             tx_kind))
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
                    SerializedMessage::BatchInfoReq(message) => self
                        .handle_batch_streaming(*message, channel)
                        .await
                        .map(|_| None),

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

            if let Some(reply) = self.handle_one_message(&buffer[..], &mut channel).await {
                let status = channel.sink().send(reply.into()).await;
                if let Err(error) = status {
                    error!("Failed to send query response: {}", error);
                }
            };
        }
    }
}
