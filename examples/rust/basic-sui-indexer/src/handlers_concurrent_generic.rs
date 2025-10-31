// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Example of using the generic concurrent Handler trait
// docs::#imports
use std::sync::Arc;
use anyhow::Result;
use async_trait::async_trait;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    pipeline::{Processor, concurrent::{Handler, BatchStatus}},
    postgres::{Connection, Db},
    types::full_checkpoint_content::Checkpoint,
};

use crate::models::StoredTransactionDigest;
use crate::schema::transaction_digests;
// docs::/#imports

pub struct ConcurrentTransactionDigestHandler;

// docs::#processor
#[async_trait]
impl Processor for ConcurrentTransactionDigestHandler {
    const NAME: &'static str = "concurrent_transaction_digest_handler";

    type Value = StoredTransactionDigest;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let checkpoint_seq = checkpoint.summary.sequence_number as i64;

        let digests = checkpoint.transactions.iter().map(|tx| {
            StoredTransactionDigest {
                tx_digest: tx.transaction.digest().to_string(),
                checkpoint_sequence_number: checkpoint_seq,
            }
        }).collect();

        Ok(digests)
    }
}
// docs::/#processor

// docs::#handler
#[async_trait]
impl Handler for ConcurrentTransactionDigestHandler {
    type Store = Db;
    type Batch = Vec<Self::Value>;

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
        batch.extend(values);
        BatchStatus::Pending
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut Connection<'a>,
    ) -> Result<usize> {
        Ok(diesel::insert_into(transaction_digests::table)
            .values(batch)
            .on_conflict(transaction_digests::tx_digest)
            .do_nothing()
            .execute(conn)
            .await?)
    }
}
// docs::/#handler
