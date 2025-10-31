// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Example of using the PostgreSQL-specific handler trait for concurrent pipelines
// docs::#imports
use std::sync::Arc;
use anyhow::Result;
use async_trait::async_trait;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    pipeline::Processor,
    postgres::{Connection, handler::Handler},
    types::full_checkpoint_content::Checkpoint,
};

use crate::models::StoredTransactionDigest;
use crate::schema::transaction_digests;
// docs::/#imports

pub struct PostgresTransactionDigestHandler;

// docs::#processor
#[async_trait]
impl Processor for PostgresTransactionDigestHandler {
    const NAME: &'static str = "postgres_transaction_digest_handler";

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
impl Handler for PostgresTransactionDigestHandler {
    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        Ok(diesel::insert_into(transaction_digests::table)
            .values(values)
            .on_conflict(transaction_digests::tx_digest)
            .do_nothing()
            .execute(conn)
            .await?)
    }
}
// docs::/#handler
