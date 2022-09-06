// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use anyhow::anyhow;
use async_trait::async_trait;
use fastcrypto::traits::ToFromBytes;
use futures::{stream::BoxStream, TryStreamExt};
use multiaddr::Multiaddr;
use mysten_network::config::Config;
use prometheus::{register_histogram_with_registry, Histogram};
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_config::genesis::Genesis;
use sui_network::{api::ValidatorClient, tonic};
use sui_types::crypto::AuthorityPublicKeyBytes;
use sui_types::messages_checkpoint::{CheckpointRequest, CheckpointResponse};
use sui_types::sui_system_state::SuiSystemState;
use sui_types::{error::SuiError, messages::*};

#[cfg(test)]
use sui_types::{committee::Committee, crypto::AuthorityKeyPair, object::Object};

use crate::epoch::reconfiguration::Reconfigurable;
use sui_network::tonic::transport::Channel;

#[async_trait]
pub trait AuthorityAPI {
    /// Initiate a new transaction to a Sui or Primary account.
    async fn handle_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError>;

    /// Execute a certificate.
    async fn handle_certificate(
        &self,
        certificate: CertifiedTransaction,
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

    async fn handle_epoch(&self, request: EpochRequest) -> Result<EpochResponse, SuiError>;
}

pub type BatchInfoResponseItemStream = BoxStream<'static, Result<BatchInfoResponseItem, SuiError>>;

#[derive(Clone)]
pub struct NetworkAuthorityClient {
    client: ValidatorClient<tonic::transport::Channel>,
    metrics: Arc<NetworkAuthorityClientMetrics>,
}

impl NetworkAuthorityClient {
    pub async fn connect(
        address: &Multiaddr,
        metrics: Arc<NetworkAuthorityClientMetrics>,
    ) -> anyhow::Result<Self> {
        let channel = mysten_network::client::connect(address)
            .await
            .map_err(|err| anyhow!(err.to_string()))?;
        Ok(Self::new(channel, metrics))
    }

    pub fn connect_lazy(
        address: &Multiaddr,
        metrics: Arc<NetworkAuthorityClientMetrics>,
    ) -> anyhow::Result<Self> {
        let channel = mysten_network::client::connect_lazy(address)
            .map_err(|err| anyhow!(err.to_string()))?;
        Ok(Self::new(channel, metrics))
    }

    pub fn new(
        channel: tonic::transport::Channel,
        metrics: Arc<NetworkAuthorityClientMetrics>,
    ) -> Self {
        Self {
            client: ValidatorClient::new(channel),
            metrics,
        }
    }

    fn client(&self) -> ValidatorClient<tonic::transport::Channel> {
        self.client.clone()
    }
}

#[async_trait]
impl Reconfigurable for NetworkAuthorityClient {
    fn needs_network_recreation() -> bool {
        true
    }

    fn recreate(
        channel: tonic::transport::Channel,
        metrics: Arc<NetworkAuthorityClientMetrics>,
    ) -> Self {
        NetworkAuthorityClient::new(channel, metrics)
    }
}

#[async_trait]
impl AuthorityAPI for NetworkAuthorityClient {
    /// Initiate a new transfer to a Sui or Primary account.
    async fn handle_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let timer = self
            .metrics
            .handle_transaction_request_latency
            .start_timer();

        let response = self
            .client()
            .transaction(transaction)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into);

        timer.stop_and_record();

        response
    }

    /// Execute a certificate.
    async fn handle_certificate(
        &self,
        certificate: CertifiedTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let timer = self
            .metrics
            .handle_certificate_request_latency
            .start_timer();

        let response = self
            .client()
            .handle_certificate(certificate)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into);

        timer.stop_and_record();

        response
    }

    async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, SuiError> {
        let timer = self
            .metrics
            .handle_account_info_request_latency
            .start_timer();

        let response = self
            .client()
            .account_info(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into);

        timer.stop_and_record();

        response
    }

    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        let timer = self
            .metrics
            .handle_object_info_request_latency
            .start_timer();

        let response = self
            .client()
            .object_info(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into);

        timer.stop_and_record();

        response
    }

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let timer = self
            .metrics
            .handle_transaction_info_request_latency
            .start_timer();

        let response = self
            .client()
            .transaction_info(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into);

        timer.stop_and_record();

        response
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
        let timer = self.metrics.handle_checkpoint_request_latency.start_timer();

        let response = self
            .client()
            .checkpoint(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into);

        timer.stop_and_record();

        response
    }

    async fn handle_epoch(&self, request: EpochRequest) -> Result<EpochResponse, SuiError> {
        self.client()
            .epoch_info(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into)
    }
}

pub fn make_network_authority_client_sets_from_system_state(
    sui_system_state: &SuiSystemState,
    network_config: &Config,
    network_metrics: Arc<NetworkAuthorityClientMetrics>,
) -> anyhow::Result<BTreeMap<AuthorityPublicKeyBytes, NetworkAuthorityClient>> {
    let mut authority_clients = BTreeMap::new();
    for validator in &sui_system_state.validators.active_validators {
        let address = Multiaddr::try_from(validator.metadata.net_address.clone())?;
        let channel = network_config
            .connect_lazy(&address)
            .map_err(|err| anyhow!(err.to_string()))?;
        let client = NetworkAuthorityClient::new(channel, network_metrics.clone());
        let name: &[u8] = &validator.metadata.name;
        let public_key_bytes = AuthorityPublicKeyBytes::from_bytes(name)?;
        authority_clients.insert(public_key_bytes, client);
    }
    Ok(authority_clients)
}

pub fn make_network_authority_client_sets_from_genesis(
    genesis: &Genesis,
    network_config: &Config,
    network_metrics: Arc<NetworkAuthorityClientMetrics>,
) -> anyhow::Result<BTreeMap<AuthorityPublicKeyBytes, NetworkAuthorityClient>> {
    let mut authority_clients = BTreeMap::new();
    for validator in genesis.validator_set() {
        let channel = network_config
            .connect_lazy(validator.network_address())
            .map_err(|err| anyhow!(err.to_string()))?;
        let client = NetworkAuthorityClient::new(channel, network_metrics.clone());
        authority_clients.insert(validator.protocol_key(), client);
    }
    Ok(authority_clients)
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

impl Reconfigurable for LocalAuthorityClient {
    fn needs_network_recreation() -> bool {
        false
    }

    fn recreate(_channel: Channel, _metrics: Arc<NetworkAuthorityClientMetrics>) -> Self {
        unreachable!(); // this function should not get called because the above function returns false
    }
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

    async fn handle_certificate(
        &self,
        certificate: CertifiedTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        if self.fault_config.fail_before_handle_confirmation {
            return Err(SuiError::GenericAuthorityError {
                error: "Mock error before handle_confirmation_transaction".to_owned(),
            });
        }
        let state = self.state.clone();
        let result = state.handle_certificate(certificate).await;
        if self.fault_config.fail_after_handle_confirmation {
            return Err(SuiError::GenericAuthorityError {
                error: "Mock error after handle_confirmation_transaction".to_owned(),
            });
        }
        result
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

    async fn handle_epoch(&self, request: EpochRequest) -> Result<EpochResponse, SuiError> {
        let state = self.state.clone();

        state.handle_epoch_request(&request)
    }
}

impl LocalAuthorityClient {
    #[cfg(test)]
    pub async fn new(committee: Committee, secret: AuthorityKeyPair, genesis: &Genesis) -> Self {
        let (tx_reconfigure_consensus, _rx_reconfigure_consensus) = tokio::sync::mpsc::channel(10);
        let state = AuthorityState::new_for_testing(
            committee,
            &secret,
            None,
            Some(genesis),
            None,
            tx_reconfigure_consensus,
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
        secret: AuthorityKeyPair,
        objects: Vec<Object>,
        genesis: &Genesis,
    ) -> Self {
        let client = Self::new(committee, secret, genesis).await;

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

#[derive(Clone)]
pub struct NetworkAuthorityClientMetrics {
    pub handle_transaction_request_latency: Histogram,
    pub handle_certificate_request_latency: Histogram,
    pub handle_account_info_request_latency: Histogram,
    pub handle_object_info_request_latency: Histogram,
    pub handle_transaction_info_request_latency: Histogram,
    pub handle_checkpoint_request_latency: Histogram,
}

impl NetworkAuthorityClientMetrics {
    pub fn new(registry: &prometheus::Registry) -> Self {
        Self {
            handle_transaction_request_latency: register_histogram_with_registry!(
                "handle_transaction_request_latency",
                "Latency of handle transaction request",
                registry
            )
            .unwrap(),
            handle_certificate_request_latency: register_histogram_with_registry!(
                "handle_certificate_request_latency",
                "Latency of handle certificate request",
                registry
            )
            .unwrap(),
            handle_account_info_request_latency: register_histogram_with_registry!(
                "handle_account_info_request_latency",
                "Latency of handle account info request",
                registry
            )
            .unwrap(),
            handle_object_info_request_latency: register_histogram_with_registry!(
                "handle_object_info_request_latency",
                "Latency of handle object info request",
                registry
            )
            .unwrap(),
            handle_transaction_info_request_latency: register_histogram_with_registry!(
                "handle_transaction_info_request_latency",
                "Latency of handle transaction info request",
                registry
            )
            .unwrap(),
            handle_checkpoint_request_latency: register_histogram_with_registry!(
                "handle_checkpoint_request_latency",
                "Latency of handle checkpoint request",
                registry
            )
            .unwrap(),
        }
    }

    pub fn new_for_tests() -> Self {
        let registry = prometheus::Registry::new();
        Self::new(&registry)
    }
}
