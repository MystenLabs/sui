// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_store_tables::LiveObject;
use crate::authority::AuthorityStore;
use crate::checkpoints::CheckpointStore;
use crate::state_accumulator::AccumulatorStore;
use move_core_types::language_storage::TypeTag;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;
use sui_rest_api::CheckpointData;
use sui_types::base_types::MoveObjectType;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::digests::TransactionDigest;
use sui_types::dynamic_field::{DynamicFieldInfo, DynamicFieldType};
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::object::Object;
use sui_types::object::Owner;
use sui_types::storage::error::Error as StorageError;
use sui_types::type_resolver::LayoutResolver;
use tracing::{debug, info};
use typed_store::rocks::{DBMap, MetricConf};
use typed_store::traits::Map;
use typed_store::traits::{TableSummary, TypedStoreDebug};
use typed_store::TypedStoreError;
use typed_store_derive::DBMapUtils;

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct OwnerIndexKey {
    pub owner: SuiAddress,
    pub object_id: ObjectID,
}

impl OwnerIndexKey {
    fn new(owner: SuiAddress, object_id: ObjectID) -> Self {
        Self { owner, object_id }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OwnerIndexInfo {
    // object_id of the object is a part of the Key
    pub version: SequenceNumber,
    pub type_: MoveObjectType,
}

impl OwnerIndexInfo {
    pub fn new(object: &Object) -> Self {
        Self {
            version: object.version(),
            type_: object.type_().expect("packages cannot be owned").to_owned(),
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct DynamicFieldKey {
    pub parent: ObjectID,
    pub field_id: ObjectID,
}

impl DynamicFieldKey {
    fn new<P: Into<ObjectID>>(parent: P, field_id: ObjectID) -> Self {
        Self {
            parent: parent.into(),
            field_id,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct DynamicFieldIndexInfo {
    // field_id of this dynamic field is a part of the Key
    pub dynamic_field_type: DynamicFieldType,
    pub name_type: TypeTag,
    pub name_value: Vec<u8>,
    // TODO do we want to also store the type of the value? We can get this for free for
    // DynamicFields, but for DynamicObjects it would require a lookup in the DB on init, or
    // scanning the transaction's output objects for the coorisponding Object to retreive its type
    // information.
    //
    // pub value_type: TypeTag,
    /// ObjectId of the child object when `dynamic_field_type == DynamicFieldType::DynamicObject`
    pub dynamic_object_id: Option<ObjectID>,
}

#[derive(Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct TransactionInfo {
    checkpoint: u64,
}

/// RocksDB tables for the RestIndexStore
///
/// NOTE: Authors and Reviewers before adding any new tables ensure that they are either:
/// - bounded in size by the live object set
/// - are prune-able and have corresponding logic in the `prune` function
#[derive(DBMapUtils)]
struct IndexStoreTables {
    /// An index of extra metadata for Transactions.
    ///
    /// Only contains entries for transactions which have yet to be pruned from the main database.
    transactions: DBMap<TransactionDigest, TransactionInfo>,

    /// An index of object ownership.
    ///
    /// Allows an efficient iterator to list all objects currently owned by a specific user
    /// account.
    owner: DBMap<OwnerIndexKey, OwnerIndexInfo>,

    /// An index of dynamic fields (children objects).
    ///
    /// Allows an efficient iterator to list all of the dynamic fields owned by a particular
    /// ObjectID.
    dynamic_field: DBMap<DynamicFieldKey, DynamicFieldIndexInfo>,
    // NOTE: Authors and Reviewers before adding any new tables ensure that they are either:
    // - bounded in size by the live object set
    // - are prune-able and have corresponding logic in the `prune` function
}

impl IndexStoreTables {
    fn is_empty(&self) -> bool {
        self.transactions.is_empty()
    }

    fn init(
        &mut self,
        authority_store: &AuthorityStore,
        checkpoint_store: &CheckpointStore,
        resolver: &mut dyn LayoutResolver,
    ) -> Result<(), StorageError> {
        info!("Initializing REST indexes");

        // Iterate through available, executed checkpoints that have yet to be pruned
        // to initialize checkpoint and transaction based indexes.
        if let Some(highest_executed_checkpint) =
            checkpoint_store.get_highest_executed_checkpoint_seq_number()?
        {
            let lowest_available_checkpoint =
                checkpoint_store.get_highest_pruned_checkpoint_seq_number()?;

            let mut batch = self.transactions.batch();

            for seq in lowest_available_checkpoint..=highest_executed_checkpint {
                let checkpoint = checkpoint_store
                    .get_checkpoint_by_sequence_number(seq)?
                    .ok_or_else(|| StorageError::missing(format!("missing checkpoint {seq}")))?;
                let contents = checkpoint_store
                    .get_checkpoint_contents(&checkpoint.content_digest)?
                    .ok_or_else(|| StorageError::missing(format!("missing checkpoint {seq}")))?;

                let info = TransactionInfo {
                    checkpoint: checkpoint.sequence_number,
                };

                batch.insert_batch(
                    &self.transactions,
                    contents.iter().map(|digests| (digests.transaction, info)),
                )?;
            }

            batch.write()?;
        }

        // Iterate through live object set to initialize object-based indexes
        for object in authority_store
            .iter_live_object_set(false)
            .filter_map(LiveObject::to_normal)
        {
            let mut batch = self.owner.batch();

            match object.owner {
                // Owner Index
                Owner::AddressOwner(owner) => {
                    let owner_key = OwnerIndexKey::new(owner, object.id());
                    let owner_info = OwnerIndexInfo::new(&object);
                    batch.insert_batch(&self.owner, [(owner_key, owner_info)])?;
                }

                // Dynamic Field Index
                Owner::ObjectOwner(parent) => {
                    if let Some(field_info) = try_create_dynamic_field_info(&object, resolver)? {
                        let field_key = DynamicFieldKey::new(parent, object.id());

                        batch.insert_batch(&self.dynamic_field, [(field_key, field_info)])?;
                    }
                }

                Owner::Shared { .. } | Owner::Immutable => continue,
            }

            batch.write()?;
        }

        info!("Finished initializing REST indexes");

        Ok(())
    }

    /// Prune data from this Index
    fn prune(
        &self,
        checkpoint_contents_to_prune: &[CheckpointContents],
    ) -> Result<(), TypedStoreError> {
        let mut batch = self.transactions.batch();

        let transactions_to_prune = checkpoint_contents_to_prune
            .iter()
            .flat_map(|contents| contents.iter().map(|digests| digests.transaction));

        batch.delete_batch(&self.transactions, transactions_to_prune)?;

        batch.write()
    }

    /// Index a Checkpoint
    fn index_checkpoint(
        &self,
        checkpoint: &CheckpointData,
        resolver: &mut dyn LayoutResolver,
    ) -> Result<(), StorageError> {
        debug!(
            checkpoint = checkpoint.checkpoint_summary.sequence_number,
            "indexing checkpoint"
        );

        let mut batch = self.transactions.batch();

        // transactions index
        {
            let info = TransactionInfo {
                checkpoint: checkpoint.checkpoint_summary.sequence_number,
            };

            batch.insert_batch(
                &self.transactions,
                checkpoint
                    .checkpoint_contents
                    .iter()
                    .map(|digests| (digests.transaction, info)),
            )?;
        }

        // owner index
        {
            for tx in &checkpoint.transactions {
                // determine changes from removed objects
                for removed_object in tx.removed_objects() {
                    match removed_object.owner() {
                        Owner::AddressOwner(address) => {
                            let owner_key = OwnerIndexKey::new(*address, removed_object.id());
                            batch.delete_batch(&self.owner, [owner_key])?;
                        }
                        Owner::ObjectOwner(object_id) => {
                            batch.delete_batch(
                                &self.dynamic_field,
                                [DynamicFieldKey::new(*object_id, removed_object.id())],
                            )?;
                        }
                        Owner::Shared { .. } | Owner::Immutable => {}
                    }
                }

                // determine changes from changed objects
                for (object, old_object) in tx.changed_objects() {
                    if let Some(old_object) = old_object {
                        if old_object.owner() != object.owner() {
                            match old_object.owner() {
                                Owner::AddressOwner(address) => {
                                    let owner_key = OwnerIndexKey::new(*address, old_object.id());
                                    batch.delete_batch(&self.owner, [owner_key])?;
                                }

                                Owner::ObjectOwner(object_id) => {
                                    batch.delete_batch(
                                        &self.dynamic_field,
                                        [DynamicFieldKey::new(*object_id, old_object.id())],
                                    )?;
                                }

                                Owner::Shared { .. } | Owner::Immutable => {}
                            }
                        }
                    }

                    match object.owner() {
                        Owner::AddressOwner(owner) => {
                            let owner_key = OwnerIndexKey::new(*owner, object.id());
                            let owner_info = OwnerIndexInfo::new(object);
                            batch.insert_batch(&self.owner, [(owner_key, owner_info)])?;
                        }
                        Owner::ObjectOwner(parent) => {
                            if let Some(field_info) =
                                try_create_dynamic_field_info(object, resolver)?
                            {
                                let field_key = DynamicFieldKey::new(*parent, object.id());

                                batch
                                    .insert_batch(&self.dynamic_field, [(field_key, field_info)])?;
                            }
                        }
                        Owner::Shared { .. } | Owner::Immutable => {}
                    }
                }
            }
        }

        batch.write()?;

        debug!(
            checkpoint = checkpoint.checkpoint_summary.sequence_number,
            "finished indexing checkpoint"
        );
        Ok(())
    }

    fn get_transaction_info(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<TransactionInfo>, TypedStoreError> {
        self.transactions.get(digest)
    }

    fn owner_iter(
        &self,
        owner: SuiAddress,
        cursor: Option<ObjectID>,
    ) -> Result<impl Iterator<Item = (OwnerIndexKey, OwnerIndexInfo)> + '_, TypedStoreError> {
        let lower_bound = OwnerIndexKey::new(owner, ObjectID::ZERO);
        let upper_bound = OwnerIndexKey::new(owner, ObjectID::MAX);
        let mut iter = self
            .owner
            .iter_with_bounds(Some(lower_bound), Some(upper_bound));

        if let Some(cursor) = cursor {
            iter = iter.skip_to(&OwnerIndexKey::new(owner, cursor))?;
        }

        Ok(iter)
    }

    fn dynamic_field_iter(
        &self,
        parent: ObjectID,
        cursor: Option<ObjectID>,
    ) -> Result<impl Iterator<Item = (DynamicFieldKey, DynamicFieldIndexInfo)> + '_, TypedStoreError>
    {
        let lower_bound = DynamicFieldKey::new(parent, ObjectID::ZERO);
        let upper_bound = DynamicFieldKey::new(parent, ObjectID::MAX);
        let mut iter = self
            .dynamic_field
            .iter_with_bounds(Some(lower_bound), Some(upper_bound));

        if let Some(cursor) = cursor {
            iter = iter.skip_to(&DynamicFieldKey::new(parent, cursor))?;
        }

        Ok(iter)
    }
}

pub struct RestIndexStore {
    tables: IndexStoreTables,
}

impl RestIndexStore {
    pub fn new(
        path: PathBuf,
        authority_store: &AuthorityStore,
        checkpoint_store: &CheckpointStore,
        resolver: &mut dyn LayoutResolver,
    ) -> Self {
        let mut tables = IndexStoreTables::open_tables_read_write(
            path,
            MetricConf::new("rest-index"),
            None,
            None,
        );

        // If the index tables are empty then we need to populate them
        if tables.is_empty() {
            tables
                .init(authority_store, checkpoint_store, resolver)
                .unwrap();
        }

        Self { tables }
    }

    pub fn new_without_init(path: PathBuf) -> Self {
        let tables = IndexStoreTables::open_tables_read_write(
            path,
            MetricConf::new("rest-index"),
            None,
            None,
        );

        Self { tables }
    }

    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    pub fn prune(
        &self,
        checkpoint_contents_to_prune: &[CheckpointContents],
    ) -> Result<(), TypedStoreError> {
        self.tables.prune(checkpoint_contents_to_prune)
    }

    pub fn index_checkpoint(
        &self,
        checkpoint: &CheckpointData,
        resolver: &mut dyn LayoutResolver,
    ) -> Result<(), StorageError> {
        self.tables.index_checkpoint(checkpoint, resolver)
    }

    pub fn get_transaction_info(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<TransactionInfo>, TypedStoreError> {
        self.tables.get_transaction_info(digest)
    }

    pub fn owner_iter(
        &self,
        owner: SuiAddress,
        cursor: Option<ObjectID>,
    ) -> Result<impl Iterator<Item = (OwnerIndexKey, OwnerIndexInfo)> + '_, TypedStoreError> {
        self.tables.owner_iter(owner, cursor)
    }

    pub fn dynamic_field_iter(
        &self,
        parent: ObjectID,
        cursor: Option<ObjectID>,
    ) -> Result<impl Iterator<Item = (DynamicFieldKey, DynamicFieldIndexInfo)> + '_, TypedStoreError>
    {
        self.tables.dynamic_field_iter(parent, cursor)
    }
}

fn try_create_dynamic_field_info(
    object: &Object,
    resolver: &mut dyn LayoutResolver,
) -> Result<Option<DynamicFieldIndexInfo>, StorageError> {
    // Skip if not a move object
    let Some(move_object) = object.data.try_as_move() else {
        return Ok(None);
    };

    // Skip any objects that aren't of type `Field<Name, Value>`
    //
    // All dynamic fields are of type:
    //   - Field<Name, Value> for dynamic fields
    //   - Field<Wrapper<Name, ID>> for dynamic field objects where the ID is the id of the pointed
    //   to object
    //
    if !move_object.type_().is_dynamic_field() {
        return Ok(None);
    }

    let (name_value, dynamic_field_type, object_id) = {
        let layout = sui_types::type_resolver::into_struct_layout(
            resolver
                .get_annotated_layout(&move_object.type_().clone().into())
                .map_err(StorageError::custom)?,
        )
        .map_err(StorageError::custom)?;

        let move_struct = move_object
            .to_move_struct(&layout)
            .map_err(StorageError::serialization)?;

        // SAFETY: move struct has already been validated to be of type DynamicField
        DynamicFieldInfo::parse_move_object(&move_struct).unwrap()
    };

    let name_type = move_object
        .type_()
        .try_extract_field_name(&dynamic_field_type)
        .expect("object is of type Field");

    let name_value = name_value
        .undecorate()
        .simple_serialize()
        .expect("serialization cannot fail");

    let dynamic_object_id = match dynamic_field_type {
        DynamicFieldType::DynamicObject => Some(object_id),
        DynamicFieldType::DynamicField => None,
    };

    let field_info = DynamicFieldIndexInfo {
        name_type,
        name_value,
        dynamic_field_type,
        dynamic_object_id,
    };

    Ok(Some(field_info))
}
