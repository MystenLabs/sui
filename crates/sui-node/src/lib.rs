// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::Network;
use anemo::PeerId;
use anemo_tower::callback::CallbackLayer;
use anemo_tower::trace::DefaultMakeSpan;
use anemo_tower::trace::DefaultOnFailure;
use anemo_tower::trace::TraceLayer;
use anyhow::anyhow;
use anyhow::Result;
use arc_swap::ArcSwap;
use fastcrypto_zkp::bn254::zk_login::JwkId;
use fastcrypto_zkp::bn254::zk_login::OIDCProvider;
use futures::future::BoxFuture;
use futures::TryFutureExt;
use mysten_common::debug_fatal;
use mysten_network::server::SUI_TLS_SERVER_NAME;
use prometheus::Registry;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::future::Future;
use std::path::PathBuf;
use std::str::FromStr;
#[cfg(msim)]
use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};
use std::time::Duration;
use sui_core::authority::authority_store_tables::{
    AuthorityPerpetualTablesOptions, AuthorityPrunerTables,
};
use sui_core::authority::backpressure::BackpressureManager;
use sui_core::authority::epoch_start_configuration::EpochFlag;
use sui_core::authority::execution_time_estimator::ExecutionTimeObserver;
use sui_core::authority::RandomnessRoundReceiver;
use sui_core::consensus_adapter::ConsensusClient;
use sui_core::consensus_manager::UpdatableConsensusClient;
use sui_core::epoch::randomness::RandomnessManager;
use sui_core::execution_cache::build_execution_cache;
use sui_core::state_accumulator::StateAccumulatorMetrics;
use sui_core::storage::RestReadStore;
use sui_core::traffic_controller::metrics::TrafficControllerMetrics;
use sui_json_rpc::bridge_api::BridgeReadApi;
use sui_json_rpc_api::JsonRpcMetrics;
use sui_network::randomness;
use sui_rpc_api::subscription::SubscriptionService;
use sui_rpc_api::RpcMetrics;
use sui_types::base_types::ConciseableName;
use sui_types::crypto::RandomnessRound;
use sui_types::digests::ChainIdentifier;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_consensus::AuthorityCapabilitiesV2;
use sui_types::messages_consensus::ConsensusTransactionKind;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::transaction::VerifiedCertificate;
use tap::tap::TapFallible;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc, watch, Mutex};
use tokio::task::{JoinHandle, JoinSet};
use tower::ServiceBuilder;
use tracing::{debug, error, warn};
use tracing::{error_span, info, Instrument};

use fastcrypto_zkp::bn254::zk_login::JWK;
pub use handle::SuiNodeHandle;
use mysten_metrics::{spawn_monitored_task, RegistryService};
use mysten_network::server::ServerBuilder;
use mysten_service::server_timing::server_timing_middleware;
use sui_archival::reader::ArchiveReaderBalancer;
use sui_archival::writer::ArchiveWriter;
use sui_config::node::{DBCheckpointConfig, RunWithRange};
use sui_config::node_config_metrics::NodeConfigMetrics;
use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
use sui_config::{ConsensusConfig, NodeConfig};
use sui_core::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use sui_core::authority::authority_store_tables::AuthorityPerpetualTables;
use sui_core::authority::epoch_start_configuration::EpochStartConfigTrait;
use sui_core::authority::epoch_start_configuration::EpochStartConfiguration;
use sui_core::authority_aggregator::{AuthAggMetrics, AuthorityAggregator};
use sui_core::authority_server::{ValidatorService, ValidatorServiceMetrics};
use sui_core::checkpoints::checkpoint_executor::metrics::CheckpointExecutorMetrics;
use sui_core::checkpoints::checkpoint_executor::{CheckpointExecutor, StopReason};
use sui_core::checkpoints::{
    CheckpointMetrics, CheckpointService, CheckpointStore, SendCheckpointToStateSync,
    SubmitCheckpointToConsensus,
};
use sui_core::consensus_adapter::{
    CheckConnection, ConnectionMonitorStatus, ConsensusAdapter, ConsensusAdapterMetrics,
};
use sui_core::consensus_manager::{ConsensusManager, ConsensusManagerTrait};
use sui_core::consensus_throughput_calculator::{
    ConsensusThroughputCalculator, ConsensusThroughputProfiler, ThroughputProfileRanges,
};
use sui_core::consensus_validator::{SuiTxValidator, SuiTxValidatorMetrics};
use sui_core::db_checkpoint_handler::DBCheckpointHandler;
use sui_core::epoch::committee_store::CommitteeStore;
use sui_core::epoch::consensus_store_pruner::ConsensusStorePruner;
use sui_core::epoch::epoch_metrics::EpochMetrics;
use sui_core::epoch::reconfiguration::ReconfigurationInitiator;
use sui_core::jsonrpc_index::IndexStore;
use sui_core::module_cache_metrics::ResolverMetrics;
use sui_core::overload_monitor::overload_monitor;
use sui_core::rpc_index::RpcIndexStore;
use sui_core::signature_verifier::SignatureVerifierMetrics;
use sui_core::state_accumulator::StateAccumulator;
use sui_core::storage::RocksDbStore;
use sui_core::transaction_orchestrator::TransactiondOrchestrator;
use sui_core::{
    authority::{AuthorityState, AuthorityStore},
    authority_client::NetworkAuthorityClient,
};
use sui_json_rpc::coin_api::CoinReadApi;
use sui_json_rpc::governance_api::GovernanceReadApi;
use sui_json_rpc::indexer_api::IndexerApi;
use sui_json_rpc::move_utils::MoveUtils;
use sui_json_rpc::read_api::ReadApi;
use sui_json_rpc::transaction_builder_api::TransactionBuilderApi;
use sui_json_rpc::transaction_execution_api::TransactionExecutionApi;
use sui_json_rpc::JsonRpcServerBuilder;
use sui_macros::fail_point;
use sui_macros::{fail_point_async, replay_log};
use sui_network::api::ValidatorServer;
use sui_network::discovery;
use sui_network::discovery::TrustedPeerChangeEvent;
use sui_network::state_sync;
use sui_protocol_config::{Chain, ProtocolConfig};
use sui_snapshot::uploader::StateSnapshotUploader;
use sui_storage::{
    http_key_value_store::HttpKVStore,
    key_value_store::{FallbackTransactionKVStore, TransactionKeyValueStore},
    key_value_store_metrics::KeyValueStoreMetrics,
};
use sui_storage::{FileCompression, StorageFormat};
use sui_types::base_types::{AuthorityName, EpochId};
use sui_types::committee::Committee;
use sui_types::crypto::KeypairTraits;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages_consensus::{
    check_total_jwk_size, AuthorityCapabilitiesV1, ConsensusTransaction,
};
use sui_types::quorum_driver_types::QuorumDriverEffectsQueueResult;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemState;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::supported_protocol_versions::SupportedProtocolVersions;
use typed_store::rocks::default_db_options;
use typed_store::DBMetrics;

use crate::metrics::{GrpcMetrics, SuiNodeMetrics};

pub mod admin;
mod handle;
pub mod metrics;

pub struct ValidatorComponents {
    validator_server_handle: SpawnOnce,
    validator_overload_monitor_handle: Option<JoinHandle<()>>,
    consensus_manager: ConsensusManager,
    consensus_store_pruner: ConsensusStorePruner,
    consensus_adapter: Arc<ConsensusAdapter>,
    // Keeping the handle to the checkpoint service tasks to shut them down during reconfiguration.
    checkpoint_service_tasks: JoinSet<()>,
    checkpoint_metrics: Arc<CheckpointMetrics>,
    sui_tx_validator_metrics: Arc<SuiTxValidatorMetrics>,
}
pub struct P2pComponents {
    p2p_network: Network,
    known_peers: HashMap<PeerId, String>,
    discovery_handle: discovery::Handle,
    state_sync_handle: state_sync::Handle,
    randomness_handle: randomness::Handle,
}

#[cfg(msim)]
mod simulator {
    use std::sync::atomic::AtomicBool;

    use super::*;
    pub(super) struct SimState {
        pub sim_node: sui_simulator::runtime::NodeHandle,
        pub sim_safe_mode_expected: AtomicBool,
        _leak_detector: sui_simulator::NodeLeakDetector,
    }

    impl Default for SimState {
        fn default() -> Self {
            Self {
                sim_node: sui_simulator::runtime::NodeHandle::current(),
                sim_safe_mode_expected: AtomicBool::new(false),
                _leak_detector: sui_simulator::NodeLeakDetector::new(),
            }
        }
    }

    type JwkInjector = dyn Fn(AuthorityName, &OIDCProvider) -> SuiResult<Vec<(JwkId, JWK)>>
        + Send
        + Sync
        + 'static;

    fn default_fetch_jwks(
        _authority: AuthorityName,
        _provider: &OIDCProvider,
    ) -> SuiResult<Vec<(JwkId, JWK)>> {
        use fastcrypto_zkp::bn254::zk_login::parse_jwks;
        // Just load a default Twitch jwk for testing.
        parse_jwks(
            sui_types::zk_login_util::DEFAULT_JWK_BYTES,
            &OIDCProvider::Twitch,
        )
        .map_err(|_| SuiError::JWKRetrievalError)
    }

    thread_local! {
        static JWK_INJECTOR: std::cell::RefCell<Arc<JwkInjector>> = std::cell::RefCell::new(Arc::new(default_fetch_jwks));
    }

    pub(super) fn get_jwk_injector() -> Arc<JwkInjector> {
        JWK_INJECTOR.with(|injector| injector.borrow().clone())
    }

    pub fn set_jwk_injector(injector: Arc<JwkInjector>) {
        JWK_INJECTOR.with(|cell| *cell.borrow_mut() = injector);
    }
}

#[cfg(msim)]
pub use simulator::set_jwk_injector;
#[cfg(msim)]
use simulator::*;
use sui_core::authority::authority_store_pruner::ObjectsCompactionFilter;
use sui_core::{
    consensus_handler::ConsensusHandlerInitializer, safe_client::SafeClientMetricsBase,
    validator_tx_finalizer::ValidatorTxFinalizer,
};
use sui_types::execution_config_utils::to_binary_config;

pub struct SuiNode {
    config: NodeConfig,
    validator_components: Mutex<Option<ValidatorComponents>>,
    /// The http server responsible for serving JSON-RPC as well as the experimental rest service
    _http_server: Option<sui_http::ServerHandle>,
    state: Arc<AuthorityState>,
    transaction_orchestrator: Option<Arc<TransactiondOrchestrator<NetworkAuthorityClient>>>,
    registry_service: RegistryService,
    metrics: Arc<SuiNodeMetrics>,

    _discovery: discovery::Handle,
    _connection_monitor_handle: consensus_core::ConnectionMonitorHandle,
    state_sync_handle: state_sync::Handle,
    randomness_handle: randomness::Handle,
    checkpoint_store: Arc<CheckpointStore>,
    accumulator: Mutex<Option<Arc<StateAccumulator>>>,
    connection_monitor_status: Arc<ConnectionMonitorStatus>,

    /// Broadcast channel to send the starting system state for the next epoch.
    end_of_epoch_channel: broadcast::Sender<SuiSystemState>,

    /// Broadcast channel to notify state-sync for new validator peers.
    trusted_peer_change_tx: watch::Sender<TrustedPeerChangeEvent>,

    backpressure_manager: Arc<BackpressureManager>,

    _db_checkpoint_handle: Option<tokio::sync::broadcast::Sender<()>>,

    #[cfg(msim)]
    sim_state: SimState,

    _state_archive_handle: Option<broadcast::Sender<()>>,

    _state_snapshot_uploader_handle: Option<broadcast::Sender<()>>,
    // Channel to allow signaling upstream to shutdown sui-node
    shutdown_channel_tx: broadcast::Sender<Option<RunWithRange>>,

    /// AuthorityAggregator of the network, created at start and beginning of each epoch.
    /// Use ArcSwap so that we could mutate it without taking mut reference.
    // TODO: Eventually we can make this auth aggregator a shared reference so that this
    // update will automatically propagate to other uses.
    auth_agg: Arc<ArcSwap<AuthorityAggregator<NetworkAuthorityClient>>>,

    subscription_service_checkpoint_sender: Option<tokio::sync::mpsc::Sender<CheckpointData>>,
}

impl fmt::Debug for SuiNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("SuiNode")
            .field("name", &self.state.name.concise())
            .finish()
    }
}

static MAX_JWK_KEYS_PER_FETCH: usize = 100;

impl SuiNode {
    pub async fn start(
        config: NodeConfig,
        registry_service: RegistryService,
        custom_rpc_runtime: Option<Handle>,
    ) -> Result<Arc<SuiNode>> {
        Self::start_async(config, registry_service, custom_rpc_runtime, "unknown").await
    }

    fn start_jwk_updater(
        config: &NodeConfig,
        metrics: Arc<SuiNodeMetrics>,
        authority: AuthorityName,
        epoch_store: Arc<AuthorityPerEpochStore>,
        consensus_adapter: Arc<ConsensusAdapter>,
    ) {
        let epoch = epoch_store.epoch();

        let supported_providers = config
            .zklogin_oauth_providers
            .get(&epoch_store.get_chain_identifier().chain())
            .unwrap_or(&BTreeSet::new())
            .iter()
            .map(|s| OIDCProvider::from_str(s).expect("Invalid provider string"))
            .collect::<Vec<_>>();

        let fetch_interval = Duration::from_secs(config.jwk_fetch_interval_seconds);

        info!(
            ?fetch_interval,
            "Starting JWK updater tasks with supported providers: {:?}", supported_providers
        );

        fn validate_jwk(
            metrics: &Arc<SuiNodeMetrics>,
            provider: &OIDCProvider,
            id: &JwkId,
            jwk: &JWK,
        ) -> bool {
            let Ok(iss_provider) = OIDCProvider::from_iss(&id.iss) else {
                warn!(
                    "JWK iss {:?} (retrieved from {:?}) is not a valid provider",
                    id.iss, provider
                );
                metrics
                    .invalid_jwks
                    .with_label_values(&[&provider.to_string()])
                    .inc();
                return false;
            };

            if iss_provider != *provider {
                warn!(
                    "JWK iss {:?} (retrieved from {:?}) does not match provider {:?}",
                    id.iss, provider, iss_provider
                );
                metrics
                    .invalid_jwks
                    .with_label_values(&[&provider.to_string()])
                    .inc();
                return false;
            }

            if !check_total_jwk_size(id, jwk) {
                warn!("JWK {:?} (retrieved from {:?}) is too large", id, provider);
                metrics
                    .invalid_jwks
                    .with_label_values(&[&provider.to_string()])
                    .inc();
                return false;
            }

            true
        }

        // metrics is:
        //  pub struct SuiNodeMetrics {
        //      pub jwk_requests: IntCounterVec,
        //      pub jwk_request_errors: IntCounterVec,
        //      pub total_jwks: IntCounterVec,
        //      pub unique_jwks: IntCounterVec,
        //  }

        for p in supported_providers.into_iter() {
            let provider_str = p.to_string();
            let epoch_store = epoch_store.clone();
            let consensus_adapter = consensus_adapter.clone();
            let metrics = metrics.clone();
            spawn_monitored_task!(epoch_store.clone().within_alive_epoch(
                async move {
                    // note: restart-safe de-duplication happens after consensus, this is
                    // just best-effort to reduce unneeded submissions.
                    let mut seen = HashSet::new();
                    loop {
                        info!("fetching JWK for provider {:?}", p);
                        metrics.jwk_requests.with_label_values(&[&provider_str]).inc();
                        match Self::fetch_jwks(authority, &p).await {
                            Err(e) => {
                                metrics.jwk_request_errors.with_label_values(&[&provider_str]).inc();
                                warn!("Error when fetching JWK for provider {:?} {:?}", p, e);
                                // Retry in 30 seconds
                                tokio::time::sleep(Duration::from_secs(30)).await;
                                continue;
                            }
                            Ok(mut keys) => {
                                metrics.total_jwks
                                    .with_label_values(&[&provider_str])
                                    .inc_by(keys.len() as u64);

                                keys.retain(|(id, jwk)| {
                                    validate_jwk(&metrics, &p, id, jwk) &&
                                    !epoch_store.jwk_active_in_current_epoch(id, jwk) &&
                                    seen.insert((id.clone(), jwk.clone()))
                                });

                                metrics.unique_jwks
                                    .with_label_values(&[&provider_str])
                                    .inc_by(keys.len() as u64);

                                // prevent oauth providers from sending too many keys,
                                // inadvertently or otherwise
                                if keys.len() > MAX_JWK_KEYS_PER_FETCH {
                                    warn!("Provider {:?} sent too many JWKs, only the first {} will be used", p, MAX_JWK_KEYS_PER_FETCH);
                                    keys.truncate(MAX_JWK_KEYS_PER_FETCH);
                                }

                                for (id, jwk) in keys.into_iter() {
                                    info!("Submitting JWK to consensus: {:?}", id);

                                    let txn = ConsensusTransaction::new_jwk_fetched(authority, id, jwk);
                                    consensus_adapter.submit(txn, None, &epoch_store)
                                        .tap_err(|e| warn!("Error when submitting JWKs to consensus {:?}", e))
                                        .ok();
                                }
                            }
                        }
                        tokio::time::sleep(fetch_interval).await;
                    }
                }
                .instrument(error_span!("jwk_updater_task", epoch)),
            ));
        }
    }

    pub async fn start_async(
        config: NodeConfig,
        registry_service: RegistryService,
        custom_rpc_runtime: Option<Handle>,
        software_version: &'static str,
    ) -> Result<Arc<SuiNode>> {
        NodeConfigMetrics::new(&registry_service.default_registry()).record_metrics(&config);
        let mut config = config.clone();
        if config.supported_protocol_versions.is_none() {
            info!(
                "populating config.supported_protocol_versions with default {:?}",
                SupportedProtocolVersions::SYSTEM_DEFAULT
            );
            config.supported_protocol_versions = Some(SupportedProtocolVersions::SYSTEM_DEFAULT);
        }

        let run_with_range = config.run_with_range;
        let is_validator = config.consensus_config().is_some();
        let is_full_node = !is_validator;
        let prometheus_registry = registry_service.default_registry();

        info!(node =? config.protocol_public_key(),
            "Initializing sui-node listening on {}", config.network_address
        );

        // Initialize metrics to track db usage before creating any stores
        DBMetrics::init(&prometheus_registry);

        // Initialize Mysten metrics.
        mysten_metrics::init_metrics(&prometheus_registry);
        // Unsupported (because of the use of static variable) and unnecessary in simtests.
        #[cfg(not(msim))]
        mysten_metrics::thread_stall_monitor::start_thread_stall_monitor();

        let genesis = config.genesis()?.clone();

        let secret = Arc::pin(config.protocol_key_pair().copy());
        let genesis_committee = genesis.committee()?;
        let committee_store = Arc::new(CommitteeStore::new(
            config.db_path().join("epochs"),
            &genesis_committee,
            None,
        ));

        let mut pruner_db = None;
        if config
            .authority_store_pruning_config
            .enable_compaction_filter
        {
            pruner_db = Some(Arc::new(AuthorityPrunerTables::open(
                &config.db_path().join("store"),
            )));
        }
        let compaction_filter = pruner_db
            .clone()
            .map(|db| ObjectsCompactionFilter::new(db, &prometheus_registry));

        // By default, only enable write stall on validators for perpetual db.
        let enable_write_stall = config.enable_db_write_stall.unwrap_or(is_validator);
        let perpetual_tables_options = AuthorityPerpetualTablesOptions {
            enable_write_stall,
            compaction_filter,
        };
        let perpetual_tables = Arc::new(AuthorityPerpetualTables::open(
            &config.db_path().join("store"),
            Some(perpetual_tables_options),
        ));
        let is_genesis = perpetual_tables
            .database_is_empty()
            .expect("Database read should not fail at init.");

        let checkpoint_store = CheckpointStore::new(&config.db_path().join("checkpoints"));
        let backpressure_manager =
            BackpressureManager::new_from_checkpoint_store(&checkpoint_store);

        let store =
            AuthorityStore::open(perpetual_tables, &genesis, &config, &prometheus_registry).await?;

        let cur_epoch = store.get_recovery_epoch_at_restart()?;
        let committee = committee_store
            .get_committee(&cur_epoch)?
            .expect("Committee of the current epoch must exist");
        let epoch_start_configuration = store
            .get_epoch_start_configuration()?
            .expect("EpochStartConfiguration of the current epoch must exist");
        let cache_metrics = Arc::new(ResolverMetrics::new(&prometheus_registry));
        let signature_verifier_metrics = SignatureVerifierMetrics::new(&prometheus_registry);

        let cache_traits = build_execution_cache(
            &config.execution_cache,
            &prometheus_registry,
            &store,
            backpressure_manager.clone(),
        );

        let auth_agg = {
            let safe_client_metrics_base = SafeClientMetricsBase::new(&prometheus_registry);
            let auth_agg_metrics = Arc::new(AuthAggMetrics::new(&prometheus_registry));
            Arc::new(ArcSwap::new(Arc::new(
                AuthorityAggregator::new_from_epoch_start_state(
                    epoch_start_configuration.epoch_start_state(),
                    &committee_store,
                    safe_client_metrics_base,
                    auth_agg_metrics,
                ),
            )))
        };

        let epoch_options = default_db_options().optimize_db_for_write_throughput(4);
        let epoch_store = AuthorityPerEpochStore::new(
            config.protocol_public_key(),
            committee.clone(),
            &config.db_path().join("store"),
            Some(epoch_options.options),
            EpochMetrics::new(&registry_service.default_registry()),
            epoch_start_configuration,
            cache_traits.backing_package_store.clone(),
            cache_traits.object_store.clone(),
            cache_metrics,
            signature_verifier_metrics,
            &config.expensive_safety_check_config,
            ChainIdentifier::from(*genesis.checkpoint().digest()),
            checkpoint_store
                .get_highest_executed_checkpoint_seq_number()
                .expect("checkpoint store read cannot fail")
                .unwrap_or(0),
        );

        info!("created epoch store");

        replay_log!(
            "Beginning replay run. Epoch: {:?}, Protocol config: {:?}",
            epoch_store.epoch(),
            epoch_store.protocol_config()
        );

        // the database is empty at genesis time
        if is_genesis {
            info!("checking SUI conservation at genesis");
            // When we are opening the db table, the only time when it's safe to
            // check SUI conservation is at genesis. Otherwise we may be in the middle of
            // an epoch and the SUI conservation check will fail. This also initialize
            // the expected_network_sui_amount table.
            cache_traits
                .reconfig_api
                .expensive_check_sui_conservation(&epoch_store)
                .expect("SUI conservation check cannot fail at genesis");
        }

        let effective_buffer_stake = epoch_store.get_effective_buffer_stake_bps();
        let default_buffer_stake = epoch_store
            .protocol_config()
            .buffer_stake_for_protocol_upgrade_bps();
        if effective_buffer_stake != default_buffer_stake {
            warn!(
                ?effective_buffer_stake,
                ?default_buffer_stake,
                "buffer_stake_for_protocol_upgrade_bps is currently overridden"
            );
        }

        checkpoint_store.insert_genesis_checkpoint(
            genesis.checkpoint(),
            genesis.checkpoint_contents().clone(),
            &epoch_store,
        );

        info!("creating state sync store");
        let state_sync_store = RocksDbStore::new(
            cache_traits.clone(),
            committee_store.clone(),
            checkpoint_store.clone(),
        );

        let index_store = if is_full_node && config.enable_index_processing {
            info!("creating index store");
            Some(Arc::new(IndexStore::new(
                config.db_path().join("indexes"),
                &prometheus_registry,
                epoch_store
                    .protocol_config()
                    .max_move_identifier_len_as_option(),
                config.remove_deprecated_tables,
                &store,
            )))
        } else {
            None
        };

        let rpc_index = if is_full_node && config.rpc().is_some_and(|rpc| rpc.enable_indexing()) {
            Some(Arc::new(RpcIndexStore::new(
                &config.db_path(),
                &store,
                &checkpoint_store,
                &epoch_store,
                &cache_traits.backing_package_store,
            )))
        } else {
            None
        };

        let chain_identifier = ChainIdentifier::from(*genesis.checkpoint().digest());

        info!("creating archive reader");
        // Create network
        // TODO only configure validators as seed/preferred peers for validators and not for
        // fullnodes once we've had a chance to re-work fullnode configuration generation.
        let archive_readers =
            ArchiveReaderBalancer::new(config.archive_reader_config(), &prometheus_registry)?;
        let (trusted_peer_change_tx, trusted_peer_change_rx) = watch::channel(Default::default());
        let (randomness_tx, randomness_rx) = mpsc::channel(
            config
                .p2p_config
                .randomness
                .clone()
                .unwrap_or_default()
                .mailbox_capacity(),
        );
        let P2pComponents {
            p2p_network,
            known_peers,
            discovery_handle,
            state_sync_handle,
            randomness_handle,
        } = Self::create_p2p_network(
            &config,
            state_sync_store.clone(),
            chain_identifier,
            trusted_peer_change_rx,
            archive_readers.clone(),
            randomness_tx,
            &prometheus_registry,
        )?;

        // We must explicitly send this instead of relying on the initial value to trigger
        // watch value change, so that state-sync is able to process it.
        send_trusted_peer_change(
            &config,
            &trusted_peer_change_tx,
            epoch_store.epoch_start_state(),
        )
        .expect("Initial trusted peers must be set");

        info!("start state archival");
        // Start archiving local state to remote store
        let state_archive_handle =
            Self::start_state_archival(&config, &prometheus_registry, state_sync_store.clone())
                .await?;

        info!("start snapshot upload");
        // Start uploading state snapshot to remote store
        let state_snapshot_handle = Self::start_state_snapshot(
            &config,
            &prometheus_registry,
            checkpoint_store.clone(),
            chain_identifier,
        )?;

        // Start uploading db checkpoints to remote store
        info!("start db checkpoint");
        let (db_checkpoint_config, db_checkpoint_handle) = Self::start_db_checkpoint(
            &config,
            &prometheus_registry,
            state_snapshot_handle.is_some(),
        )?;

        if !epoch_store
            .protocol_config()
            .simplified_unwrap_then_delete()
        {
            // We cannot prune tombstones if simplified_unwrap_then_delete is not enabled.
            config
                .authority_store_pruning_config
                .set_killswitch_tombstone_pruning(true);
        }

        let authority_name = config.protocol_public_key();
        let validator_tx_finalizer =
            config
                .enable_validator_tx_finalizer
                .then_some(Arc::new(ValidatorTxFinalizer::new(
                    auth_agg.clone(),
                    authority_name,
                    &prometheus_registry,
                )));

        info!("create authority state");
        let state = AuthorityState::new(
            authority_name,
            secret,
            config.supported_protocol_versions.unwrap(),
            store.clone(),
            cache_traits.clone(),
            epoch_store.clone(),
            committee_store.clone(),
            index_store.clone(),
            rpc_index,
            checkpoint_store.clone(),
            &prometheus_registry,
            genesis.objects(),
            &db_checkpoint_config,
            config.clone(),
            archive_readers,
            validator_tx_finalizer,
            chain_identifier,
            pruner_db,
        )
        .await;
        // ensure genesis txn was executed
        if epoch_store.epoch() == 0 {
            let txn = &genesis.transaction();
            let span = error_span!("genesis_txn", tx_digest = ?txn.digest());
            let transaction =
                sui_types::executable_transaction::VerifiedExecutableTransaction::new_unchecked(
                    sui_types::executable_transaction::ExecutableTransaction::new_from_data_and_sig(
                        genesis.transaction().data().clone(),
                        sui_types::executable_transaction::CertificateProof::Checkpoint(0, 0),
                    ),
                );
            state
                .try_execute_immediately(&transaction, None, &epoch_store)
                .instrument(span)
                .await
                .unwrap();
        }

        // Start the loop that receives new randomness and generates transactions for it.
        RandomnessRoundReceiver::spawn(state.clone(), randomness_rx);

        if config
            .expensive_safety_check_config
            .enable_secondary_index_checks()
        {
            if let Some(indexes) = state.indexes.clone() {
                sui_core::verify_indexes::verify_indexes(
                    state.get_accumulator_store().as_ref(),
                    indexes,
                )
                .expect("secondary indexes are inconsistent");
            }
        }

        let (end_of_epoch_channel, end_of_epoch_receiver) =
            broadcast::channel(config.end_of_epoch_broadcast_channel_capacity);

        let transaction_orchestrator = if is_full_node && run_with_range.is_none() {
            Some(Arc::new(
                TransactiondOrchestrator::new_with_auth_aggregator(
                    auth_agg.load_full(),
                    state.clone(),
                    end_of_epoch_receiver,
                    &config.db_path(),
                    &prometheus_registry,
                ),
            ))
        } else {
            None
        };

        let (http_server, subscription_service_checkpoint_sender) = build_http_server(
            state.clone(),
            state_sync_store,
            &transaction_orchestrator.clone(),
            &config,
            &prometheus_registry,
            custom_rpc_runtime,
            software_version,
        )
        .await?;

        let accumulator = Arc::new(StateAccumulator::new(
            cache_traits.accumulator_store.clone(),
            StateAccumulatorMetrics::new(&prometheus_registry),
        ));

        let authority_names_to_peer_ids = epoch_store
            .epoch_start_state()
            .get_authority_names_to_peer_ids();

        let network_connection_metrics = consensus_core::QuinnConnectionMetrics::new(
            "sui",
            &registry_service.default_registry(),
        );

        let authority_names_to_peer_ids = ArcSwap::from_pointee(authority_names_to_peer_ids);

        let connection_monitor_handle = consensus_core::AnemoConnectionMonitor::spawn(
            p2p_network.downgrade(),
            Arc::new(network_connection_metrics),
            known_peers,
        );

        let connection_monitor_status = ConnectionMonitorStatus {
            connection_statuses: connection_monitor_handle.connection_statuses(),
            authority_names_to_peer_ids,
        };

        let connection_monitor_status = Arc::new(connection_monitor_status);
        let sui_node_metrics = Arc::new(SuiNodeMetrics::new(&registry_service.default_registry()));

        let validator_components = if state.is_validator(&epoch_store) {
            let (components, _) = futures::join!(
                Self::construct_validator_components(
                    config.clone(),
                    state.clone(),
                    committee,
                    epoch_store.clone(),
                    checkpoint_store.clone(),
                    state_sync_handle.clone(),
                    randomness_handle.clone(),
                    Arc::downgrade(&accumulator),
                    backpressure_manager.clone(),
                    connection_monitor_status.clone(),
                    &registry_service,
                    sui_node_metrics.clone(),
                ),
                Self::reexecute_pending_consensus_certs(&epoch_store, &state,)
            );
            let mut components = components?;

            components.consensus_adapter.submit_recovered(&epoch_store);

            // Start the gRPC server
            components.validator_server_handle = components.validator_server_handle.start();

            Some(components)
        } else {
            None
        };

        // setup shutdown channel
        let (shutdown_channel, _) = broadcast::channel::<Option<RunWithRange>>(1);

        let node = Self {
            config,
            validator_components: Mutex::new(validator_components),
            _http_server: http_server,
            state,
            transaction_orchestrator,
            registry_service,
            metrics: sui_node_metrics,

            _discovery: discovery_handle,
            _connection_monitor_handle: connection_monitor_handle,
            state_sync_handle,
            randomness_handle,
            checkpoint_store,
            accumulator: Mutex::new(Some(accumulator)),
            end_of_epoch_channel,
            connection_monitor_status,
            trusted_peer_change_tx,
            backpressure_manager,

            _db_checkpoint_handle: db_checkpoint_handle,

            #[cfg(msim)]
            sim_state: Default::default(),

            _state_archive_handle: state_archive_handle,
            _state_snapshot_uploader_handle: state_snapshot_handle,
            shutdown_channel_tx: shutdown_channel,

            auth_agg,
            subscription_service_checkpoint_sender,
        };

        info!("SuiNode started!");
        let node = Arc::new(node);
        let node_copy = node.clone();
        spawn_monitored_task!(async move {
            let result = Self::monitor_reconfiguration(node_copy).await;
            if let Err(error) = result {
                warn!("Reconfiguration finished with error {:?}", error);
            }
        });

        Ok(node)
    }

    pub fn subscribe_to_epoch_change(&self) -> broadcast::Receiver<SuiSystemState> {
        self.end_of_epoch_channel.subscribe()
    }

    pub fn subscribe_to_shutdown_channel(&self) -> broadcast::Receiver<Option<RunWithRange>> {
        self.shutdown_channel_tx.subscribe()
    }

    pub fn current_epoch_for_testing(&self) -> EpochId {
        self.state.current_epoch_for_testing()
    }

    pub fn db_checkpoint_path(&self) -> PathBuf {
        self.config.db_checkpoint_path()
    }

    // Init reconfig process by starting to reject user certs
    pub async fn close_epoch(&self, epoch_store: &Arc<AuthorityPerEpochStore>) -> SuiResult {
        info!("close_epoch (current epoch = {})", epoch_store.epoch());
        self.validator_components
            .lock()
            .await
            .as_ref()
            .ok_or_else(|| SuiError::from("Node is not a validator"))?
            .consensus_adapter
            .close_epoch(epoch_store);
        Ok(())
    }

    pub fn clear_override_protocol_upgrade_buffer_stake(&self, epoch: EpochId) -> SuiResult {
        self.state
            .clear_override_protocol_upgrade_buffer_stake(epoch)
    }

    pub fn set_override_protocol_upgrade_buffer_stake(
        &self,
        epoch: EpochId,
        buffer_stake_bps: u64,
    ) -> SuiResult {
        self.state
            .set_override_protocol_upgrade_buffer_stake(epoch, buffer_stake_bps)
    }

    // Testing-only API to start epoch close process.
    // For production code, please use the non-testing version.
    pub async fn close_epoch_for_testing(&self) -> SuiResult {
        let epoch_store = self.state.epoch_store_for_testing();
        self.close_epoch(&epoch_store).await
    }

    async fn start_state_archival(
        config: &NodeConfig,
        prometheus_registry: &Registry,
        state_sync_store: RocksDbStore,
    ) -> Result<Option<tokio::sync::broadcast::Sender<()>>> {
        if let Some(remote_store_config) = &config.state_archive_write_config.object_store_config {
            let local_store_config = ObjectStoreConfig {
                object_store: Some(ObjectStoreType::File),
                directory: Some(config.archive_path()),
                ..Default::default()
            };
            let archive_writer = ArchiveWriter::new(
                local_store_config,
                remote_store_config.clone(),
                FileCompression::Zstd,
                StorageFormat::Blob,
                Duration::from_secs(600),
                256 * 1024 * 1024,
                prometheus_registry,
            )
            .await?;
            Ok(Some(archive_writer.start(state_sync_store).await?))
        } else {
            Ok(None)
        }
    }

    fn start_state_snapshot(
        config: &NodeConfig,
        prometheus_registry: &Registry,
        checkpoint_store: Arc<CheckpointStore>,
        chain_identifier: ChainIdentifier,
    ) -> Result<Option<tokio::sync::broadcast::Sender<()>>> {
        if let Some(remote_store_config) = &config.state_snapshot_write_config.object_store_config {
            let snapshot_uploader = StateSnapshotUploader::new(
                &config.db_checkpoint_path(),
                &config.snapshot_path(),
                remote_store_config.clone(),
                60,
                prometheus_registry,
                checkpoint_store,
                chain_identifier,
            )?;
            Ok(Some(snapshot_uploader.start()))
        } else {
            Ok(None)
        }
    }

    fn start_db_checkpoint(
        config: &NodeConfig,
        prometheus_registry: &Registry,
        state_snapshot_enabled: bool,
    ) -> Result<(
        DBCheckpointConfig,
        Option<tokio::sync::broadcast::Sender<()>>,
    )> {
        let checkpoint_path = Some(
            config
                .db_checkpoint_config
                .checkpoint_path
                .clone()
                .unwrap_or_else(|| config.db_checkpoint_path()),
        );
        let db_checkpoint_config = if config.db_checkpoint_config.checkpoint_path.is_none() {
            DBCheckpointConfig {
                checkpoint_path,
                perform_db_checkpoints_at_epoch_end: if state_snapshot_enabled {
                    true
                } else {
                    config
                        .db_checkpoint_config
                        .perform_db_checkpoints_at_epoch_end
                },
                ..config.db_checkpoint_config.clone()
            }
        } else {
            config.db_checkpoint_config.clone()
        };

        match (
            db_checkpoint_config.object_store_config.as_ref(),
            state_snapshot_enabled,
        ) {
            // If db checkpoint config object store not specified but
            // state snapshot object store is specified, create handler
            // anyway for marking db checkpoints as completed so that they
            // can be uploaded as state snapshots.
            (None, false) => Ok((db_checkpoint_config, None)),
            (_, _) => {
                let handler = DBCheckpointHandler::new(
                    &db_checkpoint_config.checkpoint_path.clone().unwrap(),
                    db_checkpoint_config.object_store_config.as_ref(),
                    60,
                    db_checkpoint_config
                        .prune_and_compact_before_upload
                        .unwrap_or(true),
                    config.authority_store_pruning_config.clone(),
                    prometheus_registry,
                    state_snapshot_enabled,
                )?;
                Ok((
                    db_checkpoint_config,
                    Some(DBCheckpointHandler::start(handler)),
                ))
            }
        }
    }

    fn create_p2p_network(
        config: &NodeConfig,
        state_sync_store: RocksDbStore,
        chain_identifier: ChainIdentifier,
        trusted_peer_change_rx: watch::Receiver<TrustedPeerChangeEvent>,
        archive_readers: ArchiveReaderBalancer,
        randomness_tx: mpsc::Sender<(EpochId, RandomnessRound, Vec<u8>)>,
        prometheus_registry: &Registry,
    ) -> Result<P2pComponents> {
        let (state_sync, state_sync_server) = state_sync::Builder::new()
            .config(config.p2p_config.state_sync.clone().unwrap_or_default())
            .store(state_sync_store)
            .archive_readers(archive_readers)
            .with_metrics(prometheus_registry)
            .build();

        let (discovery, discovery_server) = discovery::Builder::new(trusted_peer_change_rx)
            .config(config.p2p_config.clone())
            .build();

        let discovery_config = config.p2p_config.discovery.clone().unwrap_or_default();
        let known_peers: HashMap<PeerId, String> = discovery_config
            .allowlisted_peers
            .clone()
            .into_iter()
            .map(|ap| (ap.peer_id, "allowlisted_peer".to_string()))
            .chain(config.p2p_config.seed_peers.iter().filter_map(|peer| {
                peer.peer_id
                    .map(|peer_id| (peer_id, "seed_peer".to_string()))
            }))
            .collect();

        let (randomness, randomness_router) =
            randomness::Builder::new(config.protocol_public_key(), randomness_tx)
                .config(config.p2p_config.randomness.clone().unwrap_or_default())
                .with_metrics(prometheus_registry)
                .build();

        let p2p_network = {
            let routes = anemo::Router::new()
                .add_rpc_service(discovery_server)
                .add_rpc_service(state_sync_server);
            let routes = routes.merge(randomness_router);

            let inbound_network_metrics =
                consensus_core::NetworkRouteMetrics::new("sui", "inbound", prometheus_registry);
            let outbound_network_metrics =
                consensus_core::NetworkRouteMetrics::new("sui", "outbound", prometheus_registry);

            let service = ServiceBuilder::new()
                .layer(
                    TraceLayer::new_for_server_errors()
                        .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO))
                        .on_failure(DefaultOnFailure::new().level(tracing::Level::WARN)),
                )
                .layer(CallbackLayer::new(
                    consensus_core::MetricsMakeCallbackHandler::new(
                        Arc::new(inbound_network_metrics),
                        config.p2p_config.excessive_message_size(),
                    ),
                ))
                .service(routes);

            let outbound_layer = ServiceBuilder::new()
                .layer(
                    TraceLayer::new_for_client_and_server_errors()
                        .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO))
                        .on_failure(DefaultOnFailure::new().level(tracing::Level::WARN)),
                )
                .layer(CallbackLayer::new(
                    consensus_core::MetricsMakeCallbackHandler::new(
                        Arc::new(outbound_network_metrics),
                        config.p2p_config.excessive_message_size(),
                    ),
                ))
                .into_inner();

            let mut anemo_config = config.p2p_config.anemo_config.clone().unwrap_or_default();
            // Set the max_frame_size to be 1 GB to work around the issue of there being too many
            // staking events in the epoch change txn.
            anemo_config.max_frame_size = Some(1 << 30);

            // Set a higher default value for socket send/receive buffers if not already
            // configured.
            let mut quic_config = anemo_config.quic.unwrap_or_default();
            if quic_config.socket_send_buffer_size.is_none() {
                quic_config.socket_send_buffer_size = Some(20 << 20);
            }
            if quic_config.socket_receive_buffer_size.is_none() {
                quic_config.socket_receive_buffer_size = Some(20 << 20);
            }
            quic_config.allow_failed_socket_buffer_size_setting = true;

            // Set high-performance defaults for quinn transport.
            // With 200MiB buffer size and ~500ms RTT, max throughput ~400MiB/s.
            if quic_config.max_concurrent_bidi_streams.is_none() {
                quic_config.max_concurrent_bidi_streams = Some(500);
            }
            if quic_config.max_concurrent_uni_streams.is_none() {
                quic_config.max_concurrent_uni_streams = Some(500);
            }
            if quic_config.stream_receive_window.is_none() {
                quic_config.stream_receive_window = Some(100 << 20);
            }
            if quic_config.receive_window.is_none() {
                quic_config.receive_window = Some(200 << 20);
            }
            if quic_config.send_window.is_none() {
                quic_config.send_window = Some(200 << 20);
            }
            if quic_config.crypto_buffer_size.is_none() {
                quic_config.crypto_buffer_size = Some(1 << 20);
            }
            if quic_config.max_idle_timeout_ms.is_none() {
                quic_config.max_idle_timeout_ms = Some(30_000);
            }
            if quic_config.keep_alive_interval_ms.is_none() {
                quic_config.keep_alive_interval_ms = Some(5_000);
            }
            anemo_config.quic = Some(quic_config);

            let server_name = format!("sui-{}", chain_identifier);
            let network = Network::bind(config.p2p_config.listen_address)
                .server_name(&server_name)
                .private_key(config.network_key_pair().copy().private().0.to_bytes())
                .config(anemo_config)
                .outbound_request_layer(outbound_layer)
                .start(service)?;
            info!(
                server_name = server_name,
                "P2p network started on {}",
                network.local_addr()
            );

            network
        };

        let discovery_handle =
            discovery.start(p2p_network.clone(), config.network_key_pair().copy());
        let state_sync_handle = state_sync.start(p2p_network.clone());
        let randomness_handle = randomness.start(p2p_network.clone());

        Ok(P2pComponents {
            p2p_network,
            known_peers,
            discovery_handle,
            state_sync_handle,
            randomness_handle,
        })
    }

    async fn construct_validator_components(
        config: NodeConfig,
        state: Arc<AuthorityState>,
        committee: Arc<Committee>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        checkpoint_store: Arc<CheckpointStore>,
        state_sync_handle: state_sync::Handle,
        randomness_handle: randomness::Handle,
        accumulator: Weak<StateAccumulator>,
        backpressure_manager: Arc<BackpressureManager>,
        connection_monitor_status: Arc<ConnectionMonitorStatus>,
        registry_service: &RegistryService,
        sui_node_metrics: Arc<SuiNodeMetrics>,
    ) -> Result<ValidatorComponents> {
        let mut config_clone = config.clone();
        let consensus_config = config_clone
            .consensus_config
            .as_mut()
            .ok_or_else(|| anyhow!("Validator is missing consensus config"))?;

        let client = Arc::new(UpdatableConsensusClient::new());
        let consensus_adapter = Arc::new(Self::construct_consensus_adapter(
            &committee,
            consensus_config,
            state.name,
            connection_monitor_status.clone(),
            &registry_service.default_registry(),
            epoch_store.protocol_config().clone(),
            client.clone(),
        ));
        let consensus_manager =
            ConsensusManager::new(&config, consensus_config, registry_service, client);

        // This only gets started up once, not on every epoch. (Make call to remove every epoch.)
        let consensus_store_pruner = ConsensusStorePruner::new(
            consensus_manager.get_storage_base_path(),
            consensus_config.db_retention_epochs(),
            consensus_config.db_pruner_period(),
            &registry_service.default_registry(),
        );

        let checkpoint_metrics = CheckpointMetrics::new(&registry_service.default_registry());
        let sui_tx_validator_metrics =
            SuiTxValidatorMetrics::new(&registry_service.default_registry());

        let validator_server_handle = Self::start_grpc_validator_service(
            &config,
            state.clone(),
            consensus_adapter.clone(),
            &registry_service.default_registry(),
        )
        .await?;

        // Starts an overload monitor that monitors the execution of the authority.
        // Don't start the overload monitor when max_load_shedding_percentage is 0.
        let validator_overload_monitor_handle = if config
            .authority_overload_config
            .max_load_shedding_percentage
            > 0
        {
            let authority_state = Arc::downgrade(&state);
            let overload_config = config.authority_overload_config.clone();
            fail_point!("starting_overload_monitor");
            Some(spawn_monitored_task!(overload_monitor(
                authority_state,
                overload_config,
            )))
        } else {
            None
        };

        Self::start_epoch_specific_validator_components(
            &config,
            state.clone(),
            consensus_adapter,
            checkpoint_store,
            epoch_store,
            state_sync_handle,
            randomness_handle,
            consensus_manager,
            consensus_store_pruner,
            accumulator,
            backpressure_manager,
            validator_server_handle,
            validator_overload_monitor_handle,
            checkpoint_metrics,
            sui_node_metrics,
            sui_tx_validator_metrics,
        )
        .await
    }

    async fn start_epoch_specific_validator_components(
        config: &NodeConfig,
        state: Arc<AuthorityState>,
        consensus_adapter: Arc<ConsensusAdapter>,
        checkpoint_store: Arc<CheckpointStore>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        state_sync_handle: state_sync::Handle,
        randomness_handle: randomness::Handle,
        consensus_manager: ConsensusManager,
        consensus_store_pruner: ConsensusStorePruner,
        accumulator: Weak<StateAccumulator>,
        backpressure_manager: Arc<BackpressureManager>,
        validator_server_handle: SpawnOnce,
        validator_overload_monitor_handle: Option<JoinHandle<()>>,
        checkpoint_metrics: Arc<CheckpointMetrics>,
        sui_node_metrics: Arc<SuiNodeMetrics>,
        sui_tx_validator_metrics: Arc<SuiTxValidatorMetrics>,
    ) -> Result<ValidatorComponents> {
        let checkpoint_service = Self::build_checkpoint_service(
            config,
            consensus_adapter.clone(),
            checkpoint_store.clone(),
            epoch_store.clone(),
            state.clone(),
            state_sync_handle,
            accumulator,
            checkpoint_metrics.clone(),
        );

        // create a new map that gets injected into both the consensus handler and the consensus adapter
        // the consensus handler will write values forwarded from consensus, and the consensus adapter
        // will read the values to make decisions about which validator submits a transaction to consensus
        let low_scoring_authorities = Arc::new(ArcSwap::new(Arc::new(HashMap::new())));

        consensus_adapter.swap_low_scoring_authorities(low_scoring_authorities.clone());

        if epoch_store.randomness_state_enabled() {
            let randomness_manager = RandomnessManager::try_new(
                Arc::downgrade(&epoch_store),
                Box::new(consensus_adapter.clone()),
                randomness_handle,
                config.protocol_key_pair(),
            )
            .await;
            if let Some(randomness_manager) = randomness_manager {
                epoch_store
                    .set_randomness_manager(randomness_manager)
                    .await?;
            }
        }

        ExecutionTimeObserver::spawn(
            &epoch_store,
            Box::new(consensus_adapter.clone()),
            config.local_execution_time_channel_capacity,
        );

        let throughput_calculator = Arc::new(ConsensusThroughputCalculator::new(
            None,
            state.metrics.clone(),
        ));

        let throughput_profiler = Arc::new(ConsensusThroughputProfiler::new(
            throughput_calculator.clone(),
            None,
            None,
            state.metrics.clone(),
            ThroughputProfileRanges::from_chain(epoch_store.get_chain_identifier()),
        ));

        consensus_adapter.swap_throughput_profiler(throughput_profiler);

        let consensus_handler_initializer = ConsensusHandlerInitializer::new(
            state.clone(),
            checkpoint_service.clone(),
            epoch_store.clone(),
            low_scoring_authorities,
            throughput_calculator,
            backpressure_manager,
        );

        info!("Starting consensus manager");

        consensus_manager
            .start(
                config,
                epoch_store.clone(),
                consensus_handler_initializer,
                SuiTxValidator::new(
                    state.clone(),
                    consensus_adapter.clone(),
                    checkpoint_service.clone(),
                    state.transaction_manager().clone(),
                    sui_tx_validator_metrics.clone(),
                ),
            )
            .await;

        if !epoch_store
            .epoch_start_config()
            .is_data_quarantine_active_from_beginning_of_epoch()
        {
            checkpoint_store
                .reexecute_local_checkpoints(&state, &epoch_store)
                .await;
        }

        info!("Spawning checkpoint service");
        let checkpoint_service_tasks = checkpoint_service.spawn().await;

        if epoch_store.authenticator_state_enabled() {
            Self::start_jwk_updater(
                config,
                sui_node_metrics,
                state.name,
                epoch_store.clone(),
                consensus_adapter.clone(),
            );
        }

        Ok(ValidatorComponents {
            validator_server_handle,
            validator_overload_monitor_handle,
            consensus_manager,
            consensus_store_pruner,
            consensus_adapter,
            checkpoint_service_tasks,
            checkpoint_metrics,
            sui_tx_validator_metrics,
        })
    }

    fn build_checkpoint_service(
        config: &NodeConfig,
        consensus_adapter: Arc<ConsensusAdapter>,
        checkpoint_store: Arc<CheckpointStore>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        state: Arc<AuthorityState>,
        state_sync_handle: state_sync::Handle,
        accumulator: Weak<StateAccumulator>,
        checkpoint_metrics: Arc<CheckpointMetrics>,
    ) -> Arc<CheckpointService> {
        let epoch_start_timestamp_ms = epoch_store.epoch_start_state().epoch_start_timestamp_ms();
        let epoch_duration_ms = epoch_store.epoch_start_state().epoch_duration_ms();

        debug!(
            "Starting checkpoint service with epoch start timestamp {}
            and epoch duration {}",
            epoch_start_timestamp_ms, epoch_duration_ms
        );

        let checkpoint_output = Box::new(SubmitCheckpointToConsensus {
            sender: consensus_adapter,
            signer: state.secret.clone(),
            authority: config.protocol_public_key(),
            next_reconfiguration_timestamp_ms: epoch_start_timestamp_ms
                .checked_add(epoch_duration_ms)
                .expect("Overflow calculating next_reconfiguration_timestamp_ms"),
            metrics: checkpoint_metrics.clone(),
        });

        let certified_checkpoint_output = SendCheckpointToStateSync::new(state_sync_handle);
        let max_tx_per_checkpoint = max_tx_per_checkpoint(epoch_store.protocol_config());
        let max_checkpoint_size_bytes =
            epoch_store.protocol_config().max_checkpoint_size_bytes() as usize;

        CheckpointService::build(
            state.clone(),
            checkpoint_store,
            epoch_store,
            state.get_transaction_cache_reader().clone(),
            accumulator,
            checkpoint_output,
            Box::new(certified_checkpoint_output),
            checkpoint_metrics,
            max_tx_per_checkpoint,
            max_checkpoint_size_bytes,
        )
    }

    fn construct_consensus_adapter(
        committee: &Committee,
        consensus_config: &ConsensusConfig,
        authority: AuthorityName,
        connection_monitor_status: Arc<ConnectionMonitorStatus>,
        prometheus_registry: &Registry,
        protocol_config: ProtocolConfig,
        consensus_client: Arc<dyn ConsensusClient>,
    ) -> ConsensusAdapter {
        let ca_metrics = ConsensusAdapterMetrics::new(prometheus_registry);
        // The consensus adapter allows the authority to send user certificates through consensus.

        ConsensusAdapter::new(
            consensus_client,
            authority,
            connection_monitor_status,
            consensus_config.max_pending_transactions(),
            consensus_config.max_pending_transactions() * 2 / committee.num_members(),
            consensus_config.max_submit_position,
            consensus_config.submit_delay_step_override(),
            ca_metrics,
            protocol_config,
        )
    }

    async fn start_grpc_validator_service(
        config: &NodeConfig,
        state: Arc<AuthorityState>,
        consensus_adapter: Arc<ConsensusAdapter>,
        prometheus_registry: &Registry,
    ) -> Result<SpawnOnce> {
        let validator_service = ValidatorService::new(
            state.clone(),
            consensus_adapter,
            Arc::new(ValidatorServiceMetrics::new(prometheus_registry)),
            TrafficControllerMetrics::new(prometheus_registry),
            config.policy_config.clone(),
            config.firewall_config.clone(),
        );

        let mut server_conf = mysten_network::config::Config::new();
        server_conf.global_concurrency_limit = config.grpc_concurrency_limit;
        server_conf.load_shed = config.grpc_load_shed;
        let mut server_builder =
            ServerBuilder::from_config(&server_conf, GrpcMetrics::new(prometheus_registry));

        server_builder = server_builder.add_service(ValidatorServer::new(validator_service));

        let tls_config = sui_tls::create_rustls_server_config(
            config.network_key_pair().copy().private(),
            SUI_TLS_SERVER_NAME.to_string(),
        );
        let server = server_builder
            .bind(config.network_address(), Some(tls_config))
            .await
            .map_err(|err| anyhow!(err.to_string()))?;
        let local_addr = server.local_addr();
        info!("Listening to traffic on {local_addr}");

        Ok(SpawnOnce::new(server.serve().map_err(Into::into)))
    }

    async fn reexecute_pending_consensus_certs(
        epoch_store: &Arc<AuthorityPerEpochStore>,
        state: &Arc<AuthorityState>,
    ) {
        let pending_consensus_certificates = epoch_store
            .get_all_pending_consensus_transactions()
            .into_iter()
            .filter_map(|tx| {
                match tx.kind {
                    // shared object txns will be re-executed by consensus replay
                    ConsensusTransactionKind::CertifiedTransaction(tx)
                        if !tx.contains_shared_object() =>
                    {
                        let tx = *tx;
                        // we only need to re-execute if we previously signed the effects (which indicates we
                        // returned the effects to a client).
                        if let Some(fx_digest) = epoch_store
                            .get_signed_effects_digest(tx.digest())
                            .expect("db error")
                        {
                            // new_unchecked is safe because we never submit a transaction to consensus
                            // without verifying it
                            let tx = VerifiedExecutableTransaction::new_from_certificate(
                                VerifiedCertificate::new_unchecked(tx),
                            );
                            Some((tx, fx_digest))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            })
            .collect::<Vec<_>>();

        let digests = pending_consensus_certificates
            .iter()
            .map(|(tx, _)| *tx.digest())
            .collect::<Vec<_>>();

        info!("reexecuting pending consensus certificates: {:?}", digests);

        state.enqueue_with_expected_effects_digest(pending_consensus_certificates, epoch_store);

        // If this times out, the validator will still almost certainly start up fine. But, it is
        // possible that it may temporarily "forget" about transactions that it had previously
        // executed. This could confuse clients in some circumstances. However, the transactions
        // are still in pending_consensus_certificates, so we cannot lose any finality guarantees.
        if tokio::time::timeout(
            std::time::Duration::from_secs(60),
            state
                .get_transaction_cache_reader()
                .notify_read_executed_effects_digests(&digests),
        )
        .await
        .is_err()
        {
            debug_fatal!("Timed out waiting for effects digests to be executed");
        }
    }

    pub fn state(&self) -> Arc<AuthorityState> {
        self.state.clone()
    }

    // Only used for testing because of how epoch store is loaded.
    pub fn reference_gas_price_for_testing(&self) -> Result<u64, anyhow::Error> {
        self.state.reference_gas_price_for_testing()
    }

    pub fn clone_committee_store(&self) -> Arc<CommitteeStore> {
        self.state.committee_store().clone()
    }

    /*
    pub fn clone_authority_store(&self) -> Arc<AuthorityStore> {
        self.state.db()
    }
    */

    /// Clone an AuthorityAggregator currently used in this node's
    /// QuorumDriver, if the node is a fullnode. After reconfig,
    /// QuorumDriver builds a new AuthorityAggregator. The caller
    /// of this function will mostly likely want to call this again
    /// to get a fresh one.
    pub fn clone_authority_aggregator(
        &self,
    ) -> Option<Arc<AuthorityAggregator<NetworkAuthorityClient>>> {
        self.transaction_orchestrator
            .as_ref()
            .map(|to| to.clone_authority_aggregator())
    }

    pub fn transaction_orchestrator(
        &self,
    ) -> Option<Arc<TransactiondOrchestrator<NetworkAuthorityClient>>> {
        self.transaction_orchestrator.clone()
    }

    pub fn subscribe_to_transaction_orchestrator_effects(
        &self,
    ) -> Result<tokio::sync::broadcast::Receiver<QuorumDriverEffectsQueueResult>> {
        self.transaction_orchestrator
            .as_ref()
            .map(|to| to.subscribe_to_effects_queue())
            .ok_or_else(|| anyhow::anyhow!("Transaction Orchestrator is not enabled in this node."))
    }

    /// This function awaits the completion of checkpoint execution of the current epoch,
    /// after which it iniitiates reconfiguration of the entire system.
    pub async fn monitor_reconfiguration(self: Arc<Self>) -> Result<()> {
        let checkpoint_executor_metrics =
            CheckpointExecutorMetrics::new(&self.registry_service.default_registry());

        loop {
            let mut accumulator_guard = self.accumulator.lock().await;
            let accumulator = accumulator_guard.take().unwrap();
            let mut checkpoint_executor = CheckpointExecutor::new(
                self.state_sync_handle.subscribe_to_synced_checkpoints(),
                self.checkpoint_store.clone(),
                self.state.clone(),
                accumulator.clone(),
                self.backpressure_manager.clone(),
                self.config.checkpoint_executor_config.clone(),
                checkpoint_executor_metrics.clone(),
                self.subscription_service_checkpoint_sender.clone(),
            );

            let run_with_range = self.config.run_with_range;

            let cur_epoch_store = self.state.load_epoch_store_one_call_per_task();

            // Advertise capabilities to committee, if we are a validator.
            if let Some(components) = &*self.validator_components.lock().await {
                // TODO: without this sleep, the consensus message is not delivered reliably.
                tokio::time::sleep(Duration::from_millis(1)).await;

                let config = cur_epoch_store.protocol_config();
                let binary_config = to_binary_config(config);
                let transaction = if config.authority_capabilities_v2() {
                    ConsensusTransaction::new_capability_notification_v2(
                        AuthorityCapabilitiesV2::new(
                            self.state.name,
                            cur_epoch_store.get_chain_identifier().chain(),
                            self.config
                                .supported_protocol_versions
                                .expect("Supported versions should be populated")
                                // no need to send digests of versions less than the current version
                                .truncate_below(config.version),
                            self.state
                                .get_available_system_packages(&binary_config)
                                .await,
                        ),
                    )
                } else {
                    ConsensusTransaction::new_capability_notification(AuthorityCapabilitiesV1::new(
                        self.state.name,
                        self.config
                            .supported_protocol_versions
                            .expect("Supported versions should be populated"),
                        self.state
                            .get_available_system_packages(&binary_config)
                            .await,
                    ))
                };
                info!(?transaction, "submitting capabilities to consensus");
                components
                    .consensus_adapter
                    .submit(transaction, None, &cur_epoch_store)?;
            }

            let stop_condition = checkpoint_executor
                .run_epoch(cur_epoch_store.clone(), run_with_range)
                .await;
            drop(checkpoint_executor);

            if stop_condition == StopReason::RunWithRangeCondition {
                SuiNode::shutdown(&self).await;
                self.shutdown_channel_tx
                    .send(run_with_range)
                    .expect("RunWithRangeCondition met but failed to send shutdown message");
                return Ok(());
            }

            // Safe to call because we are in the middle of reconfiguration.
            let latest_system_state = self
                .state
                .get_object_cache_reader()
                .get_sui_system_state_object_unsafe()
                .expect("Read Sui System State object cannot fail");

            #[cfg(msim)]
            if !self
                .sim_state
                .sim_safe_mode_expected
                .load(Ordering::Relaxed)
            {
                debug_assert!(!latest_system_state.safe_mode());
            }

            #[cfg(not(msim))]
            debug_assert!(!latest_system_state.safe_mode());

            if let Err(err) = self.end_of_epoch_channel.send(latest_system_state.clone()) {
                if self.state.is_fullnode(&cur_epoch_store) {
                    warn!(
                        "Failed to send end of epoch notification to subscriber: {:?}",
                        err
                    );
                }
            }

            cur_epoch_store.record_is_safe_mode_metric(latest_system_state.safe_mode());
            let new_epoch_start_state = latest_system_state.into_epoch_start_state();

            self.auth_agg.store(Arc::new(
                self.auth_agg
                    .load()
                    .recreate_with_new_epoch_start_state(&new_epoch_start_state),
            ));

            let next_epoch_committee = new_epoch_start_state.get_sui_committee();
            let next_epoch = next_epoch_committee.epoch();
            assert_eq!(cur_epoch_store.epoch() + 1, next_epoch);

            info!(
                next_epoch,
                "Finished executing all checkpoints in epoch. About to reconfigure the system."
            );

            fail_point_async!("reconfig_delay");

            // We save the connection monitor status map regardless of validator / fullnode status
            // so that we don't need to restart the connection monitor every epoch.
            // Update the mappings that will be used by the consensus adapter if it exists or is
            // about to be created.
            let authority_names_to_peer_ids =
                new_epoch_start_state.get_authority_names_to_peer_ids();
            self.connection_monitor_status
                .update_mapping_for_epoch(authority_names_to_peer_ids);

            cur_epoch_store.record_epoch_reconfig_start_time_metric();

            let _ = send_trusted_peer_change(
                &self.config,
                &self.trusted_peer_change_tx,
                &new_epoch_start_state,
            );

            // The following code handles 4 different cases, depending on whether the node
            // was a validator in the previous epoch, and whether the node is a validator
            // in the new epoch.

            let new_validator_components = if let Some(ValidatorComponents {
                validator_server_handle,
                validator_overload_monitor_handle,
                consensus_manager,
                consensus_store_pruner,
                consensus_adapter,
                mut checkpoint_service_tasks,
                checkpoint_metrics,
                sui_tx_validator_metrics,
            }) = self.validator_components.lock().await.take()
            {
                info!("Reconfiguring the validator.");
                // Cancel the old checkpoint service tasks.
                // Waiting for checkpoint builder to finish gracefully is not possible, because it
                // may wait on transactions while consensus on peers have already shut down.
                checkpoint_service_tasks.abort_all();
                while let Some(result) = checkpoint_service_tasks.join_next().await {
                    if let Err(err) = result {
                        if err.is_panic() {
                            std::panic::resume_unwind(err.into_panic());
                        }
                        warn!("Error in checkpoint service task: {:?}", err);
                    }
                }
                info!("Checkpoint service has shut down.");

                consensus_manager.shutdown().await;
                info!("Consensus has shut down.");

                let new_epoch_store = self
                    .reconfigure_state(
                        &self.state,
                        &cur_epoch_store,
                        next_epoch_committee.clone(),
                        new_epoch_start_state,
                        accumulator.clone(),
                    )
                    .await;
                info!("Epoch store finished reconfiguration.");

                // No other components should be holding a strong reference to state accumulator
                // at this point. Confirm here before we swap in the new accumulator.
                let accumulator_metrics = Arc::into_inner(accumulator)
                    .expect("Accumulator should have no other references at this point")
                    .metrics();
                let new_accumulator = Arc::new(StateAccumulator::new(
                    self.state.get_accumulator_store().clone(),
                    accumulator_metrics,
                ));
                let weak_accumulator = Arc::downgrade(&new_accumulator);
                *accumulator_guard = Some(new_accumulator);

                consensus_store_pruner.prune(next_epoch).await;

                if self.state.is_validator(&new_epoch_store) {
                    // Only restart consensus if this node is still a validator in the new epoch.
                    Some(
                        Self::start_epoch_specific_validator_components(
                            &self.config,
                            self.state.clone(),
                            consensus_adapter,
                            self.checkpoint_store.clone(),
                            new_epoch_store.clone(),
                            self.state_sync_handle.clone(),
                            self.randomness_handle.clone(),
                            consensus_manager,
                            consensus_store_pruner,
                            weak_accumulator,
                            self.backpressure_manager.clone(),
                            validator_server_handle,
                            validator_overload_monitor_handle,
                            checkpoint_metrics,
                            self.metrics.clone(),
                            sui_tx_validator_metrics,
                        )
                        .await?,
                    )
                } else {
                    info!("This node is no longer a validator after reconfiguration");
                    None
                }
            } else {
                let new_epoch_store = self
                    .reconfigure_state(
                        &self.state,
                        &cur_epoch_store,
                        next_epoch_committee.clone(),
                        new_epoch_start_state,
                        accumulator.clone(),
                    )
                    .await;

                // No other components should be holding a strong reference to state accumulator
                // at this point. Confirm here before we swap in the new accumulator.
                let accumulator_metrics = Arc::into_inner(accumulator)
                    .expect("Accumulator should have no other references at this point")
                    .metrics();
                let new_accumulator = Arc::new(StateAccumulator::new(
                    self.state.get_accumulator_store().clone(),
                    accumulator_metrics,
                ));
                let weak_accumulator = Arc::downgrade(&new_accumulator);
                *accumulator_guard = Some(new_accumulator);

                if self.state.is_validator(&new_epoch_store) {
                    info!("Promoting the node from fullnode to validator, starting grpc server");

                    let mut components = Self::construct_validator_components(
                        self.config.clone(),
                        self.state.clone(),
                        Arc::new(next_epoch_committee.clone()),
                        new_epoch_store.clone(),
                        self.checkpoint_store.clone(),
                        self.state_sync_handle.clone(),
                        self.randomness_handle.clone(),
                        weak_accumulator,
                        self.backpressure_manager.clone(),
                        self.connection_monitor_status.clone(),
                        &self.registry_service,
                        self.metrics.clone(),
                    )
                    .await?;

                    components.validator_server_handle = components.validator_server_handle.start();

                    Some(components)
                } else {
                    None
                }
            };
            *self.validator_components.lock().await = new_validator_components;

            // Force releasing current epoch store DB handle, because the
            // Arc<AuthorityPerEpochStore> may linger.
            cur_epoch_store.release_db_handles();

            if cfg!(msim)
                && !matches!(
                    self.config
                        .authority_store_pruning_config
                        .num_epochs_to_retain_for_checkpoints(),
                    None | Some(u64::MAX) | Some(0)
                )
            {
                self.state
                .prune_checkpoints_for_eligible_epochs_for_testing(
                    self.config.clone(),
                    sui_core::authority::authority_store_pruner::AuthorityStorePruningMetrics::new_for_test(),
                )
                .await?;
            }

            info!("Reconfiguration finished");
        }
    }

    async fn shutdown(&self) {
        if let Some(validator_components) = &*self.validator_components.lock().await {
            validator_components.consensus_manager.shutdown().await;
        }
    }

    async fn reconfigure_state(
        &self,
        state: &Arc<AuthorityState>,
        cur_epoch_store: &AuthorityPerEpochStore,
        next_epoch_committee: Committee,
        next_epoch_start_system_state: EpochStartSystemState,
        accumulator: Arc<StateAccumulator>,
    ) -> Arc<AuthorityPerEpochStore> {
        let next_epoch = next_epoch_committee.epoch();

        let last_checkpoint = self
            .checkpoint_store
            .get_epoch_last_checkpoint(cur_epoch_store.epoch())
            .expect("Error loading last checkpoint for current epoch")
            .expect("Could not load last checkpoint for current epoch");

        let last_checkpoint_seq = *last_checkpoint.sequence_number();

        assert_eq!(
            Some(last_checkpoint_seq),
            self.checkpoint_store
                .get_highest_executed_checkpoint_seq_number()
                .expect("Error loading highest executed checkpoint sequence number")
        );

        let epoch_start_configuration = EpochStartConfiguration::new(
            next_epoch_start_system_state,
            *last_checkpoint.digest(),
            state.get_object_store().as_ref(),
            EpochFlag::default_flags_for_new_epoch(&state.config),
        )
        .expect("EpochStartConfiguration construction cannot fail");

        let new_epoch_store = self
            .state
            .reconfigure(
                cur_epoch_store,
                self.config.supported_protocol_versions.unwrap(),
                next_epoch_committee,
                epoch_start_configuration,
                accumulator,
                &self.config.expensive_safety_check_config,
                last_checkpoint_seq,
            )
            .await
            .expect("Reconfigure authority state cannot fail");
        info!(next_epoch, "Node State has been reconfigured");
        assert_eq!(next_epoch, new_epoch_store.epoch());
        self.state.get_reconfig_api().update_epoch_flags_metrics(
            cur_epoch_store.epoch_start_config().flags(),
            new_epoch_store.epoch_start_config().flags(),
        );

        new_epoch_store
    }

    pub fn get_config(&self) -> &NodeConfig {
        &self.config
    }

    pub fn randomness_handle(&self) -> randomness::Handle {
        self.randomness_handle.clone()
    }
}

#[cfg(not(msim))]
impl SuiNode {
    async fn fetch_jwks(
        _authority: AuthorityName,
        provider: &OIDCProvider,
    ) -> SuiResult<Vec<(JwkId, JWK)>> {
        use fastcrypto_zkp::bn254::zk_login::fetch_jwks;
        let client = reqwest::Client::new();
        fetch_jwks(provider, &client)
            .await
            .map_err(|_| SuiError::JWKRetrievalError)
    }
}

#[cfg(msim)]
impl SuiNode {
    pub fn get_sim_node_id(&self) -> sui_simulator::task::NodeId {
        self.sim_state.sim_node.id()
    }

    pub fn set_safe_mode_expected(&self, new_value: bool) {
        info!("Setting safe mode expected to {}", new_value);
        self.sim_state
            .sim_safe_mode_expected
            .store(new_value, Ordering::Relaxed);
    }

    #[allow(unused_variables)]
    async fn fetch_jwks(
        authority: AuthorityName,
        provider: &OIDCProvider,
    ) -> SuiResult<Vec<(JwkId, JWK)>> {
        get_jwk_injector()(authority, provider)
    }
}

enum SpawnOnce {
    // Mutex is only needed to make SpawnOnce Send
    Unstarted(Mutex<BoxFuture<'static, Result<()>>>),
    #[allow(unused)]
    Started(JoinHandle<Result<()>>),
}

impl SpawnOnce {
    pub fn new(future: impl Future<Output = Result<()>> + Send + 'static) -> Self {
        Self::Unstarted(Mutex::new(Box::pin(future)))
    }

    pub fn start(self) -> Self {
        match self {
            Self::Unstarted(future) => {
                let future = future.into_inner();
                let handle = tokio::spawn(future);
                Self::Started(handle)
            }
            Self::Started(_) => self,
        }
    }
}

/// Notify state-sync that a new list of trusted peers are now available.
fn send_trusted_peer_change(
    config: &NodeConfig,
    sender: &watch::Sender<TrustedPeerChangeEvent>,
    epoch_state_state: &EpochStartSystemState,
) -> Result<(), watch::error::SendError<TrustedPeerChangeEvent>> {
    sender
        .send(TrustedPeerChangeEvent {
            new_peers: epoch_state_state.get_validator_as_p2p_peers(config.protocol_public_key()),
        })
        .tap_err(|err| {
            warn!(
                "Failed to send validator peer information to state sync: {:?}",
                err
            );
        })
}

fn build_kv_store(
    state: &Arc<AuthorityState>,
    config: &NodeConfig,
    registry: &Registry,
) -> Result<Arc<TransactionKeyValueStore>> {
    let metrics = KeyValueStoreMetrics::new(registry);
    let db_store = TransactionKeyValueStore::new("rocksdb", metrics.clone(), state.clone());

    let base_url = &config.transaction_kv_store_read_config.base_url;

    if base_url.is_empty() {
        info!("no http kv store url provided, using local db only");
        return Ok(Arc::new(db_store));
    }

    let base_url: url::Url = base_url.parse().tap_err(|e| {
        error!(
            "failed to parse config.transaction_kv_store_config.base_url ({:?}) as url: {}",
            base_url, e
        )
    })?;

    let network_str = match state.get_chain_identifier().chain() {
        Chain::Mainnet => "/mainnet",
        _ => {
            info!("using local db only for kv store");
            return Ok(Arc::new(db_store));
        }
    };

    let base_url = base_url.join(network_str)?.to_string();
    let http_store = HttpKVStore::new_kv(
        &base_url,
        config.transaction_kv_store_read_config.cache_size,
        metrics.clone(),
    )?;
    info!("using local key-value store with fallback to http key-value store");
    Ok(Arc::new(FallbackTransactionKVStore::new_kv(
        db_store,
        http_store,
        metrics,
        "json_rpc_fallback",
    )))
}

pub async fn build_http_server(
    state: Arc<AuthorityState>,
    store: RocksDbStore,
    transaction_orchestrator: &Option<Arc<TransactiondOrchestrator<NetworkAuthorityClient>>>,
    config: &NodeConfig,
    prometheus_registry: &Registry,
    _custom_runtime: Option<Handle>,
    software_version: &'static str,
) -> Result<(
    Option<sui_http::ServerHandle>,
    Option<tokio::sync::mpsc::Sender<CheckpointData>>,
)> {
    // Validators do not expose these APIs
    if config.consensus_config().is_some() {
        return Ok((None, None));
    }

    let mut router = axum::Router::new();

    let json_rpc_router = {
        let mut server = JsonRpcServerBuilder::new(
            env!("CARGO_PKG_VERSION"),
            prometheus_registry,
            config.policy_config.clone(),
            config.firewall_config.clone(),
        );

        let kv_store = build_kv_store(&state, config, prometheus_registry)?;

        let metrics = Arc::new(JsonRpcMetrics::new(prometheus_registry));
        server.register_module(ReadApi::new(
            state.clone(),
            kv_store.clone(),
            metrics.clone(),
        ))?;
        server.register_module(CoinReadApi::new(
            state.clone(),
            kv_store.clone(),
            metrics.clone(),
        ))?;

        // if run_with_range is enabled we want to prevent any transactions
        // run_with_range = None is normal operating conditions
        if config.run_with_range.is_none() {
            server.register_module(TransactionBuilderApi::new(state.clone()))?;
        }
        server.register_module(GovernanceReadApi::new(state.clone(), metrics.clone()))?;
        server.register_module(BridgeReadApi::new(state.clone(), metrics.clone()))?;

        if let Some(transaction_orchestrator) = transaction_orchestrator {
            server.register_module(TransactionExecutionApi::new(
                state.clone(),
                transaction_orchestrator.clone(),
                metrics.clone(),
            ))?;
        }

        let name_service_config =
            if let (Some(package_address), Some(registry_id), Some(reverse_registry_id)) = (
                config.name_service_package_address,
                config.name_service_registry_id,
                config.name_service_reverse_registry_id,
            ) {
                sui_name_service::NameServiceConfig::new(
                    package_address,
                    registry_id,
                    reverse_registry_id,
                )
            } else {
                match state.get_chain_identifier().chain() {
                    Chain::Mainnet => sui_name_service::NameServiceConfig::mainnet(),
                    Chain::Testnet => sui_name_service::NameServiceConfig::testnet(),
                    Chain::Unknown => sui_name_service::NameServiceConfig::default(),
                }
            };

        server.register_module(IndexerApi::new(
            state.clone(),
            ReadApi::new(state.clone(), kv_store.clone(), metrics.clone()),
            kv_store,
            name_service_config,
            metrics,
            config.indexer_max_subscriptions,
        ))?;
        server.register_module(MoveUtils::new(state.clone()))?;

        let server_type = config.jsonrpc_server_type();

        server.to_router(server_type).await?
    };

    router = router.merge(json_rpc_router);

    let (subscription_service_checkpoint_sender, subscription_service_handle) =
        SubscriptionService::build(prometheus_registry);
    let rpc_router = {
        let mut rpc_service = sui_rpc_api::RpcService::new(
            Arc::new(RestReadStore::new(state.clone(), store)),
            software_version,
        );

        if let Some(config) = config.rpc.clone() {
            rpc_service.with_config(config);
        }

        rpc_service.with_metrics(RpcMetrics::new(prometheus_registry));
        rpc_service.with_subscription_service(subscription_service_handle);

        if let Some(transaction_orchestrator) = transaction_orchestrator {
            rpc_service.with_executor(transaction_orchestrator.clone())
        }

        rpc_service.into_router().await
    };

    let layers = ServiceBuilder::new()
        .map_request(|mut request: axum::http::Request<_>| {
            if let Some(connect_info) = request.extensions().get::<sui_http::ConnectInfo>() {
                let axum_connect_info = axum::extract::ConnectInfo(connect_info.remote_addr);
                request.extensions_mut().insert(axum_connect_info);
            }
            request
        })
        .layer(axum::middleware::from_fn(server_timing_middleware));

    router = router.merge(rpc_router).layer(layers);

    let handle = sui_http::Builder::new()
        .serve(&config.json_rpc_address, router)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    info!(local_addr =? handle.local_addr(), "Sui JSON-RPC server listening on {}", handle.local_addr());

    Ok((Some(handle), Some(subscription_service_checkpoint_sender)))
}

#[cfg(not(test))]
fn max_tx_per_checkpoint(protocol_config: &ProtocolConfig) -> usize {
    protocol_config.max_transactions_per_checkpoint() as usize
}

#[cfg(test)]
fn max_tx_per_checkpoint(_: &ProtocolConfig) -> usize {
    2
}
