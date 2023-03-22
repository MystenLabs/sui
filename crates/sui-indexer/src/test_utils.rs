// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use prometheus::Registry;
use tokio::task::JoinHandle;

use crate::errors::IndexerError;
use crate::store::PgIndexerStore;
use crate::utils::reset_database;
use crate::{new_pg_connection_pool, Indexer, IndexerConfig};

/// Spawns an indexer thread with provided Postgres DB url
pub async fn start_test_indexer(
    config: IndexerConfig,
) -> Result<(PgIndexerStore, JoinHandle<Result<(), IndexerError>>), anyhow::Error> {
    let pg_connection_pool = new_pg_connection_pool(&config.base_connection_url())
        .await
        .map_err(|e| anyhow!("unable to connect to Postgres, is it running? {e}"))?;
    if config.reset_db {
        reset_database(
            &mut pg_connection_pool
                .get()
                .map_err(|e| anyhow!("Fail to get pg_connection_pool {e}"))?,
            true,
        )?;
    }
    let store = PgIndexerStore::new(pg_connection_pool);

    let registry = Registry::default();
    let store_clone = store.clone();
    let handle = tokio::spawn(async move { Indexer::start(&config, &registry, store_clone).await });
    Ok((store, handle))
}
