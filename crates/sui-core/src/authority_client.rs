// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use async_trait::async_trait;
use futures::{stream::BoxStream, TryStreamExt};
use multiaddr::Multiaddr;
use std::sync::Arc;

use sui_network::{api::ValidatorClient, tonic};
use sui_types::{error::SuiError, messages::*};

use sui_types::messages_checkpoint::{CheckpointRequest, CheckpointResponse};

#[cfg(test)]
use sui_types::{
    base_types::ObjectID,
    committee::Committee,
    crypto::{KeyPair, PublicKeyBytes},
    object::Object,
};

#[cfg(test)]
use sui_config::genesis::Genesis;

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
    ) -> Result<BatchInfoResponseItemStream, SuiError>;

    async fn handle_checkpoint(
        &self,
        request: CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError>;
}

pub type BatchInfoResponseItemStream = BoxStream<'static, Result<BatchInfoResponseItem, SuiError>>;

#[derive(Clone)]
pub struct NetworkAuthorityClient {
    client: ValidatorClient<tonic::transport::Channel>,
}

impl NetworkAuthorityClient {
    pub async fn connect(address: &Multiaddr) -> anyhow::Result<Self> {
        let channel = mysten_network::client::connect(address).await?;
        Ok(Self::new(channel))
    }

    pub fn connect_lazy(address: &Multiaddr) -> anyhow::Result<Self> {
        let channel = mysten_network::client::connect_lazy(address)?;
        Ok(Self::new(channel))
    }

    pub fn new(channel: tonic::transport::Channel) -> Self {
        Self {
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
        self.client()
            .transaction(transaction)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into)
    }

    /// Confirm a transfer to a Sui or Primary account.
    async fn handle_confirmation_transaction(
        &self,
        transaction: ConfirmationTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        self.client()
            .confirmation_transaction(transaction.certificate)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into)
    }

    async fn handle_consensus_transaction(
        &self,
        transaction: ConsensusTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        self.client()
            .consensus_transaction(transaction)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into)
    }

    async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, SuiError> {
        self.client()
            .account_info(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into)
    }

    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        self.client()
            .object_info(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into)
    }

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        self.client()
            .transaction_info(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into)
    }

    /// Handle Batch information requests for this authority.
    async fn handle_batch_stream(
        &self,
        request: BatchInfoRequest,
    ) -> Result<BatchInfoResponseItemStream, SuiError> {
        let stream = self
            .client()
            .batch_info(request)
            .await
            .map(tonic::Response::into_inner)?
            .map_err(Into::into);

        Ok(Box::pin(stream))
    }

    /// Handle Object information requests for this account.
    async fn handle_checkpoint(
        &self,
        request: CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        self.client()
            .checkpoint(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into)
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
    pub state: Arc<AuthorityState>,
    pub fault_config: LocalAuthorityClientFaultConfig,
}

#[async_trait]
impl AuthorityAPI for LocalAuthorityClient {
    async fn handle_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        if self.fault_config.fail_before_handle_transaction {
            return Err(SuiError::from("Mock error before handle_transaction"));
        }
        let state = self.state.clone();
        let result = state.handle_transaction(transaction).await;
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
        let result = state.handle_confirmation_transaction(transaction).await;
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
        state.handle_account_info_request(request).await
    }

    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        let state = self.state.clone();
        state.handle_object_info_request(request).await
    }

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let state = self.state.clone();
        state.handle_transaction_info_request(request).await
    }

    /// Handle Batch information requests for this authority.
    async fn handle_batch_stream(
        &self,
        request: BatchInfoRequest,
    ) -> Result<BatchInfoResponseItemStream, SuiError> {
        let state = self.state.clone();

        let update_items = state.handle_batch_streaming(request).await?;
        Ok(Box::pin(update_items))
    }

    async fn handle_checkpoint(
        &self,
        request: CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        let state = self.state.clone();

        state.handle_checkpoint_request(&request)
    }
}

impl LocalAuthorityClient {
    #[cfg(test)]
    pub async fn new(
        committee: Committee,
        address: PublicKeyBytes,
        secret: KeyPair,
        genesis: &Genesis,
    ) -> Self {
        use crate::authority::AuthorityStore;
        use crate::checkpoints::CheckpointStore;
        use parking_lot::Mutex;
        use std::{env, fs};

        // Random directory
        let dir = env::temp_dir();
        let path = dir.join(format!("DB_{:?}", ObjectID::random()));
        fs::create_dir(&path).unwrap();

        let secret = Arc::pin(secret);

        let mut store_path = path.clone();
        store_path.push("store");
        let store = Arc::new(AuthorityStore::open(&store_path, None));
        let mut checkpoints_path = path.clone();
        checkpoints_path.push("checkpoints");
        let checkpoints = CheckpointStore::open(
            &checkpoints_path,
            None,
            committee.epoch,
            address,
            secret.clone(),
        )
        .expect("Should not fail to open local checkpoint DB");

        let state = AuthorityState::new(
            committee.clone(),
            address,
            secret.clone(),
            store,
            None,
            None,
            Some(Arc::new(Mutex::new(checkpoints))),
            genesis,
            &prometheus::Registry::new(),
        )
        .await;
        Self {
            state: Arc::new(state),
            fault_config: LocalAuthorityClientFaultConfig::default(),
        }
    }

    #[cfg(test)]
    pub async fn new_with_objects(
        committee: Committee,
        address: PublicKeyBytes,
        secret: KeyPair,
        objects: Vec<Object>,
        genesis: &Genesis,
    ) -> Self {
        let client = Self::new(committee, address, secret, genesis).await;

        for object in objects {
            client.state.insert_genesis_object(object).await;
        }

        client
    }

    pub fn new_from_authority(state: Arc<AuthorityState>) -> Self {
        Self {
            state,
            fault_config: LocalAuthorityClientFaultConfig::default(),
        }
    }
}
