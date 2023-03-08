// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_core::authority_client::{make_authority_clients, NetworkAuthorityClient};
use sui_config::genesis::Genesis;
use sui_config::{NetworkConfig};
use sui_network::{
    DEFAULT_CONNECT_TIMEOUT_SEC, DEFAULT_REQUEST_TIMEOUT_SEC,
};
use sui_types::crypto::{AuthorityPublicKeyBytes};
use sui_types::committee::{Committee, ProtocolVersion};

use prometheus::{Registry};
use std::collections::{BTreeMap};
use std::sync::Arc;

use sui_core::authority_aggregator::AuthorityAggregator;
use sui_core::epoch::committee_store::CommitteeStore;
use sui_core::signature_verifier::{SignatureVerifier};

pub struct AuthorityAggregatorBuilder<'a> {
    network_config: Option<&'a NetworkConfig>,
    genesis: Option<&'a Genesis>,
    committee_store: Option<Arc<CommitteeStore>>,
    registry: Option<&'a Registry>,
    protocol_version: ProtocolVersion,
}

impl<'a> AuthorityAggregatorBuilder<'a> {
    pub fn from_network_config(config: &'a NetworkConfig) -> Self {
        Self {
            network_config: Some(config),
            genesis: None,
            committee_store: None,
            registry: None,
            protocol_version: ProtocolVersion::MIN,
        }
    }

    pub fn from_genesis(genesis: &'a Genesis) -> Self {
        Self {
            network_config: None,
            genesis: Some(genesis),
            committee_store: None,
            registry: None,
            protocol_version: ProtocolVersion::MIN,
        }
    }

    pub fn with_protocol_version(mut self, new_version: ProtocolVersion) -> Self {
        self.protocol_version = new_version;
        self
    }

    pub fn with_committee_store(mut self, committee_store: Arc<CommitteeStore>) -> Self {
        self.committee_store = Some(committee_store);
        self
    }

    pub fn with_registry(mut self, registry: &'a Registry) -> Self {
        self.registry = Some(registry);
        self
    }

    pub fn build<S: SignatureVerifier + Default>(
        self,
    ) -> anyhow::Result<(
        AuthorityAggregator<NetworkAuthorityClient, S>,
        BTreeMap<AuthorityPublicKeyBytes, NetworkAuthorityClient>,
    )> {
        let validator_info = if let Some(network_config) = self.network_config {
            network_config.validator_set()
        } else if let Some(genesis) = self.genesis {
            genesis.validator_set()
        } else {
            anyhow::bail!("need either NetworkConfig or Genesis.");
        };
        let committee = Committee::normalize_from_weights_for_testing(
            0,
            validator_info.iter().map(|validator| (validator.protocol_key(), 1)).collect())?;
        let mut registry = &prometheus::Registry::new();
        if self.registry.is_some() {
            registry = self.registry.unwrap();
        }

        let auth_clients = make_authority_clients(
            &validator_info,
            DEFAULT_CONNECT_TIMEOUT_SEC,
            DEFAULT_REQUEST_TIMEOUT_SEC,
        );
        let committee_store = if let Some(committee_store) = self.committee_store {
            committee_store
        } else {
            Arc::new(CommitteeStore::new_for_testing(&committee))
        };
        Ok((
            AuthorityAggregator::new(committee, committee_store, auth_clients.clone(), registry),
            auth_clients,
        ))
    }
}
