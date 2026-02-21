// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    net::SocketAddr,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context as _, Result, anyhow};
use axum::{
    Json, Router,
    extract::State,
    response::IntoResponse,
    routing::{get, post},
};
use prometheus::Registry;
use rand::rngs::OsRng;
use serde::Deserialize;
use tokio::sync::{RwLock, oneshot};
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

use simulacrum::{AdvanceEpochConfig, Simulacrum, store::in_mem_store::KeyStore};
use sui_data_store::{
    CheckpointStore as _, FullCheckpointData, ObjectKey, SetupStore as _, VersionQuery,
    stores::{DataStore, FileSystemStore, ReadThroughStore},
};
use sui_futures::service::Service;
use sui_rpc::proto::sui::rpc::v2::{
    ledger_service_server::LedgerServiceServer,
    subscription_service_server::SubscriptionServiceServer,
};
use sui_rpc::proto::sui::rpc::v2::{
    state_service_server::StateServiceServer,
    transaction_execution_service_server::TransactionExecutionServiceServer,
};
use sui_rpc_api::subscription::SubscriptionService as RpcSubscriptionService;
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::{
    accumulator_root::get_accumulator_root_obj_initial_shared_version,
    base_types::SuiAddress,
    digests::{
        ChainIdentifier, CheckpointDigest, get_mainnet_chain_identifier,
        get_testnet_chain_identifier,
    },
    effects::TransactionEffects,
    gas_coin::MIST_PER_SUI,
    message_envelope::Envelope,
    messages_checkpoint::VerifiedCheckpoint,
    object::Object,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    sui_system_state::{SuiSystemState, SuiSystemStateTrait},
    supported_protocol_versions::Chain::{self},
    transaction::{GasData, TransactionData, TransactionKind},
};

use crate::grpc::{
    RpcArgs as GrpcArgs, RpcService as GrpcRpcService, TlsArgs as GrpcTlsArgs,
    ledger_service::ForkingLedgerService, state_service::ForkingStateService,
    subscription_service::ForkingSubscriptionService,
    transaction_execution_service::ForkingTransactionExecutionService,
};
use crate::{
    api::types::{AdvanceClockRequest, ApiResponse, ExecuteTxResponse, ForkingStatus},
    graphql::GraphQLClient,
    network::ForkNetwork,
    seeds::StartupSeeds,
    store::ForkingStore,
};

/// The shared state for the forking server
pub struct AppState {
    pub context: crate::context::Context,
}

impl AppState {
    pub async fn new(context: crate::context::Context) -> Self {
        Self { context }
    }
}

struct InitializedSimulacrum {
    simulacrum: Simulacrum<OsRng, ForkingStore>,
    faucet_owner: Option<SuiAddress>,
}

async fn health() -> &'static str {
    "OK"
}

async fn get_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let sim = state.context.simulacrum.read().await;
    let store = sim.store();

    let checkpoint = store
        .get_highest_checkpint()
        .map(|c| c.sequence_number)
        .unwrap_or(0);

    // Get the current epoch from the checkpoint
    let epoch = store
        .get_highest_checkpint()
        .map(|c| c.epoch())
        .unwrap_or(0);

    let clock_timestamp_ms = store.get_clock().timestamp_ms();

    let status = ForkingStatus {
        checkpoint,
        epoch,
        clock_timestamp_ms,
    };

    Json(ApiResponse {
        success: true,
        data: Some(status),
        error: None,
    })
}

async fn advance_checkpoint(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let checkpoint_sequence_number = {
        let mut sim = state.context.simulacrum.write().await;

        // create_checkpoint returns a VerifiedCheckpoint, not a Result
        let checkpoint = sim.create_checkpoint();
        info!("Advanced to checkpoint {}", checkpoint.sequence_number);
        checkpoint.sequence_number
    };

    if let Err(err) = state
        .context
        .publish_checkpoint_by_sequence_number(checkpoint_sequence_number)
        .await
    {
        warn!(
            checkpoint_sequence_number,
            "Failed to publish checkpoint to subscribers: {err}"
        );
    }

    Json(ApiResponse::<String> {
        success: true,
        data: Some(format!(
            "Advanced to checkpoint {}",
            checkpoint_sequence_number
        )),
        error: None,
    })
}

async fn advance_clock(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AdvanceClockRequest>,
) -> impl IntoResponse {
    let mut sim = state.context.simulacrum.write().await;

    let duration = Duration::from_millis(request.ms);
    sim.advance_clock(duration);
    info!("Advanced clock by {} ms", request.ms);

    Json(ApiResponse::<String> {
        success: true,
        data: Some(format!("Clock advanced by {} ms", request.ms)),
        error: None,
    })
}

async fn advance_epoch(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let latest_checkpoint_sequence_number = {
        let mut sim = state.context.simulacrum.write().await;

        // Use default configuration for advancing epoch
        let config = AdvanceEpochConfig::default();
        sim.advance_epoch(config);
        info!("Advanced to next epoch");
        sim.store()
            .get_highest_checkpint()
            .map(|cp| cp.sequence_number)
    };

    if let Some(checkpoint_sequence_number) = latest_checkpoint_sequence_number
        && let Err(err) = state
            .context
            .publish_checkpoint_by_sequence_number(checkpoint_sequence_number)
            .await
    {
        warn!(
            checkpoint_sequence_number,
            "Failed to publish checkpoint to subscribers after epoch advance: {err}"
        );
    }

    Json(ApiResponse::<String> {
        success: true,
        data: Some("Advanced to next epoch".to_string()),
        error: None,
    })
}

#[derive(Deserialize)]
struct FaucetRequest {
    address: SuiAddress,
    amount: u64,
}

async fn faucet(
    State(state): State<Arc<AppState>>,
    Json(request): Json<FaucetRequest>,
) -> impl IntoResponse {
    let FaucetRequest { address, amount } = request;
    let Some(faucet_owner) = state.context.faucet_owner else {
        return Json(ApiResponse::<ExecuteTxResponse> {
            success: false,
            data: None,
            error: Some(
                "Faucet is unavailable: no local faucet owner was configured at startup"
                    .to_string(),
            ),
        });
    };

    let mut simulacrum = state.context.simulacrum.write().await;
    let response = execute_faucet_transfer(&mut simulacrum, faucet_owner, address, amount);

    match response {
        Ok(effects) => {
            let effects_bytes = bcs::to_bytes(&effects).unwrap();
            let effects_base64 =
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &effects_bytes);

            info!("Executed transaction successfully");

            Json(ApiResponse {
                success: true,
                data: Some(ExecuteTxResponse {
                    effects: effects_base64,
                    error: None,
                }),
                error: None,
            })
        }
        Err(err) => Json(ApiResponse::<ExecuteTxResponse> {
            success: false,
            data: None,
            error: Some(format!("Failed to execute faucet transfer: {}", err)),
        }),
    }
}

fn execute_faucet_transfer(
    simulacrum: &mut Simulacrum<OsRng, ForkingStore>,
    faucet_owner: SuiAddress,
    recipient: SuiAddress,
    amount: u64,
) -> Result<TransactionEffects, anyhow::Error> {
    let required_balance = amount.saturating_add(MIST_PER_SUI);
    let Some(faucet_coin) = simulacrum
        .store()
        .owned_objects(faucet_owner)
        .filter(|object| object.is_gas_coin() && object.get_coin_value_unsafe() >= required_balance)
        .max_by_key(|object| object.get_coin_value_unsafe())
    else {
        anyhow::bail!(
            "No faucet coin with enough balance for {} Mist (required balance >= {})",
            amount,
            required_balance
        );
    };

    let programmable_tx = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.transfer_sui(recipient, Some(amount));
        builder.finish()
    };
    let kind = TransactionKind::ProgrammableTransaction(programmable_tx);
    let gas_data = GasData {
        payment: vec![faucet_coin.compute_object_reference()],
        owner: faucet_owner,
        price: simulacrum.reference_gas_price(),
        budget: MIST_PER_SUI,
    };
    let tx_data = TransactionData::new_with_gas_data(kind, faucet_owner, gas_data);
    let (effects, execution_error) = simulacrum.execute_transaction_impersonating(tx_data)?;

    if let Some(err) = execution_error {
        anyhow::bail!("faucet transfer execution error: {err:?}");
    }

    Ok(effects)
}

/// Start the forking server with programmatic shutdown/readiness signals.
pub(crate) async fn start_server_with_signals(
    startup_seeds: StartupSeeds,
    fork_network: ForkNetwork,
    checkpoint: Option<u64>,
    fullnode_url: Option<String>,
    host: String,
    server_port: u16,
    rpc_port: u16,
    data_ingestion_path: PathBuf,
    version: &'static str,
    shutdown_receiver: Option<oneshot::Receiver<()>>,
    ready_sender: Option<oneshot::Sender<()>>,
) -> Result<()> {
    let fullnode_endpoint = fork_network
        .resolve_fullnode_endpoint(fullnode_url.as_deref())
        .context("failed to resolve fullnode RPC endpoint")?;
    let client = GraphQLClient::new(fork_network.gql_endpoint().to_string());
    let (forked_at_checkpoint, protocol_version) = if let Some(cp) = checkpoint {
        (cp, client.fetch_protocol_version(Some(cp)).await?)
    } else {
        client
            .fetch_latest_checkpoint_and_protocol_version()
            .await?
    };
    let (fs_store, fs_gql_store) = initialize_data_store(
        &fork_network,
        &fullnode_endpoint,
        forked_at_checkpoint,
        &data_ingestion_path,
        version,
    )?;
    let startup_checkpoint =
        determine_startup_checkpoint(checkpoint, forked_at_checkpoint, &fs_store)?;

    let is_resuming_existing_fork =
        checkpoint.is_some() && startup_checkpoint != forked_at_checkpoint;
    if is_resuming_existing_fork {
        println!(
            "Resuming {} forked network, current checkpoint: {}, forked at checkpoint: {}, protocol version {}",
            fork_network.display_name(),
            startup_checkpoint,
            forked_at_checkpoint,
            protocol_version
        );
    } else {
        println!(
            "Starting from {} at checkpoint {} with protocol version {}",
            fork_network.display_name(),
            forked_at_checkpoint,
            protocol_version
        );
    }

    let InitializedSimulacrum {
        simulacrum,
        faucet_owner,
    } = initialize_simulacrum(
        forked_at_checkpoint,
        startup_checkpoint,
        &client,
        &startup_seeds,
        protocol_version,
        fork_network.protocol_chain(),
        fs_store,
        fs_gql_store,
    )
    .await?;
    let simulacrum = Arc::new(RwLock::new(simulacrum));

    let registry = Registry::new_custom(Some("sui_forking".into()), None)
        .context("Failed to create Prometheus registry.")?;
    let (checkpoint_sender, subscription_service_handle) = RpcSubscriptionService::build(&registry);

    let chain_id = resolve_chain_identifier(&fork_network, &client).await?;
    let context = crate::context::Context {
        simulacrum: simulacrum.clone(),
        subscription_service_handle,
        checkpoint_sender,
        chain_id,
        faucet_owner,
    };

    let grpc = start_grpc_services(context.clone(), version, &registry, rpc_port).await?;
    let grpc_handle = tokio::spawn(grpc.main());

    let state = Arc::new(AppState::new(context.clone()).await);

    let app = Router::new()
        .route("/health", get(health))
        .route("/status", get(get_status))
        .route("/advance-checkpoint", post(advance_checkpoint))
        .route("/advance-clock", post(advance_clock))
        .route("/advance-epoch", post(advance_epoch))
        .route("/faucet", post(faucet))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, server_port).parse()?;
    println!("Forking server listening on {}", addr);
    println!("Ready to accept requests");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    if let Some(ready_sender) = ready_sender {
        let _ = ready_sender.send(());
    }

    let shutdown_signal = async move {
        match shutdown_receiver {
            Some(receiver) => {
                let ctrl_c = async {
                    match tokio::signal::ctrl_c().await {
                        Ok(()) => {
                            info!("\nReceived CTRL+C, shutting down gracefully...");
                        }
                        Err(err) => {
                            warn!("Failed to install CTRL+C signal handler: {err}");
                            std::future::pending::<()>().await;
                        }
                    }
                };

                tokio::select! {
                    _ = receiver => {
                        info!("received programmatic shutdown signal");
                    }
                    _ = ctrl_c => {
                    }
                }
            }
            None => {
                if let Err(err) = tokio::signal::ctrl_c().await {
                    warn!("Failed to install CTRL+C signal handler: {err}");
                    return;
                }
                info!("\nReceived CTRL+C, shutting down gracefully...");
            }
        }
    };

    // Serve with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    // Abort the spawned tasks when the server shuts down
    grpc_handle.abort();

    info!("Server shutdown complete");

    Ok(())
}

async fn resolve_chain_identifier(
    fork_network: &ForkNetwork,
    client: &GraphQLClient,
) -> Result<ChainIdentifier> {
    match fork_network.protocol_chain() {
        Chain::Mainnet => Ok(get_mainnet_chain_identifier()),
        Chain::Testnet => Ok(get_testnet_chain_identifier()),
        Chain::Unknown => {
            let chain_identifier = client.fetch_chain_identifier().await?;
            let digest = CheckpointDigest::from_str(&chain_identifier).with_context(|| {
                format!(
                    "invalid chainIdentifier '{}' returned by {}",
                    chain_identifier,
                    client.endpoint()
                )
            })?;
            Ok(ChainIdentifier::from(digest))
        }
    }
}

/// Start the gRPC services for the forking server
async fn start_grpc_services(
    context: crate::context::Context,
    version: &'static str,
    registry: &Registry,
    rpc_port: u16,
) -> Result<Service, anyhow::Error> {
    let grpc_listen_address = SocketAddr::from(([127, 0, 0, 1], rpc_port));
    println!("RPC listening on {}", grpc_listen_address);

    let grpc_args = GrpcArgs {
        rpc_listen_address: grpc_listen_address,
        tls: GrpcTlsArgs::default(),
    };

    let subscription_service = ForkingSubscriptionService::new(context.clone());
    let ledger_service = ForkingLedgerService::new(context.clone());
    let state_service = ForkingStateService::new(context.clone());
    let tx_execution_service = ForkingTransactionExecutionService::new(context.clone());
    let grpc = GrpcRpcService::new(grpc_args, version, registry)
        .await?
        .register_encoded_file_descriptor_set(sui_rpc::proto::sui::rpc::v2::FILE_DESCRIPTOR_SET)
        .add_service(LedgerServiceServer::new(ledger_service))
        .add_service(SubscriptionServiceServer::new(subscription_service))
        .add_service(StateServiceServer::new(state_service))
        .add_service(TransactionExecutionServiceServer::new(tx_execution_service));
    let handle = grpc.run().await?;
    Ok(handle)
}

fn build_network_config(protocol_version: u64, chain: Chain, rng: &mut OsRng) -> NetworkConfig {
    ConfigBuilder::new_with_temp_dir()
        .rng(rng)
        .with_chain_start_timestamp_ms(0)
        .deterministic_committee_size(NonZeroUsize::MIN)
        .with_protocol_version(protocol_version.into())
        .with_chain_override(chain)
        .build()
}

fn load_startup_checkpoint_data(
    fs_store: &FileSystemStore,
    fs_gql_store: &ReadThroughStore<FileSystemStore, DataStore>,
    startup_checkpoint: u64,
) -> Result<FullCheckpointData, anyhow::Error> {
    match fs_store
        .get_checkpoint_by_sequence_number(startup_checkpoint)
        .context("failed to read startup checkpoint from local checkpoint store")?
    {
        Some(checkpoint) => Ok(checkpoint),
        None => fs_gql_store
            .get_checkpoint_by_sequence_number(startup_checkpoint)
            .context("failed to fetch startup checkpoint from rpc")?
            .with_context(|| format!("checkpoint {startup_checkpoint} not found")),
    }
}

fn build_verified_startup_checkpoint(
    startup_checkpoint_data: &FullCheckpointData,
) -> VerifiedCheckpoint {
    let certified_summary = startup_checkpoint_data.summary.clone();
    let env_checkpoint = Envelope::new_from_data_and_sig(
        certified_summary.data().clone(),
        certified_summary.auth_sig().clone(),
    );
    VerifiedCheckpoint::new_unchecked(env_checkpoint)
}

fn resolve_faucet_owner(keystore: &KeyStore) -> Option<SuiAddress> {
    keystore.accounts().next().map(|(owner, _)| *owner)
}

fn cache_object_if_missing(
    fs_store: &FileSystemStore,
    fs_gql_store: &ReadThroughStore<FileSystemStore, DataStore>,
    object: Object,
) -> Result<bool, anyhow::Error> {
    let object_id = object.id();
    if fs_store
        .get_object_latest(&object_id)
        .with_context(|| format!("failed to inspect local object cache for {object_id}"))?
        .is_some()
    {
        return Ok(false);
    }

    let version: u64 = object.version().into();
    let object_key = ObjectKey {
        object_id,
        version_query: VersionQuery::Version(version),
    };
    fs_gql_store
        .write_objects(vec![(object_key, object, version)])
        .with_context(|| format!("failed to seed local object {object_id}"))?;
    Ok(true)
}

/// Seeds a local faucet gas object from genesis for the configured faucet owner.
fn seed_genesis_faucet_coin(
    fs_store: &FileSystemStore,
    fs_gql_store: &ReadThroughStore<FileSystemStore, DataStore>,
    genesis_objects: &[Object],
    faucet_owner: SuiAddress,
) -> Result<(), anyhow::Error> {
    let Some(faucet_coin) = genesis_objects
        .iter()
        .filter(|object| object.is_gas_coin())
        .filter(|object| object.owner.get_address_owner_address().ok() == Some(faucet_owner))
        .max_by_key(|object| object.get_coin_value_unsafe())
        .cloned()
    else {
        warn!(
            faucet_owner = %faucet_owner,
            "No genesis gas coin found for local faucet owner; faucet may fail until a local gas coin is available"
        );
        return Ok(());
    };

    let faucet_coin_id = faucet_coin.id();
    let faucet_coin_version = faucet_coin.version().value();
    let faucet_coin_balance = faucet_coin.get_coin_value_unsafe();
    if cache_object_if_missing(fs_store, fs_gql_store, faucet_coin)? {
        info!(
            faucet_owner = %faucet_owner,
            faucet_coin_id = %faucet_coin_id,
            faucet_coin_version,
            faucet_coin_balance,
            "Seeded genesis faucet coin into local fork store"
        );
    } else {
        info!(
            faucet_owner = %faucet_owner,
            faucet_coin_id = %faucet_coin_id,
            "Genesis faucet coin already present in local fork store"
        );
    }

    Ok(())
}

fn build_initial_system_state(
    store: &ForkingStore,
    config: &NetworkConfig,
) -> Result<SuiSystemState, anyhow::Error> {
    let system_state = sui_types::sui_system_state::get_sui_system_state(store)
        .map_err(|err| anyhow!("failed to read Sui system state from startup checkpoint: {err}"))?;

    let validators = match config.genesis.sui_system_object() {
        SuiSystemState::V1(genesis_inner) => genesis_inner.validators,
        SuiSystemState::V2(genesis_inner) => genesis_inner.validators,
        #[cfg(msim)]
        _ => anyhow::bail!("unsupported genesis system state variant"),
    };

    match system_state {
        SuiSystemState::V1(mut inner) => {
            inner.validators = validators.clone();
            Ok(SuiSystemState::V1(inner))
        }
        SuiSystemState::V2(mut inner) => {
            inner.validators = validators;
            Ok(SuiSystemState::V2(inner))
        }
        #[cfg(msim)]
        _ => anyhow::bail!("unsupported system state variant"),
    }
}

fn install_validator_override_and_committee(
    store: &mut ForkingStore,
    initial_sui_system_state: &SuiSystemState,
) {
    let validator_set_override = match initial_sui_system_state {
        SuiSystemState::V1(inner) => inner.validators.clone(),
        SuiSystemState::V2(inner) => inner.validators.clone(),
    };
    store.set_system_state_validator_set_override(validator_set_override);

    let initial_committee = initial_sui_system_state
        .get_current_epoch_committee()
        .committee()
        .clone();
    store.insert_committee(initial_committee);
}

fn build_simulacrum_from_bootstrap(
    keystore: KeyStore,
    verified_checkpoint: VerifiedCheckpoint,
    initial_sui_system_state: SuiSystemState,
    config: &NetworkConfig,
    store: ForkingStore,
    rng: OsRng,
) -> Result<Simulacrum<OsRng, ForkingStore>, anyhow::Error> {
    // The mock checkpoint builder relies on the accumulator root object to be present in the store
    // with the correct initial shared version, so we need to fetch it here and pass it to the
    // simulacrum. This is needed if protocol config has enabled accumulators
    // TODO: do we need this if protocol config does not have enabled accumulators?
    let acc_initial_shared_version = get_accumulator_root_obj_initial_shared_version(&store)?
        .ok_or_else(|| anyhow!("Failed to get accumulator root object from store"))?;

    Ok(Simulacrum::new_from_custom_state(
        keystore,
        verified_checkpoint,
        initial_sui_system_state,
        config,
        store,
        rng,
        Some(acc_initial_shared_version),
    ))
}

async fn initialize_simulacrum(
    forked_at_checkpoint: u64,
    startup_checkpoint: u64,
    client: &GraphQLClient,
    startup_seeds: &StartupSeeds,
    protocol_version: u64,
    chain: Chain,
    fs_store: FileSystemStore,
    fs_gql_store: ReadThroughStore<FileSystemStore, DataStore>,
) -> Result<InitializedSimulacrum, anyhow::Error> {
    let mut rng = OsRng;
    let config = build_network_config(protocol_version, chain, &mut rng);
    let keystore = KeyStore::from_network_config(&config);
    let faucet_owner = resolve_faucet_owner(&keystore);

    let startup_checkpoint_data =
        load_startup_checkpoint_data(&fs_store, &fs_gql_store, startup_checkpoint)?;
    let verified_checkpoint = build_verified_startup_checkpoint(&startup_checkpoint_data);

    if let Some(faucet_owner) = faucet_owner {
        if let Err(err) = seed_genesis_faucet_coin(
            &fs_store,
            &fs_gql_store,
            config.genesis.objects(),
            faucet_owner,
        ) {
            warn!(
                faucet_owner = %faucet_owner,
                "Failed to seed genesis faucet coin into local fork store: {err}"
            );
        }
    } else {
        warn!("No local account keys available; faucet will be unavailable");
    }

    let mut store = ForkingStore::new(forked_at_checkpoint, fs_store, fs_gql_store);
    store.insert_checkpoint(verified_checkpoint.clone());
    store.insert_checkpoint_contents(startup_checkpoint_data.contents.clone());
    startup_seeds
        .prefetch_startup_objects(
            &store,
            client.endpoint(),
            startup_checkpoint,
            startup_checkpoint_data.summary.timestamp_ms,
        )
        .await
        .context("Failed to prefetch startup objects")?;

    let initial_sui_system_state = build_initial_system_state(&store, &config)?;
    install_validator_override_and_committee(&mut store, &initial_sui_system_state);

    let simulacrum = build_simulacrum_from_bootstrap(
        keystore,
        verified_checkpoint,
        initial_sui_system_state,
        &config,
        store,
        rng,
    )?;

    Ok(InitializedSimulacrum {
        simulacrum,
        faucet_owner,
    })
}

/// Create the data stores for the forking server, including a file system store for transactions
/// and a read-through store for objects that combines the file system store and the GraphQL RPC
/// store.
fn initialize_data_store(
    fork_network: &ForkNetwork,
    fullnode_endpoint: &str,
    at_checkpoint: u64,
    data_ingestion_path: &Path,
    version: &'static str,
) -> Result<
    (
        FileSystemStore,
        ReadThroughStore<FileSystemStore, DataStore>,
    ),
    anyhow::Error,
> {
    let forking_path = format!(
        "forking/{}/forked_at_checkpoint_{}",
        fork_network.cache_namespace(),
        at_checkpoint
    );

    let node = fork_network.node();

    let fs_base_path = data_ingestion_path.join(forking_path);
    let fs = FileSystemStore::new_with_path(node.clone(), fs_base_path.clone())
        .context("failed to initialize file-system primary cache store")?;
    let gql_rpc_store = DataStore::new_with_endpoints(
        node.clone(),
        fork_network.gql_endpoint(),
        fullnode_endpoint,
        version,
    )
    .context("failed to initialize GraphQL/fullnode data store")?;
    let fs_store = FileSystemStore::new_with_path(node, fs_base_path.clone())
        .context("failed to initialize file-system checkpoint store")?;
    let fs_gql_store = ReadThroughStore::new(fs, gql_rpc_store);

    info!("Fs base path {:?}", fs_base_path.display());
    match fork_network {
        ForkNetwork::Mainnet => {
            fs_store
                .setup(Some(Chain::Mainnet.as_str().to_string()))
                .context("failed to initialize local mainnet node mapping")?;
        }
        ForkNetwork::Testnet => {
            fs_store
                .setup(Some(Chain::Testnet.as_str().to_string()))
                .context("failed to initialize local testnet node mapping")?;
        }
        ForkNetwork::Devnet | ForkNetwork::Custom(_) => {
            let chain_id = fs_gql_store
                .setup(None)
                .context("failed to initialize dynamic chain identifier mapping")?
                .with_context(|| {
                    format!(
                        "missing chain identifier while setting up {} data store",
                        fork_network.display_name()
                    )
                })?;
            info!(
                "Resolved dynamic chain identifier for {}: {}",
                fork_network.display_name(),
                chain_id
            );
        }
    }

    Ok((fs_store, fs_gql_store))
}

fn determine_startup_checkpoint(
    checkpoint: Option<u64>,
    forked_at_checkpoint: u64,
    fs_store: &FileSystemStore,
) -> Result<u64, anyhow::Error> {
    let Some(requested_checkpoint) = checkpoint else {
        return Ok(forked_at_checkpoint);
    };

    let local_latest = fs_store
        .get_latest_checkpoint()
        .context("failed to inspect local checkpoint cache")?;

    match local_latest {
        None => Ok(requested_checkpoint),
        Some(checkpoint_data) => {
            let local_latest_sequence = checkpoint_data.summary.sequence_number;
            if local_latest_sequence < requested_checkpoint {
                anyhow::bail!(
                    "local fork cache for checkpoint {} is stale: latest local checkpoint is {}",
                    requested_checkpoint,
                    local_latest_sequence
                );
            }
            Ok(local_latest_sequence)
        }
    }
}
