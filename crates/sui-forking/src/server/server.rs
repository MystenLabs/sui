// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    net::SocketAddr,
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
use diesel::pg::PgConnection;
use diesel::prelude::*;
use prometheus::Registry;
use reqwest::Url;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::info;

use mysten_common::tempdir;
use simulacrum::{AdvanceEpochConfig, SimulacrumBuilder};
use sui_data_store::Node;
use sui_indexer_alt_jsonrpc::{RpcArgs, RpcService, config::RpcConfig};
use sui_indexer_alt_metrics::MetricsService;
use sui_indexer_alt_reader::bigtable_reader::BigtableArgs;
use sui_pg_db::{Db, DbArgs};
use sui_types::{
    base_types::SuiAddress,
    supported_protocol_versions::{
        Chain::{self},
        ProtocolConfig,
    },
    transaction::Transaction,
};

use crate::{
    graphql::GraphQLClient,
    indexers::{
        consistent_store::{ConsistentStoreConfig, start_consistent_store},
        indexer::{IndexerConfig, start_indexer},
    },
    rpc::start_rpc,
    server::*,
    store::ForkingStore,
};

/// The shared state for the forking server
pub struct AppState {
    pub context: crate::context::Context,
    pub transaction_count: Arc<AtomicUsize>,
    pub forked_at_checkpoint: u64,
    pub chain: Chain,
    pub protocol_config: ProtocolConfig,
}

impl AppState {
    pub async fn new(
        context: crate::context::Context,
        chain: Chain,
        at_checkpoint: u64,
        protocol_config: ProtocolConfig,
    ) -> Self {
        Self {
            context,
            transaction_count: Arc::new(AtomicUsize::new(0)),
            forked_at_checkpoint: 0,
            chain,
            protocol_config,
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

    let rpc_data_store = Arc::new(
        crate::store::rpc_data_store::new_rpc_data_store(Node::Testnet, version)
            .expect("Failed to create RPC data store"),
    );
    let mut simulacrum = SimulacrumBuilder::new()
        .with_chain(chain)
        .with_protocol_version(protocol_version.into())
        .build_with_store_creator(|genesis| {
            ForkingStore::new(genesis, at_checkpoint, rpc_data_store.clone())
        });

    simulacrum.set_data_ingestion_path(data_ingestion_path.clone());
    let simulacrum = Arc::new(RwLock::new(simulacrum));

    let registry = Registry::new_custom(Some("sui_forking".into()), None)
        .context("Failed to create Prometheus registry.")
        .unwrap();

    let metrics_args = sui_indexer_alt_metrics::MetricsArgs::default();
    let metrics = MetricsService::new(metrics_args, registry.clone());
    let rpc_listen_address = SocketAddr::from(([127, 0, 0, 1], 3000));
    let rpc_args = RpcArgs {
        rpc_listen_address,
        ..Default::default()
    };

    let rpc = RpcService::new(rpc_args, &registry)
        .context("Failed to create RPC service")
        .unwrap();

    println!("RPC listening on {}", rpc_listen_address);

    let pg_context = sui_indexer_alt_jsonrpc::context::Context::new(
        Some(database_url.clone()),
        None,
        DbArgs::default(),
        BigtableArgs::default(),
        RpcConfig::default(),
        rpc.metrics(),
        &registry,
    )
    .await
    .expect("Failed to create PG context");

    // Create a write connection to the database for inserting packages
    let db_writer = Db::for_write(database_url, DbArgs::default())
        .await
        .expect("Failed to create DB writer");

    let context = crate::context::Context {
        pg_context,
        simulacrum,
        db_writer,
        at_checkpoint,
        chain,
        protocol_version,
    };

    let state =
        Arc::new(AppState::new(context.clone(), chain, at_checkpoint, protocol_config).await);

    let rpc_handle = tokio::spawn(async move {
        start_rpc(context.clone(), rpc, metrics).await.unwrap();
    });
    let indexer_handle = tokio::spawn(async move {
        start_indexers(data_ingestion_path.clone(), version)
            .await
            .unwrap();
    });

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
    rpc_handle.abort();
    indexer_handle.abort();

    info!("Server shutdown complete");

    Ok(())
}

/// Start the indexers: both the main indexer and the consistent store
async fn start_indexers(data_ingestion_path: PathBuf, version: &'static str) -> Result<()> {
    let registry = prometheus::Registry::new();
    let rocksdb_db_path = tempdir().unwrap().keep();
    let db_url_str = "postgres://postgres:postgrespw@localhost:5432";
    let db_url = Url::parse(&format!("{db_url_str}/sui_indexer_alt")).unwrap();
    let _ = drop_and_recreate_db(db_url_str).unwrap();
    let indexer_config = IndexerConfig::new(db_url, data_ingestion_path.clone());
    let consistent_store_config = ConsistentStoreConfig::new(
        rocksdb_db_path.clone(),
        indexer_config.indexer_args.clone(),
        indexer_config.client_args.clone(),
    );
    let indexer = start_indexer(indexer_config, &registry).await?;
    let consistent_store =
        start_consistent_store(consistent_store_config, &registry, version).await?;

    match indexer.attach(consistent_store).main().await {
        Ok(()) | Err(sui_futures::service::Error::Terminated) => {}

        Err(sui_futures::service::Error::Aborted) => {
            std::process::exit(1);
        }

        Err(sui_futures::service::Error::Task(_)) => {
            std::process::exit(2);
        }
    }

    Ok(())
}

fn drop_and_recreate_db(db_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Connect to the 'postgres' database (not your target database)
    let mut conn = PgConnection::establish(&db_url)?;

    info!("Dropping and recreating database sui_indexer_alt...");
    // // Drop the database
    diesel::sql_query("DROP DATABASE IF EXISTS sui_indexer_alt").execute(&mut conn)?;

    // Recreate it
    diesel::sql_query("CREATE DATABASE sui_indexer_alt").execute(&mut conn)?;

    Ok(())
}
