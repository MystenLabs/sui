// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Integration tests for the BigTable watermark read/write paths.
//!
//! Each test spawns its own BigTable emulator process on a random port and creates the
//! required tables. Tests require `gcloud`, `cbt`, and the BigTable emulator on PATH.

use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use bytes::Bytes;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use sui_indexer_alt_framework_store_traits::ConcurrentConnection;
use sui_indexer_alt_framework_store_traits::Connection;
use sui_indexer_alt_framework_store_traits::Store;
use sui_indexer_alt_framework_store_traits::concurrent_connection_tests;
use sui_indexer_alt_framework_store_traits::connection_tests;
use sui_indexer_alt_framework_store_traits::testing::Harness;
use sui_kvstore::BigTableClient;
use sui_kvstore::BigTableConnection;
use sui_kvstore::BigTableStore;
use sui_kvstore::KeyValueStoreReader;
use sui_kvstore::WatermarkV0;
use sui_kvstore::WatermarkV1;
use sui_kvstore::tables;
use sui_kvstore::testing::BigTableEmulator;
use sui_kvstore::testing::INSTANCE_ID;
use sui_kvstore::testing::create_tables;
use sui_kvstore::testing::require_bigtable_emulator;

const PIPELINE: &str = "test_pipeline";

const EPOCH_HI: u64 = 7;
const CHECKPOINT_HI: u64 = 200;
const TX_HI: u64 = 42;
const TIMESTAMP_MS_HI: u64 = 99;
const READER_LO: u64 = 123;

struct WatermarkHarness {
    store: BigTableStore,
    client: BigTableClient,
    _emulator: BigTableEmulator,
}

impl WatermarkHarness {
    async fn new() -> Result<Self> {
        require_bigtable_emulator();
        let emulator = tokio::task::spawn_blocking(BigTableEmulator::start)
            .await
            .context("spawn_blocking panicked")??;
        create_tables(emulator.host(), INSTANCE_ID).await?;
        let client =
            BigTableClient::new_local(emulator.host().to_string(), INSTANCE_ID.to_string()).await?;
        let store = BigTableStore::new(client.clone());
        Ok(Self {
            store,
            client,
            _emulator: emulator,
        })
    }

    async fn connect(&self) -> Result<BigTableConnection<'_>> {
        self.store.connect().await
    }

    /// Convenience wrapper around [`read_raw_cells`] that uses the harness's client.
    async fn cells(&self, pipeline: &str) -> Result<RawCells> {
        read_raw_cells(&mut self.client.clone(), pipeline).await
    }

    /// Call `KeyValueStoreReader::get_watermark_for_pipelines` against the harness's client.
    async fn read_watermark(&self, pipelines: &[&str]) -> Result<Option<WatermarkV1>> {
        self.client
            .clone()
            .get_watermark_for_pipelines(pipelines)
            .await
    }

    /// Bootstrap a pipeline with a committed checkpoint. `pruner_watermark` and the read-side
    /// helpers hide rows whose `checkpoint_hi_inclusive < reader_lo`. To make a row visible we
    /// need to advance the committer past `reader_lo` (which `init(None)` sets to 0 — so any
    /// committed checkpoint works).
    async fn bootstrap_with_committed_checkpoint(
        &self,
        pipeline: &'static str,
        checkpoint: u64,
    ) -> Result<()> {
        let mut conn = self.connect().await?;
        conn.init_watermark(pipeline, None).await?;
        conn.set_committer_watermark(
            pipeline,
            CommitterWatermark {
                epoch_hi_inclusive: 0,
                checkpoint_hi_inclusive: checkpoint,
                tx_hi: 0,
                timestamp_ms_hi_inclusive: 0,
            },
        )
        .await?;
        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
impl Harness for WatermarkHarness {
    type Store = BigTableStore;

    async fn new() -> Self {
        WatermarkHarness::new().await.unwrap()
    }

    fn store(&self) -> &Self::Store {
        &self.store
    }
}

/// Snapshot of a watermark row's distinguishing cells.
#[derive(Default)]
struct RawCells {
    /// The v0 BCS `w` cell, if present.
    w: Option<Bytes>,
    /// True iff the v1 per-field schema has been written (detected by presence of the `ehi`
    /// cell, which is always written alongside the rest of the v1 cells).
    has_v1: bool,
    /// The value of the `chi` (checkpoint_hi_inclusive) cell — `None` when the cell is absent,
    /// which is the post-`init(None)` state.
    checkpoint_hi: Option<u64>,
    /// The schema-version (`v`) cell value, if present.
    schema_version: Option<u64>,
    /// The `rl` (reader_lo) cell value, if present.
    reader_lo: Option<u64>,
    /// The `ph` (pruner_hi) cell value, if present.
    pruner_hi: Option<u64>,
    /// The `ptm` (pruner_timestamp_ms) cell value, if present.
    pruner_timestamp_ms: Option<u64>,
}

// Shared connection test macros use `WatermarkHarness` so the emulator stays alive for each test.
connection_tests!(WatermarkHarness);
concurrent_connection_tests!(WatermarkHarness);

async fn read_raw_cells(client: &mut BigTableClient, pipeline: &str) -> Result<RawCells> {
    let key = tables::watermarks::encode_key(pipeline);
    let rows = client
        .multi_get(tables::watermarks::NAME, vec![key.clone()], None)
        .await?;
    let mut cells = RawCells::default();
    let decode_u64 = |val: &Bytes| -> u64 {
        let mut buf = [0u8; 8];
        buf.copy_from_slice(val);
        u64::from_be_bytes(buf)
    };
    for (row_key, row) in rows {
        if row_key.as_ref() != key.as_slice() {
            continue;
        }
        for (col, val) in row {
            match col.as_ref() {
                b if b == tables::watermarks::col::WATERMARK_V0.as_bytes() => cells.w = Some(val),
                b if b == tables::watermarks::col::EPOCH_HI.as_bytes() => {
                    cells.has_v1 = true;
                }
                b if b == tables::watermarks::col::CHECKPOINT_HI.as_bytes() => {
                    cells.checkpoint_hi = Some(decode_u64(&val));
                }
                b if b == tables::watermarks::col::SCHEMA_VERSION.as_bytes() => {
                    cells.schema_version = Some(decode_u64(&val));
                }
                b if b == tables::watermarks::col::READER_LO.as_bytes() => {
                    cells.reader_lo = Some(decode_u64(&val));
                }
                b if b == tables::watermarks::col::PRUNER_HI.as_bytes() => {
                    cells.pruner_hi = Some(decode_u64(&val));
                }
                b if b == tables::watermarks::col::PRUNER_TIMESTAMP_MS.as_bytes() => {
                    cells.pruner_timestamp_ms = Some(decode_u64(&val));
                }
                _ => {}
            }
        }
    }
    Ok(cells)
}

#[tokio::test]
async fn test_init_watermark_v0_bootstrap() -> Result<()> {
    let harness = WatermarkHarness::new().await?;

    // Seed a BCS `WatermarkV0` directly into the `w` column.
    let v0 = WatermarkV0 {
        epoch_hi_inclusive: EPOCH_HI,
        checkpoint_hi_inclusive: CHECKPOINT_HI,
        tx_hi: TX_HI,
        timestamp_ms_hi_inclusive: TIMESTAMP_MS_HI,
    };
    let cell = tables::watermarks::encode_v0(&v0)?;
    let entry = tables::make_entry(
        tables::watermarks::encode_key(PIPELINE),
        [cell],
        Some(TIMESTAMP_MS_HI),
    );
    harness
        .client
        .clone()
        .write_entries(tables::watermarks::NAME, [entry])
        .await?;

    // Now run init_watermark — it should bootstrap the v1 cells from the v0 committer
    // fields and leave the v0 `w` cell untouched.
    let mut conn = harness.connect().await?;
    let init = conn.init_watermark(PIPELINE, Some(0)).await?.unwrap();
    assert_eq!(init.checkpoint_hi_inclusive, Some(CHECKPOINT_HI));
    assert_eq!(init.reader_lo, Some(CHECKPOINT_HI + 1));

    let cells = harness.cells(PIPELINE).await?;
    assert!(cells.w.is_some(), "v0 `w` cell must be preserved");
    assert!(cells.has_v1, "v1 cells must be written");
    assert_eq!(cells.schema_version, Some(1));
    Ok(())
}

#[tokio::test]
async fn test_set_reader_watermark_after_init_none_skips_v0() -> Result<()> {
    let harness = WatermarkHarness::new().await?;
    let mut conn = harness.connect().await?;
    conn.init_watermark(PIPELINE, None).await?;

    assert!(conn.set_reader_watermark(PIPELINE, READER_LO).await?);

    let cells = harness.cells(PIPELINE).await?;
    assert!(
        cells.w.is_none(),
        "set_reader_watermark must not introduce `w` when checkpoint is still None"
    );
    assert!(cells.has_v1);
    assert_eq!(cells.schema_version, Some(1));
    Ok(())
}

#[tokio::test]
async fn test_pruner_watermark_saturates_when_ready() -> Result<()> {
    let harness = WatermarkHarness::new().await?;
    harness
        .bootstrap_with_committed_checkpoint(PIPELINE, CHECKPOINT_HI)
        .await?;
    let mut conn = harness.connect().await?;

    let pruner = conn
        .pruner_watermark(PIPELINE, Duration::ZERO)
        .await?
        .unwrap();
    assert_eq!(pruner.wait_for_ms, 0);
    Ok(())
}

#[tokio::test]
async fn test_set_reader_watermark_rejects_stale() -> Result<()> {
    let harness = WatermarkHarness::new().await?;
    let mut conn = harness.connect().await?;
    conn.init_watermark(PIPELINE, None).await?;

    assert!(conn.set_reader_watermark(PIPELINE, READER_LO).await?);
    let advanced = harness.cells(PIPELINE).await?;

    // Equal value must be rejected (strict `>` semantics).
    assert!(!conn.set_reader_watermark(PIPELINE, READER_LO).await?);
    // Strictly lower value must be rejected.
    assert!(!conn.set_reader_watermark(PIPELINE, READER_LO - 1).await?);

    let after_rejects = harness.cells(PIPELINE).await?;
    assert_eq!(after_rejects.reader_lo, advanced.reader_lo);
    assert_eq!(
        after_rejects.pruner_timestamp_ms, advanced.pruner_timestamp_ms,
        "rejected reader writes must not bump the pruner timestamp"
    );
    Ok(())
}

#[tokio::test]
async fn test_set_reader_watermark_advances_pruner_timestamp() -> Result<()> {
    let harness = WatermarkHarness::new().await?;
    let mut conn = harness.connect().await?;
    conn.init_watermark(PIPELINE, None).await?;

    assert!(conn.set_reader_watermark(PIPELINE, READER_LO).await?);
    let first = harness.cells(PIPELINE).await?;
    let first_ts = first.pruner_timestamp_ms.expect("ptm cell must be written");

    // Sleep past 1ms so the second timestamp is observably greater.
    tokio::time::sleep(Duration::from_millis(2)).await;

    assert!(
        conn.set_reader_watermark(PIPELINE, READER_LO + 1).await?,
        "strictly greater reader_lo must succeed"
    );
    let second = harness.cells(PIPELINE).await?;
    let second_ts = second
        .pruner_timestamp_ms
        .expect("ptm cell must still be present");
    assert!(
        second_ts > first_ts,
        "pruner_timestamp_ms must advance with reader update (was {first_ts}, now {second_ts})"
    );
    Ok(())
}

#[tokio::test]
async fn test_get_watermark_for_pipelines_hides_init_none() -> Result<()> {
    // After init(None), the row exists but `checkpoint_hi_inclusive` is `None`. The hide
    // rule must short-circuit `get_watermark_for_pipelines` to `Ok(None)`.
    let harness = WatermarkHarness::new().await?;
    {
        let mut conn = harness.connect().await?;
        conn.init_watermark(PIPELINE, None).await?;
    }
    let wm = harness.read_watermark(&[PIPELINE]).await?;
    assert!(
        wm.is_none(),
        "init(None) row must be hidden by the read API"
    );
    Ok(())
}

#[tokio::test]
async fn test_get_watermark_for_pipelines_hides_below_reader_lo() -> Result<()> {
    // A row with a real checkpoint becomes hidden once `reader_lo` is raised past it.
    let harness = WatermarkHarness::new().await?;
    harness
        .bootstrap_with_committed_checkpoint(PIPELINE, CHECKPOINT_HI)
        .await?;
    let mut conn = harness.connect().await?;

    let visible = harness.read_watermark(&[PIPELINE]).await?;
    assert!(
        visible.is_some(),
        "row with checkpoint >= reader_lo must be visible"
    );

    conn.set_reader_watermark(PIPELINE, CHECKPOINT_HI + 1)
        .await?;
    let hidden = harness.read_watermark(&[PIPELINE]).await?;
    assert!(
        hidden.is_none(),
        "row with checkpoint < reader_lo must be hidden"
    );
    Ok(())
}

#[tokio::test]
async fn test_get_watermark_for_pipelines_ignores_v0_only() -> Result<()> {
    // A row that only has the v0 `w` column (e.g. seeded by an older indexer) is no longer
    // surfaced after the switch to reading the v1 per-field columns.
    let harness = WatermarkHarness::new().await?;
    let v0 = WatermarkV0 {
        epoch_hi_inclusive: EPOCH_HI,
        checkpoint_hi_inclusive: CHECKPOINT_HI,
        tx_hi: TX_HI,
        timestamp_ms_hi_inclusive: TIMESTAMP_MS_HI,
    };
    let cell = tables::watermarks::encode_v0(&v0)?;
    let entry = tables::make_entry(
        tables::watermarks::encode_key(PIPELINE),
        [cell],
        Some(TIMESTAMP_MS_HI),
    );
    harness
        .client
        .clone()
        .write_entries(tables::watermarks::NAME, [entry])
        .await?;

    let wm = harness.read_watermark(&[PIPELINE]).await?;
    assert!(
        wm.is_none(),
        "v0-only rows must be hidden by the v1 read path"
    );
    Ok(())
}

#[tokio::test]
async fn test_get_watermark_for_pipelines_returns_minimum() -> Result<()> {
    // Across multiple pipelines, the read API selects the watermark with the lowest
    // `checkpoint_hi_inclusive`. If any pipeline is hidden, the whole result is `None`.
    const PIPELINE_LO: &str = "pipeline_lo";
    const PIPELINE_HI: &str = "pipeline_hi";
    const PIPELINE_MISSING: &str = "pipeline_missing";

    let harness = WatermarkHarness::new().await?;
    harness
        .bootstrap_with_committed_checkpoint(PIPELINE_LO, 50)
        .await?;
    harness
        .bootstrap_with_committed_checkpoint(PIPELINE_HI, 100)
        .await?;

    let wm = harness
        .read_watermark(&[PIPELINE_LO, PIPELINE_HI])
        .await?
        .unwrap();
    assert_eq!(
        wm.checkpoint_hi_inclusive,
        Some(50),
        "must select the minimum checkpoint across pipelines"
    );

    // Adding a missing pipeline must short-circuit to `None`.
    let wm = harness
        .read_watermark(&[PIPELINE_LO, PIPELINE_HI, PIPELINE_MISSING])
        .await?;
    assert!(
        wm.is_none(),
        "any missing pipeline must hide the whole result"
    );
    Ok(())
}
