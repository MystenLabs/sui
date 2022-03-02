// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use bytes::Bytes;
use network::reliable_sender::ReliableSender;
use std::net::SocketAddr;
use sui_types::{error::SuiError, messages::*};

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
}

#[derive(Clone)]
pub struct AuthorityClient {
    authority_address: SocketAddr,
}

impl AuthorityClient {
    pub fn new(authority_address: SocketAddr) -> Self {
        Self { authority_address }
    }
}

#[async_trait]
impl AuthorityAPI for AuthorityClient {
    /// Initiate a new transfer to a Sui or Primary account.
    async fn handle_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let message = ClientToAuthorityCoreMessage::Transaction(transaction);
        let bytes = Bytes::from(bincode::serialize(&message)?);
        let handle = ReliableSender::new()
            .send(self.authority_address, bytes)
            .await;
        let reply = handle.await.unwrap();
        match bincode::deserialize(&reply)? {
            AuthorityToClientCoreMessage::TransactionInfoResponse(x) => x,
            _ => Err(SuiError::UnexpectedMessage),
        }
    }

    /// Confirm a transfer to a Sui or Primary account.
    async fn handle_confirmation_transaction(
        &self,
        transaction: ConfirmationTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let certificate = transaction.certificate;
        let message = ClientToAuthorityCoreMessage::Certificate(certificate);
        let bytes = Bytes::from(bincode::serialize(&message)?);
        let handle = ReliableSender::new()
            .send(self.authority_address, bytes)
            .await;
        let reply = handle.await.unwrap();
        match bincode::deserialize(&reply)? {
            AuthorityToClientCoreMessage::TransactionInfoResponse(x) => x,
            _ => Err(SuiError::UnexpectedMessage),
        }
    }

    async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, SuiError> {
        let message = ClientToAuthorityCoreMessage::AccountInfoRequest(request);
        let bytes = Bytes::from(bincode::serialize(&message)?);
        let handle = ReliableSender::new()
            .send(self.authority_address, bytes)
            .await;
        let reply = handle.await.unwrap();
        match bincode::deserialize(&reply)? {
            AuthorityToClientCoreMessage::AccountInfoResponse(x) => x,
            _ => Err(SuiError::UnexpectedMessage),
        }
    }

    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        let message = ClientToAuthorityCoreMessage::ObjectInfoRequest(request);
        let bytes = Bytes::from(bincode::serialize(&message)?);
        let handle = ReliableSender::new()
            .send(self.authority_address, bytes)
            .await;
        let reply = handle.await.unwrap();
        match bincode::deserialize(&reply)? {
            AuthorityToClientCoreMessage::ObjectInfoResponse(x) => x,
            _ => Err(SuiError::UnexpectedMessage),
        }
    }

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let message = ClientToAuthorityCoreMessage::TransactionInfoRequest(request);
        let bytes = Bytes::from(bincode::serialize(&message)?);
        let handle = ReliableSender::new()
            .send(self.authority_address, bytes)
            .await;
        let reply = handle.await.unwrap();
        match bincode::deserialize(&reply)? {
            AuthorityToClientCoreMessage::TransactionInfoResponse(x) => x,
            _ => Err(SuiError::UnexpectedMessage),
        }
    }
}
