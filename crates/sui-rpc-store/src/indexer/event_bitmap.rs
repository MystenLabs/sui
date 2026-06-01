// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that populates the
//! [`schema::event_bitmap`](crate::schema::event_bitmap) CF.
//!
//! This first cut indexes a single dimension — event type
//! (`StructTag`) — encoded as `[DIM_EVENT_TYPE][bcs(StructTag)]`.
//! Other dimensions (emitting module, sender, etc.) extend the
//! pipeline by emitting additional rows from `process` with their
//! own discriminator byte.

use std::sync::Arc;

use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::indexer::Schema;
use crate::indexer::Store;
use crate::indexer::tx_seq_at;
use crate::schema::event_bitmap;

/// Discriminator byte for the "event type" dimension.
pub const DIM_EVENT_TYPE: u8 = 0x01;

/// Pipeline marker for `event_bitmap`.
pub struct EventBitmap;

pub struct Row {
    pub dimension_key: Vec<u8>,
    pub tx_seq: u64,
    pub event_idx: u32,
}

#[async_trait]
impl Processor for EventBitmap {
    const NAME: &'static str = "event_bitmap";
    type Value = Row;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Row>> {
        let mut rows = Vec::new();
        for (i, tx) in checkpoint.transactions.iter().enumerate() {
            let Some(events) = tx.events.as_ref() else {
                continue;
            };
            let tx_seq = tx_seq_at(checkpoint, i);
            for (event_idx, event) in events.data.iter().enumerate() {
                let type_bcs = bcs::to_bytes(&event.type_)
                    .map_err(|e| anyhow::anyhow!("bcs encode event type: {e}"))?;
                let mut dim = Vec::with_capacity(1 + type_bcs.len());
                dim.push(DIM_EVENT_TYPE);
                dim.extend_from_slice(&type_bcs);
                rows.push(Row {
                    dimension_key: dim,
                    tx_seq,
                    event_idx: event_idx as u32,
                });
            }
        }
        Ok(rows)
    }
}

#[async_trait]
impl sequential::Handler for EventBitmap {
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
        let cf = &conn.store.schema().event_bitmap;
        for row in batch {
            let (k, v) = event_bitmap::store_match(
                row.dimension_key.clone(),
                row.tx_seq,
                row.event_idx,
            );
            conn.batch.merge(cf, &k, &v)?;
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
    async fn process_runs_against_synthetic_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(1).build_checkpoint());
        let _rows = EventBitmap.process(&checkpoint).await.unwrap();
    }
}
