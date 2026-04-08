// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! BigTable Store implementation for sui-indexer-alt-framework.
//!
//! This implements the `Store`, `Connection`, and `ConcurrentConnection` traits to allow the
//! new framework to use BigTable for watermark storage. Per-pipeline watermarks are stored in
//! the `watermark_alt` table as:
//! - `w` (BCS v0 `WatermarkV0`) — kept in sync for backward compatibility.
//! - `v` (schema version, currently `1`) — marks the row as using the new schema.
//! - `ehi` / `chi` / `th` / `tmhi` / `rl` / `ph` / `ptm` — one u64 BE cell per
//!   [`WatermarkV1`] field. `chi` (checkpoint) is absent when the committer has not observed
//!   a checkpoint yet.
//!
//! The framework runs a single writer per pipeline role, so regression prevention on the
//! committer watermark is a read-then-decide pattern rather than a BigTable CAS. Reader and
//! pruner watermarks are unconditional writes.

use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::Result;
use anyhow::bail;
use async_trait::async_trait;
use bytes::Bytes;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use sui_indexer_alt_framework_store_traits::ConcurrentConnection;
use sui_indexer_alt_framework_store_traits::Connection;
use sui_indexer_alt_framework_store_traits::InitWatermark;
use sui_indexer_alt_framework_store_traits::PrunerWatermark;
use sui_indexer_alt_framework_store_traits::ReaderWatermark;
use sui_indexer_alt_framework_store_traits::Store;

use crate::WatermarkV0;
use crate::WatermarkV1;
use crate::bigtable::client::BigTableClient;
use crate::tables::watermarks::col;
use crate::tables::watermarks::decode_v0;
use crate::tables::watermarks::decode_v1;

/// A Store implementation backed by BigTable.
#[derive(Clone)]
pub struct BigTableStore {
    client: BigTableClient,
}

/// A connection to BigTable for watermark operations and data writes.
pub struct BigTableConnection<'a> {
    client: BigTableClient,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl BigTableStore {
    pub fn new(client: BigTableClient) -> Self {
        Self { client }
    }
}

impl BigTableConnection<'_> {
    /// Returns a mutable reference to the underlying BigTable client.
    pub fn client(&mut self) -> &mut BigTableClient {
        &mut self.client
    }

    /// Read a watermark for read-side methods. Enforces the "hide if `checkpoint < reader_lo`
    /// or `checkpoint == None`" rule and returns the unwrapped checkpoint for callers that
    /// need it.
    async fn get_watermark_for_read(
        &mut self,
        pipeline: &str,
    ) -> Result<Option<(WatermarkV1, u64)>> {
        let row = self.client.get_pipeline_watermark_rows(pipeline).await?;
        let Some(watermark) = decode_v1(&row)? else {
            return Ok(None);
        };
        let Some(checkpoint_hi_inclusive) = watermark
            .checkpoint_hi_inclusive
            .filter(|&cp| cp >= watermark.reader_lo)
        else {
            return Ok(None);
        };
        Ok(Some((watermark, checkpoint_hi_inclusive)))
    }
}

#[async_trait]
impl sui_indexer_alt_framework_store_traits::ConcurrentStore for BigTableStore {
    type ConcurrentConnection<'c> = BigTableConnection<'c>;
}

#[async_trait]
impl Store for BigTableStore {
    type Connection<'c> = BigTableConnection<'c>;

    async fn connect<'c>(&'c self) -> Result<Self::Connection<'c>> {
        Ok(BigTableConnection {
            client: self.client.clone(),
            _marker: std::marker::PhantomData,
        })
    }
}

#[async_trait]
impl Connection for BigTableConnection<'_> {
    async fn init_watermark(
        &mut self,
        pipeline_task: &str,
        checkpoint_hi_inclusive: Option<u64>,
    ) -> Result<Option<InitWatermark>> {
        // This initial read is to determine if we need to migrate from v0 to v1.
        let row = self
            .client
            .get_pipeline_watermark_rows(pipeline_task)
            .await?;
        let existing_v1 = decode_v1(&row)?;
        let existing_v0 = decode_v0(&row)?;

        // Case 1: row already in the v1 format → return its values, no write.
        if let Some(wm) = existing_v1 {
            return Ok(Some(InitWatermark {
                checkpoint_hi_inclusive: wm.checkpoint_hi_inclusive,
                reader_lo: Some(wm.reader_lo),
            }));
        }

        let initial = if let Some(v0) = existing_v0 {
            // Case 2: v0-only row → bootstrap a v1 watermark from the v0 committer fields.
            let reader_lo = v0.checkpoint_hi_inclusive + 1;
            WatermarkV1 {
                epoch_hi_inclusive: v0.epoch_hi_inclusive,
                checkpoint_hi_inclusive: Some(v0.checkpoint_hi_inclusive),
                tx_hi: v0.tx_hi,
                timestamp_ms_hi_inclusive: v0.timestamp_ms_hi_inclusive,
                reader_lo,
                pruner_hi: reader_lo,
                pruner_timestamp_ms: 0,
            }
        } else {
            // Case 3: nothing exists → write a fresh row from the framework's input.
            let reader_lo = checkpoint_hi_inclusive.map_or(0, |cp| cp + 1);
            WatermarkV1 {
                epoch_hi_inclusive: 0,
                checkpoint_hi_inclusive,
                tx_hi: 0,
                timestamp_ms_hi_inclusive: 0,
                reader_lo,
                pruner_hi: reader_lo,
                pruner_timestamp_ms: 0,
            }
        };

        let write_happened = self
            .client
            .create_pipeline_watermark_if_absent(pipeline_task, &initial)
            .await?;

        let (checkpoint_hi_inclusive, reader_lo) = if write_happened {
            (initial.checkpoint_hi_inclusive, initial.reader_lo)
        } else {
            let row = self
                .client
                .get_pipeline_watermark_rows(pipeline_task)
                .await?;
            let Some(wm) = decode_v1(&row)? else {
                bail!(
                    "watermark for pipeline {} missing after creation",
                    pipeline_task
                );
            };
            (wm.checkpoint_hi_inclusive, wm.reader_lo)
        };

        Ok(Some(InitWatermark {
            checkpoint_hi_inclusive,
            reader_lo: Some(reader_lo),
        }))
    }

    async fn accepts_chain_id(
        &mut self,
        _pipeline_task: &str,
        _chain_id: [u8; 32],
    ) -> Result<bool> {
        // TODO: Implement storing chain_id
        Ok(true)
    }

    async fn committer_watermark(
        &mut self,
        pipeline_task: &str,
    ) -> Result<Option<CommitterWatermark>> {
        Ok(self.get_watermark_for_read(pipeline_task).await?.map(
            |(wm, checkpoint_hi_inclusive)| CommitterWatermark {
                epoch_hi_inclusive: wm.epoch_hi_inclusive,
                checkpoint_hi_inclusive,
                tx_hi: wm.tx_hi,
                timestamp_ms_hi_inclusive: wm.timestamp_ms_hi_inclusive,
            },
        ))
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline_task: &str,
        watermark: CommitterWatermark,
    ) -> Result<bool> {
        let v0 = WatermarkV0 {
            epoch_hi_inclusive: watermark.epoch_hi_inclusive,
            checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive,
            tx_hi: watermark.tx_hi,
            timestamp_ms_hi_inclusive: watermark.timestamp_ms_hi_inclusive,
        };
        let cells = vec![
            (col::EPOCH_HI, u64_be(watermark.epoch_hi_inclusive)),
            (
                col::CHECKPOINT_HI,
                u64_be(watermark.checkpoint_hi_inclusive),
            ),
            (col::TX_HI, u64_be(watermark.tx_hi)),
            (
                col::TIMESTAMP_MS_HI,
                u64_be(watermark.timestamp_ms_hi_inclusive),
            ),
            (col::WATERMARK_V0, Bytes::from(bcs::to_bytes(&v0)?)),
        ];
        self.client
            .cas_write_pipeline_watermark_cells(
                pipeline_task,
                col::CHECKPOINT_HI,
                watermark.checkpoint_hi_inclusive,
                cells,
            )
            .await
    }
}

#[async_trait]
impl ConcurrentConnection for BigTableConnection<'_> {
    async fn reader_watermark(&mut self, pipeline: &str) -> Result<Option<ReaderWatermark>> {
        Ok(self
            .get_watermark_for_read(pipeline)
            .await?
            .map(|(wm, checkpoint_hi_inclusive)| ReaderWatermark {
                checkpoint_hi_inclusive,
                reader_lo: wm.reader_lo,
            }))
    }

    async fn pruner_watermark(
        &mut self,
        pipeline: &'static str,
        delay: Duration,
    ) -> Result<Option<PrunerWatermark>> {
        let Some((watermark, _)) = self.get_watermark_for_read(pipeline).await? else {
            return Ok(None);
        };
        // Compute max(0, (pruner_timestamp + delay) - now). Use u128 to avoid overflow when
        // summing the two operands, and saturating_sub so we never underflow when the wait
        // period has already elapsed. saturating_sub is safe because callers treat anything
        // < 1 the same.
        let pruner_ready_ms = (watermark.pruner_timestamp_ms as u128) + delay.as_millis();
        let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
        let wait_for_ms = i64::try_from(pruner_ready_ms.saturating_sub(now_ms))?;
        Ok(Some(PrunerWatermark {
            wait_for_ms,
            reader_lo: watermark.reader_lo,
            pruner_hi: watermark.pruner_hi,
        }))
    }

    async fn set_reader_watermark(
        &mut self,
        pipeline: &'static str,
        reader_lo: u64,
    ) -> Result<bool> {
        let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let cells = vec![
            (col::READER_LO, u64_be(reader_lo)),
            (col::PRUNER_TIMESTAMP_MS, u64_be(now_ms)),
        ];
        self.client
            .cas_write_pipeline_watermark_cells(pipeline, col::READER_LO, reader_lo, cells)
            .await
    }

    async fn set_pruner_watermark(
        &mut self,
        pipeline: &'static str,
        pruner_hi: u64,
    ) -> Result<bool> {
        let cells = vec![(col::PRUNER_HI, u64_be(pruner_hi))];
        self.client
            .cas_write_pipeline_watermark_cells(pipeline, col::PRUNER_HI, pruner_hi, cells)
            .await
    }
}

fn u64_be(v: u64) -> Bytes {
    Bytes::copy_from_slice(&v.to_be_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::testing::BigTableEmulator;
    use crate::testing::INSTANCE_ID;
    use crate::testing::create_tables;
    use crate::testing::require_bigtable_emulator;

    const PIPELINE: &str = "pipeline";
    const EPOCH_HI: u64 = 7;
    const CHECKPOINT_HI: u64 = 200;
    const TX_HI: u64 = 42;
    const TIMESTAMP_MS_HI: u64 = 99;

    /// Spawn a BigTable emulator and return a connected store.
    async fn store_conn() -> (BigTableEmulator, BigTableStore) {
        require_bigtable_emulator();
        let emulator = tokio::task::spawn_blocking(BigTableEmulator::start)
            .await
            .unwrap()
            .unwrap();
        create_tables(emulator.host(), INSTANCE_ID).await.unwrap();
        let client = BigTableClient::new_local(emulator.host().to_string(), INSTANCE_ID.into())
            .await
            .unwrap();
        (emulator, BigTableStore::new(client))
    }

    #[tokio::test]
    async fn test_init_watermark_returns_existing_on_conflict() {
        let (_emulator, store) = store_conn().await;
        let mut conn = store.connect().await.unwrap();

        conn.init_watermark(PIPELINE, None).await.unwrap();
        let watermark = CommitterWatermark {
            epoch_hi_inclusive: EPOCH_HI,
            checkpoint_hi_inclusive: CHECKPOINT_HI,
            tx_hi: TX_HI,
            timestamp_ms_hi_inclusive: TIMESTAMP_MS_HI,
        };
        conn.set_committer_watermark(PIPELINE, watermark)
            .await
            .unwrap();

        // init must surface the existing committer watermark regardless of the input.
        let init = conn
            .init_watermark(PIPELINE, Some(0))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(init.checkpoint_hi_inclusive, Some(CHECKPOINT_HI));
        // init_watermark now bootstraps a `reader_lo` when it surfaces an existing new-schema
        // row — set_committer_watermark leaves the `reader_lo` that init originally wrote
        // (0 for init(None) + set_committer, which is what happened here).
        assert_eq!(init.reader_lo, Some(0));
    }
}
