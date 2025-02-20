// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::backpressure::BackpressureManager;
use super::epoch_start_configuration::EpochFlag;
use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::authority_store_pruner::ObjectsCompactionFilter;
use crate::authority::authority_store_tables::{
    AuthorityPerpetualTables, AuthorityPerpetualTablesOptions, AuthorityPrunerTables,
};
use crate::authority::epoch_start_configuration::EpochStartConfiguration;
use crate::authority::{AuthorityState, AuthorityStore};
use crate::checkpoints::CheckpointStore;
use crate::epoch::committee_store::CommitteeStore;
use crate::epoch::epoch_metrics::EpochMetrics;
use crate::epoch::randomness::RandomnessManager;
use crate::execution_cache::build_execution_cache;
use crate::jsonrpc_index::IndexStore;
use crate::mock_consensus::{ConsensusMode, MockConsensusClient};
use crate::module_cache_metrics::ResolverMetrics;
use crate::rpc_index::RpcIndexStore;
use crate::signature_verifier::SignatureVerifierMetrics;
use fastcrypto::traits::KeyPair;
use prometheus::Registry;
use std::path::PathBuf;
use std::sync::Arc;
use sui_archival::reader::ArchiveReaderBalancer;
use sui_config::certificate_deny_config::CertificateDenyConfig;
use sui_config::genesis::Genesis;
use sui_config::node::AuthorityOverloadConfig;
use sui_config::node::{
    AuthorityStorePruningConfig, DBCheckpointConfig, ExpensiveSafetyCheckConfig,
};
use sui_config::transaction_deny_config::TransactionDenyConfig;
use sui_config::ExecutionCacheConfig;
use sui_macros::nondeterministic;
use sui_network::randomness;
use sui_protocol_config::ProtocolConfig;
use sui_swarm_config::genesis_config::AccountConfig;
use sui_swarm_config::network_config::NetworkConfig;
use sui_types::base_types::{AuthorityName, ObjectID};
use sui_types::crypto::AuthorityKeyPair;
use sui_types::digests::ChainIdentifier;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::object::Object;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::supported_protocol_versions::SupportedProtocolVersions;
use sui_types::transaction::VerifiedTransaction;

#[derive(Default, Clone)]
pub struct TestAuthorityBuilder<'a> {
    store_base_path: Option<PathBuf>,
    store: Option<Arc<AuthorityStore>>,
    transaction_deny_config: Option<TransactionDenyConfig>,
    certificate_deny_config: Option<CertificateDenyConfig>,
    protocol_config: Option<ProtocolConfig>,
    reference_gas_price: Option<u64>,
    node_keypair: Option<&'a AuthorityKeyPair>,
    genesis: Option<&'a Genesis>,
    starting_objects: Option<&'a [Object]>,
    expensive_safety_checks: Option<ExpensiveSafetyCheckConfig>,
    disable_indexer: bool,
    accounts: Vec<AccountConfig>,
    /// By default, we don't insert the genesis checkpoint, which isn't needed by most tests.
    insert_genesis_checkpoint: bool,
    authority_overload_config: Option<AuthorityOverloadConfig>,
    cache_config: Option<ExecutionCacheConfig>,
}

impl<'a> TestAuthorityBuilder<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_store_base_path(mut self, path: PathBuf) -> Self {
        assert!(self.store_base_path.replace(path).is_none());
        self
    }

    pub fn with_starting_objects(mut self, objects: &'a [Object]) -> Self {
        assert!(self.starting_objects.replace(objects).is_none());
        self
    }

    pub fn with_store(mut self, store: Arc<AuthorityStore>) -> Self {
        assert!(self.store.replace(store).is_none());
        self
    }

    pub fn with_transaction_deny_config(mut self, config: TransactionDenyConfig) -> Self {
        assert!(self.transaction_deny_config.replace(config).is_none());
        self
    }

    pub fn with_certificate_deny_config(mut self, config: CertificateDenyConfig) -> Self {
        assert!(self.certificate_deny_config.replace(config).is_none());
        self
    }

    pub fn with_protocol_config(mut self, config: ProtocolConfig) -> Self {
        assert!(self.protocol_config.replace(config).is_none());
        self
    }

    pub fn with_reference_gas_price(mut self, reference_gas_price: u64) -> Self {
        // If genesis is already set then setting rgp is meaningless since it will be overwritten.
        assert!(self.genesis.is_none());
        assert!(self
            .reference_gas_price
            .replace(reference_gas_price)
            .is_none());
        self
    }

    pub fn with_genesis_and_keypair(
        mut self,
        genesis: &'a Genesis,
        keypair: &'a AuthorityKeyPair,
    ) -> Self {
        assert!(self.genesis.replace(genesis).is_none());
        assert!(self.node_keypair.replace(keypair).is_none());
        self
    }

    pub fn with_keypair(mut self, keypair: &'a AuthorityKeyPair) -> Self {
        assert!(self.node_keypair.replace(keypair).is_none());
        self
    }

    /// When providing a network config, we will use the \node_idx validator's
    /// key as the keypair for the new node.
    pub fn with_network_config(self, config: &'a NetworkConfig, node_idx: usize) -> Self {
        self.with_genesis_and_keypair(
            &config.genesis,
            config.validator_configs()[node_idx].protocol_key_pair(),
        )
    }

    pub fn disable_indexer(mut self) -> Self {
        self.disable_indexer = true;
        self
    }

    pub fn insert_genesis_checkpoint(mut self) -> Self {
        self.insert_genesis_checkpoint = true;
        self
    }

    pub fn with_expensive_safety_checks(mut self, config: ExpensiveSafetyCheckConfig) -> Self {
        assert!(self.expensive_safety_checks.replace(config).is_none());
        self
    }

    pub fn with_accounts(mut self, accounts: Vec<AccountConfig>) -> Self {
        self.accounts = accounts;
        self
    }

    pub fn with_authority_overload_config(mut self, config: AuthorityOverloadConfig) -> Self {
        assert!(self.authority_overload_config.replace(config).is_none());
        self
    }

    pub fn with_cache_config(mut self, config: ExecutionCacheConfig) -> Self {
        self.cache_config = Some(config);
        self
    }

    pub async fn build(self) -> Arc<AuthorityState> {
        let mut local_network_config_builder =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir()
                .with_accounts(self.accounts)
                .with_reference_gas_price(self.reference_gas_price.unwrap_or(500));
        if let Some(protocol_config) = &self.protocol_config {
            local_network_config_builder =
                local_network_config_builder.with_protocol_version(protocol_config.version);
        }
        let local_network_config = local_network_config_builder.build();
        let genesis = &self.genesis.unwrap_or(&local_network_config.genesis);
        let genesis_committee = genesis.committee().unwrap();
        let path = self.store_base_path.unwrap_or_else(|| {
            let dir = std::env::temp_dir();
            let store_base_path =
                dir.join(format!("DB_{:?}", nondeterministic!(ObjectID::random())));
            std::fs::create_dir(&store_base_path).unwrap();
            store_base_path
        });
        let mut config = local_network_config.validator_configs()[0].clone();
        let registry = Registry::new();
        let mut pruner_db = None;
        if config
            .authority_store_pruning_config
            .enable_compaction_filter
        {
            pruner_db = Some(Arc::new(AuthorityPrunerTables::open(&path.join("store"))));
        }
        let compaction_filter = pruner_db
            .clone()
            .map(|db| ObjectsCompactionFilter::new(db, &registry));

        let authority_store = match self.store {
            Some(store) => store,
            None => {
                let perpetual_tables_options = AuthorityPerpetualTablesOptions {
                    compaction_filter,
                    ..Default::default()
                };
                let perpetual_tables = Arc::new(AuthorityPerpetualTables::open(
                    &path.join("store"),
                    Some(perpetual_tables_options),
                ));
                // unwrap ok - for testing only.
                AuthorityStore::open_with_committee_for_testing(
                    perpetual_tables,
                    &genesis_committee,
                    genesis,
                )
                .await
                .unwrap()
            }
        };
        if let Some(cache_config) = self.cache_config {
            config.execution_cache = cache_config;
        }

        let keypair = if let Some(keypair) = self.node_keypair {
            keypair
        } else {
            config.protocol_key_pair()
        };

        let secret = Arc::pin(keypair.copy());
        let name: AuthorityName = secret.public().into();
        let cache_metrics = Arc::new(ResolverMetrics::new(&registry));
        let signature_verifier_metrics = SignatureVerifierMetrics::new(&registry);
        // `_guard` must be declared here so it is not dropped before
        // `AuthorityPerEpochStore::new` is called
        let _guard = self
            .protocol_config
            .map(|config| ProtocolConfig::apply_overrides_for_testing(move |_, _| config.clone()));
        let epoch_flags = EpochFlag::default_flags_for_new_epoch(&config);
        let epoch_start_configuration = EpochStartConfiguration::new(
            genesis.sui_system_object().into_epoch_start_state(),
            *genesis.checkpoint().digest(),
            &genesis.objects(),
            epoch_flags,
        )
        .unwrap();
        let expensive_safety_checks = self.expensive_safety_checks.unwrap_or_default();

        let checkpoint_store = CheckpointStore::new(&path.join("checkpoints"));
        let backpressure_manager =
            BackpressureManager::new_from_checkpoint_store(&checkpoint_store);

        let cache_traits = build_execution_cache(
            &Default::default(),
            &registry,
            &authority_store,
            backpressure_manager.clone(),
        );

        let epoch_store = AuthorityPerEpochStore::new(
            name,
            Arc::new(genesis_committee.clone()),
            &path.join("store"),
            None,
            EpochMetrics::new(&registry),
            epoch_start_configuration,
            cache_traits.backing_package_store.clone(),
            cache_traits.object_store.clone(),
            cache_metrics,
            signature_verifier_metrics,
            &expensive_safety_checks,
            ChainIdentifier::from(*genesis.checkpoint().digest()),
            checkpoint_store
                .get_highest_executed_checkpoint_seq_number()
                .unwrap()
                .unwrap_or(0),
        );
        let committee_store = Arc::new(CommitteeStore::new(
            path.join("epochs"),
            &genesis_committee,
            None,
        ));

        if self.insert_genesis_checkpoint {
            checkpoint_store.insert_genesis_checkpoint(
                genesis.checkpoint(),
                genesis.checkpoint_contents().clone(),
                &epoch_store,
            );
        }
        let index_store = if self.disable_indexer {
            None
        } else {
            Some(Arc::new(IndexStore::new(
                path.join("indexes"),
                &registry,
                epoch_store
                    .protocol_config()
                    .max_move_identifier_len_as_option(),
                false,
                &authority_store,
            )))
        };
        let rpc_index = if self.disable_indexer {
            None
        } else {
            Some(Arc::new(RpcIndexStore::new(
                &path,
                &authority_store,
                &checkpoint_store,
                &epoch_store,
                &cache_traits.backing_package_store,
            )))
        };

        let transaction_deny_config = self.transaction_deny_config.unwrap_or_default();
        let certificate_deny_config = self.certificate_deny_config.unwrap_or_default();
        let authority_overload_config = self.authority_overload_config.unwrap_or_default();
        let mut pruning_config = AuthorityStorePruningConfig::default();
        if !epoch_store
            .protocol_config()
            .simplified_unwrap_then_delete()
        {
            // We cannot prune tombstones if simplified_unwrap_then_delete is not enabled.
            pruning_config.set_killswitch_tombstone_pruning(true);
        }

        config.transaction_deny_config = transaction_deny_config;
        config.certificate_deny_config = certificate_deny_config;
        config.authority_overload_config = authority_overload_config;
        config.authority_store_pruning_config = pruning_config;

        let chain_identifier = ChainIdentifier::from(*genesis.checkpoint().digest());

        let state = AuthorityState::new(
            name,
            secret,
            SupportedProtocolVersions::SYSTEM_DEFAULT,
            authority_store,
            cache_traits,
            epoch_store.clone(),
            committee_store,
            index_store,
            rpc_index,
            checkpoint_store,
            &registry,
            genesis.objects(),
            &DBCheckpointConfig::default(),
            config.clone(),
            ArchiveReaderBalancer::default(),
            None,
            chain_identifier,
            pruner_db,
        )
        .await;

        // Set up randomness with no-op consensus (DKG will not complete).
        if epoch_store.randomness_state_enabled() {
            let consensus_client = Box::new(MockConsensusClient::new(
                Arc::downgrade(&state),
                ConsensusMode::Noop,
            ));
            let randomness_manager = RandomnessManager::try_new(
                Arc::downgrade(&epoch_store),
                consensus_client,
                randomness::Handle::new_stub(),
                config.protocol_key_pair(),
            )
            .await;
            if let Some(randomness_manager) = randomness_manager {
                // Randomness might fail if test configuration does not permit DKG init.
                // In that case, skip setting it up.
                epoch_store
                    .set_randomness_manager(randomness_manager)
                    .await
                    .unwrap();
            }
        }

        // For any type of local testing that does not actually spawn a node, the checkpoint executor
        // won't be started, which means we won't actually execute the genesis transaction. In that case,
        // the genesis objects (e.g. all the genesis test coins) won't be accessible. Executing it
        // explicitly makes sure all genesis objects are ready for use.
        state
            .try_execute_immediately(
                &VerifiedExecutableTransaction::new_from_checkpoint(
                    VerifiedTransaction::new_unchecked(genesis.transaction().clone()),
                    genesis.epoch(),
                    genesis.checkpoint().sequence_number,
                ),
                None,
                &state.epoch_store_for_testing(),
            )
            .await
            .unwrap();

        state.get_cache_commit().commit_transaction_outputs(
            epoch_store.epoch(),
            &[*genesis.transaction().digest()],
            epoch_store
                .protocol_config()
                .use_object_per_epoch_marker_table_v2_as_option()
                .unwrap_or(false),
        );

        // We want to insert these objects directly instead of relying on genesis because
        // genesis process would set the previous transaction field for these objects, which would
        // change their object digest. This makes it difficult to write tests that want to use
        // these objects directly.
        // TODO: we should probably have a better way to do this.
        if let Some(starting_objects) = self.starting_objects {
            state
                .insert_objects_unsafe_for_testing_only(starting_objects)
                .await
                .unwrap();
        };
        state
    }
}
