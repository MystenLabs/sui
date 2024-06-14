// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::authority_store_tables::LiveObject;
use crate::authority::AuthorityStore;
use crate::checkpoints::CheckpointStore;
use move_core_types::language_storage::StructTag;
use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;
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
use sui_types::storage::BackingPackageStore;
use sui_types::storage::DynamicFieldIndexInfo;
use sui_types::storage::DynamicFieldKey;
use sui_types::type_resolver::LayoutResolver;
use tracing::{debug, info};
use typed_store::rocks::{DBMap, MetricConf};
use typed_store::traits::Map;
use typed_store::traits::{TableSummary, TypedStoreDebug};
use typed_store::TypedStoreError;
use typed_store_derive::DBMapUtils;

const CURRENT_DB_VERSION: u64 = 0;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct MetadataInfo {
    /// Version of the Database
    version: u64,
}

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

#[derive(Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct TransactionInfo {
    pub checkpoint: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct CoinIndexKey {
    coin_type: StructTag,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct CoinIndexInfo {
    pub coin_metadata_object_id: Option<ObjectID>,
    pub treasury_object_id: Option<ObjectID>,
}

impl CoinIndexInfo {
    fn merge(self, other: Self) -> Self {
        Self {
            coin_metadata_object_id: self
                .coin_metadata_object_id
                .or(other.coin_metadata_object_id),
            treasury_object_id: self.treasury_object_id.or(other.treasury_object_id),
        }
    }
}

/// RocksDB tables for the RestIndexStore
///
/// Anytime a new table is added, or and existing one has it's schema changed, make sure to also
/// update the value of `CURRENT_DB_VERSION`.
///
/// NOTE: Authors and Reviewers before adding any new tables ensure that they are either:
/// - bounded in size by the live object set
/// - are prune-able and have corresponding logic in the `prune` function
#[derive(DBMapUtils)]
struct IndexStoreTables {
    /// A singleton that store metadata information on the DB.
    ///
    /// A few uses for this singleton:
    /// - determining if the DB has been initialized (as some tables will still be empty post
    /// initializatio)
    /// - version of the DB. Everytime a new table or schema is changed the version number needs to
    /// be incremented.
    meta: DBMap<(), MetadataInfo>,

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

    /// An index of Coin Types
    ///
    /// Allows looking up information related to published Coins, like the ObjectID of its
    /// coorisponding CoinMetadata.
    coin: DBMap<CoinIndexKey, CoinIndexInfo>,
    // NOTE: Authors and Reviewers before adding any new tables ensure that they are either:
    // - bounded in size by the live object set
    // - are prune-able and have corresponding logic in the `prune` function
}

impl IndexStoreTables {
    fn open<P: Into<PathBuf>>(path: P) -> Self {
        IndexStoreTables::open_tables_read_write(
            path.into(),
            MetricConf::new("rest-index"),
            None,
            None,
        )
    }

    fn needs_to_do_initialization(&self) -> bool {
        match self.meta.get(&()) {
            Ok(Some(metadata)) => metadata.version != CURRENT_DB_VERSION,
            Ok(None) => true,
            Err(_) => true,
        }
    }

    fn needs_to_delete_old_db(&self) -> bool {
        match self.meta.get(&()) {
            Ok(Some(metadata)) => metadata.version != CURRENT_DB_VERSION,
            Ok(None) => false,
            Err(_) => true,
        }
    }

    fn init(
        &mut self,
        authority_store: &AuthorityStore,
        checkpoint_store: &CheckpointStore,
        epoch_store: &AuthorityPerEpochStore,
        package_store: &Arc<dyn BackingPackageStore + Send + Sync>,
    ) -> Result<(), StorageError> {
        info!("Initializing REST indexes");

        // Iterate through available, executed checkpoints that have yet to be pruned
        // to initialize checkpoint and transaction based indexes.
        if let Some(highest_executed_checkpint) =
            checkpoint_store.get_highest_executed_checkpoint_seq_number()?
        {
            let lowest_available_checkpoint = checkpoint_store
                .get_highest_pruned_checkpoint_seq_number()?
                .saturating_add(1);

            let checkpoint_range = lowest_available_checkpoint..=highest_executed_checkpint;

            info!(
                "Indexing {} checkpoints in range {checkpoint_range:?}",
                checkpoint_range.size_hint().0
            );
            let start_time = Instant::now();

            checkpoint_range.into_par_iter().try_for_each(|seq| {
                let checkpoint = checkpoint_store
                    .get_checkpoint_by_sequence_number(seq)?
                    .ok_or_else(|| StorageError::missing(format!("missing checkpoint {seq}")))?;
                let contents = checkpoint_store
                    .get_checkpoint_contents(&checkpoint.content_digest)?
                    .ok_or_else(|| StorageError::missing(format!("missing checkpoint {seq}")))?;

                let info = TransactionInfo {
                    checkpoint: checkpoint.sequence_number,
                };

                self.transactions
                    .multi_insert(contents.iter().map(|digests| (digests.transaction, info)))
                    .map_err(StorageError::from)
            })?;

            info!(
                "Indexing checkpoints took {} seconds",
                start_time.elapsed().as_secs()
            );
        }

        let coin_index = Mutex::new(HashMap::new());

        info!("Indexing Live Object Set");
        let start_time = Instant::now();
        std::thread::scope(|s| -> Result<(), StorageError> {
            let mut threads = Vec::new();
            const BITS: u8 = 5;
            for index in 0u8..(1 << BITS) {
                let this = &self;
                let coin_index = &coin_index;
                threads.push(s.spawn(move || {
                    this.live_object_set_index_task(
                        index,
                        BITS,
                        authority_store,
                        coin_index,
                        epoch_store,
                        package_store,
                    )
                }));
            }

            // join threads
            for thread in threads {
                thread.join().unwrap()?;
            }

            Ok(())
        })?;

        self.coin.multi_insert(coin_index.into_inner().unwrap())?;

        info!(
            "Indexing Live Object Set took {} seconds",
            start_time.elapsed().as_secs()
        );

        self.meta.insert(
            &(),
            &MetadataInfo {
                version: CURRENT_DB_VERSION,
            },
        )?;

        info!("Finished initializing REST indexes");

        Ok(())
    }

    fn live_object_set_index_task(
        &self,
        task_id: u8,
        bits: u8,
        authority_store: &AuthorityStore,
        coin_index: &Mutex<HashMap<CoinIndexKey, CoinIndexInfo>>,
        epoch_store: &AuthorityPerEpochStore,
        package_store: &Arc<dyn BackingPackageStore + Send + Sync>,
    ) -> Result<(), StorageError> {
        let mut id_bytes = [0; ObjectID::LENGTH];
        id_bytes[0] = task_id << (8 - bits);
        let start_id = ObjectID::new(id_bytes);

        id_bytes[0] |= (1 << (8 - bits)) - 1;
        for element in id_bytes.iter_mut().skip(1) {
            *element = u8::MAX;
        }
        let end_id = ObjectID::new(id_bytes);

        let mut resolver = epoch_store
            .executor()
            .type_layout_resolver(Box::new(package_store));
        let mut batch = self.owner.batch();
        let mut object_scanned: u64 = 0;
        for object in authority_store
            .perpetual_tables
            .range_iter_live_object_set(Some(start_id), Some(end_id), false)
            .filter_map(LiveObject::to_normal)
        {
            object_scanned += 1;
            if object_scanned % 2_000_000 == 0 {
                info!(
                    "[Index] Task {}: object scanned: {}",
                    task_id, object_scanned
                );
            }
            match object.owner {
                // Owner Index
                Owner::AddressOwner(owner) => {
                    let owner_key = OwnerIndexKey::new(owner, object.id());
                    let owner_info = OwnerIndexInfo::new(&object);
                    batch.insert_batch(&self.owner, [(owner_key, owner_info)])?;
                }

                // Dynamic Field Index
                Owner::ObjectOwner(parent) => {
                    if let Some(field_info) =
                        try_create_dynamic_field_info(&object, resolver.as_mut())?
                    {
                        let field_key = DynamicFieldKey::new(parent, object.id());

                        batch.insert_batch(&self.dynamic_field, [(field_key, field_info)])?;
                    }
                }

                Owner::Shared { .. } | Owner::Immutable => {}
            }

            // Look for CoinMetadata<T> and TreasuryCap<T> objects
            if let Some((key, value)) = try_create_coin_index_info(&object) {
                use std::collections::hash_map::Entry;

                match coin_index.lock().unwrap().entry(key) {
                    Entry::Occupied(o) => {
                        let (key, v) = o.remove_entry();
                        let value = value.merge(v);
                        batch.insert_batch(&self.coin, [(key, value)])?;
                    }
                    Entry::Vacant(v) => {
                        v.insert(value);
                    }
                }
            }

            // If the batch size grows to greater that 256MB then write out to the DB so that the
            // data we need to hold in memory doesn't grown unbounded.
            if batch.size_in_bytes() >= 1 << 28 {
                batch.write()?;
                batch = self.owner.batch();
            }
        }

        batch.write()?;
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

        // object indexes
        {
            let mut coin_index = HashMap::new();

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

                // coin indexing
                //
                // coin indexing relys on the fact that CoinMetadata and TreasuryCap are created in
                // the same transaction so we don't need to worry about overriding any older value
                // that may exist in the database (because there necessarily cannot be).
                for (key, value) in tx.created_objects().flat_map(try_create_coin_index_info) {
                    use std::collections::hash_map::Entry;

                    match coin_index.entry(key) {
                        Entry::Occupied(o) => {
                            let (key, v) = o.remove_entry();
                            let value = value.merge(v);
                            batch.insert_batch(&self.coin, [(key, value)])?;
                        }
                        Entry::Vacant(v) => {
                            v.insert(value);
                        }
                    }
                }
            }

            batch.insert_batch(&self.coin, coin_index)?;
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

    fn get_coin_info(
        &self,
        coin_type: &StructTag,
    ) -> Result<Option<CoinIndexInfo>, TypedStoreError> {
        let key = CoinIndexKey {
            coin_type: coin_type.to_owned(),
        };
        self.coin.get(&key)
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
        epoch_store: &AuthorityPerEpochStore,
        package_store: &Arc<dyn BackingPackageStore + Send + Sync>,
    ) -> Self {
        let tables = {
            let tables = IndexStoreTables::open(&path);

            // If the index tables are uninitialized or on an older version then we need to
            // populate them
            if tables.needs_to_do_initialization() {
                let mut tables = if tables.needs_to_delete_old_db() {
                    drop(tables);
                    typed_store::rocks::safe_drop_db(path.clone())
                        .expect("unable to destroy old rest-index db");
                    IndexStoreTables::open(path)
                } else {
                    tables
                };

                tables
                    .init(
                        authority_store,
                        checkpoint_store,
                        epoch_store,
                        package_store,
                    )
                    .expect("unable to initialize rest index from live object set");
                tables
            } else {
                tables
            }
        };

        Self { tables }
    }

    pub fn new_without_init(path: PathBuf) -> Self {
        let tables = IndexStoreTables::open(path);

        Self { tables }
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

    pub fn get_coin_info(
        &self,
        coin_type: &StructTag,
    ) -> Result<Option<CoinIndexInfo>, TypedStoreError> {
        self.tables.get_coin_info(coin_type)
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

fn try_create_coin_index_info(object: &Object) -> Option<(CoinIndexKey, CoinIndexInfo)> {
    use sui_types::coin::CoinMetadata;
    use sui_types::coin::TreasuryCap;

    object
        .type_()
        .and_then(MoveObjectType::other)
        .and_then(|object_type| {
            CoinMetadata::is_coin_metadata_with_coin_type(object_type)
                .cloned()
                .map(|coin_type| {
                    (
                        CoinIndexKey { coin_type },
                        CoinIndexInfo {
                            coin_metadata_object_id: Some(object.id()),
                            treasury_object_id: None,
                        },
                    )
                })
                .or_else(|| {
                    TreasuryCap::is_treasury_with_coin_type(object_type)
                        .cloned()
                        .map(|coin_type| {
                            (
                                CoinIndexKey { coin_type },
                                CoinIndexInfo {
                                    coin_metadata_object_id: None,
                                    treasury_object_id: Some(object.id()),
                                },
                            )
                        })
                })
        })
}
