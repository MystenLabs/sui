// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{net::SocketAddr, num::NonZeroUsize, path::PathBuf, str::FromStr, sync::Arc};

use anyhow::{Context as _, Result};
use axum::{
    Json, Router,
    extract::State,
    response::IntoResponse,
    routing::{get, post},
};
use prometheus::Registry;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

use simulacrum::{AdvanceEpochConfig, Simulacrum, store::in_mem_store::KeyStore};
use sui_data_store::{
    CheckpointStore as _, SetupStore as _,
    stores::{DataStore, FileSystemStore, ReadThroughStore},
};
use sui_types::{
    accumulator_root::get_accumulator_root_obj_initial_shared_version,
    base_types::SuiAddress,
    digests::{
        ChainIdentifier, CheckpointDigest, get_mainnet_chain_identifier,
        get_testnet_chain_identifier,
    },
    effects::TransactionEffectsAPI,
    message_envelope::Envelope,
    messages_checkpoint::VerifiedCheckpoint,
    sui_system_state::{SuiSystemState, SuiSystemStateTrait},
    supported_protocol_versions::Chain::{self},
    transaction::Transaction,
};

use crate::grpc::{
    RpcArgs as GrpcArgs, RpcService as GrpcRpcService, TlsArgs as GrpcTlsArgs,
    ledger_service::ForkingLedgerService, state_service::ForkingStateService,
    subscription_service::ForkingSubscriptionService,
    transaction_execution_service::ForkingTransactionExecutionService,
};
use crate::{
    graphql::GraphQLClient, network::ForkNetwork, seeds::InitialAccounts, store::ForkingStore,
};

use rand::rngs::OsRng;
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
use sui_swarm_config::network_config_builder::ConfigBuilder;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdvanceClockRequest {
    pub seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecuteTxRequest {
    /// Base64 encoded transaction bytes
    pub tx_bytes: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]

pub struct ExecuteTxResponse {
    /// Base64 encoded transaction effects
    pub effects: String,
    /// Execution error if any
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ForkingStatus {
    pub checkpoint: u64,
    pub epoch: u64,
}

/// The shared state for the forking server
pub struct AppState {
    pub context: crate::context::Context,
}

impl AppState {
    pub async fn new(context: crate::context::Context) -> Self {
        Self { context }
    }
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

    let status = ForkingStatus { checkpoint, epoch };

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

    let duration = std::time::Duration::from_secs(request.seconds);
    sim.advance_clock(duration);
    info!("Advanced clock by {} seconds", request.seconds);

    Json(ApiResponse::<String> {
        success: true,
        data: Some(format!("Clock advanced by {} seconds", request.seconds)),
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

async fn execute_tx(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ExecuteTxRequest>,
) -> impl IntoResponse {
    // Decode the base64 transaction bytes
    let tx_bytes = match base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &request.tx_bytes,
    ) {
        Ok(bytes) => bytes,
        Err(e) => {
            return Json(ApiResponse::<ExecuteTxResponse> {
                success: false,
                data: None,
                error: Some(format!("Failed to decode base64: {}", e)),
            });
        }
    };

    // Deserialize the transaction
    let transaction: Transaction = match bcs::from_bytes(&tx_bytes) {
        Ok(tx) => tx,
        Err(e) => {
            return Json(ApiResponse::<ExecuteTxResponse> {
                success: false,
                data: None,
                error: Some(format!("Failed to deserialize transaction: {}", e)),
            });
        }
    };

    // Execute the transaction
    let mut sim = state.context.simulacrum.write().await;
    match sim.execute_transaction(transaction) {
        Ok((effects, execution_error)) => {
            let effects_bytes = bcs::to_bytes(&effects).unwrap();
            let effects_base64 =
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &effects_bytes);

            let error_str = execution_error.map(|e| format!("{:?}", e));

            info!("Executed transaction successfully");

            Json(ApiResponse {
                success: true,
                data: Some(ExecuteTxResponse {
                    effects: effects_base64,
                    error: error_str,
                }),
                error: None,
            })
        }
        Err(e) => Json(ApiResponse::<ExecuteTxResponse> {
            success: false,
            data: None,
            error: Some(format!("Failed to execute transaction: {}", e)),
        }),
    }
}

#[derive(serde::Deserialize)]
struct FaucetRequest {
    address: SuiAddress,
    amount: u64,
}

async fn faucet(
    State(state): State<Arc<AppState>>,
    Json(request): Json<FaucetRequest>,
) -> impl IntoResponse {
    let FaucetRequest { address, amount } = request;

    let mut simulacrum = state.context.simulacrum.write().await;
    let response = simulacrum.request_gas(address, amount);

    match response {
        Ok(effects) => {
            println!("Effects {:?}", effects.created());
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
        Err(e) => Json(ApiResponse::<ExecuteTxResponse> {
            success: false,
            data: None,
            error: Some(format!("Failed to execute transaction: {}", e)),
        }),
    }
}

/// Start the forking server
pub async fn start_server(
    initial_accounts: InitialAccounts,
    fork_network: ForkNetwork,
    checkpoint: Option<u64>,
    fullnode_url: Option<String>,
    host: String,
    server_port: u16,
    rpc_port: u16,
    _data_ingestion_path: PathBuf,
    version: &'static str,
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

    let simulacrum = initialize_simulacrum(
        forked_at_checkpoint,
        startup_checkpoint,
        &client,
        &initial_accounts,
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
        .route("/execute-tx", post(execute_tx))
        .route("/faucet", post(faucet))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, server_port).parse()?;
    println!("Forking server listening on {}", addr);
    println!("Ready to accept requests");

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Set up graceful shutdown handler
    let shutdown_signal = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            warn!("Failed to install CTRL+C signal handler: {err}");
            return;
        }
        info!("\nReceived CTRL+C, shutting down gracefully...");
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

// // const SYSTEM_OBJECT_IDS: &[&str] = &["0x1", "0x2", "0x3", "0x6", "0xacc"];
//
// /// Update system objects to the versions at the forking checkpoint
// async fn update_system_objects(context: crate::context::Context) -> anyhow::Result<()> {
//     let crate::context::Context { at_checkpoint, .. } = context;
//
//     info!(
//         "Fetching system objects from RPC at checkpoint {}",
//         at_checkpoint
//     );
//     let object_ids: Vec<ObjectID> = SYSTEM_OBJECT_IDS
//         .iter()
//         .map(|id| ObjectID::from_hex_literal(id).unwrap())
//         .collect();
//
//     let keys: Vec<ObjectKey> = object_ids
//         .iter()
//         .map(|&object_id| ObjectKey {
//             object_id,
//             version_query: VersionQuery::AtCheckpoint(at_checkpoint),
//         })
//         .collect();
//
//     let simulacrum = context.simulacrum.write().await;
//     let data_store = simulacrum.store_static();
//     data_store.object_store().get_objects(&keys).unwrap();
//
//     Ok(())
// }

async fn get_sui_system_state(
    forking_store: &ForkingStore,
) -> Result<SuiSystemState, anyhow::Error> {
    let state = sui_types::sui_system_state::get_sui_system_state(forking_store)?;
    Ok(state)
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
    let grpc = GrpcRpcService::new(grpc_args, version, &registry)
        .await?
        .register_encoded_file_descriptor_set(sui_rpc::proto::sui::rpc::v2::FILE_DESCRIPTOR_SET)
        .add_service(LedgerServiceServer::new(ledger_service))
        .add_service(SubscriptionServiceServer::new(subscription_service))
        .add_service(StateServiceServer::new(state_service))
        .add_service(TransactionExecutionServiceServer::new(tx_execution_service));
    let handle = grpc.run().await?;
    Ok(handle)
}

async fn initialize_simulacrum(
    forked_at_checkpoint: u64,
    startup_checkpoint: u64,
    client: &GraphQLClient,
    initial_accounts: &InitialAccounts,
    protocol_version: u64,
    chain: Chain,
    fs_store: FileSystemStore,
    fs_gql_store: ReadThroughStore<FileSystemStore, DataStore>,
) -> Result<Simulacrum<OsRng, ForkingStore>, anyhow::Error> {
    let mut rng = rand::rngs::OsRng;
    let config = ConfigBuilder::new_with_temp_dir()
        .rng(&mut rng)
        .with_chain_start_timestamp_ms(0)
        .deterministic_committee_size(NonZeroUsize::MIN)
        .with_protocol_version(protocol_version.into())
        .with_chain_override(chain)
        .build();

    let startup_checkpoint_data = match fs_store
        .get_checkpoint_by_sequence_number(startup_checkpoint)
        .context("failed to read startup checkpoint from local checkpoint store")?
    {
        Some(checkpoint) => checkpoint,
        None => fs_gql_store
            .get_checkpoint_by_sequence_number(startup_checkpoint)
            .context("failed to fetch startup checkpoint from rpc")?
            .with_context(|| format!("checkpoint {startup_checkpoint} not found"))?,
    };

    let certified_summary = startup_checkpoint_data.summary.clone();
    let env_checkpoint = Envelope::new_from_data_and_sig(
        certified_summary.data().clone(),
        certified_summary.auth_sig().clone(),
    );
    let verified_checkpoint = VerifiedCheckpoint::new_unchecked(env_checkpoint);

    let mut store = ForkingStore::new(forked_at_checkpoint, fs_store, fs_gql_store);
    store.insert_checkpoint(verified_checkpoint.clone());
    store.insert_checkpoint_contents(startup_checkpoint_data.contents.clone());
    initial_accounts
        .prefetch_owned_objects(&store, client.endpoint(), startup_checkpoint)
        .await
        .context("Failed to prefetch startup owned objects")?;

    // Fetch the system state at this forked checkpoint and update the validator set to match the
    // one in our custom config, because we do not have the actual validators' keys from network.
    let system_state = get_sui_system_state(&store)
        .await
        .context("failed to read Sui system state from startup checkpoint")?;
    let mut inner = match system_state {
        SuiSystemState::V2(inner) => inner,
        _ => anyhow::bail!("Unsupported system state version, expected SuiSystemState::V2"),
    };
    inner.validators = match config.genesis.sui_system_object() {
        SuiSystemState::V1(genesis_inner) => genesis_inner.validators,
        SuiSystemState::V2(genesis_inner) => genesis_inner.validators,
    };
    let initial_sui_system_state = SuiSystemState::V2(inner);

    let validator_set_override = match &initial_sui_system_state {
        SuiSystemState::V1(inner) => inner.validators.clone(),
        SuiSystemState::V2(inner) => inner.validators.clone(),
    };
    store.set_system_state_validator_set_override(validator_set_override);

    let initial_committee = initial_sui_system_state
        .get_current_epoch_committee()
        .committee()
        .clone();
    store.insert_committee(initial_committee);

    let keystore = KeyStore::from_network_config(&config);

    // The mock checkpoint builder relies on the accumulator root object to be present in the store
    // with the correct initial shared version, so we need to fetch it here and pass it to the
    // simulacrum. This is needed if protocol config has enabled accumulators
    // TODO: do we need this if protocol config does not have enabled accumulators?
    let acc_initial_shared_version = get_accumulator_root_obj_initial_shared_version(&store)?
        .ok_or_else(|| anyhow::anyhow!("Failed to get accumulator root object from store"))?;
    let simulacrum = Simulacrum::new_from_custom_state(
        keystore,
        verified_checkpoint,
        initial_sui_system_state,
        &config,
        store,
        rng,
        Some(acc_initial_shared_version),
    );

    // simulacrum.set_data_ingestion_path(data_ingestion_path.clone());
    // println!("Data ingestion path: {:?}", data_ingestion_path);
    Ok(simulacrum)
}

/// Create the data stores for the forking server, including a file system store for transactions
/// and a read-through store for objects that combines the file system store and the GraphQL RPC
/// store.
fn initialize_data_store(
    fork_network: &ForkNetwork,
    fullnode_endpoint: &str,
    at_checkpoint: u64,
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

    let fs_base_path = FileSystemStore::base_path()
        .context("failed to resolve base path for file system data store")?
        .join(forking_path);
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
