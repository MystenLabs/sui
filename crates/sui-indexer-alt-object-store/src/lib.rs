// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::Context;
use async_trait::async_trait;
use bytes::Bytes;
use object_store::Error as ObjectStoreError;
use object_store::ObjectStoreExt as _;
use object_store::PutMode;
use object_store::PutPayload;
use object_store::path::Path as ObjectPath;
use serde::Deserialize;
use serde::Serialize;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use sui_indexer_alt_framework_store_traits::ConcurrentConnection;
use sui_indexer_alt_framework_store_traits::ConcurrentStore;
use sui_indexer_alt_framework_store_traits::Connection;
use sui_indexer_alt_framework_store_traits::InitWatermark;
use sui_indexer_alt_framework_store_traits::PrunerWatermark;
use sui_indexer_alt_framework_store_traits::ReaderWatermark;
use sui_indexer_alt_framework_store_traits::Store;

#[derive(Clone)]
pub struct ObjectStore {
    object_store: Arc<dyn object_store::ObjectStore>,
}

pub struct ObjectStoreConnection {
    object_store: Arc<dyn object_store::ObjectStore>,
}

/// Used to potentially migrate from the legacy watermark format that does not include `reader_lo`,
/// `pruner_hi`, and `pruner_timestamp_ms`.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct LegacyObjectStoreWatermark {
    epoch_hi_inclusive: u64,
    checkpoint_hi_inclusive: Option<u64>,
    tx_hi: u64,
    timestamp_ms_hi_inclusive: u64,
    #[serde(default)]
    reader_lo: Option<u64>,
    #[serde(default)]
    pruner_hi: Option<u64>,
    #[serde(default)]
    pruner_timestamp_ms: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ObjectStoreWatermark {
    epoch_hi_inclusive: u64,
    checkpoint_hi_inclusive: Option<u64>,
    tx_hi: u64,
    timestamp_ms_hi_inclusive: u64,
    reader_lo: u64,
    pruner_hi: u64,
    pruner_timestamp_ms: u64,
}

impl ObjectStore {
    pub fn new(object_store: Arc<dyn object_store::ObjectStore>) -> Self {
        Self { object_store }
    }
}

impl ObjectStoreConnection {
    pub fn object_store(&self) -> Arc<dyn object_store::ObjectStore> {
        self.object_store.clone()
    }

    async fn get_watermark_for_read(
        &self,
        pipeline: &str,
    ) -> anyhow::Result<Option<(ObjectStoreWatermark, u64)>> {
        let object_path = watermark_path(pipeline);
        let result = match self.object_store.get(&object_path).await {
            Ok(result) => result,
            Err(ObjectStoreError::NotFound { .. }) => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        let bytes = result.bytes().await?;
        let watermark =
            serde_json::from_slice::<ObjectStoreWatermark>(&bytes).with_context(|| {
                format!("Failed to parse watermark from object store pipeline={pipeline}")
            })?;
        // Hide watermarks where `checkpoint_hi_inclusive < reader_lo`.
        let Some(checkpoint_hi_inclusive) = watermark
            .checkpoint_hi_inclusive
            .filter(|&cp| watermark.reader_lo <= cp)
        else {
            return Ok(None);
        };

        Ok(Some((watermark, checkpoint_hi_inclusive)))
    }

    async fn get_watermark_for_write(
        &self,
        pipeline: &str,
    ) -> anyhow::Result<(ObjectStoreWatermark, Option<String>, Option<String>)> {
        let object_path = watermark_path(pipeline);
        let result = match self.object_store.get(&object_path).await {
            Ok(result) => result,
            Err(e) => return Err(e.into()),
        };

        let e_tag = result.meta.e_tag.clone();
        let version = result.meta.version.clone();
        let bytes = result.bytes().await?;
        let watermark = serde_json::from_slice::<ObjectStoreWatermark>(&bytes)
            .context("Failed to parse watermark from object store")?;

        Ok((watermark, e_tag, version))
    }

    async fn set_watermark(
        &self,
        pipeline: &str,
        watermark: ObjectStoreWatermark,
        e_tag: Option<String>,
        version: Option<String>,
    ) -> anyhow::Result<()> {
        let object_path = watermark_path(pipeline);
        let json_bytes = serde_json::to_vec(&watermark)?;
        let payload: PutPayload = Bytes::from(json_bytes).into();
        self.object_store
            .put_opts(
                &object_path,
                payload,
                PutMode::Update(object_store::UpdateVersion { e_tag, version }).into(),
            )
            .await?;
        Ok(())
    }
}

#[async_trait]
impl ConcurrentStore for ObjectStore {
    type ConcurrentConnection<'c> = ObjectStoreConnection;
}

#[async_trait]
impl Store for ObjectStore {
    type Connection<'c> = ObjectStoreConnection;

    async fn connect<'c>(&'c self) -> anyhow::Result<Self::Connection<'c>> {
        Ok(ObjectStoreConnection {
            object_store: self.object_store.clone(),
        })
    }
}

#[async_trait]
impl Connection for ObjectStoreConnection {
    async fn init_watermark(
        &mut self,
        pipeline_task: &str,
        checkpoint_hi_inclusive: Option<u64>,
    ) -> anyhow::Result<Option<InitWatermark>> {
        let object_path = watermark_path(pipeline_task);
        let reader_lo = checkpoint_hi_inclusive.map_or(0, |cp| cp + 1);
        let watermark = ObjectStoreWatermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive,
            tx_hi: 0,
            timestamp_ms_hi_inclusive: 0,
            reader_lo,
            pruner_hi: reader_lo,
            pruner_timestamp_ms: 0,
        };
        let json_bytes = serde_json::to_vec(&watermark)?;
        let payload: PutPayload = Bytes::from(json_bytes).into();
        // Try create-if-not-exists write first.
        let (checkpoint_hi_inclusive, reader_lo) = match self
            .object_store
            .put_opts(&object_path, payload, PutMode::Create.into())
            .await
        {
            Ok(_) => (checkpoint_hi_inclusive, Some(reader_lo)),
            Err(object_store::Error::AlreadyExists { .. }) => {
                // Fall back to reading existing watermark.
                let result = match self.object_store.get(&object_path).await {
                    Ok(result) => result,
                    Err(e) => return Err(e.into()),
                };
                let e_tag = result.meta.e_tag.clone();
                let version = result.meta.version.clone();
                let bytes = result.bytes().await?;
                let legacy_watermark: LegacyObjectStoreWatermark = serde_json::from_slice(&bytes)
                    .with_context(|| {
                        format!(
                            "Failed to parse legacy watermark from object store pipeline={pipeline_task}"
                        )
                    })?;

                // Write data from the legacy watermark using the new format if it is missing newly added fields.
                if legacy_watermark.reader_lo.is_none()
                    || legacy_watermark.pruner_hi.is_none()
                    || legacy_watermark.pruner_timestamp_ms.is_none()
                {
                    let watermark = ObjectStoreWatermark {
                        epoch_hi_inclusive: legacy_watermark.epoch_hi_inclusive,
                        checkpoint_hi_inclusive: legacy_watermark.checkpoint_hi_inclusive,
                        tx_hi: legacy_watermark.tx_hi,
                        timestamp_ms_hi_inclusive: legacy_watermark.timestamp_ms_hi_inclusive,
                        ..watermark
                    };
                    self.set_watermark(pipeline_task, watermark, e_tag, version)
                        .await?;
                }

                (
                    legacy_watermark.checkpoint_hi_inclusive,
                    legacy_watermark.reader_lo,
                )
            }
            Err(e) => return Err(e.into()),
        };
        Ok(Some(InitWatermark {
            checkpoint_hi_inclusive,
            reader_lo,
        }))
    }

    async fn accepts_chain_id(
        &mut self,
        pipeline_task: &str,
        chain_id: [u8; 32],
    ) -> anyhow::Result<bool> {
        crate::accepts_chain_id(self.object_store.as_ref(), pipeline_task, chain_id).await
    }

    async fn committer_watermark(
        &mut self,
        pipeline_task: &str,
    ) -> anyhow::Result<Option<CommitterWatermark>> {
        Ok(self
            .get_watermark_for_read(pipeline_task)
            .await?
            .map(|(w, checkpoint_hi_inclusive)| CommitterWatermark {
                epoch_hi_inclusive: w.epoch_hi_inclusive,
                checkpoint_hi_inclusive,
                tx_hi: w.tx_hi,
                timestamp_ms_hi_inclusive: w.timestamp_ms_hi_inclusive,
            }))
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline_task: &str,
        watermark: CommitterWatermark,
    ) -> anyhow::Result<bool> {
        let (current_watermark, e_tag, version) =
            self.get_watermark_for_write(pipeline_task).await?;

        if current_watermark
            .checkpoint_hi_inclusive
            .is_some_and(|cp| cp >= watermark.checkpoint_hi_inclusive)
        {
            return Ok(false);
        }

        let new_watermark = ObjectStoreWatermark {
            epoch_hi_inclusive: watermark.epoch_hi_inclusive,
            checkpoint_hi_inclusive: Some(watermark.checkpoint_hi_inclusive),
            tx_hi: watermark.tx_hi,
            timestamp_ms_hi_inclusive: watermark.timestamp_ms_hi_inclusive,
            ..current_watermark
        };
        self.set_watermark(pipeline_task, new_watermark, e_tag, version)
            .await?;
        Ok(true)
    }
}

#[async_trait]
impl ConcurrentConnection for ObjectStoreConnection {
    async fn reader_watermark(
        &mut self,
        pipeline: &str,
    ) -> anyhow::Result<Option<ReaderWatermark>> {
        Ok(self
            .get_watermark_for_read(pipeline)
            .await?
            .map(|(w, checkpoint_hi_inclusive)| ReaderWatermark {
                checkpoint_hi_inclusive,
                reader_lo: w.reader_lo,
            }))
    }

    async fn pruner_watermark(
        &mut self,
        pipeline: &'static str,
        delay: Duration,
    ) -> anyhow::Result<Option<PrunerWatermark>> {
        let Some((watermark, _)) = self.get_watermark_for_read(pipeline).await? else {
            return Ok(None);
        };
        // Compute max(0, (pruner_timestamp + delay) - now). Use u128 to avoid overflow
        // when summing the two operands, and saturating_sub so we never underflow when
        // the wait period has already elapsed. saturating_sub is safe because the caller treats
        // anything < 1 the same.
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
    ) -> anyhow::Result<bool> {
        let (current_watermark, e_tag, version) = self.get_watermark_for_write(pipeline).await?;

        if reader_lo <= current_watermark.reader_lo {
            return Ok(false);
        }

        let new_watermark = ObjectStoreWatermark {
            reader_lo,
            ..current_watermark
        };
        self.set_watermark(pipeline, new_watermark, e_tag, version)
            .await?;
        Ok(true)
    }

    async fn set_pruner_watermark(
        &mut self,
        pipeline: &'static str,
        pruner_hi: u64,
    ) -> anyhow::Result<bool> {
        let (current_watermark, e_tag, version) = self.get_watermark_for_write(pipeline).await?;

        if pruner_hi <= current_watermark.pruner_hi {
            return Ok(false);
        }

        let new_watermark = ObjectStoreWatermark {
            pruner_hi,
            ..current_watermark
        };
        self.set_watermark(pipeline, new_watermark, e_tag, version)
            .await?;
        Ok(true)
    }
}

fn watermark_path(pipeline: &str) -> ObjectPath {
    ObjectPath::from(format!("_metadata/watermarks/{}.json", pipeline))
}

fn chain_id_path(pipeline_task: &str) -> ObjectPath {
    ObjectPath::from(format!("_metadata/chain_id/{pipeline_task}"))
}

/// Reusable implementation of [`Connection::accepts_chain_id`] for object-store-backed
/// connections. Stores `chain_id` at `_metadata/chain_id/{pipeline_task}` on first call
/// via a conditional create; on subsequent calls reads and compares.
pub async fn accepts_chain_id(
    object_store: &dyn object_store::ObjectStore,
    pipeline_task: &str,
    chain_id: [u8; 32],
) -> anyhow::Result<bool> {
    let path = chain_id_path(pipeline_task);
    match object_store
        .put_opts(
            &path,
            chain_id.to_vec().into(),
            object_store::PutOptions {
                mode: PutMode::Create,
                ..Default::default()
            },
        )
        .await
    {
        Ok(_) => Ok(true),
        Err(ObjectStoreError::AlreadyExists { .. }) => {
            let bytes = object_store.get(&path).await?.bytes().await?;
            let stored: [u8; 32] = bytes.as_ref().try_into().ok().with_context(|| {
                format!(
                    "stored chain_id at {} has wrong length: {}",
                    path,
                    bytes.len()
                )
            })?;
            Ok(stored == chain_id)
        }
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use object_store::memory::InMemory;
    use sui_indexer_alt_framework_store_traits::concurrent_connection_tests;
    use sui_indexer_alt_framework_store_traits::connection_tests;
    use sui_indexer_alt_framework_store_traits::testing::Harness;

    use super::*;

    const PIPELINE: &str = "pipeline";

    /// One hour in milliseconds. Used in pruner watermark tests as both the expected
    /// `wait_for_ms` value and the offset for `delay_for_one_hour_wait`.
    const ONE_HOUR_MS: u64 = 3_600_000;

    // Canonical "non-default" watermark field values reused across tests so each one
    // doesn't need to invent its own. Distinct values let assertions catch field mix-ups.
    const EPOCH_HI: u64 = 7;
    const CHECKPOINT_HI: u64 = 200;
    const TX_HI: u64 = 42;
    const TIMESTAMP_MS_HI: u64 = 99;
    const READER_LO: u64 = 123;
    const PRUNER_HI: u64 = 77;
    const PRUNER_TIMESTAMP_MS: u64 = 555;

    struct ObjectStoreHarness {
        store: ObjectStore,
    }

    #[async_trait::async_trait(?Send)]
    impl Harness for ObjectStoreHarness {
        type Store = ObjectStore;

        async fn new() -> Self {
            Self {
                store: ObjectStore::new(Arc::new(InMemory::new())),
            }
        }

        fn store(&self) -> &Self::Store {
            &self.store
        }
    }

    async fn store_conn() -> ObjectStoreConnection {
        let harness = ObjectStoreHarness::new().await;
        harness.store.connect().await.unwrap()
    }

    async fn bootstrap(conn: &mut ObjectStoreConnection, checkpoint_hi_inclusive: u64) {
        conn.init_watermark(PIPELINE, None).await.unwrap();
        conn.set_committer_watermark(
            PIPELINE,
            CommitterWatermark {
                epoch_hi_inclusive: 0,
                checkpoint_hi_inclusive,
                tx_hi: 0,
                timestamp_ms_hi_inclusive: 0,
            },
        )
        .await
        .unwrap();
    }

    /// Build a delay such that `pruner_watermark` returns `wait_for_ms ~= 1h` for a
    /// watermark with `pruner_timestamp_ms = 0` (which is what `bootstrap` produces).
    /// `pruner_watermark` computes `(pruner_timestamp + delay) - now` saturating to 0,
    /// so the delay must put `pruner_timestamp + delay` past `now` for the wait to be
    /// non-zero; i.e. delay must be at least `now`.
    fn delay_for_one_hour_wait() -> Duration {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        Duration::from_millis(now_ms + ONE_HOUR_MS)
    }

    connection_tests!(ObjectStoreHarness);
    concurrent_connection_tests!(ObjectStoreHarness);

    #[tokio::test]
    async fn test_init_watermark_fresh_without_checkpoint_reader_lo() {
        let mut conn = store_conn().await;
        let watermark = conn.init_watermark(PIPELINE, None).await.unwrap().unwrap();
        assert_eq!(watermark.reader_lo, Some(0));
    }

    #[tokio::test]
    async fn test_init_watermark_fresh_with_checkpoint_reader_lo() {
        let mut conn = store_conn().await;
        let watermark = conn
            .init_watermark(PIPELINE, Some(CHECKPOINT_HI))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(watermark.reader_lo, Some(CHECKPOINT_HI + 1));
    }

    #[tokio::test]
    async fn test_init_watermark_returns_existing_reader_lo() {
        // Object-store preserves the existing reader_lo across `init_watermark` conflict. The
        // shared macro test can't assert this because bigtable has no reader_lo concept.
        // Init starts with reader_lo = 0, then set_reader_watermark advances it to READER_LO.
        let mut conn = store_conn().await;
        conn.init_watermark(PIPELINE, None).await.unwrap();
        conn.set_reader_watermark(PIPELINE, READER_LO)
            .await
            .unwrap();

        let watermark = conn
            .init_watermark(PIPELINE, Some(0))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(watermark.reader_lo, Some(READER_LO));
    }

    /// Write raw JSON bytes directly to the underlying object store, bypassing the public
    /// API. Used to verify `init_watermark`'s `AlreadyExists` branch can deserialize both
    /// the current `ObjectStoreWatermark` format and the older format (with required
    /// `checkpoint_hi_inclusive` and no `reader_lo`/`pruner_*` fields).
    async fn put_watermark_raw(conn: &ObjectStoreConnection, pipeline: &str, json: &str) {
        let path = watermark_path(pipeline);
        conn.object_store
            .put(&path, PutPayload::from(Bytes::from(json.to_owned())))
            .await
            .unwrap();
    }

    /// Read the raw stored bytes and parse them strictly as the current
    /// `ObjectStoreWatermark`. Strict parsing fails if any of the new required fields
    /// (`reader_lo`/`pruner_hi`/`pruner_timestamp_ms`) are missing, so this is the
    /// definitive way to assert a legacy watermark has been migrated.
    async fn get_watermark_strict(
        conn: &ObjectStoreConnection,
        pipeline: &str,
    ) -> ObjectStoreWatermark {
        let path = watermark_path(pipeline);
        let bytes = conn
            .object_store
            .get(&path)
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// Raw JSON in the legacy format that was removed: required `checkpoint_hi_inclusive`
    /// and no `reader_lo`/`pruner_hi`/`pruner_timestamp_ms` fields.
    fn legacy_format_json() -> String {
        format!(
            r#"{{
                "epoch_hi_inclusive": {EPOCH_HI},
                "checkpoint_hi_inclusive": {CHECKPOINT_HI},
                "tx_hi": {TX_HI},
                "timestamp_ms_hi_inclusive": {TIMESTAMP_MS_HI}
            }}"#
        )
    }

    #[tokio::test]
    async fn test_init_watermark_reads_new_format() {
        let mut conn = store_conn().await;

        // Current format with `checkpoint_hi_inclusive: null` — matches what
        // `init_watermark(.., None)` writes.
        let json = format!(
            r#"{{
                "epoch_hi_inclusive": {EPOCH_HI},
                "checkpoint_hi_inclusive": null,
                "tx_hi": {TX_HI},
                "timestamp_ms_hi_inclusive": {TIMESTAMP_MS_HI},
                "reader_lo": {READER_LO},
                "pruner_hi": {PRUNER_HI},
                "pruner_timestamp_ms": {PRUNER_TIMESTAMP_MS}
            }}"#
        );
        put_watermark_raw(&conn, PIPELINE, &json).await;

        let watermark = conn
            .init_watermark(PIPELINE, Some(CHECKPOINT_HI))
            .await
            .unwrap()
            .unwrap();
        // Stored checkpoint is null → init returns None (existing values, not the input).
        assert_eq!(watermark.checkpoint_hi_inclusive, None);
        assert_eq!(watermark.reader_lo, Some(READER_LO));
    }

    #[tokio::test]
    async fn test_init_watermark_reads_legacy_format() {
        let mut conn = store_conn().await;

        put_watermark_raw(&conn, PIPELINE, &legacy_format_json()).await;

        let watermark = conn
            .init_watermark(PIPELINE, Some(0))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, Some(CHECKPOINT_HI));
        // Missing reader_lo deserialises as None via #[serde(default)].
        assert_eq!(watermark.reader_lo, None);
    }

    #[tokio::test]
    async fn test_init_watermark_migrates_legacy_format() {
        let mut conn = store_conn().await;

        put_watermark_raw(&conn, PIPELINE, &legacy_format_json()).await;

        // The reader_lo/pruner_hi values written during migration come from the
        // ObjectStoreWatermark constructed at the top of init_watermark, which uses
        // `init_cp + 1`. Use a known input so we can assert the migrated values.
        let init_cp = 50;
        let migrated_reader_lo = init_cp + 1;

        let watermark = conn
            .init_watermark(PIPELINE, Some(init_cp))
            .await
            .unwrap()
            .unwrap();
        // Returned values reflect the legacy file as it was on disk pre-migration.
        assert_eq!(watermark.checkpoint_hi_inclusive, Some(CHECKPOINT_HI));
        assert_eq!(watermark.reader_lo, None);

        // The stored file must now strictly parse as the new format, with original
        // fields preserved and the missing fields filled in.
        let migrated_watermark = get_watermark_strict(&conn, PIPELINE).await;
        assert_eq!(migrated_watermark.epoch_hi_inclusive, EPOCH_HI);
        assert_eq!(
            migrated_watermark.checkpoint_hi_inclusive,
            Some(CHECKPOINT_HI)
        );
        assert_eq!(migrated_watermark.tx_hi, TX_HI);
        assert_eq!(
            migrated_watermark.timestamp_ms_hi_inclusive,
            TIMESTAMP_MS_HI
        );
        assert_eq!(migrated_watermark.reader_lo, migrated_reader_lo);
        assert_eq!(migrated_watermark.pruner_hi, migrated_reader_lo);
        assert_eq!(migrated_watermark.pruner_timestamp_ms, 0);
    }

    #[tokio::test]
    async fn test_init_watermark_does_not_rewrite_new_format() {
        let mut conn = store_conn().await;

        // Seed a complete new-format watermark.
        let json = format!(
            r#"{{
                "epoch_hi_inclusive": {EPOCH_HI},
                "checkpoint_hi_inclusive": {CHECKPOINT_HI},
                "tx_hi": {TX_HI},
                "timestamp_ms_hi_inclusive": {TIMESTAMP_MS_HI},
                "reader_lo": {READER_LO},
                "pruner_hi": {PRUNER_HI},
                "pruner_timestamp_ms": {PRUNER_TIMESTAMP_MS}
            }}"#
        );
        put_watermark_raw(&conn, PIPELINE, &json).await;

        // init must hit AlreadyExists and skip the migration write — none of the new
        // fields are missing, so the stored file should be untouched.
        conn.init_watermark(PIPELINE, Some(0)).await.unwrap();

        let stored_watermark = get_watermark_strict(&conn, PIPELINE).await;
        assert_eq!(stored_watermark.epoch_hi_inclusive, EPOCH_HI);
        assert_eq!(
            stored_watermark.checkpoint_hi_inclusive,
            Some(CHECKPOINT_HI)
        );
        assert_eq!(stored_watermark.tx_hi, TX_HI);
        assert_eq!(stored_watermark.timestamp_ms_hi_inclusive, TIMESTAMP_MS_HI);
        assert_eq!(stored_watermark.reader_lo, READER_LO);
        assert_eq!(stored_watermark.pruner_hi, PRUNER_HI);
        assert_eq!(stored_watermark.pruner_timestamp_ms, PRUNER_TIMESTAMP_MS);
    }

    #[tokio::test]
    async fn test_pruner_watermark_wait_for_ms() {
        let mut conn = store_conn().await;
        bootstrap(&mut conn, CHECKPOINT_HI).await;

        let watermark = conn
            .pruner_watermark(PIPELINE, delay_for_one_hour_wait())
            .await
            .unwrap()
            .unwrap();
        // Allow generous slack for slow CI.
        assert!(
            watermark.wait_for_ms > (ONE_HOUR_MS as i64 - 100_000)
                && watermark.wait_for_ms <= ONE_HOUR_MS as i64,
            "wait_for_ms = {}",
            watermark.wait_for_ms
        );
    }

    /// `bootstrap` writes `pruner_timestamp_ms = 0`, so with `delay = 0` the inner
    /// subtraction `0 - now_ms` would underflow if it weren't using `saturating_sub`.
    /// The expected output is `wait_for_ms == 0`, not a panic and not a negative value.
    #[tokio::test]
    async fn test_pruner_watermark_saturates_when_ready() {
        let mut conn = store_conn().await;
        bootstrap(&mut conn, CHECKPOINT_HI).await;

        let watermark = conn
            .pruner_watermark(PIPELINE, Duration::ZERO)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(watermark.wait_for_ms, 0);
    }

    /// With `pruner_timestamp_ms = u64::MAX`, the saturating subtraction leaves a value
    /// well above `i64::MAX`, so the final `i64::try_from` must surface an error rather
    /// than silently wrapping or panicking.
    #[tokio::test]
    async fn test_pruner_watermark_overflow() {
        let conn = store_conn().await;

        let json = format!(
            r#"{{
                "epoch_hi_inclusive": {EPOCH_HI},
                "checkpoint_hi_inclusive": {CHECKPOINT_HI},
                "tx_hi": {TX_HI},
                "timestamp_ms_hi_inclusive": {TIMESTAMP_MS_HI},
                "reader_lo": 0,
                "pruner_hi": 0,
                "pruner_timestamp_ms": {}
            }}"#,
            u64::MAX
        );
        put_watermark_raw(&conn, PIPELINE, &json).await;

        let mut conn = conn;
        let result = conn.pruner_watermark(PIPELINE, Duration::ZERO).await;
        assert!(result.is_err(), "expected overflow error, got {result:?}");
    }
}
