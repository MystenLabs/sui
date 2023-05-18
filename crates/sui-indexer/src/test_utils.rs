// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use prometheus::Registry;
use tokio::task::JoinHandle;

use sui_json_rpc_types::SuiTransactionBlockResponse;

use crate::errors::IndexerError;
use crate::store::PgIndexerStore;
use crate::utils::reset_database;
use crate::IndexerMetrics;
use crate::{new_pg_connection_pool, Indexer, IndexerConfig};

/// Spawns an indexer thread with provided Postgres DB url
pub async fn start_test_indexer(
    config: IndexerConfig,
) -> Result<(PgIndexerStore, JoinHandle<Result<(), IndexerError>>), anyhow::Error> {
    let parsed_url = config.base_connection_url()?;
    let (blocking_pool, async_pool) = new_pg_connection_pool(&parsed_url)
        .await
        .map_err(|e| anyhow!("unable to connect to Postgres, is it running? {e}"))?;
    if config.reset_db {
        reset_database(
            &mut blocking_pool
                .get()
                .map_err(|e| anyhow!("Fail to get pg_connection_pool {e}"))?,
            true,
        )?;
    }

    let registry = Registry::default();
    let indexer_metrics = IndexerMetrics::new(&registry);

    let store = PgIndexerStore::new(async_pool, blocking_pool, indexer_metrics.clone()).await;
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
