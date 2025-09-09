// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#processordeps (see sui/docs/content/guides/developer/advanced/custom-indexer.mdx)
use std::sync::Arc;
use anyhow::Result;
use sui_indexer_alt_framework::{
    pipeline::Processor,
    types::full_checkpoint_content::CheckpointData,
};

use crate::models::StoredTransactionDigest;
use crate::schema::transaction_digests::dsl::*;
// docs::/#processordeps
// docs::#handlerdeps
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    postgres::{Connection, Db},
    pipeline::sequential::Handler,
};
// docs::/#handlerdeps

pub struct TransactionDigestHandler;

// docs::#processor
impl Processor for TransactionDigestHandler {
    const NAME: &'static str = "transaction_digest_handler";

    type Value = StoredTransactionDigest;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let checkpoint_seq = checkpoint.checkpoint_summary.sequence_number as i64;

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
#[async_trait::async_trait]
impl Handler for TransactionDigestHandler {
    type Store = Db;
    type Batch = Vec<Self::Value>;

    fn batch(batch: &mut Self::Batch, values: Vec<Self::Value>) {
        batch.extend(values);
    }

    async fn commit<'a>(
        batch: &Self::Batch,
        conn: &mut Connection<'a>,
    ) -> Result<usize> {
        let inserted = diesel::insert_into(transaction_digests)
            .values(batch)
            .on_conflict(tx_digest)
            .do_nothing()
            .execute(conn)
            .await?;

        Ok(inserted)
    }
}
// docs::/#handler
