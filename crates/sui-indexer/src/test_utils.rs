// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::init_metrics;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use simulacrum::Simulacrum;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_json_rpc_types::SuiTransactionBlockResponse;
use sui_pg_db::temp::{get_available_port, TempDb};

use crate::config::{IngestionConfig, RetentionConfig, SnapshotLagConfig, UploadOptions};
use crate::database::Connection;
use crate::database::ConnectionPool;
use crate::db::ConnectionPoolConfig;
use crate::errors::IndexerError;
use crate::indexer::Indexer;
use crate::store::PgIndexerStore;
use crate::IndexerMetrics;

/// Wrapper over `Indexer::start_reader` to make it easier to configure an indexer jsonrpc reader
/// for testing.
pub async fn start_indexer_jsonrpc_for_testing(
    db_url: String,
    fullnode_url: String,
    json_rpc_url: String,
    cancel: Option<CancellationToken>,
) -> (JoinHandle<Result<(), IndexerError>>, CancellationToken) {
    let token = cancel.unwrap_or_default();

    // Reduce the connection pool size to 10 for testing
    // to prevent maxing out
    let pool_config = ConnectionPoolConfig {
        pool_size: 5,
        connection_timeout: Duration::from_secs(10),
        statement_timeout: Duration::from_secs(30),
    };

    println!("db_url: {db_url}");
    println!("pool_config: {pool_config:?}");

    let registry = prometheus::Registry::default();
    init_metrics(&registry);

    let pool = ConnectionPool::new(db_url.parse().unwrap(), pool_config)
        .await
        .unwrap();

    let handle = {
        let config = crate::config::JsonRpcConfig {
            name_service_options: crate::config::NameServiceOptions::default(),
            rpc_address: json_rpc_url.parse().unwrap(),
            rpc_client_url: fullnode_url,
        };
        let token_clone = token.clone();
        tokio::spawn(
            async move { Indexer::start_reader(&config, &registry, pool, token_clone).await },
        )
    };

    (handle, token)
}

/// Wrapper over `Indexer::start_writer_with_config` to make it easier to configure an indexer
/// writer for testing. If the config options are null, default values that have historically worked
/// for testing will be used.
pub async fn start_indexer_writer_for_testing(
    db_url: String,
    snapshot_config: Option<SnapshotLagConfig>,
    retention_config: Option<RetentionConfig>,
    data_ingestion_path: Option<PathBuf>,
    cancel: Option<CancellationToken>,
    start_checkpoint: Option<u64>,
    end_checkpoint: Option<u64>,
) -> (
    PgIndexerStore,
    JoinHandle<Result<(), IndexerError>>,
    CancellationToken,
) {
    start_indexer_writer_for_testing_with_mvr_mode(
        db_url,
        snapshot_config,
        retention_config,
        data_ingestion_path,
        cancel,
        start_checkpoint,
        end_checkpoint,
        false,
    )
    .await
}

/// Separate entrypoint for instantiating an indexer with or without MVR mode enabled. Relevant only
/// for MVR, the production indexer available through start_indexer_writer_for_testing should be
/// generally used.
pub async fn start_indexer_writer_for_testing_with_mvr_mode(
    db_url: String,
    snapshot_config: Option<SnapshotLagConfig>,
    retention_config: Option<RetentionConfig>,
    data_ingestion_path: Option<PathBuf>,
    cancel: Option<CancellationToken>,
    start_checkpoint: Option<u64>,
    end_checkpoint: Option<u64>,
    mvr_mode: bool,
) -> (
    PgIndexerStore,
    JoinHandle<Result<(), IndexerError>>,
    CancellationToken,
) {
    let token = cancel.unwrap_or_default();
    let snapshot_config = snapshot_config.unwrap_or(SnapshotLagConfig {
        snapshot_min_lag: 5,
        sleep_duration: 0,
    });

    // Reduce the connection pool size to 10 for testing to prevent maxing out
    let pool_config = ConnectionPoolConfig {
        pool_size: 5,
        connection_timeout: Duration::from_secs(10),
        statement_timeout: Duration::from_secs(30),
    };

    println!("db_url: {db_url}");
    println!("pool_config: {pool_config:?}");
    println!("{data_ingestion_path:?}");

    let registry = prometheus::Registry::default();
    init_metrics(&registry);
    let indexer_metrics = IndexerMetrics::new(&registry);

    let pool = ConnectionPool::new(db_url.parse().unwrap(), pool_config)
        .await
        .unwrap();
    let store = PgIndexerStore::new(
        pool.clone(),
        UploadOptions::default(),
        indexer_metrics.clone(),
    );

    let handle = {
        let connection = Connection::dedicated(&db_url.parse().unwrap())
            .await
            .unwrap();
        crate::db::reset_database(connection).await.unwrap();

        let store_clone = store.clone();
        let mut ingestion_config = IngestionConfig {
            start_checkpoint,
            end_checkpoint,
            ..Default::default()
        };
        ingestion_config.sources.data_ingestion_path = data_ingestion_path;
        let token_clone = token.clone();

        tokio::spawn(async move {
            Indexer::start_writer(
                ingestion_config,
                store_clone,
                indexer_metrics,
                snapshot_config,
                retention_config,
                token_clone,
                mvr_mode,
            )
            .await
        })
    };

    (store, handle, token)
}

#[derive(Clone)]
pub struct SuiTransactionBlockResponseBuilder<'a> {
    response: SuiTransactionBlockResponse,
    full_response: &'a SuiTransactionBlockResponse,
}

impl<'a> SuiTransactionBlockResponseBuilder<'a> {
    pub fn new(full_response: &'a SuiTransactionBlockResponse) -> Self {
        Self {
            response: SuiTransactionBlockResponse::default(),
            full_response,
        }
    }

    pub fn with_input(mut self) -> Self {
        self.response = SuiTransactionBlockResponse {
            transaction: self.full_response.transaction.clone(),
            ..self.response
        };
        self
    }

    pub fn with_raw_input(mut self) -> Self {
        self.response = SuiTransactionBlockResponse {
            raw_transaction: self.full_response.raw_transaction.clone(),
            ..self.response
        };
        self
    }

    pub fn with_effects(mut self) -> Self {
        self.response = SuiTransactionBlockResponse {
            effects: self.full_response.effects.clone(),
            ..self.response
        };
        self
    }

    pub fn with_events(mut self) -> Self {
        self.response = SuiTransactionBlockResponse {
            events: self.full_response.events.clone(),
            ..self.response
        };
        self
    }

    pub fn with_balance_changes(mut self) -> Self {
        self.response = SuiTransactionBlockResponse {
            balance_changes: self.full_response.balance_changes.clone(),
            ..self.response
        };
        self
    }

    pub fn with_object_changes(mut self) -> Self {
        self.response = SuiTransactionBlockResponse {
            object_changes: self.full_response.object_changes.clone(),
            ..self.response
        };
        self
    }

    pub fn with_input_and_changes(mut self) -> Self {
        self.response = SuiTransactionBlockResponse {
            transaction: self.full_response.transaction.clone(),
            balance_changes: self.full_response.balance_changes.clone(),
            object_changes: self.full_response.object_changes.clone(),
            ..self.response
        };
        self
    }

    pub fn build(self) -> SuiTransactionBlockResponse {
        SuiTransactionBlockResponse {
            transaction: self.response.transaction,
            raw_transaction: self.response.raw_transaction,
            effects: self.response.effects,
            events: self.response.events,
            balance_changes: self.response.balance_changes,
            object_changes: self.response.object_changes,
            // Use full response for any fields that aren't showable
            ..self.full_response.clone()
        }
    }
}

/// Set up a test indexer fetching from a REST endpoint served by the given Simulacrum.
pub async fn set_up(
    sim: Arc<Simulacrum>,
    data_ingestion_path: PathBuf,
) -> (
    JoinHandle<()>,
    PgIndexerStore,
    JoinHandle<Result<(), IndexerError>>,
    TempDb,
) {
    set_up_on_mvr_mode(sim, data_ingestion_path, false).await
}

/// Set up a test indexer fetching from a REST endpoint served by the given Simulacrum. With MVR
/// mode enabled, this indexer writes only to a subset of tables - `objects_snapshot`,
/// `objects_history`, `checkpoints`, `epochs`, and `packages`.
pub async fn set_up_on_mvr_mode(
    sim: Arc<Simulacrum>,
    data_ingestion_path: PathBuf,
    mvr_mode: bool,
) -> (
    JoinHandle<()>,
    PgIndexerStore,
    JoinHandle<Result<(), IndexerError>>,
    TempDb,
) {
    let database = TempDb::new().unwrap();
    let server_url: SocketAddr = format!("127.0.0.1:{}", get_available_port())
        .parse()
        .unwrap();

    let server_handle = tokio::spawn(async move {
        sui_rpc_api::RpcService::new_without_version(sim)
            .start_service(server_url)
            .await;
    });
    // Starts indexer
    let (pg_store, pg_handle, _) = start_indexer_writer_for_testing_with_mvr_mode(
        database.database().url().as_str().to_owned(),
        None,
        None,
        Some(data_ingestion_path),
        None,     /* cancel */
        None,     /* start_checkpoint */
        None,     /* end_checkpoint */
        mvr_mode, /* mvr_mode */
    )
    .await;
    (server_handle, pg_store, pg_handle, database)
}

pub async fn set_up_with_start_and_end_checkpoints(
    sim: Arc<Simulacrum>,
    data_ingestion_path: PathBuf,
    start_checkpoint: u64,
    end_checkpoint: u64,
) -> (
    JoinHandle<()>,
    PgIndexerStore,
    JoinHandle<Result<(), IndexerError>>,
    TempDb,
) {
    let database = TempDb::new().unwrap();
    let server_url: SocketAddr = format!("127.0.0.1:{}", get_available_port())
        .parse()
        .unwrap();
    let server_handle = tokio::spawn(async move {
        sui_rpc_api::RpcService::new_without_version(sim)
            .start_service(server_url)
            .await;
    });
    // Starts indexer
    let (pg_store, pg_handle, _) = start_indexer_writer_for_testing(
        database.database().url().as_str().to_owned(),
        None,
        None,
        Some(data_ingestion_path),
        None, /* cancel */
        Some(start_checkpoint),
        Some(end_checkpoint),
    )
    .await;
    (server_handle, pg_store, pg_handle, database)
}

/// Wait for the indexer to catch up to the given checkpoint sequence number.
pub async fn wait_for_checkpoint(
    pg_store: &PgIndexerStore,
    checkpoint_sequence_number: u64,
) -> Result<(), IndexerError> {
    tokio::time::timeout(Duration::from_secs(30), async {
        while {
            let cp_opt = pg_store
                .get_latest_checkpoint_sequence_number()
                .await
                .unwrap();
            cp_opt.is_none() || (cp_opt.unwrap() < checkpoint_sequence_number)
        } {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Timeout waiting for indexer to catchup to checkpoint");
    Ok(())
}

/// Wait for the indexer to catch up to the given checkpoint sequence number for objects snapshot.
pub async fn wait_for_objects_snapshot(
    pg_store: &PgIndexerStore,
    checkpoint_sequence_number: u64,
) -> Result<(), IndexerError> {
    tokio::time::timeout(Duration::from_secs(30), async {
        while {
            let cp_opt = pg_store
                .get_latest_object_snapshot_checkpoint_sequence_number()
                .await
                .unwrap();
            cp_opt.is_none() || (cp_opt.unwrap() < checkpoint_sequence_number)
        } {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Timeout waiting for indexer to catchup to checkpoint for objects snapshot");
    Ok(())
}
