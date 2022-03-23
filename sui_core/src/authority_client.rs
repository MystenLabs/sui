// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use sui_network::network::NetworkClient;
use sui_types::batch::UpdateItem;
use sui_types::{error::SuiError, messages::*, serialize::*};
use tokio::sync::mpsc::Sender;

#[async_trait]
pub trait AuthorityAPI {
    /// Initiate a new transaction to a Sui or Primary account.
    async fn handle_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError>;

    /// Confirm a transaction to a Sui or Primary account.
    async fn handle_confirmation_transaction(
        &self,
        transaction: ConfirmationTransaction,
    ) -> Result<TransactionInfoResponse, SuiError>;

    /// Handle Account information requests for this account.
    async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, SuiError>;

    /// Handle Object information requests for this account.
    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError>;

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError>;

    /// Handle Batch information requests for this authority.
    async fn handle_batch_streaming(
        &self,
        request: BatchInfoRequest,
        channel: Sender<Result<BatchInfoResponseItem, SuiError>>,
        max_errors: i32,
    ) -> Result<(), SuiError>;
}

#[derive(Clone)]
pub struct AuthorityClient(NetworkClient);

impl AuthorityClient {
    pub fn new(network_client: NetworkClient) -> Self {
        Self(network_client)
    }
}

#[async_trait]
impl AuthorityAPI for AuthorityClient {
    /// Initiate a new transfer to a Sui or Primary account.
    async fn handle_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let response = self
            .0
            .send_recv_bytes(serialize_transaction(&transaction))
            .await?;
        deserialize_transaction_info(response)
    }

    /// Confirm a transfer to a Sui or Primary account.
    async fn handle_confirmation_transaction(
        &self,
        transaction: ConfirmationTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let response = self
            .0
            .send_recv_bytes(serialize_cert(&transaction.certificate))
            .await?;
        deserialize_transaction_info(response)
    }

    async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, SuiError> {
        let response = self
            .0
            .send_recv_bytes(serialize_account_info_request(&request))
            .await?;
        deserialize_account_info(response)
    }

    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        let response = self
            .0
            .send_recv_bytes(serialize_object_info_request(&request))
            .await?;
        deserialize_object_info(response)
    }

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let response = self
            .0
            .send_recv_bytes(serialize_transaction_info_request(&request))
            .await?;
        deserialize_transaction_info(response)
    }

    /// Handle Batch information requests for this authority.
    async fn handle_batch_streaming(
        &self,
        request: BatchInfoRequest,
        channel: Sender<Result<BatchInfoResponseItem, SuiError>>,
        max_errors: i32,
    ) -> Result<(), SuiError> {
        let (tx_cancellation, tr_cancellation) = tokio::sync::oneshot::channel();
        let mut inflight_stream = self
            .0
            .send_recv_bytes_stream(serialize_batch_request(&request), tr_cancellation)
            .await?;

        let mut error_count = 0;

        // Check the messages from the inflight_stream receiver to ensure each message is a
        // BatchInfoResponseItem, then send a Result<BatchInfoResponseItem, SuiError to the channel
        // that was passed in. For each message, also check if we have reached the last batch in the
        // request, and when we do, end the inflight stream task using tx_cancellation.
        while let Some(data) = inflight_stream.receiver.recv().await {
            match deserialize_batch_info(data) {
                Ok(batch_info_response_item) => {
                    // send to the caller via the channel
                    let _ = channel.send(Ok(batch_info_response_item.clone())).await;

                    // check for ending conditions
                    match batch_info_response_item {
                        BatchInfoResponseItem(UpdateItem::Batch(signed_batch)) => {
                            if signed_batch.batch.next_sequence_number > request.end {
                                let _ = tx_cancellation.send(());
                                break;
                            }
                        }
                        BatchInfoResponseItem(UpdateItem::Transaction((seq, _digest))) => {
                            if seq > request.end {
                                let _ = tx_cancellation.send(());
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = channel.send(Result::Err(e)).await;
                    error_count = error_count + 1;
                    if error_count >= max_errors {
                        let _ = tx_cancellation.send(());
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}
