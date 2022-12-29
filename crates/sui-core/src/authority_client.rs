// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use anyhow::anyhow;
use async_trait::async_trait;
use fastcrypto::traits::ToFromBytes;
use multiaddr::Multiaddr;
use mysten_metrics::spawn_monitored_task;
use mysten_network::config::Config;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use sui_config::genesis::Genesis;
use sui_config::ValidatorInfo;
use sui_network::{api::ValidatorClient, tonic};
use sui_types::base_types::AuthorityName;
use sui_types::committee::CommitteeWithNetAddresses;
use sui_types::crypto::AuthorityPublicKeyBytes;
use sui_types::messages_checkpoint::{CheckpointRequest, CheckpointResponse};
use sui_types::sui_system_state::SuiSystemState;
use sui_types::{committee::Committee, crypto::AuthorityKeyPair, object::Object};
use sui_types::{error::SuiError, messages::*};

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

    async fn handle_checkpoint(
        &self,
        request: CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError>;

    async fn handle_committee_info_request(
        &self,
        request: CommitteeInfoRequest,
    ) -> Result<CommitteeInfoResponse, SuiError>;
}

#[derive(Clone)]
pub struct NetworkAuthorityClient {
    client: ValidatorClient<Channel>,
}

impl NetworkAuthorityClient {
    pub async fn connect(address: &Multiaddr) -> anyhow::Result<Self> {
        let channel = mysten_network::client::connect(address)
            .await
            .map_err(|err| anyhow!(err.to_string()))?;
        Ok(Self::new(channel))
    }

    pub fn connect_lazy(address: &Multiaddr) -> anyhow::Result<Self> {
        let channel = mysten_network::client::connect_lazy(address)
            .map_err(|err| anyhow!(err.to_string()))?;
        Ok(Self::new(channel))
    }

    pub fn new(channel: Channel) -> Self {
        Self {
            client: ValidatorClient::new(channel),
        }
    }

    fn client(&self) -> ValidatorClient<Channel> {
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

    /// Execute a certificate.
    async fn handle_certificate(
        &self,
        certificate: CertifiedTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        self.client()
            .handle_certificate(certificate)
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

    async fn handle_committee_info_request(
        &self,
        request: CommitteeInfoRequest,
    ) -> Result<CommitteeInfoResponse, SuiError> {
        self.client()
            .committee_info(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into)
    }
}

// This function errs on URL parsing error. This may happen
// when a validator provides a bad URL.
pub fn make_network_authority_client_sets_from_system_state(
    sui_system_state: &SuiSystemState,
    network_config: &Config,
) -> anyhow::Result<BTreeMap<AuthorityName, NetworkAuthorityClient>> {
    let mut authority_clients = BTreeMap::new();
    for validator in &sui_system_state.validators.active_validators {
        let address = Multiaddr::try_from(validator.metadata.net_address.clone())?;
        let channel = network_config
            .connect_lazy(&address)
            .map_err(|err| anyhow!(err.to_string()))?;
        let client = NetworkAuthorityClient::new(channel);
        let name: &[u8] = &validator.metadata.pubkey_bytes;
        let public_key_bytes = AuthorityName::from_bytes(name)?;
        authority_clients.insert(public_key_bytes, client);
    }
    Ok(authority_clients)
}

pub fn make_network_authority_client_sets_from_committee(
    committee: &CommitteeWithNetAddresses,
    network_config: &Config,
) -> anyhow::Result<BTreeMap<AuthorityName, NetworkAuthorityClient>> {
    let mut authority_clients = BTreeMap::new();
    for (name, _stakes) in &committee.committee.voting_rights {
        let address = committee.net_addresses.get(name).ok_or_else(|| {
            SuiError::from("Missing network address in CommitteeWithNetAddresses")
        })?;
        let address = Multiaddr::try_from(address.clone())?;
        let channel = network_config
            .connect_lazy(&address)
            .map_err(|err| anyhow!(err.to_string()))?;
        let client = NetworkAuthorityClient::new(channel);
        authority_clients.insert(*name, client);
    }
    Ok(authority_clients)
}

pub fn make_network_authority_client_sets_from_genesis(
    genesis: &Genesis,
    network_config: &Config,
) -> anyhow::Result<BTreeMap<AuthorityPublicKeyBytes, NetworkAuthorityClient>> {
    let mut authority_clients = BTreeMap::new();
    for validator in genesis.validator_set() {
        let channel = network_config
            .connect_lazy(validator.network_address())
            .map_err(|err| anyhow!(err.to_string()))?;
        let client = NetworkAuthorityClient::new(channel);
        authority_clients.insert(validator.protocol_key(), client);
    }
    Ok(authority_clients)
}

pub fn make_authority_clients(
    validator_set: &[ValidatorInfo],
    connect_timeout: Duration,
    request_timeout: Duration,
) -> BTreeMap<AuthorityName, NetworkAuthorityClient> {
    let mut authority_clients = BTreeMap::new();
    let mut network_config = mysten_network::config::Config::new();
    network_config.connect_timeout = Some(connect_timeout);
    network_config.request_timeout = Some(request_timeout);
    for authority in validator_set {
        let channel = network_config
            .connect_lazy(authority.network_address())
            .unwrap();
        let client = NetworkAuthorityClient::new(channel);
        authority_clients.insert(authority.protocol_key(), client);
    }
    authority_clients
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
        let transaction = transaction.verify()?;
        let result = state.handle_transaction(transaction).await;
        if self.fault_config.fail_after_handle_transaction {
            return Err(SuiError::GenericAuthorityError {
                error: "Mock error after handle_transaction".to_owned(),
            });
        }
        result.map(|r| r.into())
    }

    async fn handle_certificate(
        &self,
        certificate: CertifiedTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let state = self.state.clone();
        let fault_config = self.fault_config;
        spawn_monitored_task!(Self::handle_certificate(state, certificate, fault_config))
            .await
            .unwrap()
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
        state
            .handle_object_info_request(request)
            .await
            .map(|r| r.into())
    }

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let state = self.state.clone();
        state
            .handle_transaction_info_request(request)
            .await
            .map(|r| r.into())
    }

    async fn handle_checkpoint(
        &self,
        request: CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        let state = self.state.clone();

        state.handle_checkpoint_request(&request)
    }

    async fn handle_committee_info_request(
        &self,
        request: CommitteeInfoRequest,
    ) -> Result<CommitteeInfoResponse, SuiError> {
        let state = self.state.clone();

        state.handle_committee_info_request(&request)
    }
}

impl LocalAuthorityClient {
    pub async fn new(committee: Committee, secret: AuthorityKeyPair, genesis: &Genesis) -> Self {
        let state = AuthorityState::new_for_testing(committee, &secret, None, Some(genesis)).await;
        Self {
            state,
            fault_config: LocalAuthorityClientFaultConfig::default(),
        }
    }

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

    async fn handle_certificate(
        state: Arc<AuthorityState>,
        certificate: CertifiedTransaction,
        fault_config: LocalAuthorityClientFaultConfig,
    ) -> Result<TransactionInfoResponse, SuiError> {
        if fault_config.fail_before_handle_confirmation {
            return Err(SuiError::GenericAuthorityError {
                error: "Mock error before handle_confirmation_transaction".to_owned(),
            });
        }
        // Check existing effects before verifying the cert to allow querying certs finalized
        // from previous epochs.
        let tx_digest = *certificate.digest();
        let response = match state.get_tx_info_already_executed(&tx_digest).await {
            Ok(Some(response)) => response,
            _ => {
                let certificate = {
                    let epoch_store = state.epoch_store();
                    certificate.verify(epoch_store.committee())?
                };
                state.try_execute_immediately(&certificate).await?
            }
        };
        if fault_config.fail_after_handle_confirmation {
            return Err(SuiError::GenericAuthorityError {
                error: "Mock error after handle_confirmation_transaction".to_owned(),
            });
        }
        Ok(response.into())
    }
}
