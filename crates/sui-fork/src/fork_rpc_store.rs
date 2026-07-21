// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::Arc;

use anyhow::Context as _;
use move_core_types::language_storage::TypeTag;
use sui_consistent_store::Db;
use sui_consistent_store::Restore as _;
use sui_rpc_store::RpcStoreReader;
use sui_rpc_store::RpcStoreSchema;
use sui_rpc_store::indexer::balance::Balance;
use sui_rpc_store::indexer::object_by_owner::ObjectByOwner;
use sui_rpc_store::indexer::object_by_type::ObjectByType;
use sui_rpc_store::indexer::objects::Objects;
use sui_rpc_store::indexer::package_versions::PackageVersions;
use sui_rpc_store::schema::balance;
use sui_rpc_store::schema::checkpoint_contents;
use sui_rpc_store::schema::checkpoint_seq_by_digest;
use sui_rpc_store::schema::checkpoint_summary;
use sui_rpc_store::schema::effects as schema_effects;
use sui_rpc_store::schema::events as schema_events;
use sui_rpc_store::schema::object_by_owner;
use sui_rpc_store::schema::object_by_type;
use sui_rpc_store::schema::objects;
use sui_rpc_store::schema::objects::Status;
use sui_rpc_store::schema::objects::TombstoneKind;
use sui_rpc_store::schema::primitives::U64Be;
use sui_rpc_store::schema::primitives::U64Varint;
use sui_rpc_store::schema::transactions;
use sui_rpc_store::schema::tx_metadata_by_seq;
use sui_rpc_store::schema::tx_seq_by_digest;
use sui_types::SUI_ACCUMULATOR_ROOT_OBJECT_ID;
use sui_types::accumulator_root::AccumulatorKey;
use sui_types::accumulator_root::AccumulatorValue;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::coin::Coin;
use sui_types::digests::CheckpointContentsDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::effects::TransactionEvents;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;
use sui_types::object::Owner;
use sui_types::transaction::VerifiedTransaction;

use crate::live_state::ForkLiveState;
use crate::live_state::LiveState;

/// Fork-aware access to the embedded `sui-rpc-store`.
///
/// This type owns no remote-fetch policy. It writes and reads local
/// `sui-rpc-store` rows in the shapes needed by fork-specific reads and local
/// execution.
#[derive(Clone)]
pub(crate) struct ForkRpcStore {
    db: Db,
    schema: Arc<RpcStoreSchema>,
    reader: RpcStoreReader,
    /// Fork-owned `ObjectID -> current live version` pointer table.
    ///
    /// Stock `sui-rpc-store` has no column family keyed by `ObjectID` that answers
    /// "what is this object's current version, and is it live or removed?".
    /// `object_by_owner` / `object_by_type` do record the latest *live* version,
    /// but they are keyed by owner/type (not `ObjectID`) and only cover indexed
    /// owned objects, so they can't answer that for an arbitrary id. And the fork's
    /// `objects` CF is *sparse* — it caches arbitrary historical versions on demand
    /// — so a reverse scan there can't distinguish "removed" from "not cached".
    /// This table is the fork's authority for [`Self::get_latest_object_status`];
    /// see [`crate::live_state`].
    live_state: Arc<LiveState>,
}

/// Local object removal that must be written as a `sui-rpc-store` tombstone.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ObjectRemoval {
    pub(crate) object_id: ObjectID,
    pub(crate) version: SequenceNumber,
    pub(crate) kind: TombstoneKind,
}

impl ForkRpcStore {
    /// Creates a fork store handle over an already-open `sui-rpc-store` DB and
    /// schema, plus the fork-owned live-state pointer table.
    pub(crate) fn new(db: Db, schema: Arc<RpcStoreSchema>, live_state: Arc<LiveState>) -> Self {
        Self {
            reader: RpcStoreReader::new(db.clone(), schema.clone()),
            db,
            schema,
            live_state,
        }
    }

    /// Returns the cached reader for the local store.
    pub(crate) fn reader(&self) -> &RpcStoreReader {
        &self.reader
    }

    /// Returns the current authoritative local state for an object.
    ///
    /// The fork-owned live-state pointer is authoritative when present. Without a
    /// pointer we fall back to the raw `objects` rows: a tombstone is an
    /// authoritative removal, while a bare live row is only a cached historical
    /// version (the sparse-materialization case) and must not be reported as
    /// current — the caller should treat that as "unknown" and consult GraphQL.
    pub(crate) fn get_latest_object_status(
        &self,
        id: ObjectID,
    ) -> anyhow::Result<Option<(SequenceNumber, Status)>> {
        match self.live_state.get(id)? {
            Some(ForkLiveState::Live(version)) => {
                Ok(self.status_at(id, version)?.map(|status| (version, status)))
            }
            Some(ForkLiveState::Removed { version, kind }) => {
                Ok(Some((version, Status::Tombstone(kind))))
            }
            None => {
                match self.highest_status_at_or_before(id, SequenceNumber::from_u64(u64::MAX))? {
                    Some((version, status @ Status::Tombstone(_))) => Ok(Some((version, status))),
                    Some((_, Status::Live(_))) => match self
                        .highest_tombstone_at_or_before(id, SequenceNumber::from_u64(u64::MAX))?
                    {
                        Some((version, kind)) => Ok(Some((version, Status::Tombstone(kind)))),
                        None => Ok(None),
                    },
                    None => Ok(None),
                }
            }
        }
    }

    /// Returns the local object status at one exact version.
    pub(crate) fn get_object_at_version(
        &self,
        id: ObjectID,
        version: SequenceNumber,
    ) -> anyhow::Result<Option<Status>> {
        self.status_at(id, version)
    }

    /// Returns the highest local object status at or before `upper_bound`.
    ///
    /// This is used for bounded historical reads, including child-object reads
    /// that must not cross the requested root version.
    pub(crate) fn get_object_at_or_before(
        &self,
        id: ObjectID,
        upper_bound: SequenceNumber,
    ) -> anyhow::Result<Option<(SequenceNumber, Status)>> {
        self.highest_status_at_or_before(id, upper_bound)
    }

    /// Saves an object version without changing current live state or indexes.
    ///
    /// Use this for exact-version and bounded historical reads. The caller must
    /// choose a live-object write method when the object should become current.
    pub(crate) fn save_object_version_only(&self, object: &Object) -> anyhow::Result<()> {
        let mut batch = self.db.batch();
        self.stage_object_version(&mut batch, object)?;
        batch.commit().context("failed to save object version")
    }

    /// Writes a checkpoint summary, contents, and digest-to-sequence index.
    ///
    /// The supplied contents must match the content digest recorded in the
    /// checkpoint summary.
    pub(crate) fn save_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: &CheckpointContents,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            checkpoint.data().content_digest == *contents.digest(),
            "checkpoint {} content digest does not match provided contents",
            checkpoint.data().sequence_number,
        );

        let sequence = checkpoint.data().sequence_number;
        let mut batch = self.db.batch();
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
        batch.commit().context("failed to persist checkpoint")
    }

    /// Writes transaction, effects, events, and transaction metadata rows.
    ///
    /// The transaction must be present in `contents`, and the effects must
    /// correspond to the same transaction digest. The caller is responsible for
    /// persisting the containing checkpoint when that is required by readers.
    pub(crate) fn save_transaction(
        &self,
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

        let mut batch = self.db.batch();
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
        batch.commit().context("failed to persist transaction")
    }

    /// Looks up checkpoint contents by their content digest.
    ///
    /// `sui-rpc-store` indexes checkpoint summaries by checkpoint digest, so
    /// content-digest lookup scans local checkpoint summaries.
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

    /// Returns the highest checkpoint sequence persisted in the local store.
    pub(crate) fn highest_checkpoint_sequence(
        &self,
    ) -> anyhow::Result<Option<CheckpointSequenceNumber>> {
        let Some(row) = self.schema.checkpoint_summary.iter_rev(..)?.next() else {
            return Ok(None);
        };
        let (U64Be(sequence), _) = row?;
        Ok(Some(sequence))
    }

    /// Saves an object fetched from the remote chain as current local state if allowed.
    ///
    /// This records the object row and only updates the live-state pointer when
    /// the local store has no newer live version and no tombstone. It does not
    /// populate owner, type, or balance indexes.
    pub(crate) fn save_live_object_if_current(&self, object: &Object) -> anyhow::Result<()> {
        let update_live_pointer = match self.get_latest_object_status(object.id())? {
            None => true,
            Some((_, Status::Live(existing))) => existing.version() <= object.version(),
            Some((_, Status::Tombstone(_))) => false,
        };

        let mut batch = self.db.batch();
        self.stage_object_version(&mut batch, object)?;
        self.stage_package_version(&mut batch, object)?;
        batch.commit().context("failed to save live object")?;
        if update_live_pointer {
            self.live_state.set_live(object.id(), object.version())?;
        }
        Ok(())
    }

    /// Saves an address-owned seed object into current state and secondary indexes.
    ///
    /// Seed initialization is bounded by the seed manifest, so the address owner
    /// and balance indexes can be made visible without a full remote owner
    /// inventory scan.
    pub(crate) fn save_address_owned_seed_object(&self, object: &Object) -> anyhow::Result<()> {
        anyhow::ensure!(
            address_owner(object).is_some(),
            "seed object {} is not address-owned",
            object.id(),
        );

        self.save_indexed_live_object(object)
            .context("failed to save address-owned seed object")
    }

    /// Saves one object from a complete address-owner scan.
    ///
    /// The object must be owned by `owner`; the address-owner and balance index
    /// rows are written with the object.
    pub(crate) fn save_address_owner_inventory_object(
        &self,
        owner: SuiAddress,
        object: &Object,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            address_owner(object) == Some(owner),
            "object {} is not owned by address {owner}",
            object.id(),
        );

        self.save_indexed_live_object(object)
            .context("failed to save address-owner inventory object")
    }

    /// Saves one object from a complete object-owner scan.
    ///
    /// The object must be owned by `parent`; the corresponding object-owner
    /// index row is written with the object.
    pub(crate) fn save_object_owner_inventory_object(
        &self,
        parent: ObjectID,
        object: &Object,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            object.owner() == &Owner::ObjectOwner(parent.into()),
            "object {} is not owned by object {parent}",
            object.id(),
        );

        self.save_indexed_live_object(object)
            .context("failed to save object-owner inventory object")
    }

    /// Saves one object from a complete type scan.
    pub(crate) fn save_type_inventory_object(&self, object: &Object) -> anyhow::Result<()> {
        self.save_indexed_live_object(object)
            .context("failed to save type inventory object")
    }

    /// Saves a live object and its owner, type, package, and balance index rows.
    ///
    /// Existing newer live state and local tombstones are authoritative. A
    /// same-version object may add index rows for an object version that was
    /// previously stored without them.
    fn save_indexed_live_object(&self, object: &Object) -> anyhow::Result<()> {
        let status = self.get_latest_object_status(object.id())?;
        let mut batch = self.db.batch();
        self.stage_object_version(&mut batch, object)?;

        let mut make_current = false;
        match status {
            None => {
                make_current = true;
                self.stage_put_object_indexes(&mut batch, object, true)?;
            }
            Some((_, Status::Live(existing))) if existing.version() < object.version() => {
                let existing_was_indexed = self.has_object_by_owner_index(&existing)?;
                make_current = true;
                self.stage_delete_object_indexes(&mut batch, &existing)?;
                if existing_was_indexed {
                    self.stage_object_balance_delta(&mut batch, &existing, -1)?;
                }
                self.stage_put_object_indexes(&mut batch, object, true)?;
            }
            Some((_, Status::Live(existing))) if existing.version() == object.version() => {
                let existing_was_indexed = self.has_object_by_owner_index(&existing)?;
                self.stage_put_object_indexes(&mut batch, object, !existing_was_indexed)?;
            }
            Some((_, Status::Live(_))) | Some((_, Status::Tombstone(_))) => {}
        }

        batch
            .commit()
            .context("failed to save indexed live object")?;
        if make_current {
            self.live_state.set_live(object.id(), object.version())?;
        }
        Ok(())
    }

    /// Applies local execution object writes and removals to the raw `objects`
    /// CF and the fork-owned live-state pointer.
    ///
    /// Write-path contract: local execution synchronously writes only
    /// *canonical* data — object version rows and tombstones here (plus the
    /// live-state pointer), and checkpoint/transaction/effects/events rows at
    /// seal time — because the executor needs read-your-writes for its next
    /// inputs and the embedded indexer ingests each sealed checkpoint from
    /// these very rows. All *derived* indexes (owner, type, package-version,
    /// balance, bitmaps) are written by the indexer alone, and checkpoint
    /// publication blocks on `ForkRuntime::wait_for_indexed_checkpoint`, so
    /// RPC reads issued after an execution returns always see fully indexed
    /// state. Pre-fork materialization (seed and inventory saves) still
    /// writes indexes synchronously because the indexer only processes
    /// post-fork checkpoints.
    ///
    /// When the same result both removes and writes an object (e.g. wrapped
    /// then written again), the write wins and the object stays current; an
    /// object created and terminally deleted in the same result is kept only
    /// as a historical row.
    pub(crate) fn apply_local_object_diff(
        &self,
        written_objects: &BTreeMap<ObjectID, Object>,
        removed_objects: &[ObjectRemoval],
    ) -> anyhow::Result<()> {
        let terminal_deleted: std::collections::BTreeSet<_> = removed_objects
            .iter()
            .filter_map(|removed| {
                (removed.kind == TombstoneKind::Deleted).then_some(removed.object_id)
            })
            .collect();

        let mut batch = self.db.batch();

        for removed in removed_objects {
            batch.put(
                &self.schema.objects,
                &objects::Key {
                    id: removed.object_id,
                    version: removed.version,
                },
                &objects::tombstone(removed.kind),
            )?;
        }

        for object in written_objects.values() {
            self.stage_object_version(&mut batch, object)?;
        }

        batch
            .commit()
            .context("failed to apply local object diff")?;

        // Update the fork-owned live pointers after the rpc-store rows commit:
        // surviving written objects become current, removals become tombstoned.
        // Objects created and terminally deleted in the same result are kept as
        // historical rows but never made current.
        let written_live = written_objects
            .values()
            .filter(|object| !terminal_deleted.contains(&object.id()))
            .map(|object| (object.id(), object.version()));
        let removed_live = removed_objects
            .iter()
            .map(|removed| (removed.object_id, removed.version, removed.kind));
        self.live_state
            .apply_checkpoint(written_live, removed_live)?;
        Ok(())
    }

    /// Reads the raw schema status row for one object version.
    fn status_at(&self, id: ObjectID, version: SequenceNumber) -> anyhow::Result<Option<Status>> {
        self.schema
            .get_object_status_by_key(id, version)
            .map_err(Into::into)
    }

    /// Reads the highest raw status row for an object at or below `upper_bound`.
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

    /// Reads the highest tombstone for an object at or below `upper_bound`.
    ///
    /// This keeps local removals authoritative when historical object rows
    /// exist but no live pointer exists.
    fn highest_tombstone_at_or_before(
        &self,
        id: ObjectID,
        upper_bound: SequenceNumber,
    ) -> anyhow::Result<Option<(SequenceNumber, TombstoneKind)>> {
        let lower = objects::Key {
            id,
            version: SequenceNumber::from_u64(0),
        };
        let upper = objects::Key {
            id,
            version: upper_bound,
        };

        for row in self
            .schema
            .objects
            .iter_rev((Bound::Included(lower), Bound::Included(upper)))?
        {
            let (key, _) = row?;
            if let Some(Status::Tombstone(kind)) = self.status_at(id, key.version)? {
                return Ok(Some((key.version, kind)));
            }
        }
        Ok(None)
    }

    /// Stages the object-version row using `sui-rpc-store`'s restore helper.
    fn stage_object_version(
        &self,
        batch: &mut sui_consistent_store::Batch,
        object: &Object,
    ) -> anyhow::Result<()> {
        Objects.restore(self.schema.as_ref(), object, batch)
    }

    /// Returns whether the object currently has an owner-index row.
    ///
    /// The local writer uses this as the signal that the object has already
    /// contributed indexed live-object state.
    fn has_object_by_owner_index(&self, object: &Object) -> anyhow::Result<bool> {
        let Some((key, _)) = object_by_owner::store(object) else {
            return Ok(false);
        };
        Ok(self.schema.object_by_owner.get(&key)?.is_some())
    }

    /// Stages removal of owner and type index rows for the object.
    fn stage_delete_object_indexes(
        &self,
        batch: &mut sui_consistent_store::Batch,
        object: &Object,
    ) -> anyhow::Result<()> {
        if let Some((key, _)) = object_by_owner::store(object) {
            batch.delete(&self.schema.object_by_owner, &key)?;
        }
        if let Some((key, _)) = object_by_type::store(object) {
            batch.delete(&self.schema.object_by_type, &key)?;
        }
        Ok(())
    }

    /// Stages owner, type, package-version, and optionally balance rows.
    fn stage_put_object_indexes(
        &self,
        batch: &mut sui_consistent_store::Batch,
        object: &Object,
        include_balance: bool,
    ) -> anyhow::Result<()> {
        ObjectByOwner.restore(self.schema.as_ref(), object, batch)?;
        ObjectByType.restore(self.schema.as_ref(), object, batch)?;
        self.stage_package_version(batch, object)?;
        if include_balance {
            Balance.restore(self.schema.as_ref(), object, batch)?;
        }
        Ok(())
    }

    /// Stages the package-version lookup row for a Move package.
    fn stage_package_version(
        &self,
        batch: &mut sui_consistent_store::Batch,
        object: &Object,
    ) -> anyhow::Result<()> {
        PackageVersions.restore(self.schema.as_ref(), object, batch)
    }

    /// Stages the balance delta for an object that contributes to the balance index.
    ///
    /// `sign` is positive when adding an indexed live object and negative when removing one from
    /// current indexed state.
    fn stage_object_balance_delta(
        &self,
        batch: &mut sui_consistent_store::Batch,
        object: &Object,
        sign: i128,
    ) -> anyhow::Result<()> {
        let Some((owner, coin_type, coin, address)) = balance_delta(object, sign)? else {
            return Ok(());
        };
        let (key, value) = balance::delta(owner, coin_type, coin, address);
        batch.merge(&self.schema.balance, &key, &value)?;
        Ok(())
    }
}

/// Returns the address owner used by owner and balance indexes.
fn address_owner(object: &Object) -> Option<SuiAddress> {
    match object.owner() {
        Owner::AddressOwner(owner) | Owner::ConsensusAddressOwner { owner, .. } => Some(*owner),
        _ => None,
    }
}

/// Returns the balance merge operand that reverses or reapplies one indexed object.
///
/// This mirrors the object cases handled by `Balance::restore` so replacing an indexed live object
/// removes the same balance contribution that the restore helper added.
fn balance_delta(
    object: &Object,
    sign: i128,
) -> anyhow::Result<Option<(SuiAddress, TypeTag, i128, i128)>> {
    match object.owner() {
        Owner::AddressOwner(owner) | Owner::ConsensusAddressOwner { owner, .. } => {
            let Some((coin_type, value)) = coin_balance(object)? else {
                return Ok(None);
            };
            Ok(Some((*owner, coin_type, sign * i128::from(value), 0)))
        }
        Owner::ObjectOwner(parent) if *parent == SUI_ACCUMULATOR_ROOT_OBJECT_ID.into() => {
            let Some((owner, coin_type, value)) = address_balance(object) else {
                return Ok(None);
            };
            Ok(Some((owner, coin_type, 0, sign * value)))
        }
        _ => Ok(None),
    }
}

/// Extracts the coin type and coin value from an indexed coin object.
fn coin_balance(object: &Object) -> anyhow::Result<Option<(TypeTag, u64)>> {
    Ok(Coin::extract_balance_if_coin(object)
        .with_context(|| format!("failed to read coin balance from object {}", object.id()))?
        .and_then(|(coin_type, value)| match coin_type {
            TypeTag::Struct(struct_tag) => Some((TypeTag::Struct(struct_tag), value)),
            _ => None,
        }))
}

/// Extracts address-balance data from an accumulator-root dynamic-field object.
fn address_balance(object: &Object) -> Option<(SuiAddress, TypeTag, i128)> {
    let move_object = object.data.try_as_move()?;
    let TypeTag::Struct(coin_type) = move_object.type_().balance_accumulator_field_type_maybe()?
    else {
        return None;
    };
    let (key, value): (AccumulatorKey, AccumulatorValue) = move_object.try_into().ok()?;
    let value = value.as_u128()? as i128;
    (value > 0).then_some((key.owner, TypeTag::Struct(coin_type), value))
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

    fn fresh_store() -> (tempfile::TempDir, ForkRpcStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = reopen_store(&dir);
        (dir, store)
    }

    fn reopen_store(dir: &tempfile::TempDir) -> ForkRpcStore {
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        let live_state = Arc::new(LiveState::open(dir.path()).unwrap());
        ForkRpcStore::new(db, Arc::new(schema), live_state)
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

    #[test]
    fn saved_object_version_does_not_create_current_state() {
        let (_dir, store) = fresh_store();
        let id = ObjectID::random();
        let object = make_object(id, 7, Owner::Immutable);

        store.save_object_version_only(&object).unwrap();

        assert_eq!(
            store
                .get_object_at_version(id, SequenceNumber::from_u64(7))
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

        store
            .apply_local_object_diff(&BTreeMap::from([(id, base.clone())]), &[])
            .unwrap();
        store
            .apply_local_object_diff(
                &BTreeMap::new(),
                &[ObjectRemoval {
                    object_id: id,
                    version: SequenceNumber::from_u64(2),
                    kind: TombstoneKind::Deleted,
                }],
            )
            .unwrap();
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
                .get_object_at_version(id, SequenceNumber::from_u64(1))
                .unwrap(),
            Some(Status::Live(base)),
        );
    }

    #[test]
    fn seed_save_indexes_existing_raw_live_object() {
        let (_dir, store) = fresh_store();
        let id = ObjectID::random();
        let owner = SuiAddress::random_for_testing_only();
        let object = make_object(id, 1, Owner::AddressOwner(owner));

        store.save_live_object_if_current(&object).unwrap();
        assert_eq!(
            store
                .schema
                .iter_objects_owned_by_address(owner)
                .unwrap()
                .count(),
            0,
        );

        store.save_address_owned_seed_object(&object).unwrap();
        let rows = store
            .schema
            .iter_objects_owned_by_address(owner)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0.object_id, id);
    }

    #[test]
    fn seed_save_credits_existing_raw_live_coin_once() {
        let (_dir, store) = fresh_store();
        let id = ObjectID::random();
        let owner = SuiAddress::random_for_testing_only();
        let object = make_object(id, 1, Owner::AddressOwner(owner));
        let coin_type = GAS::type_();

        store.save_live_object_if_current(&object).unwrap();
        assert!(
            RpcIndexes::get_balance(store.reader(), &owner, &coin_type)
                .unwrap()
                .is_none(),
            "raw live object save should not credit balances before seeding",
        );

        store.save_address_owned_seed_object(&object).unwrap();
        store.save_address_owned_seed_object(&object).unwrap();

        let balance = RpcIndexes::get_balance(store.reader(), &owner, &coin_type)
            .unwrap()
            .expect("seed initialization should credit coin balance");
        assert_eq!(balance.coin_balance, 1_000_000);
        assert_eq!(balance.address_balance, 0);
    }

    #[test]
    fn deleted_write_in_same_diff_remains_removed_but_keeps_historical_row() {
        let (_dir, store) = fresh_store();
        let id = ObjectID::random();
        let object = make_object(id, 3, Owner::Immutable);

        store
            .apply_local_object_diff(
                &BTreeMap::from([(id, object.clone())]),
                &[ObjectRemoval {
                    object_id: id,
                    version: SequenceNumber::from_u64(2),
                    kind: TombstoneKind::Deleted,
                }],
            )
            .unwrap();

        assert_eq!(
            store.get_latest_object_status(id).unwrap(),
            Some((
                SequenceNumber::from_u64(2),
                Status::Tombstone(TombstoneKind::Deleted),
            )),
        );
        assert_eq!(
            store
                .get_object_at_version(id, SequenceNumber::from_u64(3))
                .unwrap(),
            Some(Status::Live(object)),
        );
    }

    #[test]
    fn wrapped_write_in_same_diff_becomes_live() {
        let (_dir, store) = fresh_store();
        let id = ObjectID::random();
        let object = make_object(id, 3, Owner::Immutable);

        store
            .apply_local_object_diff(
                &BTreeMap::from([(id, object.clone())]),
                &[ObjectRemoval {
                    object_id: id,
                    version: SequenceNumber::from_u64(2),
                    kind: TombstoneKind::Wrapped,
                }],
            )
            .unwrap();

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

        store
            .apply_local_object_diff(&BTreeMap::from([(id, object)]), &[])
            .unwrap();
        store
            .apply_local_object_diff(&BTreeMap::from([(id, transferred.clone())]), &[])
            .unwrap();

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

    #[test]
    fn seed_save_does_not_resurrect_transferred_object() {
        let (_dir, store) = fresh_store();
        let id = ObjectID::random();
        let owner = SuiAddress::random_for_testing_only();
        let recipient = SuiAddress::random_for_testing_only();
        let base = make_object(id, 1, Owner::AddressOwner(owner));
        let transferred = make_object(id, 2, Owner::AddressOwner(recipient));

        store
            .apply_local_object_diff(&BTreeMap::from([(id, transferred.clone())]), &[])
            .unwrap();
        store.save_address_owned_seed_object(&base).unwrap();

        // The stale pre-fork version must not be re-indexed for its old owner
        // or become current again.
        assert_eq!(
            store
                .schema
                .iter_objects_owned_by_address(owner)
                .unwrap()
                .count(),
            0,
        );
        assert_eq!(
            store.get_latest_object_status(id).unwrap(),
            Some((SequenceNumber::from_u64(2), Status::Live(transferred))),
        );
    }
}
