// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use mysten_metrics::init_metrics;
use prometheus::Registry;
use tokio::task::JoinHandle;

use std::env;
use std::net::SocketAddr;
use sui_json_rpc_types::SuiTransactionBlockResponse;
use tracing::info;

use crate::errors::IndexerError;
use crate::indexer_v2::IndexerV2;
use crate::store::{PgIndexerStore, PgIndexerStoreV2};
use crate::utils::reset_database;
use crate::{new_pg_connection_pool, Indexer, IndexerConfig};
use crate::{new_pg_connection_pool_impl, IndexerMetrics};

pub async fn start_test_indexer_v2(
    db_url: Option<String>,
    rpc_url: String,
    reader_mode_rpc_url: Option<String>,
    use_indexer_experimental_methods: bool,
) -> (PgIndexerStoreV2, JoinHandle<Result<(), IndexerError>>) {
    // Reduce the connection pool size to 10 for testing
    // to prevent maxing out
    info!("Setting DB_POOL_SIZE to 10");
    std::env::set_var("DB_POOL_SIZE", "10");

    let db_url = db_url.unwrap_or_else(|| {
        let pg_host = env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".into());
        let pg_port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "32770".into());
        let pw = env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgrespw".into());
        format!("postgres://postgres:{pw}@{pg_host}:{pg_port}")
    });

    let migrated_methods = if use_indexer_experimental_methods {
        IndexerConfig::all_implemented_methods()
    } else {
        vec![]
    };

    // Default writer mode
    let mut config = IndexerConfig {
        db_url: Some(db_url.clone()),
        rpc_client_url: rpc_url,
        migrated_methods,
        reset_db: true,
        fullnode_sync_worker: true,
        rpc_server_worker: false,
        use_v2: true,
        ..Default::default()
    };

    if let Some(reader_mode_rpc_url) = &reader_mode_rpc_url {
        let reader_mode_rpc_url = reader_mode_rpc_url
            .parse::<SocketAddr>()
            .expect("Unable to parse fullnode address");
        config.fullnode_sync_worker = false;
        config.rpc_server_worker = true;
        config.rpc_server_url = reader_mode_rpc_url.ip().to_string();
        config.rpc_server_port = reader_mode_rpc_url.port();
    }

    let parsed_url = config.get_db_url().unwrap();
    let blocking_pool = new_pg_connection_pool_impl(&parsed_url, Some(5)).unwrap();
    if config.reset_db && reader_mode_rpc_url.is_none() {
        reset_database(&mut blocking_pool.get().unwrap(), true, config.use_v2).unwrap();
    }

    let registry = prometheus::Registry::default();

    init_metrics(&registry);

    let indexer_metrics = IndexerMetrics::new(&registry);

    let store = PgIndexerStoreV2::new(blocking_pool, indexer_metrics.clone());
    let store_clone = store.clone();
    let handle = if reader_mode_rpc_url.is_some() {
        tokio::spawn(async move { IndexerV2::start_reader(&config, &registry, db_url).await })
    } else {
        tokio::spawn(
            async move { IndexerV2::start_writer(&config, store_clone, indexer_metrics).await },
        )
    };

    (store, handle)
}

/// Spawns an indexer thread with provided Postgres DB url
pub async fn start_test_indexer(
    config: IndexerConfig,
) -> Result<(PgIndexerStore, JoinHandle<Result<(), IndexerError>>), anyhow::Error> {
    let parsed_url = config.base_connection_url()?;
    let blocking_pool = new_pg_connection_pool(&parsed_url)
        .map_err(|e| anyhow!("unable to connect to Postgres, is it running? {e}"))?;
    if config.reset_db {
        reset_database(
            &mut blocking_pool
                .get()
                .map_err(|e| anyhow!("Fail to get pg_connection_pool {e}"))?,
            true,
            config.use_v2,
        )?;
    }

    let registry = Registry::default();
    let indexer_metrics = IndexerMetrics::new(&registry);

    let store = PgIndexerStore::new(blocking_pool, indexer_metrics.clone());
    let store_clone = store.clone();
    let handle = tokio::spawn(async move {
        Indexer::start(&config, &registry, store_clone, indexer_metrics, None).await
    });
    Ok((store, handle))
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
