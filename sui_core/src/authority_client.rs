// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use std::io;
use sui_network::network::NetworkClient;
use sui_network::transport::TcpDataStream;
use sui_types::batch::UpdateItem;
use sui_types::{error::SuiError, messages::*, serialize::*};

static MAX_ERRORS: i32 = 10;

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

    async fn handle_batch_stream(
        &self,
        request: BatchInfoRequest,
    ) -> Result<BatchInfoResponseItemStream, io::Error>;
}

pub type BatchInfoResponseItemStream = BoxStream<'static, Result<BatchInfoResponseItem, SuiError>>;

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
    async fn handle_batch_stream(
        &self,
        request: BatchInfoRequest,
    ) -> Result<BatchInfoResponseItemStream, io::Error> {
        let tcp_stream = self
            .0
            .connect_for_stream(serialize_batch_request(&request))
            .await?;

        let mut error_count = 0;
        let TcpDataStream { framed_read, .. } = tcp_stream;

        let stream = framed_read
            .map(|item| {
                item
                    // Convert io error to SuiCLient error
                    .map_err(|err| SuiError::ClientIoError {
                        error: format!("io error: {:?}", err),
                    })
                    // If no error try to deserialize
                    .and_then(|bytes| match deserialize_message(&bytes[..]) {
                        Ok(SerializedMessage::Error(error)) => Err(SuiError::ClientIoError {
                            error: format!("io error: {:?}", error),
                        }),
                        Ok(message) => Ok(message),
                        Err(_) => Err(SuiError::InvalidDecoding),
                    })
                    // If deserialized try to parse as Batch Item
                    .and_then(deserialize_batch_info)
            })
            // Establish conditions to stop taking from the stream
            .take_while(move |item| {
                let flag = match item {
                    Ok(BatchInfoResponseItem(UpdateItem::Batch(signed_batch))) => {
                        signed_batch.batch.next_sequence_number < request.end
                    }
                    Ok(BatchInfoResponseItem(UpdateItem::Transaction((
                        seq,
                        _digest,
                        _tx_info,
                    )))) => *seq < request.end,
                    Err(_e) => {
                        // TODO: record e
                        error_count += 1;
                        error_count < MAX_ERRORS
                    }
                };
                futures::future::ready(flag)
            });
        Ok(Box::pin(stream))
    }
}
