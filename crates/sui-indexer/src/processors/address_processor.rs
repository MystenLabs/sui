// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use std::sync::Arc;
use tracing::info;

use sui_indexer::errors::IndexerError;
use sui_indexer::metrics::IndexerAddressProcessorMetrics;
use sui_indexer::models::address_logs::{commit_address_log, read_address_log};
use sui_indexer::models::addresses::{commit_addresses, transaction_to_address, NewAddress};
use sui_indexer::models::transactions::read_transactions;
use sui_indexer::{get_pg_pool_connection, PgConnectionPool};

const TRANSACTION_BATCH_SIZE: usize = 100;

pub struct AddressProcessor {
    pg_connection_pool: Arc<PgConnectionPool>,
    pub address_processor_metrics: IndexerAddressProcessorMetrics,
}

impl AddressProcessor {
    pub fn new(
        pg_connection_pool: Arc<PgConnectionPool>,
        prometheus_registry: &Registry,
    ) -> AddressProcessor {
        let address_processor_metrics = IndexerAddressProcessorMetrics::new(prometheus_registry);
        Self {
            pg_connection_pool,
            address_processor_metrics,
        }
    }

    pub async fn start(&self) -> Result<(), IndexerError> {
        info!("Indexer address processor started...");
        let mut pg_pool_conn = get_pg_pool_connection(self.pg_connection_pool.clone())?;

        let address_log = read_address_log(&mut pg_pool_conn)?;
        let mut last_processed_id = address_log.last_processed_id;

        loop {
            // fetch transaction rows from DB, with filter id > last_processed_id and limit of batch size.
            let txn_vec =
                read_transactions(&mut pg_pool_conn, last_processed_id, TRANSACTION_BATCH_SIZE)?;
            let addr_vec: Vec<NewAddress> =
                txn_vec.into_iter().map(transaction_to_address).collect();
            last_processed_id += addr_vec.len() as i64;

            commit_addresses(&mut pg_pool_conn, addr_vec)?;
            commit_address_log(&mut pg_pool_conn, last_processed_id)?;
            self.address_processor_metrics
                .total_address_batch_processed
                .inc();
        }
    }
}
