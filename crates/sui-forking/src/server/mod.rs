// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    net::SocketAddr,
    num::NonZeroUsize,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use anyhow::{Context as _, Result};
use axum::{
    Json, Router,
    extract::State,
    response::IntoResponse,
    routing::{get, post},
};
use prometheus::Registry;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::info;

use simulacrum::{AdvanceEpochConfig, store::in_mem_store::KeyStore};
use sui_data_store::{
    Node, ObjectKey, ObjectStore, VersionQuery,
    stores::{DataStore, FileSystemStore, NODE_MAPPING_FILE, ReadThroughStore},
};
use sui_pg_db::{DbArgs, reset_database};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    digests::ChainIdentifier,
    effects::TransactionEffectsAPI,
    messages_checkpoint::{CertifiedCheckpointSummary, CheckpointContents, VerifiedCheckpoint},
    sui_system_state::{
        SuiSystemState, SuiSystemStateTrait,
        sui_system_state_inner_v1::ValidatorSetV1,
        sui_system_state_inner_v2::{self, SuiSystemStateInnerV2},
    },
    supported_protocol_versions::{
        Chain::{self},
        ProtocolConfig,
    },
    transaction::Transaction,
};

use crate::grpc::transaction_execution_service::ForkingTransactionExecutionService;
use crate::grpc::{RpcArgs as GrpcArgs, ledger_service::ForkingLedgerService};
use crate::grpc::{RpcService as GrpcRpcService, consistent_store::ForkingConsistentStore};
use crate::grpc::{TlsArgs as GrpcTlsArgs, subscription_service::ForkingSubscriptionService};
use crate::{graphql::GraphQLClient, store::ForkingStore};

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_server::ConsistentServiceServer;
use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_server::TransactionExecutionServiceServer;
use sui_rpc::proto::sui::rpc::v2::{
    ledger_service_server::LedgerServiceServer,
    subscription_service_server::SubscriptionServiceServer,
};
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
    pub forked_at_checkpoint: u64,
    pub checkpoint: u64,
    pub epoch: u64,
    pub transaction_count: usize,
}

/// The shared state for the forking server
pub struct AppState {
    pub context: crate::context::Context,
    pub transaction_count: Arc<AtomicUsize>,
    pub forked_at_checkpoint: u64,
    pub _chain: Chain,
    pub _protocol_config: ProtocolConfig,
}

impl AppState {
    pub async fn new(
        context: crate::context::Context,
        chain: Chain,
        forked_at_checkpoint: u64,
        protocol_config: ProtocolConfig,
    ) -> Self {
        Self {
            context,
            transaction_count: Arc::new(AtomicUsize::new(0)),
            forked_at_checkpoint,
            _chain: chain,
            _protocol_config: protocol_config,
        }
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

    let status = ForkingStatus {
        forked_at_checkpoint: state.forked_at_checkpoint,
        checkpoint,
        epoch,
        transaction_count: state.transaction_count.load(Ordering::SeqCst),
    };

    Json(ApiResponse {
        success: true,
        data: Some(status),
        error: None,
    })
}

async fn advance_checkpoint(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut sim = state.context.simulacrum.write().await;

    // create_checkpoint returns a VerifiedCheckpoint, not a Result
    let checkpoint = sim.create_checkpoint();
    state.transaction_count.fetch_add(1, Ordering::SeqCst);
    info!("Advanced to checkpoint {}", checkpoint.sequence_number);

    Json(ApiResponse::<String> {
        success: true,
        data: Some(format!(
            "Advanced to checkpoint {}",
            checkpoint.sequence_number
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
    state.transaction_count.fetch_add(1, Ordering::SeqCst);
    info!("Advanced clock by {} seconds", request.seconds);

    Json(ApiResponse::<String> {
        success: true,
        data: Some(format!("Clock advanced by {} seconds", request.seconds)),
        error: None,
    })
}

async fn advance_epoch(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut sim = state.context.simulacrum.write().await;

    // Use default configuration for advancing epoch
    let config = AdvanceEpochConfig::default();
    sim.advance_epoch(config);
    state.transaction_count.fetch_add(1, Ordering::SeqCst);
    info!("Advanced to next epoch");

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

            state.transaction_count.fetch_add(1, Ordering::SeqCst);
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

            state.transaction_count.fetch_add(1, Ordering::SeqCst);
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
    // accounts: InitialAccounts,
    chain: Chain,
    checkpoint: Option<u64>,
    host: String,
    port: u16,
    data_ingestion_path: PathBuf,
    version: &'static str,
) -> Result<()> {
    let chain_str = chain.as_str();
    let client = GraphQLClient::new(format!("https://graphql.{chain_str}.sui.io/graphql"));
    let (at_checkpoint, protocol_version) = if let Some(cp) = checkpoint {
        (cp, client.fetch_protocol_version(Some(cp)).await?)
    } else {
        client
            .fetch_latest_checkpoint_and_protocol_version()
            .await?
    };
    println!(
        "Starting at checkpoint {} with protocol version {}",
        at_checkpoint, protocol_version
    );
    let protocol_config = ProtocolConfig::get_for_version(protocol_version.into(), chain);
    let database_url = Url::parse("postgres://postgres:postgrespw@localhost:5432/sui_indexer_alt")?;

    {
        reset_database(database_url.clone(), DbArgs::default(), None).await?;
    }

    let node = match chain {
        Chain::Mainnet => Node::Mainnet,
        Chain::Testnet => Node::Testnet,
        Chain::Unknown => todo!("Add support for custom chains"),
    };

    let forking_path = format!(
        "forking/{}/forked_at_checkpoint_{}",
        chain.as_str(),
        at_checkpoint
    );

    let fs_base_path = FileSystemStore::base_path().unwrap().join(forking_path);
    let fs = FileSystemStore::new_with_path(node.clone(), fs_base_path.clone()).unwrap();
    let gql_rpc_store = DataStore::new(node.clone(), version).unwrap();
    let fs_transaction_store =
        FileSystemStore::new_with_path(node.clone(), fs_base_path.clone()).unwrap();
    let object_store = ReadThroughStore::new(fs, gql_rpc_store);

    info!("Fs base path {:?}", fs_base_path.display());
    let node_mapping_file = fs_base_path.join(NODE_MAPPING_FILE);
    if !fs_base_path.exists() {
        std::fs::create_dir_all(fs_base_path).unwrap();
    }
    info!("Node mapping file path: {:?}", node_mapping_file.display());
    if !node_mapping_file.exists() {
        std::fs::write(
            node_mapping_file,
            format!("{},{}", node.network_name(), chain_str),
        )
        .unwrap();
    }

    let mut rng = rand::rngs::OsRng;
    let config = ConfigBuilder::new_with_temp_dir()
        .rng(&mut rng)
        .with_chain_start_timestamp_ms(0)
        .deterministic_committee_size(NonZeroUsize::new(1).unwrap())
        .with_protocol_version(protocol_version.into())
        .with_chain_override(chain)
        .build();

    let committee = config.committee_with_network();

    // change the validator set to the sui system object to be the one in the new config we just
    // built.
    let checkpoint = fetch_checkpoint_from_graphql(client, Some(at_checkpoint)).await?;
    let store = ForkingStore::new(at_checkpoint, fs_transaction_store, object_store);

    let system_state = get_sui_system_state(&store).await.unwrap();

    let mut inner = match system_state {
        SuiSystemState::V2(inner) => inner,
        _ => panic!("Unsupported system state version"),
    };

    inner.validators = match config.genesis.sui_system_object() {
        SuiSystemState::V2(genesis_inner) => genesis_inner.validators,
        _ => panic!("Unsupported system state version"),
    };

    let initial_sui_system_state = SuiSystemState::V2(inner);

    let keystore = KeyStore::from_network_config(&config);

    // let mut simulacrum = simulacrum::Simulacrum::new_with_network_config_store(&config, rng, store);
    let mut simulacrum = simulacrum::Simulacrum::new_from_custom_state(
        keystore,
        checkpoint.0,
        initial_sui_system_state,
        &config,
        store,
        rng,
    );
    simulacrum.set_data_ingestion_path(data_ingestion_path.clone());
    println!("Data ingestion path: {:?}", data_ingestion_path);

    let simulacrum = Arc::new(RwLock::new(simulacrum));

    let registry = Registry::new_custom(Some("sui_forking".into()), None)
        .context("Failed to create Prometheus registry.")
        .unwrap();

    let rpc_listen_address = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("RPC listening on {}", rpc_listen_address);

    let grpc_args = GrpcArgs {
        rpc_listen_address,
        tls: GrpcTlsArgs::default(),
    };

    let context = crate::context::Context {
        simulacrum: simulacrum.clone(),
        at_checkpoint,
        chain,
        protocol_version,
    };

    let subscription_service = ForkingSubscriptionService::new(context.clone());
    let consistent_store = ForkingConsistentStore::new(context.clone());
    let ledger_service = ForkingLedgerService::new(simulacrum.clone(), ChainIdentifier::random());
    let tx_execution_service = ForkingTransactionExecutionService::new(context.clone());
    let grpc = GrpcRpcService::new(grpc_args, version, &registry)
        .await?
        .register_encoded_file_descriptor_set(sui_rpc::proto::sui::rpc::v2::FILE_DESCRIPTOR_SET)
        .register_encoded_file_descriptor_set(
            sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::FILE_DESCRIPTOR_SET,
        )
        .add_service(ConsistentServiceServer::new(consistent_store))
        .add_service(LedgerServiceServer::new(ledger_service))
        .add_service(SubscriptionServiceServer::new(subscription_service))
        .add_service(TransactionExecutionServiceServer::new(tx_execution_service));
    let _ = grpc.run().await?;

    let state =
        Arc::new(AppState::new(context.clone(), chain, at_checkpoint, protocol_config).await);

    let update_objects_handle = tokio::spawn(async move {
        update_system_objects(context.clone()).await.unwrap();
    });

    println!("Ready to accept requests");

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

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    println!("Forking server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Set up graceful shutdown handler
    let shutdown_signal = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C signal handler");
        info!("\nReceived CTRL+C, shutting down gracefully...");
    };

    // Serve with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    // Abort the spawned tasks when the server shuts down
    // rpc_handle.abort();
    // indexer_handle.abort();
    update_objects_handle.abort();

    info!("Server shutdown complete");

    Ok(())
}

/// Fetch a checkpoint from the GraphQL RPC and deserialize it into a
/// `VerifiedCheckpoint` and `CheckpointContents`.
///
/// We trust the RPC response, so the checkpoint is wrapped with
/// `new_unchecked` rather than verifying signatures against a committee.
pub async fn fetch_checkpoint_from_graphql(
    client: GraphQLClient,
    sequence_number: Option<u64>,
) -> Result<(VerifiedCheckpoint, CheckpointContents)> {
    let (summary_bytes, content_bytes) = client.fetch_checkpoint_bcs(sequence_number).await?;

    let certified: CertifiedCheckpointSummary =
        bcs::from_bytes(&summary_bytes).context("Failed to deserialize checkpoint summary")?;
    let contents: CheckpointContents =
        bcs::from_bytes(&content_bytes).context("Failed to deserialize checkpoint contents")?;

    let verified = VerifiedCheckpoint::new_unchecked(certified);

    Ok((verified, contents))
}

const SYSTEM_OBJECT_IDS: &[&str] = &["0x1", "0x2", "0x3", "0x5", "0x6"];

/// Update system objects to the versions at the forking checkpoint
async fn update_system_objects(context: crate::context::Context) -> anyhow::Result<()> {
    let crate::context::Context { at_checkpoint, .. } = context;

    info!(
        "Fetching system objects from RPC at checkpoint {}",
        at_checkpoint
    );
    let object_ids: Vec<ObjectID> = SYSTEM_OBJECT_IDS
        .iter()
        .map(|id| ObjectID::from_hex_literal(id).unwrap())
        .collect();

    let keys: Vec<ObjectKey> = object_ids
        .iter()
        .map(|&object_id| ObjectKey {
            object_id,
            version_query: VersionQuery::AtCheckpoint(at_checkpoint),
        })
        .collect();

    let simulacrum = context.simulacrum.write().await;
    let data_store = simulacrum.store_static();
    data_store.object_store().get_objects(&keys).unwrap();

    Ok(())
}

async fn get_sui_system_state(
    forking_store: &ForkingStore,
) -> Result<SuiSystemState, anyhow::Error> {
    let state = sui_types::sui_system_state::get_sui_system_state(forking_store)?;
    Ok(state)
}
