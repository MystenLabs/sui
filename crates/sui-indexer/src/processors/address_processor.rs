// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer::errors::IndexerError;
use sui_indexer::establish_connection;
use tracing::info;

use sui_indexer::models::address_logs::{commit_address_log, read_address_log};
use sui_indexer::models::addresses::{commit_addresses, transaction_to_address, NewAddress};
use sui_indexer::models::transactions::read_transactions;

const TRANSACTION_BATCH_SIZE: usize = 100;

pub struct AddressProcessor {
    db_url: String,
}

impl AddressProcessor {
    pub fn new(db_url: String) -> AddressProcessor {
        Self { db_url }
    }

    pub async fn start(&self) -> Result<(), IndexerError> {
        info!("Indexer address processor started...");
        let mut pg_conn = establish_connection(self.db_url.clone());

        let address_log = read_address_log(&mut pg_conn)?;
        let last_processed_id = address_log.last_processed_id;

        // fetch transaction rows from DB, with filter id > last_processed_id and limit of batch size.
        let txn_vec = read_transactions(&mut pg_conn, last_processed_id, TRANSACTION_BATCH_SIZE)?;
        let addr_vec: Vec<NewAddress> = txn_vec.into_iter().map(transaction_to_address).collect();
        let next_processed_id = last_processed_id + addr_vec.len() as i64;

        commit_addresses(&mut pg_conn, addr_vec)?;
        commit_address_log(&mut pg_conn, next_processed_id)?;
        Ok(())
    }
}
