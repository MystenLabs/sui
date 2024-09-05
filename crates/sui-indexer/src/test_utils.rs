// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::connection::SimpleConnection;
use mysten_metrics::init_metrics;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use std::path::PathBuf;
use std::time::Duration;
use sui_json_rpc_types::SuiTransactionBlockResponse;

use crate::config::IngestionConfig;
use crate::config::PruningOptions;
use crate::config::SnapshotLagConfig;
use crate::db::{new_connection_pool, ConnectionPoolConfig};
use crate::errors::IndexerError;
use crate::indexer::Indexer;
use crate::store::PgIndexerStore;
use crate::IndexerMetrics;

pub enum ReaderWriterConfig {
    Reader {
        reader_mode_rpc_url: String,
    },
    Writer {
        snapshot_config: SnapshotLagConfig,
        pruning_options: PruningOptions,
    },
}

impl ReaderWriterConfig {
    pub fn reader_mode(reader_mode_rpc_url: String) -> Self {
        Self::Reader {
            reader_mode_rpc_url,
        }
    }

    /// Instantiates a config for indexer in writer mode with the given snapshot config and epochs
    /// to keep.
    pub fn writer_mode(
        snapshot_config: Option<SnapshotLagConfig>,
        epochs_to_keep: Option<u64>,
    ) -> Self {
        Self::Writer {
            snapshot_config: snapshot_config.unwrap_or_default(),
            pruning_options: PruningOptions { epochs_to_keep },
        }
    }
}

pub async fn start_test_indexer(
    db_url: Option<String>,
    rpc_url: String,
    reader_writer_config: ReaderWriterConfig,
    data_ingestion_path: PathBuf,
) -> (
    PgIndexerStore,
    JoinHandle<Result<(), IndexerError>>,
    CancellationToken,
) {
    let token = CancellationToken::new();
    let (store, handle) = start_test_indexer_impl(
        db_url,
        rpc_url,
        reader_writer_config,
        /* reset_database */ false,
        Some(data_ingestion_path),
        token.clone(),
    )
    .await;
    (store, handle, token)
}

/// Starts an indexer reader or writer for testing depending on the `reader_writer_config`. If
/// `reset_database` is true, the database instance named in `db_url` will be dropped and
/// reinstantiated.
pub async fn start_test_indexer_impl(
    db_url: Option<String>,
    rpc_url: String,
    reader_writer_config: ReaderWriterConfig,
    reset_database: bool,
    data_ingestion_path: Option<PathBuf>,
    cancel: CancellationToken,
) -> (PgIndexerStore, JoinHandle<Result<(), IndexerError>>) {
    let db_url = db_url.unwrap_or_else(|| {
        let pg_host = "localhost";
        let pg_port = "32770";
        let pw = "postgrespw";
        format!("postgres://postgres:{pw}@{pg_host}:{pg_port}")
    });

    // Reduce the connection pool size to 10 for testing
    // to prevent maxing out
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

    let mut parsed_url = db_url.clone();

    if reset_database {
        let db_name = parsed_url.split('/').last().unwrap();
        // Switch to default to create a new database
        let (default_db_url, _) = replace_db_name(&parsed_url, "postgres");

        // Open in default mode
        let blocking_pool = new_connection_pool(&default_db_url, &pool_config).unwrap();
        let mut default_conn = blocking_pool.get().unwrap();

        // Delete the old db if it exists
        default_conn
            .batch_execute(&format!("DROP DATABASE IF EXISTS {}", db_name))
            .unwrap();

        // Create the new db
        default_conn
            .batch_execute(&format!("CREATE DATABASE {}", db_name))
            .unwrap();
        parsed_url = replace_db_name(&parsed_url, db_name).0;
    }

    let blocking_pool = new_connection_pool(&parsed_url, &pool_config).unwrap();
    let store = PgIndexerStore::new(blocking_pool.clone(), indexer_metrics.clone());

    let handle = match reader_writer_config {
        ReaderWriterConfig::Reader {
            reader_mode_rpc_url,
        } => {
            let config = crate::config::JsonRpcConfig {
                name_service_options: crate::config::NameServiceOptions::default(),
                rpc_address: reader_mode_rpc_url.parse().unwrap(),
                rpc_client_url: rpc_url.parse().unwrap(),
            };
            tokio::spawn(
                async move { Indexer::start_reader(&config, &registry, blocking_pool).await },
            )
        }
        ReaderWriterConfig::Writer {
            snapshot_config,
            pruning_options,
        } => {
            crate::db::reset_database(&mut blocking_pool.get().unwrap()).unwrap();

            let store_clone = store.clone();
            let mut ingestion_config = IngestionConfig::default();
            ingestion_config.sources.data_ingestion_path = data_ingestion_path;

            tokio::spawn(async move {
                Indexer::start_writer_with_config(
                    &ingestion_config,
                    store_clone,
                    indexer_metrics,
                    snapshot_config,
                    pruning_options,
                    cancel,
                )
                .await
            })
        }
    };

    (store, handle)
}

fn replace_db_name(db_url: &str, new_db_name: &str) -> (String, String) {
    let pos = db_url.rfind('/').expect("Unable to find / in db_url");
    let old_db_name = &db_url[pos + 1..];

    (
        format!("{}/{}", &db_url[..pos], new_db_name),
        old_db_name.to_string(),
    )
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
