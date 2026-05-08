// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The auto-registered bookkeeping schema, [`FrameworkSchema`],
//! plus the on-disk types it persists.
//!
//! Three column families are owned by the framework rather than by
//! any consumer's schema, so the crate registers them automatically
//! on every [`Db::open`](crate::Db::open) and exposes them as a
//! cheaply-constructible [`FrameworkSchema`] handle:
//!
//! - `__restore` (`PipelineTaskKey → RestoreState`) — per-pipeline
//!   restore progress; consumed by external restore drivers to
//!   resume from the last-committed per-shard cursor.
//! - `__watermark` (`PipelineTaskKey → Watermark`) — per-pipeline
//!   committer watermark; used by tip-mode drivers to learn what
//!   checkpoint each pipeline resumes from.
//! - `__chain_id` (`PipelineTaskKey → ChainId`) — per-pipeline
//!   chain identifier; used by tip-mode drivers to refuse
//!   checkpoints from a different chain than the pipeline was
//!   originally bound to.
//!
//! The double-underscore prefix marks each CF as crate-internal so
//! user schemas avoid colliding on the name.
//!
//! # Access
//!
//! Hold a [`Db`] (or [`Snapshot`](crate::Snapshot)) and call
//! [`framework`](Db::framework) /
//! [`framework`](crate::Snapshot::framework) to obtain a
//! `FrameworkSchema<&Db>` / `FrameworkSchema<&Snapshot>`. Both
//! return values are zero-`Arc`-bump — three [`DbMap`]s borrowing
//! the same reader — and scoped to the borrow that produced them.
//!
//! For an owned handle (e.g. to hold inside a longer-lived
//! [`Store`](crate::Store)-like struct, or to use with [`Batch`](crate::Batch)'s
//! typed writes), construct with
//! [`FrameworkSchema::new(db.clone())`](FrameworkSchema::new).

use bytes::Buf;
use bytes::BufMut;

use crate::Decode;
use crate::Encode;
use crate::db::Db;
use crate::error::DecodeError;
use crate::error::EncodeError;
use crate::map::DbMap;
use crate::reader::Reader;

/// Name of the column family holding per-pipeline [`RestoreState`]
/// entries. Crate-internal; user code reaches the CF through
/// [`FrameworkSchema::restore`].
pub(crate) const RESTORE_CF: &str = "__restore";

/// Name of the column family holding per-pipeline [`Watermark`]s.
/// Crate-internal; user code reaches the CF through
/// [`FrameworkSchema::watermarks`].
pub(crate) const WATERMARK_CF: &str = "__watermark";

/// Name of the column family holding per-pipeline [`ChainId`]s.
/// Crate-internal; user code reaches the CF through
/// [`FrameworkSchema::chain_ids`].
pub(crate) const CHAIN_ID_CF: &str = "__chain_id";

/// The column families owned by the framework rather than by any
/// consumer's schema. Registered automatically on every
/// [`Db::open`](crate::Db::open), and rejected when a consumer
/// schema declares one of them itself.
pub(crate) const FRAMEWORK_CFS: [&str; 3] = [RESTORE_CF, WATERMARK_CF, CHAIN_ID_CF];

/// Typed `pipeline_task` key used by the framework's internal CFs.
///
/// Encoded as raw UTF-8 bytes — the framework CFs are internal, so
/// we pick the simplest representation. Decoding produces an owned
/// `String`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineTaskKey(pub String);

impl PipelineTaskKey {
    /// Build a key from any string-ish input.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

impl Encode for PipelineTaskKey {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.0.as_bytes());
        Ok(())
    }
}

impl Decode for PipelineTaskKey {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        let mut bytes = vec![0u8; buf.remaining()];
        buf.copy_to_slice(&mut bytes);
        let s = String::from_utf8(bytes)
            .map_err(|e| DecodeError::with_source("PipelineTaskKey not valid UTF-8", e))?;
        Ok(Self(s))
    }
}

/// Per-pipeline restore progress, persisted in the `__restore` CF.
///
/// Re-export of the generated protobuf message. Drivers transition
/// a pipeline:
/// 1. `None` (no entry) → `InProgress` when restore begins.
/// 2. `InProgress` → `InProgress` with per-shard cursors advanced
///    atomically with each chunk's data writes.
/// 3. `InProgress` → `Complete` when every shard's stream has
///    been fully consumed.
///
/// Tip indexing for a pipeline must wait until its state reaches
/// `Complete`. Drivers check this on startup.
pub use crate::proto::sui::db::v1alpha::RestoreState;
pub use crate::proto::sui::db::v1alpha::restore_state;

impl Encode for RestoreState {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        crate::protobuf::encode_into(self, buf)
    }
}

impl Decode for RestoreState {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        crate::protobuf::decode(buf)
    }
}

/// Per-pipeline committer watermark, persisted in the `__watermark` CF.
///
/// Re-export of the generated protobuf message. Holds the highest
/// checkpoint each pipeline has committed plus the corresponding
/// epoch / tx / timestamp. Tip-mode drivers advance this
/// atomically with each pipeline's data writes, and read it on
/// restart to decide where to resume.
pub use crate::proto::sui::db::v1alpha::Watermark;

impl Watermark {
    /// Build a watermark for `checkpoint`, leaving every other
    /// field at zero. Useful in tests and for callers that only
    /// care about the checkpoint axis (snapshot eviction order,
    /// `Db::at_snapshot` keying).
    pub fn for_checkpoint(checkpoint: u64) -> Self {
        Self {
            checkpoint_hi_inclusive: checkpoint,
            ..Self::default()
        }
    }
}

impl Encode for Watermark {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        crate::protobuf::encode_into(self, buf)
    }
}

impl Decode for Watermark {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        crate::protobuf::decode(buf)
    }
}

/// Typed chain-identifier value (`[u8; 32]`) persisted in the
/// `__chain_id` CF.
///
/// Tip-mode drivers store the chain id the pipeline was first
/// bound to so the framework can refuse checkpoints from a
/// different chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainId(pub [u8; 32]);

impl Encode for ChainId {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(&self.0);
        Ok(())
    }
}

impl Decode for ChainId {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() != 32 {
            return Err(DecodeError::msg("ChainId wire size mismatch"));
        }
        let mut id = [0u8; 32];
        buf.copy_to_slice(&mut id);
        Ok(Self(id))
    }
}

/// Typed handles into the framework's auto-registered column
/// families.
///
/// The three CFs (`__restore`, `__watermark`, `__chain_id`)
/// are registered automatically by
/// [`Db::open`](crate::Db::open), so this schema does not need to
/// be declared by user schemas. Obtain a borrowed handle via
/// [`Db::framework`] or [`Snapshot::framework`](crate::Snapshot::framework);
/// construct an owned one via [`FrameworkSchema::new`].
///
/// `R` defaults to [`Db`] for symmetry with [`DbMap`].
pub struct FrameworkSchema<R: Reader + Clone = Db> {
    /// Per-pipeline [`RestoreState`] entries. See the `__restore`
    /// CF.
    pub restore: DbMap<PipelineTaskKey, RestoreState, R>,
    /// Per-pipeline [`Watermark`] entries. See the `__watermark`
    /// CF.
    pub watermarks: DbMap<PipelineTaskKey, Watermark, R>,
    /// Per-pipeline [`ChainId`] entries. See the `__chain_id` CF.
    pub chain_ids: DbMap<PipelineTaskKey, ChainId, R>,
}

impl<R: Reader + Clone> FrameworkSchema<R> {
    /// Construct a `FrameworkSchema` bound to `reader`.
    ///
    /// The constructor is infallible because the three framework
    /// CFs are auto-registered by [`Db::open`](crate::Db::open), so
    /// the CF-existence check that [`DbMap::new`] performs is
    /// redundant here. Each field clones `reader` once.
    pub fn new(reader: R) -> Self {
        Self {
            restore: DbMap::new_unchecked(reader.clone(), RESTORE_CF),
            watermarks: DbMap::new_unchecked(reader.clone(), WATERMARK_CF),
            chain_ids: DbMap::new_unchecked(reader, CHAIN_ID_CF),
        }
    }
}

impl<R: Reader + Clone> std::fmt::Debug for FrameworkSchema<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameworkSchema").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::DbOptions;
    use crate::Schema;
    use crate::error::OpenError;

    /// Minimal user schema (no extra CFs) used to open a database
    /// purely to exercise the auto-registered framework CFs.
    #[derive(Debug)]
    struct EmptySchema;

    impl Schema for EmptySchema {
        fn cfs(_: &crate::options::CfOptionsResolver) -> Vec<crate::CfDescriptor> {
            vec![]
        }

        fn open(_: &Db) -> Result<Self, OpenError> {
            Ok(Self)
        }
    }

    fn open() -> (TempDir, Db) {
        let dir = TempDir::new().unwrap();
        let (db, _schema) = Db::open::<EmptySchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db)
    }

    #[test]
    fn framework_cfs_are_auto_registered() {
        let (_dir, db) = open();
        assert!(db.cf_handle(RESTORE_CF).is_some());
        assert!(db.cf_handle(WATERMARK_CF).is_some());
        assert!(db.cf_handle(CHAIN_ID_CF).is_some());
    }

    #[test]
    fn db_framework_returns_borrowed_schema() {
        let (_dir, db) = open();
        let fw = db.framework();
        // No data written; reads return None.
        let key = PipelineTaskKey::new("balances");
        assert!(fw.restore.get(&key).unwrap().is_none());
        assert!(fw.watermarks.get(&key).unwrap().is_none());
        assert!(fw.chain_ids.get(&key).unwrap().is_none());
    }

    #[test]
    fn owned_framework_round_trips_watermark() {
        let (_dir, db) = open();
        let fw = FrameworkSchema::new(db.clone());
        let key = PipelineTaskKey::new("p");
        let w = Watermark {
            epoch_hi_inclusive: 3,
            checkpoint_hi_inclusive: 42,
            tx_hi: 99,
            timestamp_ms_hi_inclusive: 1_700_000_000_000,
        };

        let mut batch = db.batch();
        batch.put(&fw.watermarks, &key, &w).unwrap();
        batch.commit().unwrap();

        assert_eq!(db.framework().watermarks.get(&key).unwrap(), Some(w));
    }

    #[test]
    fn owned_framework_round_trips_chain_id() {
        let (_dir, db) = open();
        let fw = FrameworkSchema::new(db.clone());
        let key = PipelineTaskKey::new("p");
        let chain_id = ChainId([7u8; 32]);

        let mut batch = db.batch();
        batch.put(&fw.chain_ids, &key, &chain_id).unwrap();
        batch.commit().unwrap();

        assert_eq!(db.framework().chain_ids.get(&key).unwrap(), Some(chain_id));
    }

    #[test]
    fn owned_framework_round_trips_restore_state() {
        let (_dir, db) = open();
        let fw = FrameworkSchema::new(db.clone());
        let key = PipelineTaskKey::new("p");
        let state =
            RestoreState::default().with_complete(restore_state::Complete { restored_at: 7 });

        let mut batch = db.batch();
        batch.put(&fw.restore, &key, &state).unwrap();
        batch.commit().unwrap();

        assert_eq!(db.framework().restore.get(&key).unwrap(), Some(state));
    }

    #[test]
    fn pipeline_task_key_round_trips() {
        let key = PipelineTaskKey::new("balances@indexer_a");
        let mut buf = Vec::new();
        key.encode_into(&mut buf).unwrap();
        let mut slice = buf.as_slice();
        let decoded = PipelineTaskKey::decode(&mut slice).unwrap();
        assert_eq!(decoded, key);
    }

    #[test]
    fn watermark_round_trips_through_encoding() {
        let w = Watermark {
            epoch_hi_inclusive: 3,
            checkpoint_hi_inclusive: 12345,
            tx_hi: 99,
            timestamp_ms_hi_inclusive: 1_700_000_000_000,
        };
        let buf = w.encode().unwrap();
        let decoded = Watermark::decode(&mut buf.as_slice()).unwrap();
        assert_eq!(decoded, w);
    }

    #[test]
    fn watermark_for_checkpoint_sets_only_checkpoint_field() {
        let w = Watermark::for_checkpoint(42);
        assert_eq!(w.checkpoint_hi_inclusive, 42);
        assert_eq!(w.epoch_hi_inclusive, 0);
        assert_eq!(w.tx_hi, 0);
        assert_eq!(w.timestamp_ms_hi_inclusive, 0);
    }

    #[test]
    fn watermark_default_round_trips() {
        // A default Watermark has every field at 0; prost
        // serializes it to an empty buffer (proto3 skips default
        // scalars) and decoding returns the all-zeros value.
        let w = Watermark::default();
        let buf = w.encode().unwrap();
        assert!(buf.is_empty(), "default watermark should encode to 0 bytes");
        let decoded = Watermark::decode(&mut buf.as_slice()).unwrap();
        assert_eq!(decoded, w);
    }

    #[test]
    fn chain_id_decode_rejects_wrong_length() {
        let bytes = [0u8; 16];
        let mut slice = bytes.as_slice();
        let err = ChainId::decode(&mut slice).unwrap_err();
        assert!(format!("{err:#}").contains("wire size mismatch"));
    }

    #[test]
    fn snapshot_framework_reads_pre_snapshot_state() {
        // The borrowed-snapshot accessor returns a FrameworkSchema
        // whose reads see the captured snapshot state.
        let (_dir, db) = open();
        let key = PipelineTaskKey::new("p");
        let w = Watermark {
            checkpoint_hi_inclusive: 10,
            ..Watermark::default()
        };
        let fw = FrameworkSchema::new(db.clone());
        let mut batch = db.batch();
        batch.put(&fw.watermarks, &key, &w).unwrap();
        batch.commit().unwrap();

        db.take_snapshot(Watermark::for_checkpoint(1));

        let w2 = Watermark {
            checkpoint_hi_inclusive: 999,
            ..Watermark::default()
        };
        let mut batch = db.batch();
        batch.put(&fw.watermarks, &key, &w2).unwrap();
        batch.commit().unwrap();

        let snap = db.at_snapshot(1).unwrap();
        let fw_snap = snap.framework();
        assert_eq!(fw_snap.watermarks.get(&key).unwrap(), Some(w));
    }

    // RestoreState encoding tests.

    fn round_trip(state: &RestoreState) -> RestoreState {
        let buf = state.encode().unwrap();
        RestoreState::decode(&mut buf.as_slice()).unwrap()
    }

    #[test]
    fn restore_state_round_trip_complete() {
        let s = RestoreState::default().with_complete(restore_state::Complete {
            restored_at: 12_345,
        });
        assert_eq!(round_trip(&s), s);
    }

    #[test]
    fn restore_state_round_trip_in_progress_empty() {
        let s = RestoreState::default().with_in_progress(restore_state::InProgress {
            target_checkpoint: 999,
            shards: std::collections::BTreeMap::new(),
        });
        assert_eq!(round_trip(&s), s);
    }

    #[test]
    fn restore_state_round_trip_in_progress_with_shards() {
        let s = RestoreState::default().with_in_progress(restore_state::InProgress {
            target_checkpoint: 1,
            shards: [
                (
                    0u32,
                    restore_state::ShardProgress::default()
                        .with_in_progress(bytes::Bytes::from_static(b"cursor-0")),
                ),
                (
                    7u32,
                    restore_state::ShardProgress::default()
                        .with_done(restore_state::shard_progress::Done {}),
                ),
            ]
            .into_iter()
            .collect(),
        });
        assert_eq!(round_trip(&s), s);
    }

    #[test]
    fn restore_state_empty_message_round_trips_to_empty_message() {
        // An empty buffer decodes to a RestoreState with no oneof
        // variant set. Callers that care about "is there a state?"
        // inspect `restore_state.state` directly.
        let bytes: [u8; 0] = [];
        let decoded = RestoreState::decode(&mut bytes.as_slice()).unwrap();
        assert!(decoded.state.is_none());
    }
}
