// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use bytes::Bytes;
use roaring::RoaringBitmap;
use scoped_futures::ScopedFutureExt;
use sui_futures::service::Service;
use sui_indexer_alt_framework::pipeline::sequential::Handler;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use sui_indexer_alt_framework_store_traits::Connection;
use sui_indexer_alt_framework_store_traits::SequentialStore;
use sui_indexer_alt_framework_store_traits::Store;
use sui_inverted_index::IndexDimension;
use sui_types::full_checkpoint_content::Checkpoint;

use super::NUM_SHARDS;
use super::shard_for;
use crate::WatermarkV1;
use crate::bigtable::client::BigTableClient;
use crate::bigtable::mock_server::ExpectedCall;
use crate::bigtable::mock_server::MockBigtableServer;
use crate::config::SequentialLayer;
use crate::handlers::BitmapBatch;
use crate::handlers::BitmapIndexHandler;
use crate::handlers::BitmapIndexProcessor;
use crate::handlers::BitmapIndexValue;
use crate::rate_limiter::CompositeRateLimiter;
use crate::store::BigTableStore;
use crate::tables;
use crate::tables::event_bitmap_index;
use crate::tables::transaction_bitmap_index;

const PIPELINE: &str = "test_bitmap";
const TABLE: &str = transaction_bitmap_index::NAME;
const FAMILY: &str = tables::FAMILY;
const COL: &str = transaction_bitmap_index::col::BITMAP;
const BUCKET_SIZE: u64 = transaction_bitmap_index::BUCKET_SIZE;

struct TestProcessor;

impl BitmapIndexProcessor for TestProcessor {
    const NAME: &'static str = PIPELINE;
    const TABLE: &'static str = TABLE;
    const COLUMN: &'static str = COL;
    const SCHEMA_VERSION: u32 = transaction_bitmap_index::SCHEMA_VERSION;
    const BUCKET_ID_WIDTH: usize = transaction_bitmap_index::BUCKET_ID_WIDTH;

    fn for_each_indexed_bit<F>(&self, _: &Checkpoint, _: F)
    where
        F: FnMut(u64, u32, IndexDimension, &[u8]),
    {
    }

    fn is_sealed(bucket_id: u64, watermark: CommitterWatermark) -> bool {
        watermark.tx_hi >= (bucket_id + 1) * BUCKET_SIZE
    }
}

struct EventSealProcessor;

impl BitmapIndexProcessor for EventSealProcessor {
    const NAME: &'static str = "test_event_bitmap";
    const TABLE: &'static str = TABLE;
    const COLUMN: &'static str = COL;
    const SCHEMA_VERSION: u32 = transaction_bitmap_index::SCHEMA_VERSION;
    const BUCKET_ID_WIDTH: usize = transaction_bitmap_index::BUCKET_ID_WIDTH;

    fn for_each_indexed_bit<F>(&self, _: &Checkpoint, _: F)
    where
        F: FnMut(u64, u32, IndexDimension, &[u8]),
    {
    }

    fn is_sealed(bucket_id: u64, watermark: CommitterWatermark) -> bool {
        let seal_tx_hi = ((bucket_id + 1) * event_bitmap_index::BUCKET_SIZE)
            .div_ceil(event_bitmap_index::MAX_EVENTS_PER_TX as u64);
        watermark.tx_hi >= seal_tx_hi
    }
}

/// Create a mock BigTable server + store + client. Does NOT drive
/// `init_watermark`, so tests that want to exercise that method against
/// a pre-seeded row can use this variant and then call it themselves.
async fn setup_without_init_watermark() -> (MockBigtableServer, BigTableStore, BigTableClient) {
    let mock = MockBigtableServer::new();
    let (addr, handle) = mock.start().await.unwrap();
    std::mem::forget(handle);
    let client = BigTableClient::new_for_host(addr.to_string(), "test".to_string(), "test")
        .await
        .unwrap();
    let store = BigTableStore::new(client.clone());
    (mock, store, client)
}

fn register_test_committer_with(
    store: &BigTableStore,
    write_chunk_size: usize,
    write_concurrency: usize,
) -> Service {
    store
        .runtime_builder()
        .with_bitmap_committer::<TestProcessor>(
            write_chunk_size,
            write_concurrency,
            Arc::new(CompositeRateLimiter::noop()),
            None,
        )
        .into_service()
}

fn register_test_committer(store: &BigTableStore) -> Service {
    let config = SequentialLayer::default();
    register_test_committer_with(
        store,
        config.max_rows_or_default(),
        config.write_concurrency.unwrap_or(1),
    )
}

async fn setup_with_committer(
    write_chunk_size: usize,
    write_concurrency: usize,
) -> (
    MockBigtableServer,
    BigTableStore,
    BitmapIndexHandler<TestProcessor>,
    Service,
) {
    let (mock, store, _client) = setup_without_init_watermark().await;
    store
        .connect()
        .await
        .unwrap()
        .init_watermark(PIPELINE, None)
        .await
        .unwrap();
    let handler = BitmapIndexHandler::new(TestProcessor);
    let service = register_test_committer_with(&store, write_chunk_size, write_concurrency);
    (mock, store, handler, service)
}

async fn setup() -> (
    MockBigtableServer,
    BigTableStore,
    BitmapIndexHandler<TestProcessor>,
    Service,
) {
    let (mock, store, _client) = setup_without_init_watermark().await;
    // Simulate the framework's pre-commit `init_watermark` call so the
    // generation task finds a populated `init_results` entry when it reads it on
    // its first message.
    store
        .connect()
        .await
        .unwrap()
        .init_watermark(PIPELINE, None)
        .await
        .unwrap();
    // `service` carries the pipeline's `JoinHandle`s; callers must keep
    // it alive for the duration of the test because dropping a Service
    // aborts all its tasks.
    let handler = BitmapIndexHandler::new(TestProcessor);
    let service = register_test_committer(&store);
    (mock, store, handler, service)
}

fn watermark(cp: u64, tx_hi: u64, ts_ms: u64) -> CommitterWatermark {
    CommitterWatermark {
        epoch_hi_inclusive: 0,
        checkpoint_hi_inclusive: cp,
        tx_hi,
        timestamp_ms_hi_inclusive: ts_ms,
    }
}

async fn create_watermark_v1(
    client: &mut BigTableClient,
    watermark: CommitterWatermark,
    bucket_start_cp: Option<u64>,
) {
    client
        .create_pipeline_watermark_if_absent(
            PIPELINE,
            &WatermarkV1 {
                epoch_hi_inclusive: watermark.epoch_hi_inclusive,
                checkpoint_hi_inclusive: Some(watermark.checkpoint_hi_inclusive),
                tx_hi: watermark.tx_hi,
                timestamp_ms_hi_inclusive: watermark.timestamp_ms_hi_inclusive,
                reader_lo: 0,
                pruner_hi: 0,
                pruner_timestamp_ms: 0,
                bucket_start_cp,
            },
        )
        .await
        .unwrap();
}

fn make_batch(values: Vec<BitmapIndexValue>) -> BitmapBatch {
    let handler = BitmapIndexHandler::new(TestProcessor);
    let mut batch = BitmapBatch::default();
    handler.batch(&mut batch, values.into_iter());
    batch
}

fn value(
    row_key: &[u8],
    bucket_id: u64,
    bits: &[u32],
    max_cp: u64,
    max_ts_ms: u64,
) -> BitmapIndexValue {
    let mut bitmap = RoaringBitmap::new();
    for &b in bits {
        bitmap.insert(b);
    }
    let shard_id = shard_for(row_key) as u8;
    BitmapIndexValue {
        row_key: Bytes::copy_from_slice(row_key),
        bucket_id,
        bitmap,
        max_cp,
        max_ts_ms,
        shard_id,
    }
}

fn one_row_key_per_shard() -> Vec<Vec<u8>> {
    let mut by_shard = vec![None; NUM_SHARDS];
    for i in 0.. {
        let row_key = format!("v1#dim#{i:010}").into_bytes();
        let shard = shard_for(&row_key);
        if by_shard[shard].is_none() {
            by_shard[shard] = Some(row_key);
            if by_shard.iter().all(Option::is_some) {
                break;
            }
        }
    }

    by_shard.into_iter().map(Option::unwrap).collect()
}

async fn persisted_watermark(store: &BigTableStore) -> Option<CommitterWatermark> {
    let mut conn = store.connect().await.unwrap();
    conn.committer_watermark(PIPELINE).await.unwrap()
}

async fn persisted_bitmap(mock: &MockBigtableServer, row_key: &[u8]) -> Option<RoaringBitmap> {
    let bytes = mock
        .get_cell(TABLE, row_key, FAMILY, COL.as_bytes())
        .await?;
    Some(RoaringBitmap::deserialize_from(bytes.as_ref()).unwrap())
}

async fn write_seed_bitmap(
    client: &mut BigTableClient,
    row_key: &[u8],
    bits: &[u32],
    timestamp_ms: u64,
) {
    let mut bitmap = RoaringBitmap::new();
    for &bit in bits {
        bitmap.insert(bit);
    }
    let mut buf = Vec::with_capacity(bitmap.serialized_size());
    bitmap.serialize_into(&mut buf).unwrap();
    client
        .write_entries(
            TABLE,
            [tables::make_entry(
                Bytes::copy_from_slice(row_key),
                [(COL, Bytes::from(buf))],
                Some(timestamp_ms),
            )],
        )
        .await
        .unwrap();
}

async fn wait_for_watermark_at_least(store: &BigTableStore, checkpoint: u64) -> CommitterWatermark {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(watermark) = persisted_watermark(store).await
            && watermark.checkpoint_hi_inclusive >= checkpoint
        {
            return watermark;
        }
        if std::time::Instant::now() >= deadline {
            panic!("timed out waiting for watermark checkpoint {checkpoint}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

async fn wait_for_bitmap(mock: &MockBigtableServer, row_key: &[u8]) -> RoaringBitmap {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(bitmap) = persisted_bitmap(mock, row_key).await {
            return bitmap;
        }
        if std::time::Instant::now() >= deadline {
            panic!(
                "timed out waiting for bitmap row {}",
                String::from_utf8_lossy(row_key),
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

#[tokio::test]
async fn writes_partial_bucket_and_advances_watermark() {
    let (mock, store, handler, _service) = setup().await;
    let handler = Arc::new(handler);

    // Bucket 0 spans [0, BUCKET_SIZE); watermark tx_hi lands strictly
    // inside it, so nothing seals.
    let row_key = b"v1#dim#0000000000";
    let batch = make_batch(vec![value(row_key, 0, &[0, 5], 0, 1000)]);
    let w = watermark(0, 10, 1000);

    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w).await?;
                h.commit(&batch, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    let persisted = wait_for_watermark_at_least(&store, 0).await;
    let bm = wait_for_bitmap(&mock, row_key).await;
    assert!(bm.contains(0));
    assert!(bm.contains(5));
    assert_eq!(persisted.checkpoint_hi_inclusive, 0);
}

/// `Handler::commit` returns a *lagging* count of rows BigTable accepted since
/// the previous return — not the logical input row count. Depending on
/// scheduling, the first return may already observe the row write or the next
/// commit may drain it; across both drains the row must be counted exactly once.
#[tokio::test]
async fn take_rows_written_counts_each_durable_row_once() {
    let (_mock, store, handler, _service) = setup().await;
    let handler = Arc::new(handler);

    let row_key = b"v1#dim#0000000000";
    let batch = make_batch(vec![value(row_key, 0, &[0, 5], 0, 1000)]);

    let h = handler.clone();
    let affected = store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, watermark(0, 10, 1000))
                    .await?;
                h.commit(&batch, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    // Wait for the row write to land before the second commit so any lagging
    // count is available to drain.
    wait_for_watermark_at_least(&store, 0).await;

    let empty = make_batch(vec![]);
    let h = handler.clone();
    let next_affected = store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, watermark(1, 10, 1001))
                    .await?;
                h.commit(&empty, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    assert_eq!(affected + next_affected, 1);
}

#[tokio::test]
async fn keeps_active_bucket_rows_for_later_bits() {
    let (mock, store, handler, _service) = setup().await;
    let handler = Arc::new(handler);

    let row_key = b"v1#dim#0000000000";
    let batch1 = make_batch(vec![value(row_key, 0, &[1], 1, 1000)]);
    let w1 = watermark(1, 10, 1000);
    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w1).await?;
                h.commit(&batch1, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    wait_for_watermark_at_least(&store, 1).await;

    let batch2 = make_batch(vec![value(row_key, 0, &[2], 2, 2000)]);
    let w2 = watermark(2, 20, 2000);
    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w2).await?;
                h.commit(&batch2, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    wait_for_watermark_at_least(&store, 2).await;

    let bm = wait_for_bitmap(&mock, row_key).await;
    assert!(
        bm.contains(1),
        "first bit lost after active-bucket clean state"
    );
    assert!(bm.contains(2), "second bit missing");
}

/// Until the in-flight chunk's write lands, the watermark must not advance.
#[tokio::test]
async fn watermark_only_advances_after_rows_durable() {
    let (mock, store, handler, _service) = setup().await;
    let write_gate = mock.pause_next_mutate_rows().await;
    let handler = Arc::new(handler);

    let row_key = b"v1#dim#0000000000";
    let batch = make_batch(vec![value(row_key, 0, &[0, 5], 1, 1500)]);
    let w = watermark(1, 10, 1500);

    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w).await?;
                h.commit(&batch, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    write_gate.wait_for_arrival().await;
    // The row write has reached BigTable but is blocked before durability. The
    // watermark must not advance until the write is released and reported.
    assert!(
        persisted_watermark(&store).await.is_none(),
        "watermark advanced before write landed",
    );

    write_gate.release();
    wait_for_watermark_at_least(&store, 1).await;
}

/// Regression test for watermark gating across a write failure —
/// when W1's write fails and W2 commits new bits to the same row
/// before the retry lands, the committer must NOT promote the
/// watermark past W1 until the failed row's retry lands durably.
#[tokio::test]
async fn retry_preserves_generation_until_failed_write_lands() {
    let (mock, store, handler, _service) = setup().await;
    let first_attempt_gate = mock.pause_next_mutate_rows().await;
    let handler = Arc::new(handler);

    let row_key: &[u8] = b"v1#dim#0000000000";
    // Fail the first write deterministically.
    mock.expect(ExpectedCall {
        row_keys: vec![row_key],
        failures: HashMap::from([(0, 8)]),
    })
    .await;

    let batch1 = make_batch(vec![value(row_key, 0, &[1], 1, 1000)]);
    let w1 = watermark(1, 10, 1000);

    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w1).await?;
                h.commit(&batch1, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    first_attempt_gate.wait_for_arrival().await;
    let retry_gate = mock.pause_next_mutate_rows_with_timestamp(1_000_000).await;
    first_attempt_gate.release();
    retry_gate.wait_for_arrival().await;

    // W2 commits with new bits on the same row while W1's retry is blocked.
    let batch2 = make_batch(vec![value(row_key, 0, &[2], 2, 2000)]);
    let w2 = watermark(2, 20, 2000);

    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w2).await?;
                h.commit(&batch2, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    // While W1's retry is still blocked: the watermark must NOT be at or above
    // 2, even though the framework has accepted W2's commit.
    let early = persisted_watermark(&store).await;
    assert!(
        early.is_none_or(|wm| wm.checkpoint_hi_inclusive < 2),
        "watermark advanced to W2 while W1's retry was still in flight: {early:?}",
    );

    retry_gate.release();
    let persisted = wait_for_watermark_at_least(&store, 2).await;

    // Final bitmap includes both bits.
    let bm = wait_for_bitmap(&mock, row_key).await;
    assert!(bm.contains(1), "W1 bit lost");
    assert!(bm.contains(2), "W2 bit lost");
    assert_eq!(persisted.checkpoint_hi_inclusive, 2);
}

/// Regression test for interleaved same-row commits while a write is
/// in flight. W1 schedules a row write; before W1's write lands, W2 adds new
/// bits to the same row. With a concurrent writer, either row write may arrive
/// last at BigTable; the final persisted bitmap must still contain both bits.
#[tokio::test]
async fn interleaved_commits_both_persist() {
    let (mock, store, handler, _service) = setup_with_committer(1, 2).await;
    let first_write_gate = mock.pause_next_mutate_rows().await;
    let handler = Arc::new(handler);

    let row_key: &[u8] = b"v1#dim#0000000000";

    // W1: adds bit 1.
    let batch1 = make_batch(vec![value(row_key, 0, &[1], 1, 1000)]);
    let w1 = watermark(1, 10, 1000);
    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w1).await?;
                h.commit(&batch1, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    first_write_gate.wait_for_arrival().await;

    // W2: adds bit 2 on the same row while W1's lower-timestamp write is
    // blocked in BigTable.
    let batch2 = make_batch(vec![value(row_key, 0, &[2], 2, 2000)]);
    let w2 = watermark(2, 20, 2000);
    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w2).await?;
                h.commit(&batch2, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    first_write_gate.release();
    let persisted = wait_for_watermark_at_least(&store, 2).await;

    // Both row writes must have landed. Under maxversions=1 + cell
    // timestamp = checkpoint timestamp, W2's (later, higher-ts)
    // write wins — and its bitmap cumulatively contains W1's bit
    // too.
    let bm = wait_for_bitmap(&mock, row_key).await;
    assert!(bm.contains(1));
    assert!(bm.contains(2));
    assert_eq!(persisted.checkpoint_hi_inclusive, 2);
}

/// Same-row writes can complete out of order when the writer runs with
/// concurrency. If W1 fails once, W2 can persist a higher-timestamp cumulative
/// bitmap before W1 retries its older partial bitmap. BigTable's cell-version
/// ordering must keep the newer cumulative row authoritative.
#[tokio::test]
async fn concurrent_retry_of_older_same_row_write_cannot_clobber_newer_bitmap() {
    let (mock, store, handler, _service) = setup_with_committer(1, 2).await;
    let handler = Arc::new(handler);

    let row_key: &[u8] = b"v1#dim#0000000000";
    let retry_gate = mock
        .pause_nth_mutate_rows_with_timestamp(1, 1_000_000)
        .await;
    mock.expect(ExpectedCall {
        row_keys: vec![row_key],
        failures: HashMap::from([(0, 8)]),
    })
    .await;

    let batch1 = make_batch(vec![value(row_key, 0, &[1], 1, 1000)]);
    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, watermark(1, 10, 1000))
                    .await?;
                h.commit(&batch1, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    let batch2 = make_batch(vec![value(row_key, 0, &[2], 2, 2000)]);
    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, watermark(2, 20, 2000))
                    .await?;
                h.commit(&batch2, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    retry_gate.wait_for_arrival().await;
    let before_retry_release = wait_for_bitmap(&mock, row_key).await;
    assert!(before_retry_release.contains(1));
    assert!(before_retry_release.contains(2));

    retry_gate.release();
    let persisted = wait_for_watermark_at_least(&store, 2).await;
    let bm = wait_for_bitmap(&mock, row_key).await;
    assert!(bm.contains(1), "older bit must survive retry ordering");
    assert!(bm.contains(2), "newer bit must survive older retry");
    assert_eq!(bm.len(), 2);
    assert_eq!(persisted.checkpoint_hi_inclusive, 2);
}

/// Rows distributed across many shards must all land durably. This
/// exercises batch partitioning plus generation countdowns:
/// even if shards finish out of order, watermarks must promote strictly
/// in cp order and the final persisted state must contain every row.
#[tokio::test]
async fn many_shards_all_rows_durable() {
    let (mock, store, handler, _service) = setup().await;
    let handler = Arc::new(handler);

    let row_keys = one_row_key_per_shard();
    let values = row_keys
        .iter()
        .map(|rk| value(rk, 0, &[0], 1, 1500))
        .collect();
    let batch = make_batch(values);
    let w = watermark(1, 10, 1500);

    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w).await?;
                h.commit(&batch, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    let persisted = wait_for_watermark_at_least(&store, 1).await;

    for rk in &row_keys {
        let bm = wait_for_bitmap(&mock, rk).await;
        assert_eq!(
            bm.len(),
            1,
            "row {} must contain exactly the committed bit",
            String::from_utf8_lossy(rk),
        );
        assert!(
            bm.contains(0),
            "row {} missing committed bit",
            String::from_utf8_lossy(rk),
        );
    }
    assert_eq!(persisted.checkpoint_hi_inclusive, 1);
}

/// A generation with more dirty rows than the writer chunk size should promote
/// only after every chunk reports durable rows for that generation.
#[tokio::test]
async fn generation_waits_for_all_writer_chunks() {
    let (mock, store, handler, _service) = setup_with_committer(2, 1).await;
    let handler = Arc::new(handler);

    let target_shard = shard_for(b"v1#chunked#target");
    let row_keys: Vec<Vec<u8>> = (0..)
        .map(|i| format!("v1#chunked#{i:010}").into_bytes())
        .filter(|row| shard_for(row) == target_shard)
        .take(5)
        .collect();
    let batch = make_batch(
        row_keys
            .iter()
            .enumerate()
            .map(|(i, rk)| value(rk, 0, &[i as u32], 1, 1500))
            .collect(),
    );

    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, watermark(1, 10, 1500))
                    .await?;
                h.commit(&batch, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    let persisted = wait_for_watermark_at_least(&store, 1).await;
    assert_eq!(
        mock.mutate_rows_count.load(Ordering::Relaxed),
        3,
        "five rows with chunk size two should flush as three MutateRows calls",
    );
    for (i, rk) in row_keys.iter().enumerate() {
        let bm = wait_for_bitmap(&mock, rk).await;
        assert_eq!(bm.len(), 1);
        assert!(bm.contains(i as u32));
    }
    assert_eq!(persisted.checkpoint_hi_inclusive, 1);
}

/// Per-cp generations may finish row writes out of order, but watermarks
/// must promote strictly in cp order. Here commit 1's rows take a long mock
/// write; commit 2 has no rows at all (empty batch). Commit 2 can finish
/// scheduling quickly, but commit 1's row write gates promotion of both
/// watermarks until it is durable.
#[tokio::test]
async fn out_of_order_generation_flushed_preserves_watermark_order() {
    let (mock, store, handler, _service) = setup().await;
    let first_write_gate = mock.pause_next_mutate_rows().await;
    let handler = Arc::new(handler);

    let row_key = b"v1#dim#0000000000";
    let batch1 = make_batch(vec![value(row_key, 0, &[1], 1, 1000)]);
    let w1 = watermark(1, 10, 1000);
    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w1).await?;
                h.commit(&batch1, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    first_write_gate.wait_for_arrival().await;

    // Commit 2: empty batch, but set_committer_watermark with a
    // higher cp. This commit's generation completes essentially instantly
    // once all 64 shards finish scheduling zero row writes.
    let batch2 = make_batch(vec![]);
    let w2 = watermark(2, 20, 2000);
    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w2).await?;
                h.commit(&batch2, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    // Before commit 1's write lands: no watermark (w1 is waiting on the
    // row write; w2 is blocked behind w1 by contiguous promotion).
    assert!(
        persisted_watermark(&store).await.is_none(),
        "watermark must not promote while commit 1 is still in flight",
    );

    first_write_gate.release();
    let persisted = wait_for_watermark_at_least(&store, 2).await;

    // After durable: both commits' watermarks promote; highest wins.
    assert_eq!(persisted.checkpoint_hi_inclusive, 2);
    assert_eq!(persisted.tx_hi, 20);
    wait_for_bitmap(&mock, row_key).await;
}

/// `WatermarkWriter::run`'s retry-on-failure branch (`pending = Some(req); sleep;
/// continue`) must keep the failed `Commit` and re-issue it on the next iteration.
/// If a refactor ever broke this — e.g., dropped `pending` or exited the loop on
/// Err — watermark advancement would silently freeze in production.
#[tokio::test]
async fn watermark_writer_retries_after_cas_failure() {
    let (mock, store, handler, _service) = setup().await;
    let handler = Arc::new(handler);

    // Fail exactly the next CheckAndMutateRow. After `setup()` runs
    // `init_watermark`, the next CheckAndMutateRow is the bitmap committer's
    // watermark CAS for this commit; row writes go through MutateRows.
    mock.fail_next_n_check_and_mutate(1);

    let row_key = b"v1#dim#0000000000";
    let batch = make_batch(vec![value(row_key, 0, &[1], 1, 1000)]);
    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, watermark(1, 10, 1000))
                    .await?;
                h.commit(&batch, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    let persisted = wait_for_watermark_at_least(&store, 1).await;
    assert_eq!(persisted.checkpoint_hi_inclusive, 1);
    let bm = wait_for_bitmap(&mock, row_key).await;
    assert!(bm.contains(1));
}

async fn persisted_bucket_start_cp(mock: &MockBigtableServer) -> Option<u64> {
    let bytes = mock
        .get_cell(
            tables::watermarks::NAME,
            PIPELINE.as_bytes(),
            FAMILY,
            tables::watermarks::col::BUCKET_START_CP.as_bytes(),
        )
        .await?;
    let arr: [u8; 8] = bytes
        .as_ref()
        .try_into()
        .expect("bucket_start_cp cell is 8 bytes");
    Some(u64::from_be_bytes(arr))
}

/// When a commit advances `tx_hi` across a bucket-seal boundary, the generation
/// task records that commit's `checkpoint_hi_inclusive` as the new
/// `bitmap_bucket_start_cp` and writes it alongside the watermark.
#[tokio::test]
async fn bucket_start_cp_written_on_transition() {
    let (mock, store, handler, _service) = setup().await;
    let handler = Arc::new(handler);

    // Commit 1 keeps tx_hi inside bucket 0; commit 2 crosses into
    // bucket 1. The generation task promotes in checkpoint order, so
    // commit 2's transition watermark overwrites the column.
    let row1 = b"v1#dim#0000000001";
    let batch1 = make_batch(vec![value(row1, 0, &[0], 1, 1000)]);
    let w1 = watermark(1, 10, 1000);
    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w1).await?;
                h.commit(&batch1, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    let row2 = b"v1#dim#0000000002";
    let batch2 = make_batch(vec![value(row2, 1, &[0], 2, 2000)]);
    let w2 = watermark(2, BUCKET_SIZE + 5, 2000);
    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w2).await?;
                h.commit(&batch2, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();
    wait_for_watermark_at_least(&store, 2).await;
    assert_eq!(
        persisted_bucket_start_cp(&mock).await,
        Some(2),
        "bucket transition must record the crossing commit's cp",
    );
}

/// The bitmap-only replay floor is written in the same CAS-guarded bundle as
/// the committer watermark. A stale watermark retry must not roll `b` back
/// after a newer checkpoint has already advanced it.
#[tokio::test]
async fn stale_watermark_retry_cannot_clobber_bucket_start_cp() {
    let (mock, store, _client) = setup_without_init_watermark().await;
    let mut conn = store.connect().await.unwrap();
    conn.init_watermark(PIPELINE, None).await.unwrap();

    conn.client()
        .set_committer_watermark_cells(PIPELINE, &watermark(2, BUCKET_SIZE + 5, 2_000), Some(2))
        .await
        .unwrap();
    conn.client()
        .set_committer_watermark_cells(PIPELINE, &watermark(1, 10, 1_000), Some(1))
        .await
        .unwrap();

    let persisted = conn.committer_watermark(PIPELINE).await.unwrap().unwrap();
    assert_eq!(persisted.checkpoint_hi_inclusive, 2);
    assert_eq!(
        persisted_bucket_start_cp(&mock).await,
        Some(2),
        "stale committer-watermark retries must leave bucket_start_cp unchanged",
    );
}

/// `bucket_start_cp` must update on a bucket transition even when the commit
/// produces zero rows. The framework can deliver checkpoints with no
/// dimension-emitting transactions; those still advance `tx_hi` and may cross
/// bucket boundaries.
#[tokio::test]
async fn empty_batch_commit_records_bucket_start_cp_on_transition() {
    let (mock, store, handler, _service) = setup().await;
    let handler = Arc::new(handler);

    // First commit stays inside bucket 0 so we have a baseline.
    let row = b"v1#dim#0000000010";
    let batch = make_batch(vec![value(row, 0, &[0], 1, 1000)]);
    let w1 = watermark(1, 10, 1000);
    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w1).await?;
                h.commit(&batch, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();
    wait_for_watermark_at_least(&store, 1).await;
    assert_eq!(persisted_bucket_start_cp(&mock).await, Some(0));

    // Second commit: empty batch, but tx_hi crosses into bucket 1.
    let empty = make_batch(vec![]);
    let w2 = watermark(2, BUCKET_SIZE + 5, 2000);
    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w2).await?;
                h.commit(&empty, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();
    wait_for_watermark_at_least(&store, 2).await;
    assert_eq!(
        persisted_bucket_start_cp(&mock).await,
        Some(2),
        "empty-batch commit that crosses a bucket boundary must still \
         record the transition cp",
    );
}

/// A single commit whose `tx_hi` jumps past multiple bucket boundaries at once
/// must drive `current_bucket_id` all the way forward and record this commit's
/// cp as the new bucket's `bucket_start_cp`. Exercises the `while seal_fn(...)
/// <= watermark.tx_hi` advance loop in `record_generation_started`.
#[tokio::test]
async fn commit_jumps_multiple_buckets_in_one_watermark() {
    let (mock, store, handler, _service) = setup().await;
    let handler = Arc::new(handler);

    // tx_hi lands well inside bucket 3 — three boundaries crossed in this
    // commit. The active bucket after promotion must be bucket 3.
    let target_bucket = 3u64;
    let target_tx_hi = target_bucket * BUCKET_SIZE + 7;
    let row = b"v1#dim#0000000003";
    let batch = make_batch(vec![value(row, target_bucket, &[0], 1, 1000)]);
    let w = watermark(1, target_tx_hi, 1000);

    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w).await?;
                h.commit(&batch, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();

    wait_for_watermark_at_least(&store, 1).await;
    wait_for_bitmap(&mock, row).await;
    assert_eq!(
        persisted_bucket_start_cp(&mock).await,
        Some(1),
        "multi-bucket-jump commit must record this cp as the active bucket's start",
    );
}

/// On restart mid-bucket, the generation task seeds `current_bucket_start_cp`
/// from the persisted column and rewrites the same value on subsequent watermarks
/// until the next bucket transition. Proves the column's value survives restart.
#[tokio::test]
async fn bucket_start_cp_seeded_from_column_on_restart() {
    // This test writes its own pre-persisted watermark row
    // and then calls `init_watermark` against it; `setup()`'s pre-bootstrap
    // call would populate the map before the row is written.
    let (mock, store, _client) = setup_without_init_watermark().await;

    // Pre-persist a mid-bucket watermark + a known bucket_start_cp so
    // init_watermark exposes them. The generation task reads that store
    // init state when the first commit arrives.
    let pre_tx_hi = BUCKET_SIZE / 2;
    let pre_cp = 10u64;
    let pre_bucket_start_cp = 7u64;
    let w_seed = watermark(pre_cp, pre_tx_hi, 500);
    let mut seed_conn = store.connect().await.unwrap();
    create_watermark_v1(seed_conn.client(), w_seed, Some(pre_bucket_start_cp)).await;
    let init = seed_conn.init_watermark(PIPELINE, None).await.unwrap();
    assert!(init.is_some(), "persisted watermark must be seen");
    drop(seed_conn);

    // Fresh committer; first commit stays inside bucket 0 so no transition
    // fires. The re-persisted column must still be the seeded value — evidence
    // the generation task initialized from it rather than resetting to 0.
    let handler = BitmapIndexHandler::new(TestProcessor);
    let _service = register_test_committer(&store);
    let handler = Arc::new(handler);

    let row = b"v1#dim#0000000001";
    let batch = make_batch(vec![value(row, 0, &[1], pre_cp + 1, 1500)]);
    let w = watermark(pre_cp + 1, pre_tx_hi + 1, 1500);
    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w).await?;
                h.commit(&batch, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();
    wait_for_watermark_at_least(&store, pre_cp + 1).await;

    assert_eq!(
        persisted_bucket_start_cp(&mock).await,
        Some(pre_bucket_start_cp),
        "generation task must seed current_bucket_start_cp from the persisted column",
    );
}

/// Restart mid-bucket. Without re-emitting pre-restart bits, the post-restart
/// committer overwrites the cell with its own (post-restart-only) bitmap —
/// pre-restart bits are gone. This locks in the contract that motivates
/// `init_watermark`'s clamp to `bucket_start_cp - 1`: the framework MUST
/// re-stream the active bucket's checkpoints so every contributing bit is
/// re-emitted.
#[tokio::test]
async fn mid_bucket_restart_without_replay_loses_pre_restart_bits() {
    let (mock, store, _client) = setup_without_init_watermark().await;

    let pre_tx_hi = BUCKET_SIZE / 2;
    let pre_cp = 5u64;
    let bucket_start_cp = 1u64;
    let pre_bit = 0u32;
    let new_bit = 1u32;
    let row_key = b"v1#dim#0000000000";

    let mut seed_conn = store.connect().await.unwrap();
    let w_seed = watermark(pre_cp, pre_tx_hi, 500);
    create_watermark_v1(seed_conn.client(), w_seed, Some(bucket_start_cp)).await;

    write_seed_bitmap(seed_conn.client(), row_key, &[pre_bit], 500).await;

    let init = seed_conn
        .init_watermark(PIPELINE, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        init.checkpoint_hi_inclusive,
        Some(0),
        "clamp must roll back to the checkpoint before bucket start"
    );
    drop(seed_conn);

    // Fresh committer. Skip the natural replay (i.e. the framework didn't
    // re-stream cp=1..=5) and commit only the post-restart bit. The in-memory
    // bitmap holds exactly {new_bit}; under maxversions=1 the higher-ts write
    // wins, so the persisted cell ends up at {new_bit} only.
    let handler = BitmapIndexHandler::new(TestProcessor);
    let _service = register_test_committer(&store);
    let handler = Arc::new(handler);

    let batch = make_batch(vec![value(row_key, 0, &[new_bit], 6, 1500)]);
    let w = watermark(6, pre_tx_hi + 1, 1500);
    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w).await?;
                h.commit(&batch, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();
    wait_for_watermark_at_least(&store, 6).await;

    let bm = wait_for_bitmap(&mock, row_key).await;
    assert!(
        bm.contains(new_bit),
        "post-restart bit must be present in cell"
    );
    assert!(
        !bm.contains(pre_bit),
        "without replay, pre-restart bits are overwritten — exactly why \
         init_watermark clamps to the active bucket's start",
    );
}

/// Restart mid-bucket with the expected framework replay. Re-emitting the
/// active bucket's earlier bits rebuilds in-memory cumulative state before new
/// bits arrive, so the final cell keeps both pre- and post-restart bits.
#[tokio::test]
async fn mid_bucket_restart_with_replay_preserves_pre_restart_bits() {
    let (mock, store, _client) = setup_without_init_watermark().await;

    let pre_tx_hi = BUCKET_SIZE / 2;
    let pre_cp = 5u64;
    let bucket_start_cp = 1u64;
    let pre_bit = 0u32;
    let new_bit = 1u32;
    let row_key = b"v1#dim#0000000000";

    let mut seed_conn = store.connect().await.unwrap();
    let w_seed = watermark(pre_cp, pre_tx_hi, 500);
    create_watermark_v1(seed_conn.client(), w_seed, Some(bucket_start_cp)).await;
    write_seed_bitmap(seed_conn.client(), row_key, &[pre_bit], 500).await;

    let init = seed_conn
        .init_watermark(PIPELINE, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        init.checkpoint_hi_inclusive,
        Some(bucket_start_cp - 1),
        "restart must resume just before the active bucket replay floor",
    );
    drop(seed_conn);

    let handler = BitmapIndexHandler::new(TestProcessor);
    let _service = register_test_committer(&store);
    let handler = Arc::new(handler);

    let replay_batch = make_batch(vec![value(row_key, 0, &[pre_bit], pre_cp, 1_000)]);
    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, watermark(pre_cp, pre_tx_hi, 1_000))
                    .await?;
                h.commit(&replay_batch, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();
    wait_for_watermark_at_least(&store, pre_cp).await;

    let post_restart_batch = make_batch(vec![value(row_key, 0, &[new_bit], 6, 1_500)]);
    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, watermark(6, pre_tx_hi + 1, 1_500))
                    .await?;
                h.commit(&post_restart_batch, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();
    wait_for_watermark_at_least(&store, 6).await;

    let bm = wait_for_bitmap(&mock, row_key).await;
    assert!(bm.contains(pre_bit), "replayed pre-restart bit lost");
    assert!(bm.contains(new_bit), "post-restart bit missing");
    assert_eq!(bm.len(), 2);
}

/// Restart after bucket 0 is sealed but bucket 1 is still active. Replaying
/// the bucket-start checkpoint can include tail rows for bucket 0; those must
/// not rewrite already-durable sealed bucket rows from partial in-memory state.
#[tokio::test]
async fn restart_replay_skips_buckets_sealed_before_startup() {
    let (mock, store, _client) = setup_without_init_watermark().await;

    let startup_tx_hi = BUCKET_SIZE + 10;
    let startup_cp = 7u64;
    let bucket_start_cp = 4u64;
    let sealed_row = b"v1#dim#0000000000".to_vec();
    let sealed_shard = shard_for(&sealed_row);
    let active_row = (0..)
        .map(|i| format!("v1#active_dim_{i}#0000000001").into_bytes())
        .find(|row| shard_for(row) == sealed_shard)
        .expect("same-shard active row key");

    let mut seed_conn = store.connect().await.unwrap();
    let w_seed = watermark(startup_cp, startup_tx_hi, 7_000);
    create_watermark_v1(seed_conn.client(), w_seed, Some(bucket_start_cp)).await;

    write_seed_bitmap(seed_conn.client(), &sealed_row, &[0, 1], 7_000).await;

    let init = seed_conn
        .init_watermark(PIPELINE, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        init.checkpoint_hi_inclusive,
        Some(bucket_start_cp - 1),
        "restart must replay from the active bucket's replay floor"
    );
    drop(seed_conn);

    let handler = BitmapIndexHandler::new(TestProcessor);
    let _service = register_test_committer(&store);
    let handler = Arc::new(handler);

    let replay_cp = bucket_start_cp;
    let batch = make_batch(vec![
        // This is a partial sealed-bucket row from the replayed boundary
        // checkpoint. Without the startup bucket filter it can overwrite the
        // full sealed bitmap at the same checkpoint timestamp.
        value(&sealed_row, 0, &[0], replay_cp, 7_000),
        value(&active_row, 1, &[0], replay_cp, 7_000),
    ]);
    let w = watermark(replay_cp, startup_tx_hi + 1, 7_000);
    let h = handler.clone();
    store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(PIPELINE, w).await?;
                h.commit(&batch, conn).await
            }
            .scope_boxed()
        })
        .await
        .unwrap();
    wait_for_bitmap(&mock, &active_row).await;

    let sealed = wait_for_bitmap(&mock, &sealed_row).await;
    assert!(
        sealed.contains(1),
        "sealed bucket row must not be overwritten by partial replay"
    );
    assert_eq!(sealed.len(), 2);

    let active = wait_for_bitmap(&mock, &active_row).await;
    assert!(
        active.contains(0),
        "active bucket replay row must be written"
    );
}

#[test]
fn event_bitmap_seal_handles_exact_boundaries() {
    let txs_per_event_bucket =
        event_bitmap_index::BUCKET_SIZE / event_bitmap_index::MAX_EVENTS_PER_TX as u64;
    let wm = |tx_hi: u64| CommitterWatermark {
        tx_hi,
        ..Default::default()
    };
    // Bucket 0: sealed at tx_hi == txs_per_event_bucket, not before.
    assert!(!EventSealProcessor::is_sealed(
        0,
        wm(txs_per_event_bucket - 1)
    ));
    assert!(EventSealProcessor::is_sealed(0, wm(txs_per_event_bucket)));
    // Bucket 1: sealed at tx_hi == 2 * txs_per_event_bucket, not before.
    assert!(!EventSealProcessor::is_sealed(
        1,
        wm(txs_per_event_bucket * 2 - 1),
    ));
    assert!(EventSealProcessor::is_sealed(
        1,
        wm(txs_per_event_bucket * 2)
    ));
}

/// Distinct processor whose committer is intentionally **not** registered with
/// the store, so calls to `commit_bitmap_batch::<UnregisteredProcessor>` panic
/// at the registry lookup. Used only by the negative-path tests below.
struct UnregisteredProcessor;

impl BitmapIndexProcessor for UnregisteredProcessor {
    const NAME: &'static str = "unregistered_pipeline";
    const TABLE: &'static str = TABLE;
    const COLUMN: &'static str = COL;
    const SCHEMA_VERSION: u32 = transaction_bitmap_index::SCHEMA_VERSION;
    const BUCKET_ID_WIDTH: usize = transaction_bitmap_index::BUCKET_ID_WIDTH;

    fn for_each_indexed_bit<F>(&self, _: &Checkpoint, _: F)
    where
        F: FnMut(u64, u32, IndexDimension, &[u8]),
    {
    }

    fn is_sealed(bucket_id: u64, watermark: CommitterWatermark) -> bool {
        watermark.tx_hi >= (bucket_id + 1) * BUCKET_SIZE
    }
}

/// Calling the bitmap commit path without first staging a watermark via
/// `set_committer_watermark` is a contract violation in the framework's
/// sequential-pipeline flow. Verify the connection bails rather than silently
/// committing without a watermark.
#[tokio::test]
async fn commit_bitmap_batch_without_staged_watermark_bails() {
    let (_mock, store, handler, _service) = setup().await;
    let handler = Arc::new(handler);

    let batch = make_batch(vec![]);
    let h = handler.clone();
    let err = store
        .transaction(move |conn| {
            async move {
                // Intentionally skip set_committer_watermark.
                h.commit(&batch, conn).await
            }
            .scope_boxed()
        })
        .await
        .expect_err("must bail without a staged watermark");
    assert!(
        err.to_string()
            .contains("set_committer_watermark must be called"),
        "unexpected error: {err}",
    );
}

/// The bitmap connection enforces that the staged watermark's pipeline matches
/// the processor's pipeline name — otherwise the framework's transaction
/// invariants (one staged pipeline per transaction) are being violated.
#[tokio::test]
async fn commit_bitmap_batch_with_mismatched_pipeline_bails() {
    let (_mock, store, handler, _service) = setup().await;
    let handler = Arc::new(handler);

    let batch = make_batch(vec![]);
    let h = handler.clone();
    let err = store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark("other_pipeline", watermark(1, 10, 1000))
                    .await?;
                h.commit(&batch, conn).await
            }
            .scope_boxed()
        })
        .await
        .expect_err("must bail when staged pipeline differs from handler pipeline");
    assert!(
        err.to_string().contains("staged watermark for"),
        "unexpected error: {err}",
    );
}

/// A bitmap commit for a pipeline whose committer was never registered with
/// the store must panic — silently dropping data would be worse than crashing
/// the pipeline.
#[tokio::test]
#[should_panic(expected = "bitmap committer for `unregistered_pipeline` is not registered")]
async fn commit_bitmap_batch_panics_when_committer_not_registered() {
    let (_mock, store, _handler, _service) = setup().await;
    store
        .connect()
        .await
        .unwrap()
        .init_watermark(UnregisteredProcessor::NAME, None)
        .await
        .unwrap();

    let unregistered_handler = Arc::new(BitmapIndexHandler::new(UnregisteredProcessor));
    let batch = make_batch(vec![]);
    let h = unregistered_handler.clone();
    let _ = store
        .transaction(move |conn| {
            async move {
                conn.set_committer_watermark(UnregisteredProcessor::NAME, watermark(1, 10, 1000))
                    .await?;
                h.commit(&batch, conn).await
            }
            .scope_boxed()
        })
        .await;
}
