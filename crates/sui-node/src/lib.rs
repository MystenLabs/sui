// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
#[cfg(msim)]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anemo::Network;
use anemo_tower::callback::CallbackLayer;
use anemo_tower::trace::DefaultMakeSpan;
use anemo_tower::trace::DefaultOnFailure;
use anemo_tower::trace::TraceLayer;
use anyhow::anyhow;
use anyhow::Result;
use arc_swap::ArcSwap;
use futures::TryFutureExt;
use mysten_common::sync::async_once_cell::AsyncOnceCell;
use prometheus::Registry;
use sui_core::consensus_adapter::LazyNarwhalClient;
use sui_json_rpc::api::JsonRpcMetrics;
use sui_types::sui_system_state::SuiSystemState;
use tap::tap::TapFallible;
use tokio::runtime::Handle;
use tokio::sync::broadcast;
use tokio::sync::oneshot;
use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;
use tower::ServiceBuilder;
use tracing::{debug, warn};
use tracing::{error_span, info, Instrument};

use checkpoint_executor::CheckpointExecutor;
pub use handle::SuiNodeHandle;
use mysten_metrics::{spawn_monitored_task, RegistryService};
use mysten_network::server::ServerBuilder;
use narwhal_network::metrics::MetricsMakeCallbackHandler;
use narwhal_network::metrics::{NetworkConnectionMetrics, NetworkMetrics};
use sui_config::node::DBCheckpointConfig;
use sui_config::{ConsensusConfig, NodeConfig};
use sui_core::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use sui_core::authority::epoch_start_configuration::EpochStartConfigTrait;
use sui_core::authority::epoch_start_configuration::EpochStartConfiguration;
use sui_core::authority_aggregator::AuthorityAggregator;
use sui_core::authority_server::ValidatorService;
use sui_core::checkpoints::checkpoint_executor;
use sui_core::checkpoints::{
    CheckpointMetrics, CheckpointService, CheckpointStore, SendCheckpointToStateSync,
    SubmitCheckpointToConsensus,
};
use sui_core::consensus_adapter::{
    CheckConnection, ConnectionMonitorStatus, ConsensusAdapter, ConsensusAdapterMetrics,
};
use sui_core::consensus_handler::ConsensusHandler;
use sui_core::consensus_validator::{SuiTxValidator, SuiTxValidatorMetrics};
use sui_core::db_checkpoint_handler::DBCheckpointHandler;
use sui_core::epoch::committee_store::CommitteeStore;
use sui_core::epoch::data_removal::EpochDataRemover;
use sui_core::epoch::epoch_metrics::EpochMetrics;
use sui_core::epoch::reconfiguration::ReconfigurationInitiator;
use sui_core::module_cache_metrics::ResolverMetrics;
use sui_core::narwhal_manager::{NarwhalConfiguration, NarwhalManager, NarwhalManagerMetrics};
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
use sui_json_rpc::{JsonRpcServerBuilder, ServerHandle};
use sui_macros::fail_point_async;
use sui_network::api::ValidatorServer;
use sui_network::discovery;
use sui_network::discovery::TrustedPeerChangeEvent;
use sui_network::state_sync;
use sui_protocol_config::{ProtocolConfig, SupportedProtocolVersions};
use sui_storage::IndexStore;
use sui_types::base_types::{AuthorityName, EpochId, TransactionDigest};
use sui_types::committee::Committee;
use sui_types::crypto::KeypairTraits;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{AuthorityCapabilities, ConsensusTransaction};
use sui_types::quorum_driver_types::QuorumDriverEffectsQueueResult;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemState;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use sui_types::sui_system_state::SuiSystemStateTrait;
use typed_store::rocks::default_db_options;
use typed_store::DBMetrics;

use crate::metrics::GrpcMetrics;

pub mod admin;
mod handle;
pub mod metrics;

pub struct ValidatorComponents {
    validator_server_handle: JoinHandle<Result<()>>,
    narwhal_manager: NarwhalManager,
    narwhal_epoch_data_remover: EpochDataRemover,
    consensus_adapter: Arc<ConsensusAdapter>,
    // dropping this will eventually stop checkpoint tasks. The receiver side of this channel
    // is copied into each checkpoint service task, and they are listening to any change to this
    // channel. When the sender is dropped, a change is triggered and those tasks will exit.
    checkpoint_service_exit: watch::Sender<()>,
    checkpoint_metrics: Arc<CheckpointMetrics>,
    sui_tx_validator_metrics: Arc<SuiTxValidatorMetrics>,
}

pub struct SuiNode {
    config: NodeConfig,
    validator_components: Mutex<Option<ValidatorComponents>>,
    _json_rpc_service: Option<ServerHandle>,
    state: Arc<AuthorityState>,
    transaction_orchestrator: Option<Arc<TransactiondOrchestrator<NetworkAuthorityClient>>>,
    registry_service: RegistryService,

    _discovery: discovery::Handle,
    state_sync: state_sync::Handle,
    checkpoint_store: Arc<CheckpointStore>,
    accumulator: Arc<StateAccumulator>,
    connection_monitor_status: Arc<ConnectionMonitorStatus>,

    /// Broadcast channel to send the starting system state for the next epoch.
    end_of_epoch_channel: broadcast::Sender<SuiSystemState>,

    /// Broadcast channel to notify state-sync for new validator peers.
    trusted_peer_change_tx: watch::Sender<TrustedPeerChangeEvent>,

    _db_checkpoint_handle: Option<oneshot::Sender<()>>,

    #[cfg(msim)]
    sim_node: sui_simulator::runtime::NodeHandle,

    #[cfg(msim)]
    sim_safe_mode_expected: AtomicBool,
}

impl fmt::Debug for SuiNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("SuiNode")
            .field("name", &self.state.name.concise())
            .finish()
    }
}

impl SuiNode {
    pub async fn start(
        config: &NodeConfig,
        registry_service: RegistryService,
        custom_rpc_runtime: Option<Handle>,
    ) -> Result<Arc<SuiNode>> {
        let node_one_cell = Arc::new(AsyncOnceCell::<Arc<SuiNode>>::new());
        Self::start_async(
            config,
            registry_service,
            node_one_cell.clone(),
            custom_rpc_runtime,
        )
        .await?;
        Ok(node_one_cell.get().await)
    }

    pub async fn start_async(
        config: &NodeConfig,
        registry_service: RegistryService,
        node_once_cell: Arc<AsyncOnceCell<Arc<SuiNode>>>,
        custom_rpc_runtime: Option<Handle>,
    ) -> Result<()> {
        let mut config = config.clone();
        if config.supported_protocol_versions.is_none() {
            info!(
                "populating config.supported_protocol_versions with default {:?}",
                SupportedProtocolVersions::SYSTEM_DEFAULT
            );
            config.supported_protocol_versions = Some(SupportedProtocolVersions::SYSTEM_DEFAULT);
        }

        let is_validator = config.consensus_config().is_some();
        let is_full_node = !is_validator;
        let prometheus_registry = registry_service.default_registry();

        info!(node =? config.protocol_public_key(),
            "Initializing sui-node listening on {}", config.network_address
        );

        // Initialize metrics to track db usage before creating any stores
        DBMetrics::init(&prometheus_registry);
        mysten_metrics::init_metrics(&prometheus_registry);

        let genesis = config.genesis()?;

        let secret = Arc::pin(config.protocol_key_pair().copy());
        let genesis_committee = genesis.committee()?;
        let committee_store = Arc::new(CommitteeStore::new(
            config.db_path().join("epochs"),
            &genesis_committee,
            None,
        ));

        let perpetual_options = default_db_options().optimize_db_for_write_throughput(4);
        let store = AuthorityStore::open(
            &config.db_path().join("store"),
            Some(perpetual_options.options),
            genesis,
            &committee_store,
            config.indirect_objects_threshold,
            config
                .expensive_safety_check_config
                .enable_epoch_sui_conservation_check(),
            &prometheus_registry,
        )
        .await?;
        let cur_epoch = store.get_recovery_epoch_at_restart()?;
        let committee = committee_store
            .get_committee(&cur_epoch)?
            .expect("Committee of the current epoch must exist");
        let epoch_start_configuration = store
            .get_epoch_start_configuration()?
            .expect("EpochStartConfiguration of the current epoch must exist");
        let cache_metrics = Arc::new(ResolverMetrics::new(&prometheus_registry));
        let signature_verifier_metrics = SignatureVerifierMetrics::new(&prometheus_registry);

        let epoch_options = default_db_options().optimize_db_for_write_throughput(4);
        let epoch_store = AuthorityPerEpochStore::new(
            config.protocol_public_key(),
            committee.clone(),
            &config.db_path().join("store"),
            Some(epoch_options.options),
            EpochMetrics::new(&registry_service.default_registry()),
            epoch_start_configuration,
            store.clone(),
            cache_metrics,
            signature_verifier_metrics,
            &config.expensive_safety_check_config,
        );

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

        let checkpoint_store = CheckpointStore::new(&config.db_path().join("checkpoints"));
        checkpoint_store.insert_genesis_checkpoint(
            genesis.checkpoint(),
            genesis.checkpoint_contents().clone(),
            &epoch_store,
        );
        let state_sync_store = RocksDbStore::new(
            store.clone(),
            committee_store.clone(),
            checkpoint_store.clone(),
        );

        let index_store = if is_full_node && config.enable_index_processing {
            Some(Arc::new(IndexStore::new(
                config.db_path().join("indexes"),
                &prometheus_registry,
                epoch_store
                    .protocol_config()
                    .max_move_identifier_len_as_option(),
            )))
        } else {
            None
        };

        // Create network
        // TODO only configure validators as seed/preferred peers for validators and not for
        // fullnodes once we've had a chance to re-work fullnode configuration generation.
        let (trusted_peer_change_tx, trusted_peer_change_rx) = watch::channel(Default::default());
        let (p2p_network, discovery_handle, state_sync_handle) = Self::create_p2p_network(
            &config,
            state_sync_store,
            trusted_peer_change_rx,
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

        let db_checkpoint_config = if config.db_checkpoint_config.checkpoint_path.is_none() {
            DBCheckpointConfig {
                checkpoint_path: Some(config.db_checkpoint_path()),
                ..config.db_checkpoint_config.clone()
            }
        } else {
            config.db_checkpoint_config.clone()
        };

        let db_checkpoint_handle = match db_checkpoint_config
            .checkpoint_path
            .as_ref()
            .zip(db_checkpoint_config.object_store_config.as_ref())
        {
            Some((path, object_store_config)) => {
                let handler = DBCheckpointHandler::new(
                    path,
                    object_store_config,
                    60,
                    db_checkpoint_config
                        .prune_and_compact_before_upload
                        .unwrap_or(true),
                    config.indirect_objects_threshold,
                )?;
                Some(handler.start())
            }
            None => None,
        };

        let state = AuthorityState::new(
            config.protocol_public_key(),
            secret,
            config.supported_protocol_versions.unwrap(),
            store.clone(),
            epoch_store.clone(),
            committee_store.clone(),
            index_store.clone(),
            checkpoint_store.clone(),
            &prometheus_registry,
            config.authority_store_pruning_config,
            genesis.objects(),
            &db_checkpoint_config,
            config.expensive_safety_check_config.clone(),
            config.transaction_deny_config.clone(),
            config.indirect_objects_threshold,
        )
        .await;
        // ensure genesis txn was executed
        if epoch_store.epoch() == 0 {
            let txn = &genesis.transaction();
            let span = error_span!("genesis_txn", tx_digest = ?txn.digest());
            let transaction = sui_types::messages::VerifiedExecutableTransaction::new_unchecked(
                sui_types::messages::ExecutableTransaction::new_from_data_and_sig(
                    genesis.transaction().data().clone(),
                    sui_types::certificate_proof::CertificateProof::Checkpoint(0, 0),
                ),
            );
            state
                .try_execute_immediately(&transaction, None, &epoch_store)
                .instrument(span)
                .await
                .unwrap();
        }

        let (end_of_epoch_channel, end_of_epoch_receiver) =
            broadcast::channel(config.end_of_epoch_broadcast_channel_capacity);

        let transaction_orchestrator = if is_full_node {
            Some(Arc::new(
                TransactiondOrchestrator::new_with_network_clients(
                    state.clone(),
                    end_of_epoch_receiver,
                    &config.db_path(),
                    &prometheus_registry,
                )
                .await?,
            ))
        } else {
            None
        };

        let json_rpc_service = build_server(
            state.clone(),
            &transaction_orchestrator.clone(),
            &config,
            &prometheus_registry,
            custom_rpc_runtime,
        )
        .await?;

        let accumulator = Arc::new(StateAccumulator::new(store));

        let authority_names_to_peer_ids = epoch_store
            .epoch_start_state()
            .get_authority_names_to_peer_ids();

        let authority_names_to_hostnames = epoch_store
            .epoch_start_state()
            .get_authority_names_to_hostnames();

        let network_connection_metrics =
            NetworkConnectionMetrics::new("sui", &registry_service.default_registry());

        let authority_names_to_peer_ids = ArcSwap::from_pointee(authority_names_to_peer_ids);

        let (_connection_monitor_handle, connection_statuses) =
            narwhal_network::connectivity::ConnectionMonitor::spawn(
                p2p_network.downgrade(),
                network_connection_metrics,
                HashMap::new(),
                None,
            );

        let connection_monitor_status = ConnectionMonitorStatus {
            connection_statuses,
            authority_names_to_peer_ids,
        };

        let connection_monitor_status = Arc::new(connection_monitor_status);

        let validator_components = if state.is_validator(&epoch_store) {
            let components = Self::construct_validator_components(
                &config,
                state.clone(),
                committee,
                epoch_store.clone(),
                checkpoint_store.clone(),
                state_sync_handle.clone(),
                accumulator.clone(),
                connection_monitor_status.clone(),
                authority_names_to_hostnames,
                &registry_service,
            )
            .await?;
            // This is only needed during cold start.
            components.consensus_adapter.submit_recovered(&epoch_store);

            Some(components)
        } else {
            None
        };

        let node = Self {
            config,
            validator_components: Mutex::new(validator_components),
            _json_rpc_service: json_rpc_service,
            state,
            transaction_orchestrator,
            registry_service,

            _discovery: discovery_handle,
            state_sync: state_sync_handle,
            checkpoint_store,
            accumulator,
            end_of_epoch_channel,
            connection_monitor_status,
            trusted_peer_change_tx,

            _db_checkpoint_handle: db_checkpoint_handle,
            #[cfg(msim)]
            sim_node: sui_simulator::runtime::NodeHandle::current(),
            #[cfg(msim)]
            sim_safe_mode_expected: AtomicBool::new(false),
        };

        info!("SuiNode started!");
        let node = Arc::new(node);
        let node_copy = node.clone();
        spawn_monitored_task!(async move { Self::monitor_reconfiguration(node_copy).await });

        node_once_cell
            .set(node)
            .expect("Failed to set Arc<Node> in node_once_cell");
        Ok(())
    }

    pub fn subscribe_to_epoch_change(&self) -> broadcast::Receiver<SuiSystemState> {
        self.end_of_epoch_channel.subscribe()
    }

    #[cfg(msim)]
    pub fn set_safe_mode_expected(&self, new_value: bool) {
        info!("Setting safe mode expected to {}", new_value);
        self.sim_safe_mode_expected
            .store(new_value, Ordering::Relaxed);
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

    pub fn is_transaction_executed_in_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<bool> {
        self.state
            .database
            .is_transaction_executed_in_checkpoint(digest)
    }

    fn create_p2p_network(
        config: &NodeConfig,
        state_sync_store: RocksDbStore,
        trusted_peer_change_rx: watch::Receiver<TrustedPeerChangeEvent>,
        prometheus_registry: &Registry,
    ) -> Result<(Network, discovery::Handle, state_sync::Handle)> {
        let (state_sync, state_sync_server) = state_sync::Builder::new()
            .config(config.p2p_config.state_sync.clone().unwrap_or_default())
            .store(state_sync_store)
            .with_metrics(prometheus_registry)
            .build();

        let (discovery, discovery_server) = discovery::Builder::new(trusted_peer_change_rx)
            .config(config.p2p_config.clone())
            .build();

        let p2p_network = {
            let routes = anemo::Router::new()
                .add_rpc_service(discovery_server)
                .add_rpc_service(state_sync_server);

            let inbound_network_metrics =
                NetworkMetrics::new("sui", "inbound", prometheus_registry);
            let outbound_network_metrics =
                NetworkMetrics::new("sui", "outbound", prometheus_registry);

            let service = ServiceBuilder::new()
                .layer(
                    TraceLayer::new_for_server_errors()
                        .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO))
                        .on_failure(DefaultOnFailure::new().level(tracing::Level::WARN)),
                )
                .layer(CallbackLayer::new(MetricsMakeCallbackHandler::new(
                    Arc::new(inbound_network_metrics),
                    config.p2p_config.excessive_message_size(),
                )))
                .service(routes);

            let outbound_layer = ServiceBuilder::new()
                .layer(
                    TraceLayer::new_for_client_and_server_errors()
                        .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO))
                        .on_failure(DefaultOnFailure::new().level(tracing::Level::WARN)),
                )
                .layer(CallbackLayer::new(MetricsMakeCallbackHandler::new(
                    Arc::new(outbound_network_metrics),
                    config.p2p_config.excessive_message_size(),
                )))
                .into_inner();

            let mut anemo_config = config.p2p_config.anemo_config.clone().unwrap_or_default();
            // Set the max_frame_size to be 2 GB to work around the issue of there being too many
            // staking events in the epoch change txn.
            anemo_config.max_frame_size = Some(2 << 30);

            let network = Network::bind(config.p2p_config.listen_address)
                .server_name("sui")
                .private_key(config.network_key_pair().copy().private().0.to_bytes())
                .config(anemo_config)
                .outbound_request_layer(outbound_layer)
                .start(service)?;
            info!("P2p network started on {}", network.local_addr());

            network
        };

        let discovery_handle = discovery.start(p2p_network.clone());
        let state_sync_handle = state_sync.start(p2p_network.clone());

        Ok((p2p_network, discovery_handle, state_sync_handle))
    }

    async fn construct_validator_components(
        config: &NodeConfig,
        state: Arc<AuthorityState>,
        committee: Arc<Committee>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        checkpoint_store: Arc<CheckpointStore>,
        state_sync_handle: state_sync::Handle,
        accumulator: Arc<StateAccumulator>,
        connection_monitor_status: Arc<ConnectionMonitorStatus>,
        authority_names_to_hostnames: HashMap<AuthorityName, String>,
        registry_service: &RegistryService,
    ) -> Result<ValidatorComponents> {
        let consensus_config = config
            .consensus_config()
            .ok_or_else(|| anyhow!("Validator is missing consensus config"))?;

        let consensus_adapter = Arc::new(Self::construct_consensus_adapter(
            &committee,
            consensus_config,
            state.name,
            connection_monitor_status.clone(),
            &registry_service.default_registry(),
        ));
        let narwhal_manager =
            Self::construct_narwhal_manager(config, consensus_config, registry_service)?;

        let mut narwhal_epoch_data_remover =
            EpochDataRemover::new(narwhal_manager.get_storage_base_path());

        // This only gets started up once, not on every epoch. (Make call to remove every epoch.)
        narwhal_epoch_data_remover.run().await;

        let checkpoint_metrics = CheckpointMetrics::new(&registry_service.default_registry());
        let sui_tx_validator_metrics =
            SuiTxValidatorMetrics::new(&registry_service.default_registry());

        let validator_server_handle = Self::start_grpc_validator_service(
            config,
            state.clone(),
            consensus_adapter.clone(),
            &registry_service.default_registry(),
        )
        .await?;

        Self::start_epoch_specific_validator_components(
            config,
            state.clone(),
            consensus_adapter,
            checkpoint_store,
            epoch_store,
            state_sync_handle,
            narwhal_manager,
            narwhal_epoch_data_remover,
            accumulator,
            authority_names_to_hostnames,
            validator_server_handle,
            checkpoint_metrics,
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
        narwhal_manager: NarwhalManager,
        narwhal_epoch_data_remover: EpochDataRemover,
        accumulator: Arc<StateAccumulator>,
        authority_names_to_hostnames: HashMap<AuthorityName, String>,
        validator_server_handle: JoinHandle<Result<()>>,
        checkpoint_metrics: Arc<CheckpointMetrics>,
        sui_tx_validator_metrics: Arc<SuiTxValidatorMetrics>,
    ) -> Result<ValidatorComponents> {
        let (checkpoint_service, checkpoint_service_exit) = Self::start_checkpoint_service(
            config,
            consensus_adapter.clone(),
            checkpoint_store,
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

        let new_epoch_start_state = epoch_store.epoch_start_state();
        let committee = new_epoch_start_state.get_narwhal_committee();

        let consensus_handler = Arc::new(ConsensusHandler::new(
            epoch_store.clone(),
            checkpoint_service.clone(),
            state.transaction_manager().clone(),
            state.db(),
            low_scoring_authorities,
            authority_names_to_hostnames,
            committee,
            state.metrics.clone(),
        ));

        let transactions_addr = &config
            .consensus_config
            .as_ref()
            .ok_or_else(|| anyhow!("Validator is missing consensus config"))?
            .address;
        let worker_cache = new_epoch_start_state.get_narwhal_worker_cache(transactions_addr);

        narwhal_manager
            .start(
                new_epoch_start_state.get_narwhal_committee(),
                worker_cache,
                consensus_handler,
                SuiTxValidator::new(
                    epoch_store,
                    state.transaction_manager().clone(),
                    sui_tx_validator_metrics.clone(),
                ),
            )
            .await;

        Ok(ValidatorComponents {
            validator_server_handle,
            narwhal_manager,
            narwhal_epoch_data_remover,
            consensus_adapter,
            checkpoint_service_exit,
            checkpoint_metrics,
            sui_tx_validator_metrics,
        })
    }

    fn start_checkpoint_service(
        config: &NodeConfig,
        consensus_adapter: Arc<ConsensusAdapter>,
        checkpoint_store: Arc<CheckpointStore>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        state: Arc<AuthorityState>,
        state_sync_handle: state_sync::Handle,
        accumulator: Arc<StateAccumulator>,
        checkpoint_metrics: Arc<CheckpointMetrics>,
    ) -> (Arc<CheckpointService>, watch::Sender<()>) {
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

        CheckpointService::spawn(
            state.clone(),
            checkpoint_store,
            epoch_store,
            Box::new(state.db()),
            accumulator,
            checkpoint_output,
            Box::new(certified_checkpoint_output),
            checkpoint_metrics,
            max_tx_per_checkpoint,
            max_checkpoint_size_bytes,
        )
    }

    fn construct_narwhal_manager(
        config: &NodeConfig,
        consensus_config: &ConsensusConfig,
        registry_service: &RegistryService,
    ) -> Result<NarwhalManager> {
        let narwhal_config = NarwhalConfiguration {
            primary_keypair: config.protocol_key_pair().copy(),
            network_keypair: config.network_key_pair().copy(),
            worker_ids_and_keypairs: vec![(0, config.worker_key_pair().copy())],
            storage_base_path: consensus_config.db_path().to_path_buf(),
            parameters: consensus_config.narwhal_config().to_owned(),
            registry_service: registry_service.clone(),
        };

        let metrics = NarwhalManagerMetrics::new(&registry_service.default_registry());

        Ok(NarwhalManager::new(narwhal_config, metrics))
    }

    fn construct_consensus_adapter(
        committee: &Committee,
        consensus_config: &ConsensusConfig,
        authority: AuthorityName,
        connection_monitor_status: Arc<ConnectionMonitorStatus>,
        prometheus_registry: &Registry,
    ) -> ConsensusAdapter {
        let ca_metrics = ConsensusAdapterMetrics::new(prometheus_registry);
        // The consensus adapter allows the authority to send user certificates through consensus.

        ConsensusAdapter::new(
            Box::new(LazyNarwhalClient::new(
                consensus_config.address().to_owned(),
            )),
            authority,
            Box::new(connection_monitor_status),
            consensus_config.max_pending_transactions(),
            consensus_config.max_pending_transactions() * 2 / committee.num_members(),
            ca_metrics,
        )
    }

    async fn start_grpc_validator_service(
        config: &NodeConfig,
        state: Arc<AuthorityState>,
        consensus_adapter: Arc<ConsensusAdapter>,
        prometheus_registry: &Registry,
    ) -> Result<tokio::task::JoinHandle<Result<()>>> {
        let validator_service =
            ValidatorService::new(state.clone(), consensus_adapter, prometheus_registry).await?;

        let mut server_conf = mysten_network::config::Config::new();
        server_conf.global_concurrency_limit = config.grpc_concurrency_limit;
        server_conf.load_shed = config.grpc_load_shed;
        let mut server_builder =
            ServerBuilder::from_config(&server_conf, GrpcMetrics::new(prometheus_registry));

        server_builder = server_builder.add_service(ValidatorServer::new(validator_service));

        let server = server_builder
            .bind(config.network_address())
            .await
            .map_err(|err| anyhow!(err.to_string()))?;
        let local_addr = server.local_addr();
        info!("Listening to traffic on {local_addr}");
        let grpc_server = spawn_monitored_task!(server.serve().map_err(Into::into));

        Ok(grpc_server)
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

    pub fn clone_authority_store(&self) -> Arc<AuthorityStore> {
        self.state.db()
    }

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
        let mut checkpoint_executor = CheckpointExecutor::new(
            self.state_sync.subscribe_to_synced_checkpoints(),
            self.checkpoint_store.clone(),
            self.state.database.clone(),
            self.state.transaction_manager().clone(),
            self.accumulator.clone(),
            self.config.checkpoint_executor_config.clone(),
            &self.registry_service.default_registry(),
        );

        loop {
            let cur_epoch_store = self.state.load_epoch_store_one_call_per_task();

            // Advertise capabilities to committee, if we are a validator.
            if let Some(components) = &*self.validator_components.lock().await {
                // TODO: without this sleep, the consensus message is not delivered reliably.
                tokio::time::sleep(Duration::from_millis(1)).await;

                let config = cur_epoch_store.protocol_config();
                let max_binary_format_version = config.move_binary_format_version();
                let no_extraneous_module_bytes = config.no_extraneous_module_bytes();
                let transaction =
                    ConsensusTransaction::new_capability_notification(AuthorityCapabilities::new(
                        self.state.name,
                        self.config
                            .supported_protocol_versions
                            .expect("Supported versions should be populated"),
                        self.state
                            .get_available_system_packages(
                                max_binary_format_version,
                                no_extraneous_module_bytes,
                            )
                            .await,
                    ));
                info!(?transaction, "submitting capabilities to consensus");
                components
                    .consensus_adapter
                    .submit(transaction, None, &cur_epoch_store)?;
            }

            checkpoint_executor.run_epoch(cur_epoch_store.clone()).await;
            let latest_system_state = self
                .state
                .get_sui_system_state_object_during_reconfig()
                .expect("Read Sui System State object cannot fail");

            #[cfg(msim)]
            if !self.sim_safe_mode_expected.load(Ordering::Relaxed) {
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

            let authority_names_to_hostnames =
                new_epoch_start_state.get_authority_names_to_hostnames();

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
                narwhal_manager,
                narwhal_epoch_data_remover,
                consensus_adapter,
                checkpoint_service_exit,
                checkpoint_metrics,
                sui_tx_validator_metrics,
            }) = self.validator_components.lock().await.take()
            {
                info!("Reconfiguring the validator.");
                // Stop the old checkpoint service.
                drop(checkpoint_service_exit);

                narwhal_manager.shutdown().await;

                let new_epoch_store = self
                    .reconfigure_state(
                        &cur_epoch_store,
                        next_epoch_committee.clone(),
                        new_epoch_start_state,
                        &checkpoint_executor,
                    )
                    .await;

                narwhal_epoch_data_remover
                    .remove_old_data(next_epoch - 1)
                    .await;

                if self.state.is_validator(&new_epoch_store) {
                    // Only restart Narwhal if this node is still a validator in the new epoch.
                    Some(
                        Self::start_epoch_specific_validator_components(
                            &self.config,
                            self.state.clone(),
                            consensus_adapter,
                            self.checkpoint_store.clone(),
                            new_epoch_store.clone(),
                            self.state_sync.clone(),
                            narwhal_manager,
                            narwhal_epoch_data_remover,
                            self.accumulator.clone(),
                            authority_names_to_hostnames,
                            validator_server_handle,
                            checkpoint_metrics,
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
                        &cur_epoch_store,
                        next_epoch_committee.clone(),
                        new_epoch_start_state,
                        &checkpoint_executor,
                    )
                    .await;

                if self.state.is_validator(&new_epoch_store) {
                    info!("Promoting the node from fullnode to validator, starting grpc server");

                    Some(
                        Self::construct_validator_components(
                            &self.config,
                            self.state.clone(),
                            Arc::new(next_epoch_committee.clone()),
                            new_epoch_store.clone(),
                            self.checkpoint_store.clone(),
                            self.state_sync.clone(),
                            self.accumulator.clone(),
                            self.connection_monitor_status.clone(),
                            authority_names_to_hostnames,
                            &self.registry_service,
                        )
                        .await?,
                    )
                } else {
                    None
                }
            };
            *self.validator_components.lock().await = new_validator_components;
            info!("Reconfiguration finished");
        }
    }

    async fn reconfigure_state(
        &self,
        cur_epoch_store: &AuthorityPerEpochStore,
        next_epoch_committee: Committee,
        next_epoch_start_system_state: EpochStartSystemState,
        checkpoint_executor: &CheckpointExecutor,
    ) -> Arc<AuthorityPerEpochStore> {
        let next_epoch = next_epoch_committee.epoch();

        let last_checkpoint = self
            .checkpoint_store
            .get_epoch_last_checkpoint(cur_epoch_store.epoch())
            .expect("Error loading last checkpoint for current epoch")
            .expect("Could not load last checkpoint for current epoch");
        let epoch_start_configuration =
            EpochStartConfiguration::new(next_epoch_start_system_state, *last_checkpoint.digest());

        let new_epoch_store = self
            .state
            .reconfigure(
                cur_epoch_store,
                self.config.supported_protocol_versions.unwrap(),
                next_epoch_committee,
                epoch_start_configuration,
                checkpoint_executor,
                self.accumulator.clone(),
                &self.config.expensive_safety_check_config,
            )
            .await
            .expect("Reconfigure authority state cannot fail");
        info!(next_epoch, "Node State has been reconfigured");
        assert_eq!(next_epoch, new_epoch_store.epoch());
        self.state.database.update_epoch_flags_metrics(
            cur_epoch_store.epoch_start_config().flags(),
            new_epoch_store.epoch_start_config().flags(),
        );
        new_epoch_store
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

pub async fn build_server(
    state: Arc<AuthorityState>,
    transaction_orchestrator: &Option<Arc<TransactiondOrchestrator<NetworkAuthorityClient>>>,
    config: &NodeConfig,
    prometheus_registry: &Registry,
    custom_runtime: Option<Handle>,
) -> Result<Option<ServerHandle>> {
    // Validators do not expose these APIs
    if config.consensus_config().is_some() {
        return Ok(None);
    }

    let mut server = JsonRpcServerBuilder::new(env!("CARGO_PKG_VERSION"), prometheus_registry);
    let metrics = Arc::new(JsonRpcMetrics::new(prometheus_registry));
    server.register_module(ReadApi::new(state.clone(), metrics.clone()))?;
    server.register_module(CoinReadApi::new(state.clone(), metrics.clone()))?;
    server.register_module(TransactionBuilderApi::new(state.clone()))?;
    server.register_module(GovernanceReadApi::new(state.clone(), metrics.clone()))?;

    if let Some(transaction_orchestrator) = transaction_orchestrator {
        server.register_module(TransactionExecutionApi::new(
            state.clone(),
            transaction_orchestrator.clone(),
            metrics.clone(),
        ))?;
    }

    server.register_module(IndexerApi::new(
        state.clone(),
        ReadApi::new(state.clone(), metrics.clone()),
        config.name_service_resolver_object_id,
        metrics.clone(),
    ))?;
    server.register_module(MoveUtils::new(state.clone()))?;

    let rpc_server_handle = server
        .start(config.json_rpc_address, custom_runtime)
        .await?;

    Ok(Some(rpc_server_handle))
}

#[cfg(not(test))]
fn max_tx_per_checkpoint(protocol_config: &ProtocolConfig) -> usize {
    protocol_config.max_transactions_per_checkpoint() as usize
}

#[cfg(test)]
fn max_tx_per_checkpoint(_: &ProtocolConfig) -> usize {
    2
}
