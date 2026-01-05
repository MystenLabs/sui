// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_client::{
    AuthorityAPI, NetworkAuthorityClient, make_authority_clients_with_timeout_config,
    make_network_authority_clients_with_network_config,
};
use crate::safe_client::{SafeClient, SafeClientMetrics, SafeClientMetricsBase};
#[cfg(test)]
use crate::test_authority_clients::MockAuthorityApi;
use sui_authority_aggregation::ReduceOutput;
use sui_authority_aggregation::quorum_map_then_reduce_with_timeout;
use sui_config::genesis::Genesis;
use sui_network::{
    DEFAULT_CONNECT_TIMEOUT_SEC, DEFAULT_REQUEST_TIMEOUT_SEC, default_mysten_network_config,
};
use sui_swarm_config::network_config::NetworkConfig;
use sui_types::crypto::AuthorityPublicKeyBytes;
use sui_types::error::UserInputError;
use sui_types::object::Object;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use sui_types::sui_system_state::{SuiSystemState, SuiSystemStateTrait};
use sui_types::{
    base_types::*,
    committee::Committee,
    error::{SuiError, SuiResult},
};
use tracing::debug;

use crate::epoch::committee_store::CommitteeStore;
use prometheus::Registry;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;
use sui_types::committee::{CommitteeWithNetworkMetadata, StakeUnit};
use sui_types::messages_grpc::{LayoutGenerationOption, ObjectInfoRequest};
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemState;

pub const DEFAULT_RETRIES: usize = 4;

#[derive(Clone)]
pub struct TimeoutConfig {
    pub pre_quorum_timeout: Duration,
    pub post_quorum_timeout: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            pre_quorum_timeout: Duration::from_secs(60),
            post_quorum_timeout: Duration::from_secs(7),
        }
    }
}

#[derive(Clone)]
pub struct AuthorityAggregator<A: Clone> {
    /// Our Sui committee.
    pub committee: Arc<Committee>,
    /// For more human readable metrics reporting.
    /// It's OK for this map to be empty or missing validators, it then defaults
    /// to use concise validator public keys.
    pub validator_display_names: Arc<HashMap<AuthorityName, String>>,
    /// Reference gas price for the current epoch.
    pub reference_gas_price: u64,
    /// How to talk to this committee.
    pub authority_clients: Arc<BTreeMap<AuthorityName, Arc<SafeClient<A>>>>,
    /// Metric base for the purpose of creating new safe clients during reconfiguration.
    pub safe_client_metrics_base: SafeClientMetricsBase,
    pub timeouts: TimeoutConfig,
    /// Store here for clone during re-config.
    pub committee_store: Arc<CommitteeStore>,
}

impl<A: Clone> AuthorityAggregator<A> {
    pub fn new(
        committee: Committee,
        validator_display_names: Arc<HashMap<AuthorityName, String>>,
        reference_gas_price: u64,
        committee_store: Arc<CommitteeStore>,
        authority_clients: BTreeMap<AuthorityName, A>,
        safe_client_metrics_base: SafeClientMetricsBase,
        timeouts: TimeoutConfig,
    ) -> Self {
        Self {
            committee: Arc::new(committee),
            validator_display_names,
            reference_gas_price,
            authority_clients: create_safe_clients(
                authority_clients,
                &committee_store,
                &safe_client_metrics_base,
            ),
            safe_client_metrics_base,
            timeouts,
            committee_store,
        }
    }

    pub fn get_client(&self, name: &AuthorityName) -> Option<&Arc<SafeClient<A>>> {
        self.authority_clients.get(name)
    }

    pub fn clone_client_test_only(&self, name: &AuthorityName) -> Arc<SafeClient<A>>
    where
        A: Clone,
    {
        self.authority_clients[name].clone()
    }

    pub fn clone_committee_store(&self) -> Arc<CommitteeStore> {
        self.committee_store.clone()
    }

    pub fn clone_inner_committee_test_only(&self) -> Committee {
        (*self.committee).clone()
    }

    pub fn clone_inner_clients_test_only(&self) -> BTreeMap<AuthorityName, SafeClient<A>> {
        (*self.authority_clients)
            .clone()
            .into_iter()
            .map(|(k, v)| (k, (*v).clone()))
            .collect()
    }

    pub fn get_display_name(&self, name: &AuthorityName) -> String {
        self.validator_display_names
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.concise().to_string())
    }
}

fn create_safe_clients<A: Clone>(
    authority_clients: BTreeMap<AuthorityName, A>,
    committee_store: &Arc<CommitteeStore>,
    safe_client_metrics_base: &SafeClientMetricsBase,
) -> Arc<BTreeMap<AuthorityName, Arc<SafeClient<A>>>> {
    Arc::new(
        authority_clients
            .into_iter()
            .map(|(name, api)| {
                (
                    name,
                    Arc::new(SafeClient::new(
                        api,
                        committee_store.clone(),
                        name,
                        SafeClientMetrics::new(safe_client_metrics_base, name),
                    )),
                )
            })
            .collect(),
    )
}

impl AuthorityAggregator<NetworkAuthorityClient> {
    /// Create a new network authority aggregator by reading the committee and network addresses
    /// information from the given epoch start system state.
    pub fn new_from_epoch_start_state(
        epoch_start_state: &EpochStartSystemState,
        committee_store: &Arc<CommitteeStore>,
        safe_client_metrics_base: SafeClientMetricsBase,
    ) -> Self {
        let committee = epoch_start_state.get_sui_committee_with_network_metadata();
        let validator_display_names = epoch_start_state.get_authority_names_to_hostnames();
        Self::new_from_committee(
            committee,
            Arc::new(validator_display_names),
            epoch_start_state.reference_gas_price(),
            committee_store,
            safe_client_metrics_base,
        )
    }

    /// Create a new AuthorityAggregator using information from the given epoch start system state.
    /// This is typically used during reconfiguration to create a new AuthorityAggregator with the
    /// new committee and network addresses.
    pub fn recreate_with_new_epoch_start_state(
        &self,
        epoch_start_state: &EpochStartSystemState,
    ) -> Self {
        Self::new_from_epoch_start_state(
            epoch_start_state,
            &self.committee_store,
            self.safe_client_metrics_base.clone(),
        )
    }

    pub fn new_from_committee(
        committee: CommitteeWithNetworkMetadata,
        validator_display_names: Arc<HashMap<AuthorityName, String>>,
        reference_gas_price: u64,
        committee_store: &Arc<CommitteeStore>,
        safe_client_metrics_base: SafeClientMetricsBase,
    ) -> Self {
        let net_config = default_mysten_network_config();
        let authority_clients =
            make_network_authority_clients_with_network_config(&committee, &net_config);
        Self::new(
            committee.committee().clone(),
            validator_display_names,
            reference_gas_price,
            committee_store.clone(),
            authority_clients,
            safe_client_metrics_base,
            Default::default(),
        )
    }
}

impl<A> AuthorityAggregator<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    /// Query the object with highest version number from the authorities.
    /// We stop after receiving responses from 2f+1 validators.
    /// This function is untrusted because we simply assume each response is valid and there are no
    /// byzantine validators.
    /// Because of this, this function should only be used for testing or benchmarking.
    pub async fn get_latest_object_version_for_testing(
        &self,
        object_id: ObjectID,
    ) -> SuiResult<Object> {
        #[derive(Debug, Default)]
        struct State {
            latest_object_version: Option<Object>,
            total_weight: StakeUnit,
        }
        let initial_state = State::default();
        let result = quorum_map_then_reduce_with_timeout(
                self.committee.clone(),
                self.authority_clients.clone(),
                initial_state,
                |_name, client| {
                    Box::pin(async move {
                        let request =
                            ObjectInfoRequest::latest_object_info_request(object_id, /* generate_layout */ LayoutGenerationOption::None);
                        let mut retry_count = 0;
                        loop {
                            match client.handle_object_info_request(request.clone()).await {
                                Ok(object_info) => return Ok(object_info),
                                Err(err) => {
                                    retry_count += 1;
                                    if retry_count > 3 {
                                        return Err(err);
                                    }
                                    tokio::time::sleep(Duration::from_secs(1)).await;
                                }
                            }
                        }
                    })
                },
                |mut state, name, weight, result| {
                    Box::pin(async move {
                        state.total_weight += weight;
                        match result {
                            Ok(object_info) => {
                                debug!("Received object info response from validator {:?} with version: {:?}", name.concise(), object_info.object.version());
                                if state.latest_object_version.as_ref().is_none_or(|latest| {
                                    object_info.object.version() > latest.version()
                                }) {
                                    state.latest_object_version = Some(object_info.object);
                                }
                            }
                            Err(err) => {
                                debug!("Received error from validator {:?}: {:?}", name.concise(), err);
                            }
                        };
                        if state.total_weight >= self.committee.quorum_threshold() {
                            if let Some(object) = state.latest_object_version {
                                return ReduceOutput::Success(object);
                            } else {
                                return ReduceOutput::Failed(state);
                            }
                        }
                        ReduceOutput::Continue(state)
                    })
                },
                // A long timeout before we hear back from a quorum
                self.timeouts.pre_quorum_timeout,
            )
            .await.map_err(|_state| SuiError::from(UserInputError::ObjectNotFound {
                object_id,
                version: None,
            }))?;
        Ok(result.0)
    }

    /// Get the latest system state object from the authorities.
    /// This function assumes all validators are honest.
    /// It should only be used for testing or benchmarking.
    pub async fn get_latest_system_state_object_for_testing(
        &self,
    ) -> anyhow::Result<SuiSystemState> {
        #[derive(Debug, Default)]
        struct State {
            latest_system_state: Option<SuiSystemState>,
            total_weight: StakeUnit,
        }
        let initial_state = State::default();
        let result = quorum_map_then_reduce_with_timeout(
            self.committee.clone(),
            self.authority_clients.clone(),
            initial_state,
            |_name, client| Box::pin(async move { client.handle_system_state_object().await }),
            |mut state, name, weight, result| {
                Box::pin(async move {
                    state.total_weight += weight;
                    match result {
                        Ok(system_state) => {
                            debug!(
                                "Received system state object from validator {:?} with epoch: {:?}",
                                name.concise(),
                                system_state.epoch()
                            );
                            if state
                                .latest_system_state
                                .as_ref()
                                .is_none_or(|latest| system_state.epoch() > latest.epoch())
                            {
                                state.latest_system_state = Some(system_state);
                            }
                        }
                        Err(err) => {
                            debug!(
                                "Received error from validator {:?}: {:?}",
                                name.concise(),
                                err
                            );
                        }
                    };
                    if state.total_weight >= self.committee.quorum_threshold() {
                        if let Some(system_state) = state.latest_system_state {
                            return ReduceOutput::Success(system_state);
                        } else {
                            return ReduceOutput::Failed(state);
                        }
                    }
                    ReduceOutput::Continue(state)
                })
            },
            // A long timeout before we hear back from a quorum
            self.timeouts.pre_quorum_timeout,
        )
        .await
        .map_err(|_| anyhow::anyhow!("Failed to get latest system state from the authorities"))?;
        Ok(result.0)
    }
}

#[derive(Default)]
pub struct AuthorityAggregatorBuilder<'a> {
    network_config: Option<&'a NetworkConfig>,
    genesis: Option<&'a Genesis>,
    committee: Option<Committee>,
    reference_gas_price: Option<u64>,
    committee_store: Option<Arc<CommitteeStore>>,
    registry: Option<&'a Registry>,
    timeouts_config: Option<TimeoutConfig>,
}

impl<'a> AuthorityAggregatorBuilder<'a> {
    pub fn from_network_config(config: &'a NetworkConfig) -> Self {
        Self {
            network_config: Some(config),
            ..Default::default()
        }
    }

    pub fn from_genesis(genesis: &'a Genesis) -> Self {
        Self {
            genesis: Some(genesis),
            ..Default::default()
        }
    }

    pub fn from_committee(committee: Committee) -> Self {
        Self {
            committee: Some(committee),
            ..Default::default()
        }
    }

    #[cfg(test)]
    pub fn from_committee_size(committee_size: usize) -> Self {
        let (committee, _keypairs) = Committee::new_simple_test_committee_of_size(committee_size);
        Self::from_committee(committee)
    }

    pub fn with_committee_store(mut self, committee_store: Arc<CommitteeStore>) -> Self {
        self.committee_store = Some(committee_store);
        self
    }

    pub fn with_registry(mut self, registry: &'a Registry) -> Self {
        self.registry = Some(registry);
        self
    }

    pub fn with_timeouts_config(mut self, timeouts_config: TimeoutConfig) -> Self {
        self.timeouts_config = Some(timeouts_config);
        self
    }

    fn get_network_committee(&self) -> CommitteeWithNetworkMetadata {
        self.get_genesis()
            .unwrap_or_else(|| panic!("need either NetworkConfig or Genesis."))
            .committee_with_network()
    }

    fn get_committee_authority_names_to_hostnames(&self) -> HashMap<AuthorityName, String> {
        if let Some(genesis) = self.get_genesis() {
            let state = genesis
                .sui_system_object()
                .into_genesis_version_for_tooling();
            state
                .validators
                .active_validators
                .iter()
                .map(|v| {
                    let metadata = v.verified_metadata();
                    let name = metadata.sui_pubkey_bytes();

                    (name, metadata.name.clone())
                })
                .collect()
        } else {
            HashMap::new()
        }
    }

    fn get_reference_gas_price(&self) -> u64 {
        self.reference_gas_price.unwrap_or_else(|| {
            self.get_genesis()
                .map(|g| g.reference_gas_price())
                .unwrap_or(1000)
        })
    }

    fn get_genesis(&self) -> Option<&Genesis> {
        if let Some(network_config) = self.network_config {
            Some(&network_config.genesis)
        } else if let Some(genesis) = self.genesis {
            Some(genesis)
        } else {
            None
        }
    }

    fn get_committee(&self) -> Committee {
        self.committee
            .clone()
            .unwrap_or_else(|| self.get_network_committee().committee().clone())
    }

    pub fn build_network_clients(
        self,
    ) -> (
        AuthorityAggregator<NetworkAuthorityClient>,
        BTreeMap<AuthorityPublicKeyBytes, NetworkAuthorityClient>,
    ) {
        let network_committee = self.get_network_committee();
        let auth_clients = make_authority_clients_with_timeout_config(
            &network_committee,
            DEFAULT_CONNECT_TIMEOUT_SEC,
            DEFAULT_REQUEST_TIMEOUT_SEC,
        );
        let auth_agg = self.build_custom_clients(auth_clients.clone());
        (auth_agg, auth_clients)
    }

    pub fn build_custom_clients<C: Clone>(
        self,
        authority_clients: BTreeMap<AuthorityName, C>,
    ) -> AuthorityAggregator<C> {
        let committee = self.get_committee();
        let validator_display_names = self.get_committee_authority_names_to_hostnames();
        let reference_gas_price = self.get_reference_gas_price();
        let registry = Registry::new();
        let registry = self.registry.unwrap_or(&registry);
        let safe_client_metrics_base = SafeClientMetricsBase::new(registry);

        let committee_store = self
            .committee_store
            .unwrap_or_else(|| Arc::new(CommitteeStore::new_for_testing(&committee)));

        let timeouts_config = self.timeouts_config.unwrap_or_default();

        AuthorityAggregator::new(
            committee,
            Arc::new(validator_display_names),
            reference_gas_price,
            committee_store,
            authority_clients,
            safe_client_metrics_base,
            timeouts_config,
        )
    }

    #[cfg(test)]
    pub fn build_mock_authority_aggregator(self) -> AuthorityAggregator<MockAuthorityApi> {
        let committee = self.get_committee();
        let clients = committee
            .names()
            .map(|name| {
                (
                    *name,
                    MockAuthorityApi::new(
                        Duration::from_millis(100),
                        Arc::new(std::sync::Mutex::new(30)),
                    ),
                )
            })
            .collect();
        self.build_custom_clients(clients)
    }
}
