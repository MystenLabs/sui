// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::events`](crate::schema::events) CF: one row per
//! executed transaction carrying its [`TransactionEvents`].
//!
//! Transactions that emitted no events get an empty
//! `TransactionEvents` row rather than no row at all, so callers
//! can rely on "row missing" meaning "this `tx_seq` hasn't been
//! indexed yet" instead of conflating it with "this transaction
//! emitted nothing".

use std::sync::Arc;

use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::effects::TransactionEvents;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::indexer::Schema;
use crate::indexer::Store;
use crate::indexer::tx_seq_at;
use crate::schema::events;
use crate::schema::keys::U64Be;

/// Pipeline marker for `events`.
pub struct Events;

pub struct Row {
    pub tx_seq: u64,
    pub value: events::Value,
}

#[async_trait]
impl Processor for Events {
    const NAME: &'static str = "events";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let mut rows = Vec::with_capacity(checkpoint.transactions.len());
        for (i, tx) in checkpoint.transactions.iter().enumerate() {
            let owned;
            let events = match tx.events.as_ref() {
                Some(events) => events,
                None => {
                    owned = TransactionEvents::default();
                    &owned
                }
            };
            rows.push(Row {
                tx_seq: tx_seq_at(checkpoint, i),
                value: events::store(events),
            });
        }
        Ok(rows)
    }
}

#[async_trait]
impl sequential::Handler for Events {
    type Store = Store;
    type Batch = Vec<Row>;

    fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Row>) {
        batch.extend(values);
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut sui_consistent_store::Connection<'a, Schema>,
    ) -> anyhow::Result<usize> {
        let cf = &conn.store.schema().events;
        for row in batch {
            conn.batch.put(cf, &U64Be(row.tx_seq), &row.value)?;
        }
        Ok(batch.len())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;

    #[tokio::test]
    async fn process_emits_one_row_per_transaction() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(2).build_checkpoint());
        let rows = Events.process(&checkpoint).await.unwrap();
        assert_eq!(rows.len(), checkpoint.transactions.len());
    }
}
