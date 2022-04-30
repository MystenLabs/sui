// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use async_trait::async_trait;
use futures::lock::Mutex;
use futures::stream::{self, BoxStream};
use futures::StreamExt;
use std::collections::VecDeque;
use std::io;
use std::sync::Arc;
use sui_network::network::NetworkClient;
use sui_network::transport::TcpDataStream;
use sui_types::batch::UpdateItem;
use sui_types::{error::SuiError, messages::*, serialize::*};

#[cfg(test)]
use sui_types::{
    base_types::ObjectID,
    committee::Committee,
    crypto::{KeyPair, PublicKeyBytes},
    object::Object,
};

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

    /// Processes consensus request.
    async fn handle_consensus_transaction(
        &self,
        transaction: ConsensusTransaction,
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
pub struct NetworkAuthorityClient(NetworkClient);

impl NetworkAuthorityClient {
    pub fn new(network_client: NetworkClient) -> Self {
        Self(network_client)
    }
}

#[async_trait]
impl AuthorityAPI for NetworkAuthorityClient {
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

    async fn handle_consensus_transaction(
        &self,
        transaction: ConsensusTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let response = self
            .0
            .send_recv_bytes(serialize_consensus_transaction(&transaction))
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
                    // Convert io error to SuiClient error
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
                    Ok(BatchInfoResponseItem(UpdateItem::Transaction((seq, _digest)))) => {
                        *seq < request.end
                    }
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

#[derive(Clone, Copy, Default)]
pub struct LocalAuthorityClientFaultConfig {
    pub fail_before_handle_transaction: bool,
    pub fail_after_handle_transaction: bool,
    pub fail_before_handle_confirmation: bool,
    pub fail_after_handle_confirmation: bool,
}

impl LocalAuthorityClientFaultConfig {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone)]
pub struct LocalAuthorityClient {
    pub state: Arc<Mutex<AuthorityState>>,
    pub fault_config: LocalAuthorityClientFaultConfig,
}

#[async_trait]
impl AuthorityAPI for LocalAuthorityClient {
    async fn handle_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        if self.fault_config.fail_before_handle_transaction {
            return Err(SuiError::GenericAuthorityError {
                error: "Mock error before handle_transaction".to_owned(),
            });
        }
        let state = self.state.clone();
        let result = state.lock().await.handle_transaction(transaction).await;
        if self.fault_config.fail_after_handle_transaction {
            return Err(SuiError::GenericAuthorityError {
                error: "Mock error after handle_transaction".to_owned(),
            });
        }
        result
    }

    async fn handle_confirmation_transaction(
        &self,
        transaction: ConfirmationTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        if self.fault_config.fail_before_handle_confirmation {
            return Err(SuiError::GenericAuthorityError {
                error: "Mock error before handle_confirmation_transaction".to_owned(),
            });
        }
        let state = self.state.clone();
        let result = state
            .lock()
            .await
            .handle_confirmation_transaction(transaction)
            .await;
        if self.fault_config.fail_after_handle_confirmation {
            return Err(SuiError::GenericAuthorityError {
                error: "Mock error after handle_confirmation_transaction".to_owned(),
            });
        }
        result
    }

    async fn handle_consensus_transaction(
        &self,
        _transaction: ConsensusTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        unimplemented!("LocalAuthorityClient does not support consensus transaction");
    }

    async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, SuiError> {
        let state = self.state.clone();

        let result = state
            .lock()
            .await
            .handle_account_info_request(request)
            .await;
        result
    }

    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        let state = self.state.clone();
        let x = state.lock().await.handle_object_info_request(request).await;
        x
    }

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let state = self.state.clone();

        let result = state
            .lock()
            .await
            .handle_transaction_info_request(request)
            .await;
        result
    }

    /// Handle Batch information requests for this authority.
    async fn handle_batch_stream(
        &self,
        request: BatchInfoRequest,
    ) -> Result<BatchInfoResponseItemStream, io::Error> {
        let state = self.state.clone();

        let update_items = state.lock().await.handle_batch_info_request(request).await;

        let (items, _): (VecDeque<_>, VecDeque<_>) = update_items.into_iter().unzip();
        let stream = stream::iter(items.into_iter()).then(|mut item| async move {
            let i = item.pop_front();
            match i {
                Some(i) => Ok(BatchInfoResponseItem(i)),
                None => Result::Err(SuiError::BatchErrorSender),
            }
        });
        Ok(Box::pin(stream))
    }
}

impl LocalAuthorityClient {
    #[cfg(test)]
    pub async fn new(committee: Committee, address: PublicKeyBytes, secret: KeyPair) -> Self {
        use crate::authority::AuthorityStore;
        use std::{env, fs};
        use sui_adapter::genesis;

        // Random directory
        let dir = env::temp_dir();
        let path = dir.join(format!("DB_{:?}", ObjectID::random()));
        fs::create_dir(&path).unwrap();

        let store = Arc::new(AuthorityStore::open(path, None));
        let state = AuthorityState::new(
            committee.clone(),
            address,
            Arc::pin(secret),
            store,
            genesis::clone_genesis_compiled_modules(),
            &mut genesis::get_genesis_context(),
        )
        .await;
        Self {
            state: Arc::new(Mutex::new(state)),
            fault_config: LocalAuthorityClientFaultConfig::default(),
        }
    }

    #[cfg(test)]
    pub async fn new_with_objects(
        committee: Committee,
        address: PublicKeyBytes,
        secret: KeyPair,
        objects: Vec<Object>,
    ) -> Self {
        let client = Self::new(committee, address, secret).await;
        {
            let client_ref = client.state.as_ref().try_lock().unwrap();
            for object in objects {
                client_ref.insert_genesis_object(object).await;
            }
        }
        client
    }
}
