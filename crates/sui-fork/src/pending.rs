// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! In-memory staging for the checkpoint currently being sealed.
//!
//! Simulacrum hands the store a checkpoint summary, then its transactions, effects, events, and
//! object diffs piecemeal through the `SimulatorStore` insert methods, and finally the checkpoint
//! contents. This buffer holds those pieces until the contents arrive, so the caller can persist
//! the whole checkpoint into the rpc-store as one atomic batch.
//!
//! Object diffs are the one staged entry reads consult. Execution resolves the next transaction's
//! inputs (and the settlement and barrier system transactions sealed with the checkpoint) through
//! the store, so the staged diffs double as a read overlay over the rpc-store until the seal
//! commits them. Overlay versions always outrank persisted ones, because local execution
//! Lamport-bumps past every live version while remote fetches are pinned at or below the fork
//! checkpoint, so a read takes the overlay's entry when one exists and falls through to the store
//! otherwise. Checkpoint, transaction, effects, and event entries stay write-only, since sealing
//! completes synchronously inside the checkpoint publication path, so by the time an execution
//! returns, its rows are already in the rpc-store.
//!
//! Nothing here is persisted. Staged entries that have not been sealed are lost on process
//! restart, which is the crash-safety contract, because a checkpoint either commits whole at seal
//! time or leaves no trace.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::RwLock;

use anyhow::Context as _;
use anyhow::bail;

use sui_rpc_store::schema::objects::Status;
use sui_rpc_store::schema::objects::TombstoneKind;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::effects::TransactionEvents;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;
use sui_types::transaction::VerifiedTransaction;

use crate::local_store::ObjectRemoval;

#[derive(Default)]
struct PendingTransaction {
    transaction: Option<VerifiedTransaction>,
    effects: Option<TransactionEffects>,
    events: Option<TransactionEvents>,
}

/// One fully staged transaction, cloned out of the buffer at seal time.
pub(crate) struct StagedTransaction {
    pub(crate) digest: TransactionDigest,
    pub(crate) transaction: VerifiedTransaction,
    pub(crate) effects: TransactionEffects,
    pub(crate) events: TransactionEvents,
}

/// One execution result's object writes and removals, staged until seal.
#[derive(Clone)]
pub(crate) struct ObjectDiff {
    pub(crate) written: BTreeMap<ObjectID, Object>,
    pub(crate) removed: Vec<ObjectRemoval>,
}

/// Staged object diffs plus the merged views the read overlay serves.
///
/// The views mirror the rows `LocalStore::stage_local_object_diff` will write at seal, and they
/// are maintained on record rather than derived on read so overlay lookups stay O(log n) on the
/// execution hot path.
#[derive(Default)]
struct PendingObjects {
    /// Diffs in arrival order, replayed verbatim into the seal batch.
    diffs: Vec<ObjectDiff>,

    /// What the checkpoint-pinned version row would say per object once sealed.
    current: BTreeMap<ObjectID, (SequenceNumber, Status)>,

    /// What the raw object rows would say per version once sealed.
    versions: BTreeMap<ObjectID, BTreeMap<SequenceNumber, Status>>,
}

/// Staging buffer for the in-flight checkpoint and its transactions.
///
/// `RwLock` provides the interior mutability needed behind the shared `Arc<ForkStoreInner>` that
/// every cloned `ForkStore` holds.
#[derive(Default)]
pub(crate) struct PendingCheckpointBuffer {
    checkpoint: RwLock<Option<VerifiedCheckpoint>>,
    transactions: RwLock<BTreeMap<TransactionDigest, PendingTransaction>>,
    objects: RwLock<PendingObjects>,
}

impl PendingCheckpointBuffer {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn record_checkpoint(&self, checkpoint: VerifiedCheckpoint) -> anyhow::Result<()> {
        let mut pending = self
            .checkpoint
            .write()
            .ok()
            .context("pending checkpoint lock poisoned")?;
        *pending = Some(checkpoint);
        Ok(())
    }

    pub(crate) fn record_transaction(
        &self,
        transaction: VerifiedTransaction,
    ) -> anyhow::Result<()> {
        let digest = *transaction.digest();
        let mut pending = self
            .transactions
            .write()
            .ok()
            .context("pending transaction lock poisoned")?;
        pending.entry(digest).or_default().transaction = Some(transaction);
        Ok(())
    }

    pub(crate) fn record_effects(&self, effects: TransactionEffects) -> anyhow::Result<()> {
        let digest = *effects.transaction_digest();
        let mut pending = self
            .transactions
            .write()
            .ok()
            .context("pending transaction lock poisoned")?;
        pending.entry(digest).or_default().effects = Some(effects);
        Ok(())
    }

    pub(crate) fn record_events(
        &self,
        tx_digest: TransactionDigest,
        events: TransactionEvents,
    ) -> anyhow::Result<()> {
        let mut pending = self
            .transactions
            .write()
            .ok()
            .context("pending transaction lock poisoned")?;
        pending.entry(tx_digest).or_default().events = Some(events);
        Ok(())
    }

    /// Stage one execution result's object writes and removals.
    ///
    /// The merged views mirror the row-level semantics of `LocalStore::stage_local_object_diff`.
    /// Removals apply before writes so an object removed and rewritten in the same result ends up
    /// live, and an object created and terminally deleted in the same result keeps only its
    /// historical version row.
    pub(crate) fn record_object_diff(
        &self,
        written: BTreeMap<ObjectID, Object>,
        removed: Vec<ObjectRemoval>,
    ) -> anyhow::Result<()> {
        let mut pending = self
            .objects
            .write()
            .ok()
            .context("pending objects lock poisoned")?;

        let terminal_deleted: BTreeSet<ObjectID> = removed
            .iter()
            .filter_map(|removal| {
                (removal.kind == TombstoneKind::Deleted).then_some(removal.object_id)
            })
            .collect();

        for removal in &removed {
            pending
                .versions
                .entry(removal.object_id)
                .or_default()
                .insert(removal.version, Status::Tombstone(removal.kind));
            pending.current.insert(
                removal.object_id,
                (removal.version, Status::Tombstone(removal.kind)),
            );
        }
        for object in written.values() {
            pending
                .versions
                .entry(object.id())
                .or_default()
                .insert(object.version(), Status::Live(object.clone()));
            if !terminal_deleted.contains(&object.id()) {
                pending.current.insert(
                    object.id(),
                    (object.version(), Status::Live(object.clone())),
                );
            }
        }

        pending.diffs.push(ObjectDiff { written, removed });
        Ok(())
    }

    /// Return the staged current state for an object, if the in-flight checkpoint touched it.
    pub(crate) fn latest_object(
        &self,
        id: ObjectID,
    ) -> anyhow::Result<Option<(SequenceNumber, Status)>> {
        let pending = self
            .objects
            .read()
            .ok()
            .context("pending objects lock poisoned")?;
        Ok(pending.current.get(&id).cloned())
    }

    /// Return the staged status at one exact version, if the in-flight checkpoint wrote it.
    pub(crate) fn object_at_version(
        &self,
        id: ObjectID,
        version: SequenceNumber,
    ) -> anyhow::Result<Option<Status>> {
        let pending = self
            .objects
            .read()
            .ok()
            .context("pending objects lock poisoned")?;
        Ok(pending
            .versions
            .get(&id)
            .and_then(|versions| versions.get(&version))
            .cloned())
    }

    /// Return the highest staged version at or below `upper_bound`, if any.
    ///
    /// Staged versions always outrank persisted ones for the same object, so callers take a hit
    /// here as the bounded read's result and fall through to the store only on a miss.
    pub(crate) fn object_at_or_before(
        &self,
        id: ObjectID,
        upper_bound: SequenceNumber,
    ) -> anyhow::Result<Option<(SequenceNumber, Status)>> {
        let pending = self
            .objects
            .read()
            .ok()
            .context("pending objects lock poisoned")?;
        Ok(pending.versions.get(&id).and_then(|versions| {
            versions
                .range(..=upper_bound)
                .next_back()
                .map(|(version, status)| (*version, status.clone()))
        }))
    }

    /// Clone every staged object diff, in arrival order, for the seal.
    pub(crate) fn staged_object_diffs(&self) -> anyhow::Result<Vec<ObjectDiff>> {
        let pending = self
            .objects
            .read()
            .ok()
            .context("pending objects lock poisoned")?;
        Ok(pending.diffs.clone())
    }

    /// Return the staged checkpoint matching `contents`, validating that the contents digest is
    /// the one the checkpoint committed to.
    pub(crate) fn checkpoint_for_contents(
        &self,
        contents: &CheckpointContents,
    ) -> anyhow::Result<VerifiedCheckpoint> {
        let pending = self
            .checkpoint
            .read()
            .ok()
            .context("pending checkpoint lock poisoned")?;
        let Some(checkpoint) = pending.as_ref() else {
            bail!(
                "checkpoint contents {} inserted without a pending checkpoint",
                contents.digest(),
            );
        };
        if checkpoint.data().content_digest != *contents.digest() {
            bail!(
                "pending checkpoint {} references contents {}, but inserted contents are {}",
                checkpoint.data().sequence_number,
                checkpoint.data().content_digest,
                contents.digest(),
            );
        }
        Ok(checkpoint.clone())
    }

    /// Clone every staged transaction referenced by `contents`, in checkpoint order.
    ///
    /// Entries stay staged until [`Self::clear_sealed`] confirms they were durably saved.
    pub(crate) fn staged_transactions_for(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: &CheckpointContents,
    ) -> anyhow::Result<Vec<StagedTransaction>> {
        let pending = self
            .transactions
            .read()
            .ok()
            .context("pending transaction lock poisoned")?;
        let mut staged = Vec::new();
        for execution in contents.iter() {
            let digest = execution.transaction;
            let Some(entry) = pending.get(&digest) else {
                bail!(
                    "checkpoint {} references transaction {digest}, but no pending transaction was recorded",
                    checkpoint.data().sequence_number,
                );
            };
            let transaction = entry.transaction.clone().with_context(|| {
                format!(
                    "checkpoint {} references transaction {digest}, but transaction data is missing",
                    checkpoint.data().sequence_number,
                )
            })?;
            let effects = entry.effects.clone().with_context(|| {
                format!(
                    "checkpoint {} references transaction {digest}, but transaction effects are missing",
                    checkpoint.data().sequence_number,
                )
            })?;
            let events = entry.events.clone().unwrap_or_default();
            staged.push(StagedTransaction {
                digest,
                transaction,
                effects,
                events,
            });
        }
        Ok(staged)
    }

    /// Drop every staged entry for a checkpoint whose rows were durably saved.
    pub(crate) fn clear_sealed(
        &self,
        checkpoint: &VerifiedCheckpoint,
        digests: impl IntoIterator<Item = TransactionDigest>,
    ) -> anyhow::Result<()> {
        // Sealing is serialized, so every staged diff belongs to the
        // checkpoint that just committed.
        let mut objects = self
            .objects
            .write()
            .ok()
            .context("pending objects lock poisoned")?;
        *objects = PendingObjects::default();
        drop(objects);

        let mut pending = self
            .transactions
            .write()
            .ok()
            .context("pending transaction lock poisoned")?;
        for digest in digests {
            pending.remove(&digest);
        }
        drop(pending);

        let mut pending_checkpoint = self
            .checkpoint
            .write()
            .ok()
            .context("pending checkpoint lock poisoned")?;
        if pending_checkpoint
            .as_ref()
            .is_some_and(|pending| pending.digest() == checkpoint.digest())
        {
            *pending_checkpoint = None;
        }
        Ok(())
    }
}
