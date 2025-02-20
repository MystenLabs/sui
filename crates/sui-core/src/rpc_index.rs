// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::AuthorityStore;
use crate::checkpoints::CheckpointStore;
use crate::par_index_live_object_set::LiveObjectIndexer;
use crate::par_index_live_object_set::ParMakeLiveObjectIndexer;
use move_core_types::language_storage::StructTag;
use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;
use sui_types::base_types::MoveObjectType;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::digests::TransactionDigest;
use sui_types::dynamic_field::visitor as DFV;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::layout_resolver::LayoutResolver;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::object::Object;
use sui_types::object::Owner;
use sui_types::storage::error::Error as StorageError;
use sui_types::storage::BackingPackageStore;
use sui_types::storage::DynamicFieldIndexInfo;
use sui_types::storage::DynamicFieldKey;
use tracing::{debug, info};
use typed_store::rocks::{DBMap, MetricConf};
use typed_store::traits::Map;
use typed_store::traits::{TableSummary, TypedStoreDebug};
use typed_store::DBMapUtils;
use typed_store::TypedStoreError;

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

/// RocksDB tables for the RpcIndexStore
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
    ///     initialization)
    /// - version of the DB. Everytime a new table or schema is changed the version number needs to
    ///     be incremented.
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
            MetricConf::new("rpc-index"),
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
        info!("Initializing RPC indexes");

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

        let make_live_object_indexer = RpcParLiveObjectSetIndexer {
            tables: self,
            coin_index: &coin_index,
            epoch_store,
            package_store,
        };

        crate::par_index_live_object_set::par_index_live_object_set(
            authority_store,
            &make_live_object_indexer,
        )?;

        self.coin.multi_insert(coin_index.into_inner().unwrap())?;

        self.meta.insert(
            &(),
            &MetadataInfo {
                version: CURRENT_DB_VERSION,
            },
        )?;

        info!("Finished initializing RPC indexes");

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
    ) -> Result<typed_store::rocks::DBBatch, StorageError> {
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
                for removed_object in tx.removed_objects_pre_version() {
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
                        // TODO: Implement support for ConsensusV2 objects.
                        Owner::ConsensusV2 { .. } => todo!(),
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
                                // TODO: Implement support for ConsensusV2 objects.
                                Owner::ConsensusV2 { .. } => todo!(),
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
                                try_create_dynamic_field_info(object, resolver)
                                    .ok()
                                    .flatten()
                            {
                                let field_key = DynamicFieldKey::new(*parent, object.id());

                                batch
                                    .insert_batch(&self.dynamic_field, [(field_key, field_info)])?;
                            }
                        }
                        Owner::Shared { .. } | Owner::Immutable => {}
                        // TODO: Implement support for ConsensusV2 objects.
                        Owner::ConsensusV2 { .. } => todo!(),
                    }
                }

                // coin indexing
                //
                // coin indexing relies on the fact that CoinMetadata and TreasuryCap are created in
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

        debug!(
            checkpoint = checkpoint.checkpoint_summary.sequence_number,
            "finished indexing checkpoint"
        );

        Ok(batch)
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

pub struct RpcIndexStore {
    tables: IndexStoreTables,
    pending_updates: Mutex<BTreeMap<u64, typed_store::rocks::DBBatch>>,
}

impl RpcIndexStore {
    /// Given the provided directory, construct the path to the db
    fn db_path(dir: &Path) -> PathBuf {
        dir.join("rpc-index")
    }

    pub fn new(
        dir: &Path,
        authority_store: &AuthorityStore,
        checkpoint_store: &CheckpointStore,
        epoch_store: &AuthorityPerEpochStore,
        package_store: &Arc<dyn BackingPackageStore + Send + Sync>,
    ) -> Self {
        let path = Self::db_path(dir);

        let tables = {
            let tables = IndexStoreTables::open(&path);

            // If the index tables are uninitialized or on an older version then we need to
            // populate them
            if tables.needs_to_do_initialization() {
                let mut tables = if tables.needs_to_delete_old_db() {
                    drop(tables);
                    typed_store::rocks::safe_drop_db(path.clone())
                        .expect("unable to destroy old rpc-index db");
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
                    .expect("unable to initialize rpc index from live object set");
                tables
            } else {
                tables
            }
        };

        Self {
            tables,
            pending_updates: Default::default(),
        }
    }

    pub fn new_without_init(dir: &Path) -> Self {
        let path = Self::db_path(dir);
        let tables = IndexStoreTables::open(path);

        Self {
            tables,
            pending_updates: Default::default(),
        }
    }

    pub fn prune(
        &self,
        checkpoint_contents_to_prune: &[CheckpointContents],
    ) -> Result<(), TypedStoreError> {
        self.tables.prune(checkpoint_contents_to_prune)
    }

    /// Index a checkpoint and stage the index updated in `pending_updates`.
    ///
    /// Updates will not be committed to the database until `commit_update_for_checkpoint` is
    /// called.
    pub fn index_checkpoint(&self, checkpoint: &CheckpointData, resolver: &mut dyn LayoutResolver) {
        let sequence_number = checkpoint.checkpoint_summary.sequence_number;
        let batch = self
            .tables
            .index_checkpoint(checkpoint, resolver)
            .expect("db error");

        self.pending_updates
            .lock()
            .unwrap()
            .insert(sequence_number, batch);
    }

    /// Commits the pending updates for the provided checkpoint number.
    ///
    /// Invariants:
    /// - `index_checkpoint` must have been called for the provided checkpoint
    /// - Callers of this function must ensure that it is called for each checkpoint in sequential
    ///   order. This will panic if the provided checkpoint does not match the expected next
    ///   checkpoint to commit.
    pub fn commit_update_for_checkpoint(&self, checkpoint: u64) -> Result<(), StorageError> {
        let next_batch = self.pending_updates.lock().unwrap().pop_first();

        // Its expected that the next batch exists
        let (next_sequence_number, batch) = next_batch.unwrap();
        assert_eq!(
            checkpoint, next_sequence_number,
            "commit_update_for_checkpoint must be called in order"
        );

        Ok(batch.write()?)
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

    let layout = resolver
        .get_annotated_layout(&move_object.type_().clone().into())
        .map_err(StorageError::custom)?
        .into_layout();

    let field = DFV::FieldVisitor::deserialize(move_object.contents(), &layout)
        .map_err(StorageError::custom)?;

    let value_metadata = field.value_metadata().map_err(StorageError::custom)?;

    Ok(Some(DynamicFieldIndexInfo {
        name_type: field.name_layout.into(),
        name_value: field.name_bytes.to_owned(),
        dynamic_field_type: field.kind,
        dynamic_object_id: if let DFV::ValueMetadata::DynamicObjectField(id) = value_metadata {
            Some(id)
        } else {
            None
        },
    }))
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

struct RpcParLiveObjectSetIndexer<'a> {
    tables: &'a IndexStoreTables,
    coin_index: &'a Mutex<HashMap<CoinIndexKey, CoinIndexInfo>>,
    epoch_store: &'a AuthorityPerEpochStore,
    package_store: &'a Arc<dyn BackingPackageStore + Send + Sync>,
}

struct RpcLiveObjectIndexer<'a> {
    tables: &'a IndexStoreTables,
    batch: typed_store::rocks::DBBatch,
    coin_index: &'a Mutex<HashMap<CoinIndexKey, CoinIndexInfo>>,
    resolver: Box<dyn LayoutResolver + 'a>,
}

impl<'a> ParMakeLiveObjectIndexer for RpcParLiveObjectSetIndexer<'a> {
    type ObjectIndexer = RpcLiveObjectIndexer<'a>;

    fn make_live_object_indexer(&self) -> Self::ObjectIndexer {
        RpcLiveObjectIndexer {
            tables: self.tables,
            batch: self.tables.owner.batch(),
            coin_index: self.coin_index,
            resolver: self
                .epoch_store
                .executor()
                .type_layout_resolver(Box::new(self.package_store)),
        }
    }
}

impl<'a> LiveObjectIndexer for RpcLiveObjectIndexer<'a> {
    fn index_object(&mut self, object: Object) -> Result<(), StorageError> {
        match object.owner {
            // Owner Index
            Owner::AddressOwner(owner) => {
                let owner_key = OwnerIndexKey::new(owner, object.id());
                let owner_info = OwnerIndexInfo::new(&object);
                self.batch
                    .insert_batch(&self.tables.owner, [(owner_key, owner_info)])?;
            }

            // Dynamic Field Index
            Owner::ObjectOwner(parent) => {
                if let Some(field_info) =
                    try_create_dynamic_field_info(&object, self.resolver.as_mut())?
                {
                    let field_key = DynamicFieldKey::new(parent, object.id());

                    self.batch
                        .insert_batch(&self.tables.dynamic_field, [(field_key, field_info)])?;
                }
            }

            Owner::Shared { .. } | Owner::Immutable => {}
            // TODO: Implement support for ConsensusV2 objects.
            Owner::ConsensusV2 { .. } => todo!(),
        }

        // Look for CoinMetadata<T> and TreasuryCap<T> objects
        if let Some((key, value)) = try_create_coin_index_info(&object) {
            use std::collections::hash_map::Entry;

            match self.coin_index.lock().unwrap().entry(key) {
                Entry::Occupied(o) => {
                    let (key, v) = o.remove_entry();
                    let value = value.merge(v);
                    self.batch.insert_batch(&self.tables.coin, [(key, value)])?;
                }
                Entry::Vacant(v) => {
                    v.insert(value);
                }
            }
        }

        // If the batch size grows to greater that 128MB then write out to the DB so that the
        // data we need to hold in memory doesn't grown unbounded.
        if self.batch.size_in_bytes() >= 1 << 27 {
            std::mem::replace(&mut self.batch, self.tables.owner.batch()).write()?;
        }

        Ok(())
    }

    fn finish(self) -> Result<(), StorageError> {
        self.batch.write()?;
        Ok(())
    }
}
