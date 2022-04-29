// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use futures::{SinkExt, StreamExt};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::{io, sync::Arc};
use sui_network::{
    network::NetworkServer,
    transport::{spawn_server, MessageHandler, RwChannel, SpawnedServer},
};
use sui_types::{
    batch::UpdateItem, crypto::VerificationObligation, error::*, messages::*, serialize::*,
};
use tokio::sync::mpsc::Sender;

use std::time::Duration;
use tracing::{error, info, warn, Instrument};

use crate::consensus_adapter::{ConsensusAdapter, ConsensusInput};
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use tokio::sync::broadcast::error::RecvError;

#[cfg(test)]
#[path = "unit_tests/server_tests.rs"]
mod server_tests;

/*
    The number of input chunks the authority will try to process in parallel.

    TODO: provide a configuration parameter to allow authority operators to
    set it, or a dynamic mechanism to adapt it according to observed workload.
*/
const CHUNK_SIZE: usize = 36;
const MIN_BATCH_SIZE: u64 = 1000;
const MAX_DELAY_MILLIS: u64 = 5_000; // 5 sec

pub struct AuthorityServer {
    server: NetworkServer,
    pub state: Arc<AuthorityState>,
    consensus_adapter: ConsensusAdapter,
}

impl AuthorityServer {
    pub fn new(
        base_address: String,
        base_port: u16,
        buffer_size: usize,
        state: Arc<AuthorityState>,
        consensus_address: SocketAddr,
        tx_consensus_listener: Sender<ConsensusInput>,
    ) -> Self {
        let consensus_adapter = ConsensusAdapter::new(
            consensus_address,
            buffer_size,
            state.committee.clone(),
            tx_consensus_listener,
            /* max_delay */ Duration::from_millis(1_000),
        );
        Self {
            server: NetworkServer::new(base_address, base_port, buffer_size),
            state,
            consensus_adapter,
        }
    }

    /// Create a batch subsystem, register it with the authority state, and
    /// launch a task that manages it. Return the join handle of this task.
    pub async fn spawn_batch_subsystem(
        self: &Arc<Self>,
        min_batch_size: u64,
        max_delay: Duration,
    ) -> SuiResult<tokio::task::JoinHandle<SuiResult<()>>> {
        // Start the batching subsystem, and register the handles with the authority.
        let local_server = self.clone();

        let _batch_join_handle = tokio::task::spawn(async move {
            local_server
                .state
                .run_batch_service(min_batch_size, max_delay)
                .await
        });

        Ok(_batch_join_handle)
    }

    pub async fn spawn(self) -> Result<SpawnedServer<AuthorityServer>, io::Error> {
        let address = format!("{}:{}", self.server.base_address, self.server.base_port);
        self.spawn_with_bind_address(&address).await
    }

    pub async fn spawn_with_bind_address(
        self,
        address: &str,
    ) -> Result<SpawnedServer<AuthorityServer>, io::Error> {
        let buffer_size = self.server.buffer_size;
        let guarded_state = Arc::new(self);

        // Start the batching subsystem
        let _join_handle = guarded_state
            .spawn_batch_subsystem(MIN_BATCH_SIZE, Duration::from_millis(MAX_DELAY_MILLIS))
            .await;

        spawn_server(address, guarded_state, buffer_size).await
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
        let mut subscriber = self.state.subscribe_batch();
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
                    return Err(SuiError::SubscriptionItemsDroppedError(number_skipped));
                }
            }
        }

        Ok(())
    }

    async fn handle_one_message<'a, 'b, A>(
        &'a self,
        message: SerializedMessage,
        channel: &mut A,
    ) -> Option<Vec<u8>>
    where
        A: RwChannel<'b>,
    {
        let reply = match message {
            SerializedMessage::Transaction(message) => {
                let tx_digest = message.digest();
                // Enable Trace Propagation across spans/processes using tx_digest
                let span = tracing::debug_span!(
                    "process_tx",
                    ?tx_digest,
                    tx_kind = message.data.kind_as_str()
                );
                // No allocations: it's a 'static str!
                self.state
                    .handle_transaction(*message)
                    .instrument(span)
                    .await
                    .map(|info| Some(serialize_transaction_info(&info)))
            }
            SerializedMessage::Cert(message) => {
                let confirmation_transaction = ConfirmationTransaction {
                    certificate: message.as_ref().clone(),
                };
                let tx_digest = *message.digest();
                let span = tracing::debug_span!(
                    "process_cert",
                    ?tx_digest,
                    tx_kind = message.transaction.data.kind_as_str()
                );
                match self
                    .state
                    .handle_confirmation_transaction(confirmation_transaction)
                    .instrument(span)
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
            SerializedMessage::ConsensusTransaction(message) => self
                .consensus_adapter
                .submit(&message)
                .await
                .map(|info| Some(serialize_transaction_info(&info))),

            _ => Err(SuiError::UnexpectedMessage),
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
                warn!("User query failed: {error}");
                self.server.increment_user_errors();
                Some(serialize_error(&error))
            }
        }
    }

    /// For each Transaction and Certificate updates a verification
    /// obligation structure, and returns an error either if the collection in the
    /// obligation went wrong or the verification of the signatures went wrong.
    fn batch_verify_one_chunk(
        &self,
        one_chunk: Vec<Result<(SerializedMessage, BytesMut), SuiError>>,
    ) -> Result<VecDeque<(SerializedMessage, BytesMut)>, SuiError> {
        let one_chunk: Result<Vec<_>, _> = one_chunk.into_iter().collect();
        let one_chunk = one_chunk?;

        // Now create a verification obligation
        let mut obligation = VerificationObligation::default();
        let load_verification: Result<VecDeque<(SerializedMessage, BytesMut)>, SuiError> =
            one_chunk
                .into_iter()
                .map(|mut item| {
                    let (message, _message_bytes) = &mut item;
                    match message {
                        SerializedMessage::Transaction(message) => {
                            message.is_checked = true;
                            message.add_to_verification_obligation(&mut obligation)?;
                        }
                        SerializedMessage::Cert(message) => {
                            message.is_checked = true;
                            message.add_to_verification_obligation(
                                &self.state.committee,
                                &mut obligation,
                            )?;
                        }
                        _ => {}
                    };
                    Ok(item)
                })
                .collect();

        // Check the obligations and the verification is
        let one_chunk = load_verification?;
        obligation.verify_all()?;
        Ok(one_chunk)
    }
}

#[async_trait]
impl<'a, A> MessageHandler<A> for AuthorityServer
where
    A: 'static + RwChannel<'a> + Unpin + Send,
{
    async fn handle_messages(&self, mut channel: A) -> () {
        /*
            Take messages in chunks of CHUNK_SIZE and parses them, keeps also the
            original bytes for later use, and reports any errors. Special care is
            taken to turn all errors into SuiError.
        */

        while let Some(one_chunk) = channel
            .stream()
            .map(|msg_bytes_result| {
                msg_bytes_result
                    .map_err(|_| SuiError::InvalidDecoding)
                    .and_then(|msg_bytes| {
                        deserialize_message(&msg_bytes[..])
                            .map_err(|_| SuiError::InvalidDecoding)
                            .map(|msg| (msg, msg_bytes))
                    })
            })
            .ready_chunks(CHUNK_SIZE)
            .next()
            .await
        {
            /*
                Very the signatures of the chunk as a whole
            */
            let one_chunk = self.batch_verify_one_chunk(one_chunk);

            /*
                If this is an error send back the error and drop the connection.
                Here we make the choice to bail out as soon as either any parsing
                or signature / commitee verification operation fails. The client
                should know better than give invalid input. All conditions can be
                trivially checked on the client side, so there should be no surprises
                here for well behaved clients.
            */
            if let Err(err) = one_chunk {
                // If the response channel is closed there is no much we can do
                // to handle the error result.
                let _ = channel.sink().send(serialize_error(&err).into()).await;
                return;
            }
            let mut one_chunk = one_chunk.unwrap();

            // Process each message
            while let Some((_message, _buffer)) = one_chunk.pop_front() {
                if let Some(reply) = self.handle_one_message(_message, &mut channel).await {
                    let status = channel.sink().send(reply.into()).await;
                    if let Err(error) = status {
                        error!("Failed to send query response: {error}");
                    }
                };
            }
        }
    }
}
