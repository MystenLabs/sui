// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use async_trait::async_trait;
use futures::{
    lock::Mutex,
    stream::{self, BoxStream},
    StreamExt, TryStreamExt,
};
use std::{collections::VecDeque, io, sync::Arc};
use sui_network::{
    api::{BincodeEncodedPayload, ValidatorClient},
    network::NetworkClient,
    tonic,
};
use sui_types::{error::SuiError, messages::*};

#[cfg(test)]
use sui_types::{
    base_types::ObjectID,
    committee::Committee,
    crypto::{KeyPair, PublicKeyBytes},
    object::Object,
};

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
pub struct NetworkAuthorityClient {
    _network_client: NetworkClient,
    client: ValidatorClient<tonic::transport::Channel>,
}

impl NetworkAuthorityClient {
    pub fn new(network_client: NetworkClient) -> Self {
        let uri = format!(
            "http://{}:{}",
            network_client.base_address(),
            network_client.base_port()
        )
        .parse()
        .unwrap();
        let channel = tonic::transport::Channel::builder(uri)
            .connect_timeout(network_client.send_timeout())
            .timeout(network_client.recv_timeout())
            .connect_lazy();
        let client = ValidatorClient::new(channel);
        Self {
            _network_client: network_client,
            client,
        }
    }

    pub fn with_channel(channel: tonic::transport::Channel, network_client: NetworkClient) -> Self {
        Self {
            _network_client: network_client,
            client: ValidatorClient::new(channel),
        }
    }

    fn client(&self) -> ValidatorClient<tonic::transport::Channel> {
        self.client.clone()
    }
}

#[async_trait]
impl AuthorityAPI for NetworkAuthorityClient {
    /// Initiate a new transfer to a Sui or Primary account.
    async fn handle_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let request = BincodeEncodedPayload::try_from(&transaction).unwrap();
        let response = self
            .client()
            .transaction(request)
            .await
            .map_err(|_| SuiError::UnexpectedMessage)?
            .into_inner();

        response
            .deserialize()
            .map_err(|_| SuiError::UnexpectedMessage)
    }

    /// Confirm a transfer to a Sui or Primary account.
    async fn handle_confirmation_transaction(
        &self,
        transaction: ConfirmationTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let request = BincodeEncodedPayload::try_from(&transaction).unwrap();
        let response = self
            .client()
            .confirmation_transaction(request)
            .await
            .map_err(|_| SuiError::UnexpectedMessage)?
            .into_inner();

        response
            .deserialize()
            .map_err(|_| SuiError::UnexpectedMessage)
    }

    async fn handle_consensus_transaction(
        &self,
        transaction: ConsensusTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let request = BincodeEncodedPayload::try_from(&transaction).unwrap();
        let response = self
            .client()
            .consensus_transaction(request)
            .await
            .map_err(|e| SuiError::GenericAuthorityError {
                error: e.to_string(),
            })?
            .into_inner();

        response
            .deserialize()
            .map_err(|e| SuiError::GenericAuthorityError {
                error: e.to_string(),
            })
    }

    async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, SuiError> {
        let request = BincodeEncodedPayload::try_from(&request).unwrap();
        let response = self
            .client()
            .account_info(request)
            .await
            .map_err(|_| SuiError::UnexpectedMessage)?
            .into_inner();

        response
            .deserialize()
            .map_err(|_| SuiError::UnexpectedMessage)
    }

    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        let request = BincodeEncodedPayload::try_from(&request).unwrap();
        let response = self
            .client()
            .object_info(request)
            .await
            .map_err(|_| SuiError::UnexpectedMessage)?
            .into_inner();

        response
            .deserialize()
            .map_err(|_| SuiError::UnexpectedMessage)
    }

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let request = BincodeEncodedPayload::try_from(&request).unwrap();
        let response = self
            .client()
            .transaction_info(request)
            .await
            .map_err(|_| SuiError::UnexpectedMessage)?
            .into_inner();

        response
            .deserialize()
            .map_err(|_| SuiError::UnexpectedMessage)
    }

    /// Handle Batch information requests for this authority.
    async fn handle_batch_stream(
        &self,
        request: BatchInfoRequest,
    ) -> Result<BatchInfoResponseItemStream, io::Error> {
        let request = BincodeEncodedPayload::try_from(&request).unwrap();
        let response_stream = self
            .client()
            .batch_info(request)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
            .into_inner();

        let stream = response_stream
            .map_err(|_| SuiError::UnexpectedMessage)
            .and_then(|item| {
                let response_item = item
                    .deserialize::<BatchInfoResponseItem>()
                    .map_err(|_| SuiError::UnexpectedMessage);
                futures::future::ready(response_item)
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
