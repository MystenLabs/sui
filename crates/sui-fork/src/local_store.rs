// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Fork-aware access to the embedded `sui-rpc-store`.
//!
//! Local execution persists nothing until a checkpoint seals. Execution stages transactions,
//! effects, events, and object diffs in memory, and [`LocalStore::seal_checkpoint`] commits every
//! canonical row (object versions, tombstones, checkpoint-pinned versions, checkpoint summary and
//! contents, and transaction data) in one atomic batch, so a crash leaves either the whole
//! checkpoint or none of it. Derived indexes (owner, type, package-version, balance, bitmaps) are
//! written by the embedded indexer alone from the sealed rows, and checkpoint publication blocks
//! until it catches up, so RPC reads issued after an execution returns see fully indexed state.
//!
//! The one-shot seed load is the exception. It runs at fork creation, before anything executes,
//! and writes the raw rows and the whole derived-index surface synchronously, because the indexer
//! only processes post-fork checkpoints and never sees the pre-fork state the seed establishes.
//! Remote-truth writes (pre-fork checkpoints and transactions fetched on demand) also commit
//! eagerly in their own batches, since they mirror data another chain already finalized.

use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::Arc;

use anyhow::Context as _;
use tracing::info;

use sui_consistent_store::Db;
use sui_consistent_store::FrameworkSchema;
use sui_consistent_store::PipelineTaskKey;
use sui_consistent_store::Restore as _;
use sui_consistent_store::RestoreState;
use sui_consistent_store::Watermark;
use sui_consistent_store::restore_state;
use sui_rpc_store::RpcStoreReader;
use sui_rpc_store::RpcStoreSchema;
use sui_rpc_store::indexer::balance::Balance;
use sui_rpc_store::indexer::object_by_owner::ObjectByOwner;
use sui_rpc_store::indexer::object_by_type::ObjectByType;
use sui_rpc_store::indexer::object_version_by_checkpoint::ObjectVersionByCheckpoint;
use sui_rpc_store::indexer::objects::Objects;
use sui_rpc_store::indexer::package_versions::PackageVersions;
use sui_rpc_store::schema::checkpoint_contents;
use sui_rpc_store::schema::checkpoint_seq_by_digest;
use sui_rpc_store::schema::checkpoint_summary;
use sui_rpc_store::schema::effects as schema_effects;
use sui_rpc_store::schema::events as schema_events;
use sui_rpc_store::schema::object_version_by_checkpoint;
use sui_rpc_store::schema::objects;
use sui_rpc_store::schema::objects::Status;
use sui_rpc_store::schema::objects::TombstoneKind;
use sui_rpc_store::schema::primitives::U64Be;
use sui_rpc_store::schema::primitives::U64Varint;
use sui_rpc_store::schema::transactions;
use sui_rpc_store::schema::tx_metadata_by_seq;
use sui_rpc_store::schema::tx_seq_by_digest;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::digests::CheckpointContentsDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::effects::TransactionEvents;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;
use sui_types::transaction::VerifiedTransaction;

use crate::pending::ObjectDiff;
use crate::pending::StagedTransaction;
use crate::services::FORK_INDEXER_PIPELINES;

/// Synthetic pipeline key recording that the one-shot seed load committed.
///
/// It lives in the framework's `__restore` column family, which matches the claim being recorded
/// ("this bulk-load finished at this checkpoint") and is the only framework CF nothing reads by
/// iteration. `restore_in_progress` walks the named pipeline cohorts and the indexer reads
/// per-pipeline keys, so a synthetic key here is invisible to both. The `__watermark` CF would not
/// do, because the pruner takes a minimum over every row it holds.
const SEED_RESTORE_PIPELINE: &str = "sui_fork_seed";

/// Fork-aware access to the embedded `sui-rpc-store`.
///
/// This type owns no remote-fetch policy. It writes and reads local `sui-rpc-store` rows in the
/// shapes needed by fork-specific reads and local execution.
#[derive(Clone)]
pub(crate) struct LocalStore {
    db: Db,
    schema: Arc<RpcStoreSchema>,
    reader: RpcStoreReader,
    /// The checkpoint this fork diverged at.
    ///
    /// Pre-fork objects are materialized lazily from a remote query pinned here, so each one is
    /// live as of this checkpoint. It is also the floor for the checkpoint numbering of locally
    /// executed checkpoints.
    forked_at_checkpoint: CheckpointSequenceNumber,
}

/// Local object removal that must be written as a `sui-rpc-store` tombstone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ObjectRemoval {
    pub(crate) object_id: ObjectID,
    pub(crate) version: SequenceNumber,
    pub(crate) kind: TombstoneKind,
}

impl LocalStore {
    /// Create a fork store handle over an already-open `sui-rpc-store` DB and schema, anchored at
    /// the checkpoint the fork diverged at.
    pub(crate) fn new(
        db: Db,
        schema: Arc<RpcStoreSchema>,
        forked_at_checkpoint: CheckpointSequenceNumber,
    ) -> Self {
        Self {
            // The stock default bounds the indexed tip by every pipeline, but
            // the fork runs only [`FORK_INDEXER_PIPELINES`] — the rest of the
            // column families it writes itself, in the same batch as their
            // data, so they have no watermark and never will. Left at the
            // default, the reported tip would be `None` forever.
            reader: RpcStoreReader::new(db.clone(), schema.clone())
                .with_pipelines(FORK_INDEXER_PIPELINES.iter().copied()),
            db,
            schema,
            forked_at_checkpoint,
        }
    }

    pub(crate) fn reader(&self) -> &RpcStoreReader {
        &self.reader
    }

    /// Return the current authoritative local state for an object.
    ///
    /// The state resolves from the checkpoint-pinned version index, bounded at the checkpoint the
    /// fork is currently producing. A row there is the fork's authority, because it was written
    /// either by local execution or by a pre-fork materialization that resolved the object as of
    /// the fork checkpoint, and absence means the fork has no knowledge of the object at all, so
    /// the caller should consult GraphQL. The raw `objects` rows cannot resolve an object's
    /// current state, because they are sparse from caching arbitrary historical versions on
    /// demand, so the greatest row present is not necessarily current, and a scan that finds
    /// nothing cannot separate "removed" from "never cached".
    pub(crate) fn get_latest_object_status(
        &self,
        id: ObjectID,
    ) -> anyhow::Result<Option<(SequenceNumber, Status)>> {
        let bound = self.executing_checkpoint()?;
        let Some(version) = self.schema.get_object_version_at_checkpoint(id, bound)? else {
            return Ok(None);
        };
        Ok(self.status_at(id, version)?.map(|status| (version, status)))
    }

    /// Return the local object status at one exact version.
    pub(crate) fn get_object_status_at_version(
        &self,
        id: ObjectID,
        version: SequenceNumber,
    ) -> anyhow::Result<Option<Status>> {
        self.status_at(id, version)
    }

    /// Return the highest local object status at or before `upper_bound`.
    ///
    /// Bounded historical reads use this, including child-object reads that must not cross the
    /// requested root version.
    pub(crate) fn get_object_at_or_before(
        &self,
        id: ObjectID,
        upper_bound: SequenceNumber,
    ) -> anyhow::Result<Option<(SequenceNumber, Status)>> {
        self.highest_status_at_or_before(id, upper_bound)
    }

    /// Save an object version without changing current live state or indexes.
    ///
    /// Use this for exact-version and bounded historical reads, and choose a live-object write
    /// method when the object should become current. No checkpoint-pinned version row is written,
    /// because the caller asked for one specific version, which is evidence about a point in
    /// history and none at all about what is live, and recording it would let an older version win
    /// a currency read. See [`Self::get_latest_object_status`].
    pub(crate) fn save_object_version_only(&self, object: &Object) -> anyhow::Result<()> {
        let mut batch = self.db.batch();
        self.stage_object_version(&mut batch, object)?;
        batch.commit().context("failed to save object version")
    }

    /// Write a checkpoint summary, contents, and digest-to-sequence index in its own batch.
    ///
    /// This is the eager path for checkpoints that are remote truth, meaning pre-fork rows fetched
    /// from the forked-from chain and the fork-point checkpoint persisted at startup. Locally
    /// sealed checkpoints go through [`Self::seal_checkpoint`] instead.
    pub(crate) fn save_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: &CheckpointContents,
    ) -> anyhow::Result<()> {
        let mut batch = self.db.batch();
        self.stage_checkpoint(&mut batch, checkpoint, contents)?;
        batch.commit().context("failed to persist checkpoint")
    }

    /// Stage a checkpoint summary, contents, and digest-to-sequence index.
    ///
    /// The supplied contents must match the content digest recorded in the checkpoint summary.
    fn stage_checkpoint(
        &self,
        batch: &mut sui_consistent_store::Batch,
        checkpoint: &VerifiedCheckpoint,
        contents: &CheckpointContents,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            checkpoint.data().content_digest == *contents.digest(),
            "checkpoint {} content digest does not match provided contents",
            checkpoint.data().sequence_number,
        );

        let sequence = checkpoint.data().sequence_number;
        batch.put(
            &self.schema.checkpoint_summary,
            &U64Be(sequence),
            &checkpoint_summary::store(checkpoint.data(), checkpoint.auth_sig()),
        )?;
        batch.put(
            &self.schema.checkpoint_contents,
            &U64Be(sequence),
            &checkpoint_contents::store(contents),
        )?;
        batch.put(
            &self.schema.checkpoint_seq_by_digest,
            &checkpoint_seq_by_digest::Key(*checkpoint.digest()),
            &U64Varint(sequence),
        )?;
        Ok(())
    }

    /// Write transaction, effects, events, and transaction metadata rows in their own batch.
    ///
    /// Like [`Self::save_checkpoint`], this is the eager path for pre-fork transactions fetched
    /// from the forked-from chain. Locally executed transactions are staged into the seal batch by
    /// [`Self::seal_checkpoint`].
    pub(crate) fn save_transaction(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: &CheckpointContents,
        transaction: &VerifiedTransaction,
        transaction_effects: &TransactionEffects,
        transaction_events: &TransactionEvents,
    ) -> anyhow::Result<()> {
        let mut batch = self.db.batch();
        self.stage_transaction(
            &mut batch,
            checkpoint,
            contents,
            transaction,
            transaction_effects,
            transaction_events,
        )?;
        batch.commit().context("failed to persist transaction")
    }

    /// Stage transaction, effects, events, and transaction metadata rows.
    ///
    /// The transaction must be present in `contents`, and the effects must correspond to the same
    /// transaction digest. The caller is responsible for persisting the containing checkpoint when
    /// readers require it.
    fn stage_transaction(
        &self,
        batch: &mut sui_consistent_store::Batch,
        checkpoint: &VerifiedCheckpoint,
        contents: &CheckpointContents,
        transaction: &VerifiedTransaction,
        transaction_effects: &TransactionEffects,
        transaction_events: &TransactionEvents,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            checkpoint.data().content_digest == *contents.digest(),
            "checkpoint {} content digest does not match provided contents",
            checkpoint.data().sequence_number,
        );

        let digest = *transaction.digest();
        anyhow::ensure!(
            transaction_effects.transaction_digest() == &digest,
            "effects transaction digest {} does not match transaction digest {digest}",
            transaction_effects.transaction_digest(),
        );

        let Some((tx_seq, position)) = contents
            .enumerate_transactions(checkpoint.data())
            .enumerate()
            .find_map(|(position, (tx_seq, execution))| {
                (execution.transaction == digest).then_some((tx_seq, position))
            })
        else {
            anyhow::bail!(
                "transaction {digest} is not present in checkpoint {} contents",
                checkpoint.data().sequence_number,
            );
        };
        let position = u32::try_from(position).context("checkpoint position does not fit u32")?;
        let event_count =
            u32::try_from(transaction_events.data.len()).context("event count does not fit u32")?;

        let signed = transaction.data();
        let metadata = tx_metadata_by_seq::Metadata {
            digest,
            checkpoint_seq: checkpoint.data().sequence_number,
            ckpt_position: position,
            event_count,
            timestamp_ms: checkpoint.data().timestamp_ms,
        };

        batch.put(
            &self.schema.tx_seq_by_digest,
            &tx_seq_by_digest::Key(digest),
            &U64Varint(tx_seq),
        )?;
        batch.put(
            &self.schema.transactions,
            &U64Be(tx_seq),
            &transactions::store(signed.transaction_data(), signed.tx_signatures()),
        )?;
        batch.put(
            &self.schema.effects,
            &U64Be(tx_seq),
            &schema_effects::store(transaction_effects, &[]),
        )?;
        batch.put(
            &self.schema.events,
            &U64Be(tx_seq),
            &schema_events::store(transaction_events),
        )?;
        batch.put(
            &self.schema.tx_metadata_by_seq,
            &U64Be(tx_seq),
            &tx_metadata_by_seq::store(&metadata),
        )?;
        Ok(())
    }

    /// Look up checkpoint contents by their content digest.
    ///
    /// `sui-rpc-store` indexes checkpoint summaries by checkpoint digest, so a content-digest
    /// lookup scans local checkpoint summaries.
    pub(crate) fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> anyhow::Result<Option<CheckpointContents>> {
        for row in self.schema.checkpoint_summary.iter(..)? {
            let (U64Be(sequence), _) = row?;
            let Some(checkpoint) = self.schema.get_checkpoint_summary(sequence)? else {
                continue;
            };
            if checkpoint.data().content_digest != *digest {
                continue;
            }
            return self
                .schema
                .get_checkpoint_contents(sequence)
                .map_err(Into::into);
        }
        Ok(None)
    }

    /// Return the highest checkpoint sequence persisted in the local store.
    pub(crate) fn highest_checkpoint_sequence(
        &self,
    ) -> anyhow::Result<Option<CheckpointSequenceNumber>> {
        let Some(row) = self.schema.checkpoint_summary.iter_rev(..)?.next() else {
            return Ok(None);
        };
        let (U64Be(sequence), _) = row?;
        Ok(Some(sequence))
    }

    /// Return the checkpoint that in-flight local execution is producing.
    ///
    /// This is the upper bound for currency reads over the checkpoint-pinned version index, and it
    /// covers every persisted row, since those only ever sit at or below the highest sealed
    /// checkpoint. It is always one past the highest persisted checkpoint, because sealing is
    /// serialized by the publication lock so only one checkpoint is ever in flight, and the fork
    /// produces nothing at or below the checkpoint it diverged at. The seal keys each staged diff
    /// at the sealed checkpoint's own sequence number, which equals this value at the moment of
    /// sealing.
    fn executing_checkpoint(&self) -> anyhow::Result<CheckpointSequenceNumber> {
        let highest = self
            .highest_checkpoint_sequence()?
            .unwrap_or(self.forked_at_checkpoint)
            .max(self.forked_at_checkpoint);
        Ok(highest + 1)
    }

    /// Save an object fetched from the remote chain as current local state if allowed.
    ///
    /// The object row is always written, and currency is claimed only when the local store has no
    /// newer live version and no tombstone. Owner, type, and balance indexes stay untouched.
    pub(crate) fn save_live_object_if_current(&self, object: &Object) -> anyhow::Result<()> {
        let update_live_pointer = match self.get_latest_object_status(object.id())? {
            None => true,
            Some((_, Status::Live(existing))) => existing.version() <= object.version(),
            Some((_, Status::Tombstone(_))) => false,
        };

        info!("Saving object to DB: {} v{}", object.id(), object.version());

        let mut batch = self.db.batch();
        self.stage_object_version(&mut batch, object)?;
        self.stage_package_version(&mut batch, object)?;
        if update_live_pointer {
            self.stage_restored_object_version(&mut batch, object)?;
        }
        batch.commit().context("failed to save live object")?;
        Ok(())
    }

    /// Report whether the one-shot seed load has already committed.
    ///
    /// The seed load must run exactly once over the lifetime of a fork directory, because
    /// `Balance` writes through a merge operator and replaying it would double-count every seeded
    /// coin. The marker is written inside the load's own batch, so it is set if and only if that
    /// batch landed.
    pub(crate) fn seed_load_complete(&self) -> anyhow::Result<bool> {
        let state = FrameworkSchema::new(self.db.clone())
            .restore
            .get(&PipelineTaskKey::new(SEED_RESTORE_PIPELINE))
            .context("failed to read seed restore state")?;
        Ok(state
            .is_some_and(|state| matches!(state.state, Some(restore_state::State::Complete(_)))))
    }

    /// Bulk-load the fork's seed objects and every index `sui-rpc-store` derives from them, in one
    /// atomic batch.
    ///
    /// This is the fork's whole pre-fork index surface. The enumerations that produced `objects`
    /// are checkpoint-pinned and cannot be re-run once the fork has diverged, so the load happens
    /// once, at fork creation, before anything has executed, and that ordering is what lets it
    /// write blind, with no existing live state to reconcile against, no stale index rows to
    /// retract, and no balance contribution to reverse. Committing the completion marker in the
    /// same batch makes the load unrepeatable, since a crash either leaves the fork with the whole
    /// seed set or with none of it, and a restart can tell which without inspecting the rows.
    pub(crate) fn restore_seed_objects(&self, objects: &[Object]) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.seed_load_complete()?,
            "seed objects have already been loaded into this fork directory",
        );

        let mut batch = self.db.batch();
        for object in objects {
            self.stage_restored_object(&mut batch, object)
                .with_context(|| format!("failed to stage seed object {}", object.id()))?;
        }

        let framework = FrameworkSchema::new(self.db.clone());
        let complete = RestoreState {
            state: Some(restore_state::State::Complete(restore_state::Complete {
                restored_at: self.forked_at_checkpoint,
            })),
        };
        batch
            .put(
                &framework.restore,
                &PipelineTaskKey::new(SEED_RESTORE_PIPELINE),
                &complete,
            )
            .context("failed to stage seed completion marker")?;

        // A restore is only half done without watermarks: the rows say what the
        // indexes hold, the watermarks say through which checkpoint, and every
        // reader asks the second question. The stock restore driver writes them
        // for the same reason, and the embedded indexer picks up from
        // `forked_at_checkpoint + 1`, so the two meet without a gap.
        let watermark = Watermark::for_checkpoint(self.forked_at_checkpoint);
        for pipeline in FORK_INDEXER_PIPELINES {
            batch
                .put(
                    &framework.watermarks,
                    &PipelineTaskKey::new(*pipeline),
                    &watermark,
                )
                .with_context(|| format!("failed to stage {pipeline} seed watermark"))?;
        }

        batch.commit().context("failed to load seed objects")?;
        Ok(())
    }

    /// Stage every row `sui-rpc-store` derives from one live object, covering the raw object
    /// version, the owner, type, balance and package-version indexes, and the checkpoint-pinned
    /// live-state row at the fork point.
    ///
    /// This is deliberately the same pipeline set a snapshot restore registers, so the
    /// derived-index surface comes from the stock `Restore` impls rather than from anything this
    /// crate maintains itself.
    fn stage_restored_object(
        &self,
        batch: &mut sui_consistent_store::Batch,
        object: &Object,
    ) -> anyhow::Result<()> {
        let schema = self.schema.as_ref();
        Objects.restore(schema, object, batch)?;
        ObjectByOwner.restore(schema, object, batch)?;
        ObjectByType.restore(schema, object, batch)?;
        Balance.restore(schema, object, batch)?;
        PackageVersions.restore(schema, object, batch)?;
        self.stage_restored_object_version(batch, object)
    }

    /// Stage local execution object writes and removals into the raw `objects` CF and the
    /// checkpoint-pinned version index, keyed at `checkpoint`, the sealed checkpoint that contains
    /// them.
    ///
    /// When the same result both removes and writes an object (e.g. wrapped then written again),
    /// the write wins and the object stays current, while an object created and terminally deleted
    /// in the same result is kept only as a historical row.
    /// `PendingCheckpointBuffer::record_object_diff` follows the same rules, so overlay reads
    /// match what these rows will say once committed.
    pub(crate) fn stage_local_object_diff(
        &self,
        batch: &mut sui_consistent_store::Batch,
        checkpoint: CheckpointSequenceNumber,
        written_objects: &BTreeMap<ObjectID, Object>,
        removed_objects: &[ObjectRemoval],
    ) -> anyhow::Result<()> {
        let terminal_deleted: std::collections::BTreeSet<_> = removed_objects
            .iter()
            .filter_map(|removed| {
                (removed.kind == TombstoneKind::Deleted).then_some(removed.object_id)
            })
            .collect();

        // Removals stage before writes so that an object removed and rewritten
        // in the same result ends up live: both write the same
        // checkpoint-pinned key, and the later put wins.
        for removed in removed_objects {
            batch.put(
                &self.schema.objects,
                &objects::Key {
                    id: removed.object_id,
                    version: removed.version,
                },
                &objects::tombstone(removed.kind),
            )?;
            self.stage_object_version_at_checkpoint(
                batch,
                removed.object_id,
                checkpoint,
                removed.version,
            )?;
        }

        for object in written_objects.values() {
            self.stage_object_version(batch, object)?;
            if !terminal_deleted.contains(&object.id()) {
                self.stage_object_version_at_checkpoint(
                    batch,
                    object.id(),
                    checkpoint,
                    object.version(),
                )?;
            }
        }

        Ok(())
    }

    /// Seal one locally produced checkpoint atomically, committing every staged object diff, the
    /// checkpoint summary and contents, and every staged transaction in a single batch.
    ///
    /// This is the fork's only durability point for locally executed state. Committing everything
    /// together makes a crash recoverable without inspection, because either the checkpoint exists
    /// with all of its rows, or nothing was written and a restart resumes from the previous tip
    /// with only in-memory loss.
    pub(crate) fn seal_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: &CheckpointContents,
        diffs: &[ObjectDiff],
        transactions: &[StagedTransaction],
    ) -> anyhow::Result<()> {
        let sequence = checkpoint.data().sequence_number;
        let mut batch = self.db.batch();
        for diff in diffs {
            self.stage_local_object_diff(&mut batch, sequence, &diff.written, &diff.removed)?;
        }
        self.stage_checkpoint(&mut batch, checkpoint, contents)?;
        for transaction in transactions {
            self.stage_transaction(
                &mut batch,
                checkpoint,
                contents,
                &transaction.transaction,
                &transaction.effects,
                &transaction.events,
            )?;
        }
        batch
            .commit()
            .with_context(|| format!("failed to seal checkpoint {sequence}"))
    }

    /// Read the raw schema status row for one object version.
    fn status_at(&self, id: ObjectID, version: SequenceNumber) -> anyhow::Result<Option<Status>> {
        self.schema
            .get_object_status_by_key(id, version)
            .map_err(Into::into)
    }

    /// Read the highest raw status row for an object at or below `upper_bound`.
    fn highest_status_at_or_before(
        &self,
        id: ObjectID,
        upper_bound: SequenceNumber,
    ) -> anyhow::Result<Option<(SequenceNumber, Status)>> {
        let lower = objects::Key {
            id,
            version: SequenceNumber::from_u64(0),
        };
        let upper = objects::Key {
            id,
            version: upper_bound,
        };

        let Some(row) = self
            .schema
            .objects
            .iter_rev((Bound::Included(lower), Bound::Included(upper)))?
            .next()
        else {
            return Ok(None);
        };
        let (key, _) = row?;
        let Some(status) = self.status_at(id, key.version)? else {
            return Ok(None);
        };
        Ok(Some((key.version, status)))
    }

    /// Stage the object-version row using `sui-rpc-store`'s restore helper.
    fn stage_object_version(
        &self,
        batch: &mut sui_consistent_store::Batch,
        object: &Object,
    ) -> anyhow::Result<()> {
        Objects.restore(self.schema.as_ref(), object, batch)
    }

    /// Stage the checkpoint-pinned row recording that `id` ended `checkpoint` at `version`, which
    /// may be a live version or a tombstone version.
    ///
    /// Only locally executed checkpoints may be keyed this way. The row asserts that nothing
    /// changed the object between `checkpoint` and the next row, which holds for the post-fork
    /// range because the fork executed all of it.
    fn stage_object_version_at_checkpoint(
        &self,
        batch: &mut sui_consistent_store::Batch,
        id: ObjectID,
        checkpoint: CheckpointSequenceNumber,
        version: SequenceNumber,
    ) -> anyhow::Result<()> {
        let (key, value) = object_version_by_checkpoint::store(id, checkpoint, version);
        batch.put(&self.schema.object_version_by_checkpoint, &key, &value)?;
        Ok(())
    }

    /// Stage the restore-floor row asserting that the object's version is its live version as of
    /// the fork checkpoint.
    ///
    /// This is the shape a live-set restore writes at its anchor, and it asserts the same thing
    /// here, that the object predates everything the fork executed. Callers must have resolved the
    /// object from a remote query pinned at the fork checkpoint, because an exact-version fetch of
    /// some older version proves nothing about what is live and must not be recorded.
    fn stage_restored_object_version(
        &self,
        batch: &mut sui_consistent_store::Batch,
        object: &Object,
    ) -> anyhow::Result<()> {
        ObjectVersionByCheckpoint::for_restore(self.forked_at_checkpoint).restore(
            self.schema.as_ref(),
            object,
            batch,
        )
    }

    fn stage_package_version(
        &self,
        batch: &mut sui_consistent_store::Batch,
        object: &Object,
    ) -> anyhow::Result<()> {
        PackageVersions.restore(self.schema.as_ref(), object, batch)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use move_core_types::language_storage::StructTag;
    use sui_consistent_store::DbOptions;
    use sui_types::base_types::SuiAddress;
    use sui_types::gas_coin::GAS;
    use sui_types::object::Data;
    use sui_types::object::MoveObject;
    use sui_types::object::ObjectInner;
    use sui_types::object::Owner;
    use sui_types::storage::RpcIndexes;

    use super::*;

    /// Non-zero so that rows written at the fork checkpoint are distinguishable from the `(id, 0)`
    /// synthetic floor the indexer writes.
    const TEST_FORK_CHECKPOINT: CheckpointSequenceNumber = 100;

    fn fresh_store() -> (tempfile::TempDir, LocalStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = reopen_store(&dir);
        (dir, store)
    }

    fn reopen_store(dir: &tempfile::TempDir) -> LocalStore {
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        LocalStore::new(db, Arc::new(schema), TEST_FORK_CHECKPOINT)
    }

    fn make_object(id: ObjectID, version: u64, owner: Owner) -> Object {
        let move_obj = MoveObject::new_gas_coin(SequenceNumber::from_u64(version), id, 1_000_000);
        ObjectInner {
            owner,
            data: Data::Move(move_obj),
            previous_transaction: sui_types::digests::TransactionDigest::genesis_marker(),
            storage_rebate: 0,
        }
        .into()
    }

    /// Stage-and-commit shorthand standing in for the production seal. It stages a single diff
    /// into a single batch keyed at the executing checkpoint, which is exactly the sealed
    /// checkpoint's sequence number at the moment a real seal runs.
    fn apply_diff(
        store: &LocalStore,
        written: &BTreeMap<ObjectID, Object>,
        removed: &[ObjectRemoval],
    ) {
        let checkpoint = store.executing_checkpoint().unwrap();
        let mut batch = store.db.batch();
        store
            .stage_local_object_diff(&mut batch, checkpoint, written, removed)
            .unwrap();
        batch.commit().unwrap();
    }

    #[test]
    fn saved_object_version_does_not_create_current_state() {
        let (_dir, store) = fresh_store();
        let id = ObjectID::random();
        let object = make_object(id, 7, Owner::Immutable);

        store.save_object_version_only(&object).unwrap();

        assert_eq!(
            store
                .get_object_status_at_version(id, SequenceNumber::from_u64(7))
                .unwrap(),
            Some(Status::Live(object.clone())),
        );
        assert_eq!(store.get_latest_object_status(id).unwrap(), None);
        assert_eq!(
            store
                .get_object_at_or_before(id, SequenceNumber::from_u64(8))
                .unwrap(),
            Some((SequenceNumber::from_u64(7), Status::Live(object))),
        );
    }

    /// A version-keyed fetch is evidence about one point in history and says nothing about what is
    /// live, so it must not disturb authority already established for the object. Recording such a
    /// fetch in the version index, whether at the fork checkpoint or at the checkpoint the version
    /// was created in, would make this read resolve to the older version.
    ///
    /// System packages are what make the scenario reachable, since an upgraded user package lives
    /// under a fresh object id while `0x2` and friends carry every version they have ever had
    /// under one id.
    #[test]
    fn exact_version_fetch_does_not_displace_established_currency() {
        let (_dir, store) = fresh_store();
        let id = ObjectID::random();
        let current = make_object(id, 9, Owner::Immutable);
        let historical = make_object(id, 5, Owner::Immutable);

        store.save_live_object_if_current(&current).unwrap();
        store.save_object_version_only(&historical).unwrap();

        assert_eq!(
            store.get_latest_object_status(id).unwrap(),
            Some((SequenceNumber::from_u64(9), Status::Live(current))),
            "an older version fetched by exact key must not become current",
        );
        assert_eq!(
            store
                .get_object_status_at_version(id, SequenceNumber::from_u64(5))
                .unwrap(),
            Some(Status::Live(historical)),
            "the historical row is still readable by version",
        );
    }

    /// Pre-fork materialization claims currency as of the fork checkpoint, so its row belongs at
    /// that checkpoint and nowhere else. A row above it would outrank locally executed
    /// checkpoints, and one below would lose to the indexer's synthetic floors.
    #[test]
    fn pre_fork_materialization_is_recorded_at_the_fork_checkpoint() {
        let (_dir, store) = fresh_store();
        let id = ObjectID::random();
        let object = make_object(id, 4, Owner::Immutable);

        store.save_live_object_if_current(&object).unwrap();

        let rows: Vec<_> = store
            .schema
            .iter_object_versions_by_checkpoint(id)
            .unwrap()
            .map(|row| row.unwrap().0.checkpoint)
            .collect();
        assert_eq!(rows, vec![TEST_FORK_CHECKPOINT]);
    }

    /// Locally executed changes are keyed by the checkpoint sealing them. Getting that wrong is
    /// silent, because the row still resolves, just at the wrong point in history.
    #[test]
    fn local_execution_is_recorded_at_the_executing_checkpoint() {
        let (_dir, store) = fresh_store();
        let id = ObjectID::random();
        let object = make_object(id, 1, Owner::Immutable);

        apply_diff(&store, &BTreeMap::from([(id, object)]), &[]);

        let rows: Vec<_> = store
            .schema
            .iter_object_versions_by_checkpoint(id)
            .unwrap()
            .map(|row| row.unwrap().0.checkpoint)
            .collect();
        assert_eq!(
            rows,
            vec![TEST_FORK_CHECKPOINT + 1],
            "the first locally executed checkpoint follows the fork checkpoint",
        );
    }

    #[test]
    fn live_object_save_survives_reopen() {
        let (dir, store) = fresh_store();
        let id = ObjectID::random();
        let object = make_object(id, 5, Owner::Immutable);

        store.save_live_object_if_current(&object).unwrap();
        drop(store);

        let store = reopen_store(&dir);
        assert_eq!(
            store.get_latest_object_status(id).unwrap(),
            Some((SequenceNumber::from_u64(5), Status::Live(object))),
        );
    }

    #[test]
    fn local_delete_writes_tombstone_and_blocks_base_resurrection() {
        let (_dir, store) = fresh_store();
        let id = ObjectID::random();
        let owner = SuiAddress::random_for_testing_only();
        let base = make_object(id, 1, Owner::AddressOwner(owner));

        apply_diff(&store, &BTreeMap::from([(id, base.clone())]), &[]);
        apply_diff(
            &store,
            &BTreeMap::new(),
            &[ObjectRemoval {
                object_id: id,
                version: SequenceNumber::from_u64(2),
                kind: TombstoneKind::Deleted,
            }],
        );
        store.save_live_object_if_current(&base).unwrap();

        assert_eq!(
            store.get_latest_object_status(id).unwrap(),
            Some((
                SequenceNumber::from_u64(2),
                Status::Tombstone(TombstoneKind::Deleted),
            )),
        );
        assert_eq!(
            store
                .get_object_status_at_version(id, SequenceNumber::from_u64(1))
                .unwrap(),
            Some(Status::Live(base)),
        );
    }

    /// Routing the seed load through the stock `Restore` impls means one call produces the whole
    /// derived-index surface together with the raw object row.
    #[test]
    fn seed_load_writes_every_derived_index() {
        let (_dir, store) = fresh_store();
        let id = ObjectID::random();
        let owner = SuiAddress::random_for_testing_only();
        let object = make_object(id, 4, Owner::AddressOwner(owner));

        store
            .restore_seed_objects(std::slice::from_ref(&object))
            .unwrap();

        assert_eq!(
            store.get_latest_object_status(id).unwrap(),
            Some((SequenceNumber::from_u64(4), Status::Live(object.clone()))),
        );

        let owned: Vec<_> = store
            .schema
            .iter_objects_owned_by_address(owner)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(owned.len(), 1);
        assert_eq!(owned[0].0.object_id, id);

        let object_type: StructTag = object.type_().unwrap().clone().into();
        assert_eq!(
            store
                .schema
                .iter_objects_of_type(&sui_rpc_store::schema::type_filter::TypeFilter::Type(
                    object_type,
                ))
                .unwrap()
                .count(),
            1,
        );

        let balance = RpcIndexes::get_balance(store.reader(), &owner, &GAS::type_())
            .unwrap()
            .expect("seed load should credit the coin balance");
        assert_eq!(balance.coin_balance, 1_000_000);
        assert_eq!(balance.address_balance, 0);

        // Pre-fork state is live *as of the fork checkpoint* and nowhere else:
        // a row above would outrank locally executed checkpoints, one below
        // would lose to the indexer's synthetic floors.
        let rows: Vec<_> = store
            .schema
            .iter_object_versions_by_checkpoint(id)
            .unwrap()
            .map(|row| row.unwrap().0.checkpoint)
            .collect();
        assert_eq!(rows, vec![TEST_FORK_CHECKPOINT]);
    }

    /// `Balance` accumulates through a merge operator, so a replayed seed load would silently
    /// double every seeded coin. The completion marker commits with the rows precisely so that a
    /// second attempt cannot get that far.
    #[test]
    fn seed_load_refuses_to_run_twice() {
        let (_dir, store) = fresh_store();
        let owner = SuiAddress::random_for_testing_only();
        let object = make_object(ObjectID::random(), 1, Owner::AddressOwner(owner));

        assert!(!store.seed_load_complete().unwrap());
        store
            .restore_seed_objects(std::slice::from_ref(&object))
            .unwrap();
        assert!(store.seed_load_complete().unwrap());

        let err = store
            .restore_seed_objects(&[object])
            .expect_err("a second seed load must be refused");
        assert!(format!("{err:#}").contains("already been loaded"));

        let balance = RpcIndexes::get_balance(store.reader(), &owner, &GAS::type_())
            .unwrap()
            .expect("balance should exist");
        assert_eq!(
            balance.coin_balance, 1_000_000,
            "balance was double-counted"
        );
    }

    /// The marker lives in the database rather than in a JSON sidecar, so that it commits
    /// atomically with the rows it describes and survives reopen.
    #[test]
    fn seed_load_marker_survives_reopen() {
        let (dir, store) = fresh_store();
        store.restore_seed_objects(&[]).unwrap();
        drop(store);

        assert!(reopen_store(&dir).seed_load_complete().unwrap());
    }

    /// The seed load is a restore, and a restore without watermarks leaves every reader unable to
    /// say through which checkpoint the indexes hold.
    ///
    /// `min_committed` reports `None` if any pipeline in the reader's set lacks a watermark, and
    /// the RPC layer turns that into "rpc index is empty" for every ledger read. So a fork must
    /// report its own checkpoint as the indexed tip from the moment the seed commits, before the
    /// embedded indexer has run at all.
    #[test]
    fn seed_load_reports_the_fork_checkpoint_as_the_indexed_tip() {
        let (_dir, store) = fresh_store();

        assert_eq!(
            RpcIndexes::get_highest_indexed_checkpoint_seq_number(store.reader()).unwrap(),
            None,
            "an unseeded fork has nothing indexed",
        );

        store.restore_seed_objects(&[]).unwrap();

        assert_eq!(
            RpcIndexes::get_highest_indexed_checkpoint_seq_number(store.reader()).unwrap(),
            Some(TEST_FORK_CHECKPOINT),
        );
        assert_eq!(
            RpcIndexes::get_highest_live_indexed_checkpoint_seq_number(store.reader()).unwrap(),
            Some(TEST_FORK_CHECKPOINT),
            "the live cohort bounds the health check and must be seeded too",
        );
    }

    #[test]
    fn deleted_write_in_same_diff_remains_removed_but_keeps_historical_row() {
        let (_dir, store) = fresh_store();
        let id = ObjectID::random();
        let object = make_object(id, 3, Owner::Immutable);

        apply_diff(
            &store,
            &BTreeMap::from([(id, object.clone())]),
            &[ObjectRemoval {
                object_id: id,
                version: SequenceNumber::from_u64(2),
                kind: TombstoneKind::Deleted,
            }],
        );

        assert_eq!(
            store.get_latest_object_status(id).unwrap(),
            Some((
                SequenceNumber::from_u64(2),
                Status::Tombstone(TombstoneKind::Deleted),
            )),
        );
        assert_eq!(
            store
                .get_object_status_at_version(id, SequenceNumber::from_u64(3))
                .unwrap(),
            Some(Status::Live(object)),
        );
    }

    #[test]
    fn wrapped_write_in_same_diff_becomes_live() {
        let (_dir, store) = fresh_store();
        let id = ObjectID::random();
        let object = make_object(id, 3, Owner::Immutable);

        apply_diff(
            &store,
            &BTreeMap::from([(id, object.clone())]),
            &[ObjectRemoval {
                object_id: id,
                version: SequenceNumber::from_u64(2),
                kind: TombstoneKind::Wrapped,
            }],
        );

        assert_eq!(
            store.get_latest_object_status(id).unwrap(),
            Some((SequenceNumber::from_u64(3), Status::Live(object))),
        );
    }

    #[test]
    fn local_diff_leaves_derived_indexes_to_the_indexer() {
        let (_dir, store) = fresh_store();
        let id = ObjectID::random();
        let owner = SuiAddress::random_for_testing_only();
        let recipient = SuiAddress::random_for_testing_only();
        let object = make_object(id, 1, Owner::AddressOwner(owner));
        let transferred = make_object(id, 2, Owner::AddressOwner(recipient));

        apply_diff(&store, &BTreeMap::from([(id, object)]), &[]);
        apply_diff(&store, &BTreeMap::from([(id, transferred.clone())]), &[]);

        // Canonical state is current immediately...
        assert_eq!(
            store.get_latest_object_status(id).unwrap(),
            Some((
                SequenceNumber::from_u64(2),
                Status::Live(transferred.clone()),
            )),
        );

        // ...but derived index rows belong to the embedded indexer: local
        // execution must not write owner or type rows synchronously.
        for address in [owner, recipient] {
            assert_eq!(
                store
                    .schema
                    .iter_objects_owned_by_address(address)
                    .unwrap()
                    .count(),
                0,
            );
        }
        let object_type: StructTag = transferred.type_().unwrap().clone().into();
        assert_eq!(
            store
                .schema
                .iter_objects_of_type(&sui_rpc_store::schema::type_filter::TypeFilter::Type(
                    object_type,
                ))
                .unwrap()
                .count(),
            0,
        );
    }

    /// The seed load writes blind, with no reconciliation against existing state, which is only
    /// safe because it runs before the fork executes anything. What keeps a seeded object from
    /// outliving its own history afterwards is the checkpoint ordering, since the seed's floor row
    /// sits at the fork checkpoint and the first locally executed checkpoint outranks it.
    #[test]
    fn local_execution_supersedes_a_seeded_object() {
        let (_dir, store) = fresh_store();
        let id = ObjectID::random();
        let owner = SuiAddress::random_for_testing_only();
        let recipient = SuiAddress::random_for_testing_only();
        let seeded = make_object(id, 1, Owner::AddressOwner(owner));
        let transferred = make_object(id, 2, Owner::AddressOwner(recipient));

        store.restore_seed_objects(&[seeded]).unwrap();
        apply_diff(&store, &BTreeMap::from([(id, transferred.clone())]), &[]);

        assert_eq!(
            store.get_latest_object_status(id).unwrap(),
            Some((SequenceNumber::from_u64(2), Status::Live(transferred))),
        );
    }
}
