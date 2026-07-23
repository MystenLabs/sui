// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Fork-owned authoritative "current live state" pointer for objects.
//!
//! `sui-rpc-store`'s `objects` column family stores every version the fork has
//! ever materialized, but the fork populates it *sparsely* — arbitrary pre-fork
//! versions are fetched on demand for exact-version and bounded child reads. As
//! a result, the greatest `(id, version)` row present locally is **not**
//! necessarily the object's current live version, and a reverse scan cannot
//! distinguish "removed" from "not-yet-materialized".
//!
//! This module keeps a small, fork-local table `ObjectID -> ForkLiveState` that
//! records the authoritative current state of every object the fork actually
//! knows to be current: the base version materialized at the fork checkpoint,
//! and every version written or removed by local execution. It is the
//! `sui-fork` equivalent of the old filesystem `latest` marker, and it lets the
//! fork answer "latest live object", "is this object removed", and "should I
//! fall back to GraphQL" correctly against the sparse `objects` CF.
//!
//! It is backed by its own single-column-family [`sui_consistent_store::Db`]
//! (opened synchronously) under `{data_dir}/live_state/`, separate from the
//! `sui-rpc-store` database. The split is historical rather than forced:
//! `Schema` composes publicly, so a fork-owned schema could host this column
//! family in the main database — making row and pointer commits atomic —
//! without modifying `sui-rpc-store`. See design/storage.md § "Known gaps".

use std::path::Path;

use anyhow::Context as _;
use bytes::Buf;
use bytes::BufMut;

use sui_consistent_store::CfDescriptor;
use sui_consistent_store::CfOptionsResolver;
use sui_consistent_store::Db;
use sui_consistent_store::DbMap;
use sui_consistent_store::DbOptions;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::Schema;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_consistent_store::error::OpenError;
use sui_rpc_store::schema::objects::TombstoneKind;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;

/// On-disk column family name for the live-state pointer table.
const CF_NAME: &str = "fork_live_state";

/// Subdirectory of the fork data dir holding the live-state database.
const LIVE_STATE_DIR: &str = "live_state";

/// The authoritative current state of an object as tracked by the fork.
///
/// Removals reuse `sui-rpc-store`'s [`TombstoneKind`] (`Deleted` / `Wrapped`);
/// `unwrapped_then_deleted` is already collapsed into `Deleted` upstream, so no
/// fork-specific removal kind is needed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ForkLiveState {
    /// The object is currently live at `version`; the object bytes live in the
    /// `sui-rpc-store` `objects` CF at `(id, version)`.
    Live(SequenceNumber),
    /// The object was removed from the live set at `version`.
    Removed {
        version: SequenceNumber,
        kind: TombstoneKind,
    },
}

impl ForkLiveState {
    /// Whether this state represents a removed object. Removed objects must not
    /// be resurrected from a remote GraphQL fallback.
    #[cfg(test)]
    pub(crate) fn is_removed(&self) -> bool {
        matches!(self, ForkLiveState::Removed { .. })
    }
}

/// Key wrapper encoding an `ObjectID` as its raw 32 bytes.
struct Key(ObjectID);

impl Encode for Key {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.0.as_ref());
        Ok(())
    }
}

impl Decode for Key {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() != ObjectID::LENGTH {
            return Err(DecodeError::msg(format!(
                "expected {} bytes for {CF_NAME} Key, got {}",
                ObjectID::LENGTH,
                buf.remaining(),
            )));
        }
        let mut bytes = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut bytes);
        Ok(Key(ObjectID::new(bytes)))
    }
}

// Value tags.
const TAG_LIVE: u8 = 0;
const TAG_REMOVED: u8 = 1;
// Removal-kind discriminants.
const KIND_DELETED: u8 = 0;
const KIND_WRAPPED: u8 = 1;

impl Encode for ForkLiveState {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        match self {
            ForkLiveState::Live(version) => {
                buf.put_u8(TAG_LIVE);
                buf.put_u64(version.value());
            }
            ForkLiveState::Removed { version, kind } => {
                buf.put_u8(TAG_REMOVED);
                buf.put_u64(version.value());
                buf.put_u8(match kind {
                    TombstoneKind::Deleted => KIND_DELETED,
                    TombstoneKind::Wrapped => KIND_WRAPPED,
                });
            }
        }
        Ok(())
    }
}

impl Decode for ForkLiveState {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() < 1 {
            return Err(DecodeError::msg("empty ForkLiveState value"));
        }
        let tag = buf.get_u8();
        match tag {
            TAG_LIVE => {
                if buf.remaining() != 8 {
                    return Err(DecodeError::msg("expected 8 bytes for Live version"));
                }
                Ok(ForkLiveState::Live(SequenceNumber::from_u64(buf.get_u64())))
            }
            TAG_REMOVED => {
                if buf.remaining() != 9 {
                    return Err(DecodeError::msg(
                        "expected 9 bytes for Removed version + kind",
                    ));
                }
                let version = SequenceNumber::from_u64(buf.get_u64());
                let kind = match buf.get_u8() {
                    KIND_DELETED => TombstoneKind::Deleted,
                    KIND_WRAPPED => TombstoneKind::Wrapped,
                    other => {
                        return Err(DecodeError::msg(format!(
                            "unrecognised removal kind: {other}"
                        )));
                    }
                };
                Ok(ForkLiveState::Removed { version, kind })
            }
            other => Err(DecodeError::msg(format!(
                "unrecognised ForkLiveState tag: {other}"
            ))),
        }
    }
}

/// Single-column-family schema for the live-state database.
struct LiveStateSchema {
    live_state: DbMap<Key, ForkLiveState>,
}

impl Schema for LiveStateSchema {
    fn cfs(opts: &CfOptionsResolver) -> Vec<CfDescriptor> {
        vec![CfDescriptor::new(CF_NAME, opts.options(CF_NAME))]
    }

    fn open(db: &Db) -> Result<Self, OpenError> {
        Ok(Self {
            live_state: DbMap::new(db.clone(), CF_NAME)?,
        })
    }
}

/// Fork-local authoritative live-state pointer table.
pub(crate) struct LiveState {
    db: Db,
    schema: LiveStateSchema,
}

impl LiveState {
    /// Open (or create) the live-state database under `{root}/live_state/`.
    ///
    /// Synchronous: [`sui_consistent_store::Db::open`] does not require a Tokio
    /// runtime, so this can run from the fork's synchronous constructors as well
    /// as its async startup path.
    pub(crate) fn open(root: &Path) -> anyhow::Result<Self> {
        let path = root.join(LIVE_STATE_DIR);
        let (db, schema) = Db::open::<LiveStateSchema>(&path, DbOptions::default())
            .with_context(|| format!("failed to open live-state db at {}", path.display()))?;
        Ok(Self { db, schema })
    }

    /// Return the authoritative current state of `id`, or `None` if the fork has
    /// no live pointer for it (meaning: unknown — the caller may fall back to
    /// GraphQL).
    pub(crate) fn get(&self, id: ObjectID) -> anyhow::Result<Option<ForkLiveState>> {
        Ok(self.schema.live_state.get(&Key(id))?)
    }

    /// Record `id` as currently live at `version`. Used when a base object is
    /// materialized at the fork checkpoint and when local execution writes a new
    /// object version.
    pub(crate) fn set_live(&self, id: ObjectID, version: SequenceNumber) -> anyhow::Result<()> {
        let mut batch = self.db.batch();
        batch.put(
            &self.schema.live_state,
            &Key(id),
            &ForkLiveState::Live(version),
        )?;
        batch.commit().context("failed to write live-state pointer")
    }

    /// Atomically apply the live-state changes from one executed checkpoint:
    /// every removed object becomes `Removed { version, kind }`, every written
    /// object becomes `Live(version)`.
    ///
    /// Writes are staged after removals so that an object which is both removed
    /// and rewritten in the same result (e.g. wrapped then written again) ends up
    /// `Live`. Objects created and terminally deleted in the same result are
    /// excluded from `written` by the caller, so they correctly stay `Removed`.
    pub(crate) fn apply_checkpoint<W, R>(&self, written: W, removed: R) -> anyhow::Result<()>
    where
        W: IntoIterator<Item = (ObjectID, SequenceNumber)>,
        R: IntoIterator<Item = (ObjectID, SequenceNumber, TombstoneKind)>,
    {
        let mut batch = self.db.batch();
        for (id, version, kind) in removed {
            batch.put(
                &self.schema.live_state,
                &Key(id),
                &ForkLiveState::Removed { version, kind },
            )?;
        }
        for (id, version) in written {
            batch.put(
                &self.schema.live_state,
                &Key(id),
                &ForkLiveState::Live(version),
            )?;
        }
        batch
            .commit()
            .context("failed to apply checkpoint live-state")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_temp() -> (tempfile::TempDir, LiveState) {
        let dir = tempfile::tempdir().unwrap();
        let live = LiveState::open(dir.path()).unwrap();
        (dir, live)
    }

    #[test]
    fn unknown_object_has_no_pointer() {
        let (_dir, live) = open_temp();
        assert!(live.get(ObjectID::random()).unwrap().is_none());
    }

    #[test]
    fn set_live_round_trips() {
        let (_dir, live) = open_temp();
        let id = ObjectID::random();
        live.set_live(id, SequenceNumber::from_u64(7)).unwrap();
        assert_eq!(
            live.get(id).unwrap(),
            Some(ForkLiveState::Live(SequenceNumber::from_u64(7))),
        );
    }

    #[test]
    fn apply_checkpoint_writes_and_removes() {
        let (_dir, live) = open_temp();
        let written = ObjectID::random();
        let deleted = ObjectID::random();
        let wrapped = ObjectID::random();

        live.apply_checkpoint(
            [(written, SequenceNumber::from_u64(3))],
            [
                (deleted, SequenceNumber::from_u64(4), TombstoneKind::Deleted),
                (wrapped, SequenceNumber::from_u64(4), TombstoneKind::Wrapped),
            ],
        )
        .unwrap();

        assert_eq!(
            live.get(written).unwrap(),
            Some(ForkLiveState::Live(SequenceNumber::from_u64(3))),
        );
        assert_eq!(
            live.get(deleted).unwrap(),
            Some(ForkLiveState::Removed {
                version: SequenceNumber::from_u64(4),
                kind: TombstoneKind::Deleted,
            }),
        );
        assert!(live.get(wrapped).unwrap().unwrap().is_removed());
    }

    #[test]
    fn removal_supersedes_prior_live_pointer() {
        let (_dir, live) = open_temp();
        let id = ObjectID::random();
        live.set_live(id, SequenceNumber::from_u64(1)).unwrap();
        live.apply_checkpoint(
            std::iter::empty(),
            [(id, SequenceNumber::from_u64(2), TombstoneKind::Deleted)],
        )
        .unwrap();
        assert!(live.get(id).unwrap().unwrap().is_removed());
    }

    #[test]
    fn persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let id = ObjectID::random();
        {
            let live = LiveState::open(dir.path()).unwrap();
            live.set_live(id, SequenceNumber::from_u64(9)).unwrap();
        }
        let live = LiveState::open(dir.path()).unwrap();
        assert_eq!(
            live.get(id).unwrap(),
            Some(ForkLiveState::Live(SequenceNumber::from_u64(9))),
        );
    }
}
