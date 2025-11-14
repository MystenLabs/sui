use anyhow::Result;
use axum::{
    Json, Router,
    extract::State,
    response::IntoResponse,
    routing::{get, post},
};
use simulacrum::{AdvanceEpochConfig, Simulacrum};
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};
use sui_types::transaction::Transaction;
use tower_http::cors::CorsLayer;
use tracing::info;

use crate::{
    consistent_store::{self, start_consistent_store},
    indexer::{self, start_indexer},
    rpc::start_rpc,
    types::*,
};

use diesel::prelude::*;

pub struct AppState {
    pub simulacrum: Arc<RwLock<Simulacrum>>,
    pub transaction_count: Arc<AtomicUsize>,
    pub forked_at_checkpoint: u64,
}

impl AppState {
    pub async fn new(data_ingestion_path: PathBuf) -> Self {
        let mut simulacrum = Simulacrum::new();
        simulacrum.set_data_ingestion_path(data_ingestion_path);
        let simulacrum = Arc::new(RwLock::new(simulacrum));
        let rpc = start_rpc(simulacrum.clone())
            .await
            .expect("Failed to start RPC server");
        Self {
            simulacrum,
            transaction_count: Arc::new(AtomicUsize::new(0)),
            forked_at_checkpoint: 0,
        }
    }
}

async fn health() -> &'static str {
    "OK"
}

async fn get_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let sim = state.simulacrum.read().unwrap();
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
    let mut sim = state.simulacrum.write().unwrap();

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
    let mut sim = state.simulacrum.write().unwrap();

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
    let mut sim = state.simulacrum.write().unwrap();

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
    let mut sim = state.simulacrum.write().unwrap();
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

/// Start the forking server
pub async fn start_server(
    host: String,
    port: u16,
    data_ingestion_path: PathBuf,
    version: &'static str,
) -> Result<()> {
    let data_ingestion_path_clone = data_ingestion_path.clone();
    // Start indexers
    tokio::spawn(async move {
        if let Err(e) = start_indexers(data_ingestion_path.clone(), version).await {
            eprintln!("Failed to start indexers: {:?}", e);
        }
    });

    let state = Arc::new(AppState::new(data_ingestion_path_clone.clone()).await);

    let app = Router::new()
        .route("/health", get(health))
        .route("/status", get(get_status))
        .route("/advance-checkpoint", post(advance_checkpoint))
        .route("/advance-clock", post(advance_clock))
        .route("/advance-epoch", post(advance_epoch))
        .route("/execute-tx", post(execute_tx))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Start the indexers: both the main indexer and the consistent store
async fn start_indexers(data_ingestion_path: PathBuf, version: &'static str) -> Result<()> {
    let registry = prometheus::Registry::new();
    let cancel = tokio_util::sync::CancellationToken::new();
    let rocksdb_db_path = mysten_common::tempdir().unwrap().keep();
    let db_url_str = "postgres://postgres:postgrespw@localhost:5432";
    let db_url = reqwest::Url::parse(&format!("{db_url_str}/sui_indexer_alt")).unwrap();
    let _ = drop_and_recreate_db(db_url_str).unwrap();
    let indexer_config = indexer::IndexerConfig::new(db_url, data_ingestion_path.clone());
    let consistent_store_config = consistent_store::ConsistentStoreConfig::new(
        rocksdb_db_path.clone(),
        indexer_config.indexer_args.clone(),
        indexer_config.client_args.clone(),
        version,
    );
    start_indexer(indexer_config, &registry, cancel.clone()).await?;
    start_consistent_store(consistent_store_config, &registry, cancel.clone()).await?;

    Ok(())
}

use diesel::pg::PgConnection;

fn drop_and_recreate_db(db_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Connect to the 'postgres' database (not your target database)
    let mut conn = PgConnection::establish(&db_url)?;

    println!("Dropping and recreating database sui_indexer_alt...");
    // Drop the database
    diesel::sql_query("DROP DATABASE IF EXISTS sui_indexer_alt").execute(&mut conn)?;

    // Recreate it
    diesel::sql_query("CREATE DATABASE sui_indexer_alt").execute(&mut conn)?;

    Ok(())
}
