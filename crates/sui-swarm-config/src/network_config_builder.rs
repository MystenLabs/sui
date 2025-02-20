// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::time::Duration;
use std::{num::NonZeroUsize, path::Path, sync::Arc};

use rand::rngs::OsRng;
use sui_config::genesis::{TokenAllocation, TokenDistributionScheduleBuilder};
use sui_config::node::AuthorityOverloadConfig;
use sui_config::ExecutionCacheConfig;
use sui_macros::nondeterministic;
use sui_types::base_types::{AuthorityName, SuiAddress};
use sui_types::committee::{Committee, ProtocolVersion};
use sui_types::crypto::{get_key_pair_from_rng, AccountKeyPair, KeypairTraits, PublicKey};
use sui_types::object::Object;
use sui_types::supported_protocol_versions::SupportedProtocolVersions;
use sui_types::traffic_control::{PolicyConfig, RemoteFirewallConfig};

use crate::genesis_config::{AccountConfig, ValidatorGenesisConfigBuilder, DEFAULT_GAS_AMOUNT};
use crate::genesis_config::{GenesisConfig, ValidatorGenesisConfig};
use crate::network_config::NetworkConfig;
use crate::node_config_builder::ValidatorConfigBuilder;

pub enum CommitteeConfig {
    Size(NonZeroUsize),
    Validators(Vec<ValidatorGenesisConfig>),
    AccountKeys(Vec<AccountKeyPair>),
    /// Indicates that a committee should be deterministically generated, using the provided rng
    /// as a source of randomness as well as generating deterministic network port information.
    Deterministic((NonZeroUsize, Option<Vec<AccountKeyPair>>)),
}

pub type SupportedProtocolVersionsCallback = Arc<
    dyn Fn(
            usize,                 /* validator idx */
            Option<AuthorityName>, /* None for fullnode */
        ) -> SupportedProtocolVersions
        + Send
        + Sync
        + 'static,
>;

#[derive(Clone)]
pub enum ProtocolVersionsConfig {
    // use SYSTEM_DEFAULT
    Default,
    // Use one range for all validators.
    Global(SupportedProtocolVersions),
    // A closure that returns the versions for each validator.
    // TODO: This doesn't apply to fullnodes.
    PerValidator(SupportedProtocolVersionsCallback),
}

pub type StateAccumulatorV2EnabledCallback = Arc<dyn Fn(usize) -> bool + Send + Sync + 'static>;

#[derive(Clone)]
pub enum StateAccumulatorV2EnabledConfig {
    Global(bool),
    PerValidator(StateAccumulatorV2EnabledCallback),
}

pub struct ConfigBuilder<R = OsRng> {
    rng: Option<R>,
    config_directory: PathBuf,
    supported_protocol_versions_config: Option<ProtocolVersionsConfig>,
    committee: CommitteeConfig,
    genesis_config: Option<GenesisConfig>,
    reference_gas_price: Option<u64>,
    additional_objects: Vec<Object>,
    jwk_fetch_interval: Option<Duration>,
    num_unpruned_validators: Option<usize>,
    authority_overload_config: Option<AuthorityOverloadConfig>,
    execution_cache_config: Option<ExecutionCacheConfig>,
    data_ingestion_dir: Option<PathBuf>,
    policy_config: Option<PolicyConfig>,
    firewall_config: Option<RemoteFirewallConfig>,
    max_submit_position: Option<usize>,
    submit_delay_step_override_millis: Option<u64>,
    state_accumulator_v2_enabled_config: Option<StateAccumulatorV2EnabledConfig>,
}

impl ConfigBuilder {
    pub fn new<P: AsRef<Path>>(config_directory: P) -> Self {
        Self {
            rng: Some(OsRng),
            config_directory: config_directory.as_ref().into(),
            supported_protocol_versions_config: None,
            // FIXME: A network with only 1 validator does not have liveness.
            // We need to change this. There are some tests that depend on it though.
            committee: CommitteeConfig::Size(NonZeroUsize::new(1).unwrap()),
            genesis_config: None,
            reference_gas_price: None,
            additional_objects: vec![],
            jwk_fetch_interval: None,
            num_unpruned_validators: None,
            authority_overload_config: None,
            execution_cache_config: None,
            data_ingestion_dir: None,
            policy_config: None,
            firewall_config: None,
            max_submit_position: None,
            submit_delay_step_override_millis: None,
            state_accumulator_v2_enabled_config: None,
        }
    }

    pub fn new_with_temp_dir() -> Self {
        Self::new(nondeterministic!(tempfile::tempdir().unwrap()).into_path())
    }
}

impl<R> ConfigBuilder<R> {
    pub fn committee(mut self, committee: CommitteeConfig) -> Self {
        self.committee = committee;
        self
    }

    pub fn committee_size(mut self, committee_size: NonZeroUsize) -> Self {
        self.committee = CommitteeConfig::Size(committee_size);
        self
    }

    pub fn deterministic_committee_size(mut self, committee_size: NonZeroUsize) -> Self {
        self.committee = CommitteeConfig::Deterministic((committee_size, None));
        self
    }

    pub fn deterministic_committee_validators(mut self, keys: Vec<AccountKeyPair>) -> Self {
        self.committee = CommitteeConfig::Deterministic((
            NonZeroUsize::new(keys.len()).expect("Validator keys should be non empty"),
            Some(keys),
        ));
        self
    }

    pub fn with_validator_account_keys(mut self, keys: Vec<AccountKeyPair>) -> Self {
        self.committee = CommitteeConfig::AccountKeys(keys);
        self
    }

    pub fn with_validators(mut self, validators: Vec<ValidatorGenesisConfig>) -> Self {
        self.committee = CommitteeConfig::Validators(validators);
        self
    }

    pub fn with_genesis_config(mut self, genesis_config: GenesisConfig) -> Self {
        assert!(self.genesis_config.is_none(), "Genesis config already set");
        self.genesis_config = Some(genesis_config);
        self
    }

    pub fn with_num_unpruned_validators(mut self, n: usize) -> Self {
        self.num_unpruned_validators = Some(n);
        self
    }

    pub fn with_jwk_fetch_interval(mut self, i: Duration) -> Self {
        self.jwk_fetch_interval = Some(i);
        self
    }

    pub fn with_data_ingestion_dir(mut self, path: PathBuf) -> Self {
        self.data_ingestion_dir = Some(path);
        self
    }

    pub fn with_reference_gas_price(mut self, reference_gas_price: u64) -> Self {
        self.reference_gas_price = Some(reference_gas_price);
        self
    }

    pub fn with_accounts(mut self, accounts: Vec<AccountConfig>) -> Self {
        self.get_or_init_genesis_config().accounts = accounts;
        self
    }

    pub fn with_chain_start_timestamp_ms(mut self, chain_start_timestamp_ms: u64) -> Self {
        self.get_or_init_genesis_config()
            .parameters
            .chain_start_timestamp_ms = chain_start_timestamp_ms;
        self
    }

    pub fn with_objects<I: IntoIterator<Item = Object>>(mut self, objects: I) -> Self {
        self.additional_objects.extend(objects);
        self
    }

    pub fn with_epoch_duration(mut self, epoch_duration_ms: u64) -> Self {
        self.get_or_init_genesis_config()
            .parameters
            .epoch_duration_ms = epoch_duration_ms;
        self
    }

    pub fn with_protocol_version(mut self, protocol_version: ProtocolVersion) -> Self {
        self.get_or_init_genesis_config()
            .parameters
            .protocol_version = protocol_version;
        self
    }

    pub fn with_supported_protocol_versions(mut self, c: SupportedProtocolVersions) -> Self {
        self.supported_protocol_versions_config = Some(ProtocolVersionsConfig::Global(c));
        self
    }

    pub fn with_supported_protocol_version_callback(
        mut self,
        func: SupportedProtocolVersionsCallback,
    ) -> Self {
        self.supported_protocol_versions_config = Some(ProtocolVersionsConfig::PerValidator(func));
        self
    }

    pub fn with_supported_protocol_versions_config(mut self, c: ProtocolVersionsConfig) -> Self {
        self.supported_protocol_versions_config = Some(c);
        self
    }

    pub fn with_state_accumulator_v2_enabled(mut self, enabled: bool) -> Self {
        self.state_accumulator_v2_enabled_config =
            Some(StateAccumulatorV2EnabledConfig::Global(enabled));
        self
    }

    pub fn with_state_accumulator_v2_enabled_callback(
        mut self,
        func: StateAccumulatorV2EnabledCallback,
    ) -> Self {
        self.state_accumulator_v2_enabled_config =
            Some(StateAccumulatorV2EnabledConfig::PerValidator(func));
        self
    }

    pub fn with_state_accumulator_v2_enabled_config(
        mut self,
        c: StateAccumulatorV2EnabledConfig,
    ) -> Self {
        self.state_accumulator_v2_enabled_config = Some(c);
        self
    }

    pub fn with_authority_overload_config(mut self, c: AuthorityOverloadConfig) -> Self {
        self.authority_overload_config = Some(c);
        self
    }

    pub fn with_execution_cache_config(mut self, c: ExecutionCacheConfig) -> Self {
        self.execution_cache_config = Some(c);
        self
    }

    pub fn with_policy_config(mut self, config: Option<PolicyConfig>) -> Self {
        self.policy_config = config;
        self
    }

    pub fn with_firewall_config(mut self, config: Option<RemoteFirewallConfig>) -> Self {
        self.firewall_config = config;
        self
    }

    pub fn with_max_submit_position(mut self, max_submit_position: usize) -> Self {
        self.max_submit_position = Some(max_submit_position);
        self
    }

    pub fn with_submit_delay_step_override_millis(
        mut self,
        submit_delay_step_override_millis: u64,
    ) -> Self {
        self.submit_delay_step_override_millis = Some(submit_delay_step_override_millis);
        self
    }

    pub fn rng<N: rand::RngCore + rand::CryptoRng>(self, rng: N) -> ConfigBuilder<N> {
        ConfigBuilder {
            rng: Some(rng),
            config_directory: self.config_directory,
            supported_protocol_versions_config: self.supported_protocol_versions_config,
            committee: self.committee,
            genesis_config: self.genesis_config,
            reference_gas_price: self.reference_gas_price,
            additional_objects: self.additional_objects,
            num_unpruned_validators: self.num_unpruned_validators,
            jwk_fetch_interval: self.jwk_fetch_interval,
            authority_overload_config: self.authority_overload_config,
            execution_cache_config: self.execution_cache_config,
            data_ingestion_dir: self.data_ingestion_dir,
            policy_config: self.policy_config,
            firewall_config: self.firewall_config,
            max_submit_position: self.max_submit_position,
            submit_delay_step_override_millis: self.submit_delay_step_override_millis,
            state_accumulator_v2_enabled_config: self.state_accumulator_v2_enabled_config,
        }
    }

    fn get_or_init_genesis_config(&mut self) -> &mut GenesisConfig {
        if self.genesis_config.is_none() {
            self.genesis_config = Some(GenesisConfig::for_local_testing());
        }
        self.genesis_config.as_mut().unwrap()
    }
}

impl<R: rand::RngCore + rand::CryptoRng> ConfigBuilder<R> {
    //TODO right now we always randomize ports, we may want to have a default port configuration
    pub fn build(self) -> NetworkConfig {
        let committee = self.committee;

        let mut rng = self.rng.unwrap();
        let validators = match committee {
            CommitteeConfig::Size(size) => {
                // We always get fixed protocol keys from this function (which is isolated from
                // external test randomness because it uses a fixed seed). Necessary because some
                // tests call `make_tx_certs_and_signed_effects`, which locally forges a cert using
                // this same committee.
                let (_, keys) = Committee::new_simple_test_committee_of_size(size.into());

                keys.into_iter()
                    .map(|authority_key| {
                        let mut builder = ValidatorGenesisConfigBuilder::new()
                            .with_protocol_key_pair(authority_key);
                        if let Some(rgp) = self.reference_gas_price {
                            builder = builder.with_gas_price(rgp);
                        }
                        builder.build(&mut rng)
                    })
                    .collect::<Vec<_>>()
            }

            CommitteeConfig::Validators(v) => v,

            CommitteeConfig::AccountKeys(keys) => {
                // See above re fixed protocol keys
                let (_, protocol_keys) = Committee::new_simple_test_committee_of_size(keys.len());
                keys.into_iter()
                    .zip(protocol_keys)
                    .map(|(account_key, protocol_key)| {
                        let mut builder = ValidatorGenesisConfigBuilder::new()
                            .with_protocol_key_pair(protocol_key)
                            .with_account_key_pair(account_key);
                        if let Some(rgp) = self.reference_gas_price {
                            builder = builder.with_gas_price(rgp);
                        }
                        builder.build(&mut rng)
                    })
                    .collect::<Vec<_>>()
            }
            CommitteeConfig::Deterministic((size, keys)) => {
                // If no keys are provided, generate them.
                let keys = keys.unwrap_or(
                    (0..size.get())
                        .map(|_| get_key_pair_from_rng(&mut rng).1)
                        .collect(),
                );

                let mut configs = vec![];
                for (i, key) in keys.into_iter().enumerate() {
                    let port_offset = 8000 + i * 10;
                    let mut builder = ValidatorGenesisConfigBuilder::new()
                        .with_ip("127.0.0.1".to_owned())
                        .with_account_key_pair(key)
                        .with_deterministic_ports(port_offset as u16);
                    if let Some(rgp) = self.reference_gas_price {
                        builder = builder.with_gas_price(rgp);
                    }
                    configs.push(builder.build(&mut rng));
                }
                configs
            }
        };

        let genesis_config = self
            .genesis_config
            .unwrap_or_else(GenesisConfig::for_local_testing);

        let (account_keys, allocations) = genesis_config.generate_accounts(&mut rng).unwrap();

        let token_distribution_schedule = {
            let mut builder = TokenDistributionScheduleBuilder::new();
            for allocation in allocations {
                builder.add_allocation(allocation);
            }
            // Add allocations for each validator
            for validator in &validators {
                let account_key: PublicKey = validator.account_key_pair.public();
                let address = SuiAddress::from(&account_key);
                // Give each validator some gas so they can pay for their transactions.
                let gas_coin = TokenAllocation {
                    recipient_address: address,
                    amount_mist: DEFAULT_GAS_AMOUNT,
                    staked_with_validator: None,
                };
                let stake = TokenAllocation {
                    recipient_address: address,
                    amount_mist: validator.stake,
                    staked_with_validator: Some(address),
                };
                builder.add_allocation(gas_coin);
                builder.add_allocation(stake);
            }
            builder.build()
        };

        let genesis = {
            let mut builder = sui_genesis_builder::Builder::new()
                .with_parameters(genesis_config.parameters)
                .add_objects(self.additional_objects);

            for (i, validator) in validators.iter().enumerate() {
                let name = validator
                    .name
                    .clone()
                    .unwrap_or(format!("validator-{i}").to_string());
                let validator_info = validator.to_validator_info(name);
                builder =
                    builder.add_validator(validator_info.info, validator_info.proof_of_possession);
            }

            builder = builder.with_token_distribution_schedule(token_distribution_schedule);

            for validator in &validators {
                builder = builder.add_validator_signature(&validator.key_pair);
            }

            builder.build()
        };

        let validator_configs = validators
            .into_iter()
            .enumerate()
            .map(|(idx, validator)| {
                let mut builder = ValidatorConfigBuilder::new()
                    .with_config_directory(self.config_directory.clone())
                    .with_policy_config(self.policy_config.clone())
                    .with_firewall_config(self.firewall_config.clone());

                if let Some(max_submit_position) = self.max_submit_position {
                    builder = builder.with_max_submit_position(max_submit_position);
                }

                if let Some(submit_delay_step_override_millis) =
                    self.submit_delay_step_override_millis
                {
                    builder = builder
                        .with_submit_delay_step_override_millis(submit_delay_step_override_millis);
                }

                if let Some(jwk_fetch_interval) = self.jwk_fetch_interval {
                    builder = builder.with_jwk_fetch_interval(jwk_fetch_interval);
                }

                if let Some(authority_overload_config) = &self.authority_overload_config {
                    builder =
                        builder.with_authority_overload_config(authority_overload_config.clone());
                }

                if let Some(execution_cache_config) = &self.execution_cache_config {
                    builder = builder.with_execution_cache_config(execution_cache_config.clone());
                }

                if let Some(path) = &self.data_ingestion_dir {
                    builder = builder.with_data_ingestion_dir(path.clone());
                }

                if let Some(spvc) = &self.supported_protocol_versions_config {
                    let supported_versions = match spvc {
                        ProtocolVersionsConfig::Default => {
                            SupportedProtocolVersions::SYSTEM_DEFAULT
                        }
                        ProtocolVersionsConfig::Global(v) => *v,
                        ProtocolVersionsConfig::PerValidator(func) => {
                            func(idx, Some(validator.key_pair.public().into()))
                        }
                    };
                    builder = builder.with_supported_protocol_versions(supported_versions);
                }
                if let Some(acc_v2_config) = &self.state_accumulator_v2_enabled_config {
                    let state_accumulator_v2_enabled: bool = match acc_v2_config {
                        StateAccumulatorV2EnabledConfig::Global(enabled) => *enabled,
                        StateAccumulatorV2EnabledConfig::PerValidator(func) => func(idx),
                    };
                    builder =
                        builder.with_state_accumulator_v2_enabled(state_accumulator_v2_enabled);
                }
                if let Some(num_unpruned_validators) = self.num_unpruned_validators {
                    if idx < num_unpruned_validators {
                        builder = builder.with_unpruned_checkpoints();
                    }
                }
                builder.build(validator, genesis.clone())
            })
            .collect();
        NetworkConfig {
            validator_configs,
            genesis,
            account_keys,
        }
    }
}

#[cfg(test)]
mod tests {
    use sui_config::node::Genesis;

    #[test]
    fn serialize_genesis_config_in_place() {
        let dir = tempfile::TempDir::new().unwrap();
        let network_config = crate::network_config_builder::ConfigBuilder::new(&dir).build();
        let genesis = network_config.genesis;

        let g = Genesis::new(genesis);

        let mut s = serde_yaml::to_string(&g).unwrap();
        let loaded_genesis: Genesis = serde_yaml::from_str(&s).unwrap();
        loaded_genesis
            .genesis()
            .unwrap()
            .checkpoint_contents()
            .digest(); // cache digest before comparing.
        assert_eq!(g, loaded_genesis);

        // If both in-place and file location are provided, prefer the in-place variant
        s.push_str("\ngenesis-file-location: path/to/file");
        let loaded_genesis: Genesis = serde_yaml::from_str(&s).unwrap();
        loaded_genesis
            .genesis()
            .unwrap()
            .checkpoint_contents()
            .digest(); // cache digest before comparing.
        assert_eq!(g, loaded_genesis);
    }

    #[test]
    fn load_genesis_config_from_file() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let genesis_config = Genesis::new_from_file(file.path());

        let dir = tempfile::TempDir::new().unwrap();
        let network_config = crate::network_config_builder::ConfigBuilder::new(&dir).build();
        let genesis = network_config.genesis;
        genesis.save(file.path()).unwrap();

        let loaded_genesis = genesis_config.genesis().unwrap();
        loaded_genesis.checkpoint_contents().digest(); // cache digest before comparing.
        assert_eq!(&genesis, loaded_genesis);
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashSet;
    use std::sync::Arc;
    use sui_config::genesis::Genesis;
    use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
    use sui_types::epoch_data::EpochData;
    use sui_types::gas::SuiGasStatus;
    use sui_types::in_memory_storage::InMemoryStorage;
    use sui_types::metrics::LimitsMetrics;
    use sui_types::sui_system_state::SuiSystemStateTrait;
    use sui_types::transaction::CheckedInputObjects;

    #[test]
    fn roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let network_config = crate::network_config_builder::ConfigBuilder::new(&dir).build();
        let genesis = network_config.genesis;

        let s = serde_yaml::to_string(&genesis).unwrap();
        let from_s: Genesis = serde_yaml::from_str(&s).unwrap();
        // cache the digest so that the comparison succeeds.
        from_s.checkpoint_contents().digest();
        assert_eq!(genesis, from_s);
    }

    #[test]
    fn genesis_transaction() {
        let builder = crate::network_config_builder::ConfigBuilder::new_with_temp_dir();
        let network_config = builder.build();
        let genesis = network_config.genesis;
        let protocol_version = ProtocolVersion::new(genesis.sui_system_object().protocol_version());
        let protocol_config = ProtocolConfig::get_for_version(protocol_version, Chain::Unknown);

        let genesis_transaction = genesis.transaction().clone();

        let genesis_digest = *genesis_transaction.digest();

        let silent = true;
        let executor = sui_execution::executor(&protocol_config, silent, None)
            .expect("Creating an executor should not fail here");

        // Use a throwaway metrics registry for genesis transaction execution.
        let registry = prometheus::Registry::new();
        let metrics = Arc::new(LimitsMetrics::new(&registry));
        let expensive_checks = false;
        let certificate_deny_set = HashSet::new();
        let epoch = EpochData::new_test();
        let transaction_data = &genesis_transaction.data().intent_message().value;
        let (kind, signer, _) = transaction_data.execution_parts();
        let input_objects = CheckedInputObjects::new_for_genesis(vec![]);

        let (_inner_temp_store, _, effects, _timings, _execution_error) = executor
            .execute_transaction_to_effects(
                &InMemoryStorage::new(Vec::new()),
                &protocol_config,
                metrics,
                expensive_checks,
                &certificate_deny_set,
                &epoch.epoch_id(),
                epoch.epoch_start_timestamp(),
                input_objects,
                vec![],
                SuiGasStatus::new_unmetered(),
                kind,
                signer,
                genesis_digest,
                &mut None,
            );

        assert_eq!(&effects, genesis.effects());
    }
}
