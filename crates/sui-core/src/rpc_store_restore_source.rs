// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! [`RestoreSource`] backed by a validator's
//! [`AuthorityPerpetualTables`].
//!
//! Streams every `LiveObject::Normal` in the perpetual store
//! into the `sui-consistent-store` restore driver, sharded by
//! `ObjectID` prefix so multiple shards can iterate in parallel.
//!
//! # Sharding
//!
//! The `ObjectID` space is split into 32 shards by the top
//! `SHARD_BITS = 5` bits of the first byte (matching the split
//! used by `par_index_live_object_set`).
//! Each shard yields chunks of [`CHUNK_SIZE`] objects; the
//! `RestoreChunk::cursor` is the 32-byte ObjectID of the last
//! object in that chunk, so resuming with `Some(c)` starts the
//! next iteration immediately after that id.
//!
//! # Snapshot consistency
//!
//! Each shard's stream opens exactly one RocksDB iterator and
//! drives it to completion from a single `spawn_blocking` task,
//! pushing chunks back over a tokio mpsc. RocksDB iterators
//! created without an explicit snapshot implicitly pin one at
//! construction time, so a shard sees a single point-in-time
//! view for its full run — including the merge-based `balance`
//! pipeline, which is safe against concurrent execution.
//!
//! Different shards take their snapshots at the moments their
//! `spawn_blocking` tasks start, so cross-shard skew can still
//! exist if the validator commits between shard launches. This
//! does not affect any of the `sui-rpc-store` pipelines because
//! every object lives in exactly one shard.
//!
//! A side-effect of holding open one iterator per shard for the
//! full restore is that the SSTs it references stay pinned and
//! cannot compact away for the duration. That is acceptable for
//! a one-shot bootstrap.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use futures::stream;
use futures::stream::BoxStream;
use sui_consistent_store::ChainId;
use sui_consistent_store::restore::RestoreChunk;
use sui_consistent_store::restore::RestoreSource;
use sui_types::base_types::ObjectID;
use sui_types::object::Object;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::authority::authority_store_tables::AuthorityPerpetualTables;
use crate::authority::authority_store_tables::LiveObject;

/// Bits of the first `ObjectID` byte used to choose a shard.
/// `1 << SHARD_BITS` shards, matching the constant in
/// `par_index_live_object_set`.
const SHARD_BITS: u32 = 5;

/// Total number of shards (`1 << SHARD_BITS`).
const SHARDS: u32 = 1 << SHARD_BITS;

/// Bit shift placing the shard id in the high bits of the first
/// `ObjectID` byte.
const SHARD_PREFIX_SHIFT: u32 = 8 - SHARD_BITS;

/// Default objects per [`RestoreChunk`]. Tuned to keep the
/// per-pipeline batch comfortably under a few MB of writes
/// while still amortising the per-chunk commit overhead.
pub const CHUNK_SIZE: usize = 50_000;

/// [`RestoreSource`] over an
/// [`AuthorityPerpetualTables`]. Construct via
/// [`PerpetualStoreRestoreSource::new`].
pub struct PerpetualStoreRestoreSource {
    perpetual: Arc<AuthorityPerpetualTables>,
    target_checkpoint: u64,
    chain_id: ChainId,
    chunk_size: usize,
}

impl PerpetualStoreRestoreSource {
    /// Build a source rooted at `perpetual`, anchored at
    /// `target_checkpoint` and `chain_id`. Tip indexing will
    /// resume at `target_checkpoint + 1` once restore finishes
    /// — pick the highest executed checkpoint the perpetual
    /// store has seen at restore time. `chain_id` is pinned
    /// into `__chain_id` on finalise so subsequent tip
    /// indexing refuses checkpoints from the wrong chain.
    pub fn new(
        perpetual: Arc<AuthorityPerpetualTables>,
        target_checkpoint: u64,
        chain_id: ChainId,
    ) -> Self {
        Self {
            perpetual,
            target_checkpoint,
            chain_id,
            chunk_size: CHUNK_SIZE,
        }
    }

    /// Override the per-chunk object count. Useful for tests
    /// that want to exercise multi-chunk shards without
    /// materialising 50k objects.
    pub fn with_chunk_size(mut self, chunk_size: usize) -> Self {
        assert!(chunk_size > 0, "chunk_size must be > 0");
        self.chunk_size = chunk_size;
        self
    }
}

/// Inclusive `[start, end]` `ObjectID` range covered by `shard_id`.
fn shard_range(shard_id: u32) -> (ObjectID, ObjectID) {
    let prefix = (shard_id as u8) << SHARD_PREFIX_SHIFT;
    let mut start = [0u8; ObjectID::LENGTH];
    start[0] = prefix;
    let mut end = [0xffu8; ObjectID::LENGTH];
    end[0] = prefix | ((1 << SHARD_PREFIX_SHIFT) - 1);
    (ObjectID::new(start), ObjectID::new(end))
}

/// Increment `id` as a 256-bit big-endian integer, returning
/// `None` on overflow.
fn next_id(id: ObjectID) -> Option<ObjectID> {
    let mut bytes = id.into_bytes();
    for byte in bytes.iter_mut().rev() {
        if *byte == 0xff {
            *byte = 0;
        } else {
            *byte += 1;
            return Some(ObjectID::new(bytes));
        }
    }
    None
}

#[async_trait]
impl RestoreSource for PerpetualStoreRestoreSource {
    fn target_checkpoint(&self) -> u64 {
        self.target_checkpoint
    }

    fn target_chain_id(&self) -> ChainId {
        self.chain_id
    }

    fn shards(&self) -> u32 {
        SHARDS
    }

    fn stream(
        &self,
        shard_id: u32,
        cursor: Option<Bytes>,
    ) -> BoxStream<'_, anyhow::Result<RestoreChunk>> {
        let (shard_start, shard_end) = shard_range(shard_id);

        let start_id = match cursor {
            None => Some(shard_start),
            Some(bytes) => match ObjectID::from_bytes(&bytes[..]) {
                Ok(id) => next_id(id).filter(|n| *n <= shard_end),
                Err(e) => {
                    return stream::once(async move {
                        Err(anyhow::anyhow!("invalid perpetual-store cursor: {e}"))
                    })
                    .boxed();
                }
            },
        };

        let Some(start_id) = start_id else {
            return stream::empty().boxed();
        };

        // Bounded mpsc applies backpressure on the iterator
        // task so it pauses when the driver hasn't committed
        // the previous chunk yet.
        let (tx, rx) = mpsc::channel::<anyhow::Result<RestoreChunk>>(2);
        let perpetual = self.perpetual.clone();
        let chunk_size = self.chunk_size;

        tokio::task::spawn_blocking(move || {
            iterate_shard(perpetual, start_id, shard_end, chunk_size, tx);
        });

        ReceiverStream::new(rx).boxed()
    }
}

/// Drive one shard's iteration end-to-end in a single
/// `spawn_blocking` task.
///
/// Opens exactly one `range_iter_live_object_set` and pushes
/// chunks of up to `chunk_size` `LiveObject::Normal` rows over
/// `tx`. The iterator's implicit RocksDB snapshot is held for
/// the lifetime of this function, so the whole shard observes
/// a single point-in-time view of the perpetual store.
///
/// Returns early without sending anything if the receiver is
/// dropped (e.g. the driver was cancelled).
fn iterate_shard(
    perpetual: Arc<AuthorityPerpetualTables>,
    start_id: ObjectID,
    shard_end: ObjectID,
    chunk_size: usize,
    tx: mpsc::Sender<anyhow::Result<RestoreChunk>>,
) {
    let iter = perpetual.range_iter_live_object_set(Some(start_id), Some(shard_end), false);
    let mut buffer: Vec<Object> = Vec::with_capacity(chunk_size.min(1024));

    for live in iter {
        let LiveObject::Normal(obj) = live else {
            continue;
        };
        buffer.push(obj);
        if buffer.len() >= chunk_size {
            let chunk = std::mem::replace(&mut buffer, Vec::with_capacity(chunk_size.min(1024)));
            if send_chunk(&tx, chunk).is_err() {
                return;
            }
        }
    }

    if !buffer.is_empty() {
        let _ = send_chunk(&tx, buffer);
    }
}

/// Wrap `objects` in a [`RestoreChunk`] (cursor = last object's
/// id) and blocking-send it. Returns `Err(())` if the receiver
/// is closed so the caller can stop iterating.
fn send_chunk(
    tx: &mpsc::Sender<anyhow::Result<RestoreChunk>>,
    objects: Vec<Object>,
) -> Result<(), ()> {
    let last_id = objects.last().expect("non-empty chunk").id();
    let chunk = RestoreChunk {
        objects,
        cursor: Bytes::copy_from_slice(&last_id.into_bytes()),
    };
    tx.blocking_send(Ok(chunk)).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use tempfile::TempDir;

    use super::*;

    fn open_perpetual() -> (TempDir, Arc<AuthorityPerpetualTables>) {
        let dir = TempDir::new().unwrap();
        let perpetual = Arc::new(AuthorityPerpetualTables::open(dir.path(), None, None));
        (dir, perpetual)
    }

    fn obj_with_first_byte(first: u8, last: u8) -> Object {
        let mut bytes = [0u8; ObjectID::LENGTH];
        bytes[0] = first;
        bytes[ObjectID::LENGTH - 1] = last;
        Object::immutable_with_id_for_testing(ObjectID::new(bytes))
    }

    /// Hand-pick a representative shard and verify the shard
    /// range covers the right ObjectID prefixes.
    #[test]
    fn shard_range_covers_correct_prefixes() {
        let (s0, e0) = shard_range(0);
        assert_eq!(s0.into_bytes()[0], 0x00);
        assert_eq!(e0.into_bytes()[0], 0x07);

        let (s1, e1) = shard_range(1);
        assert_eq!(s1.into_bytes()[0], 0x08);
        assert_eq!(e1.into_bytes()[0], 0x0F);

        let (s31, e31) = shard_range(31);
        assert_eq!(s31.into_bytes()[0], 0xF8);
        assert_eq!(e31.into_bytes()[0], 0xFF);
        // Last byte of the upper bound is 0xFF.
        assert_eq!(e31.into_bytes()[ObjectID::LENGTH - 1], 0xFF);
    }

    #[test]
    fn next_id_increments_with_carry() {
        let mut bytes = [0u8; ObjectID::LENGTH];
        bytes[ObjectID::LENGTH - 1] = 0xff;
        bytes[ObjectID::LENGTH - 2] = 0x01;
        let inc = next_id(ObjectID::new(bytes)).unwrap().into_bytes();
        let mut expected = [0u8; ObjectID::LENGTH];
        expected[ObjectID::LENGTH - 1] = 0x00;
        expected[ObjectID::LENGTH - 2] = 0x02;
        assert_eq!(inc, expected);
    }

    #[test]
    fn next_id_overflow_returns_none() {
        let max = ObjectID::new([0xff; ObjectID::LENGTH]);
        assert_eq!(next_id(max), None);
    }

    /// End-to-end smoke: seed objects across two shards, drain
    /// every shard's stream, confirm every object lands exactly
    /// once and shard boundaries are respected.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn streams_objects_across_shards() {
        let (_dir, perpetual) = open_perpetual();

        // Insert four objects across shard 0 (first byte in
        // 0x00..=0x07) and shard 1 (0x08..=0x0F).
        let inserted: Vec<Object> = [(0x01, 0xaa), (0x05, 0xbb), (0x0a, 0xcc), (0x0f, 0xdd)]
            .into_iter()
            .map(|(first, last)| obj_with_first_byte(first, last))
            .collect();
        for o in &inserted {
            perpetual.insert_object_test_only(o.clone()).unwrap();
        }

        let source = PerpetualStoreRestoreSource::new(perpetual.clone(), 7, ChainId([9u8; 32]))
            .with_chunk_size(1);
        assert_eq!(source.target_checkpoint(), 7);
        assert_eq!(source.shards(), SHARDS);

        // Drain shard 0 and shard 1; assert every other shard is empty.
        let mut got = BTreeSet::new();
        for shard in 0..SHARDS {
            let mut stream = source.stream(shard, None);
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.unwrap();
                for o in chunk.objects {
                    got.insert(o.id());
                }
            }
        }
        let want: BTreeSet<_> = inserted.iter().map(|o| o.id()).collect();
        assert_eq!(got, want);
    }

    /// Resume from a cursor that points at the first object in
    /// a shard and confirm the second object (and only the
    /// second) is yielded.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn resume_from_cursor_skips_already_yielded() {
        let (_dir, perpetual) = open_perpetual();

        let a = obj_with_first_byte(0x01, 0x10);
        let b = obj_with_first_byte(0x01, 0x20);
        perpetual.insert_object_test_only(a.clone()).unwrap();
        perpetual.insert_object_test_only(b.clone()).unwrap();

        // Shard 0 covers first byte 0x00..=0x07, so both
        // objects live there.
        let source = PerpetualStoreRestoreSource::new(perpetual.clone(), 0, ChainId([0u8; 32]));
        let cursor = Bytes::copy_from_slice(&a.id().into_bytes());
        let mut stream = source.stream(0, Some(cursor));
        let mut yielded = Vec::new();
        while let Some(chunk) = stream.next().await {
            for o in chunk.unwrap().objects {
                yielded.push(o.id());
            }
        }
        assert_eq!(yielded, vec![b.id()]);
    }
}
