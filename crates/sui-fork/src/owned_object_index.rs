// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! DBMap-backed owned-object index for the experimental `sui-fork` tool.
//!
//! [`OwnedObjectIndexStore`] owns the typed-store tables that map address owners to their live
//! objects. It is held alongside [`crate::filesystem::FilesystemStore`] inside the `DataStore`, so
//! the index can evolve independently of the filesystem object cache.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context as _;
use anyhow::bail;
use itertools::Itertools as _;

use move_core_types::language_storage::StructTag;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::object::Object;
use sui_types::object::Owner;
use sui_types::storage::OwnedObjectInfo;
use sui_types::storage::error::Error as StorageError;
use sui_types::storage::error::Result as StorageResult;
use typed_store::DBMapUtils;
use typed_store::Map;
use typed_store::metrics::SamplingInterval;
use typed_store::rocks::DBBatch;
use typed_store::rocks::DBMap;
use typed_store::rocks::MetricConf;

use crate::DataStore;
use crate::ObjectKey;
use crate::ObjectRead;
use crate::VersionQuery;
use crate::filesystem::INDICES_DIR;

/// Typed-store owned-object index directory under `indices`.
const OWNED_OBJECTS_INDEX_DB_DIR: &str = "owned_objects_db";
/// Current owned-object index schema version.
const OWNED_OBJECT_INDEX_VERSION: u64 = 1;

/// Ordered owner index key.
///
/// Rows are grouped by owner, then by object type. Coin-like objects store `!balance` so ascending
/// scans return the largest balances first, matching the v2 RPC index.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Deserialize, serde::Serialize)]
struct OwnedObjectIndexKey {
    owner: SuiAddress,
    object_type: StructTag,
    inverted_balance: Option<u64>,
    object_id: ObjectID,
}

/// Singleton metadata row used to distinguish an initialized-empty index from a missing index.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
struct OwnedObjectIndexMetadata {
    /// Schema version of the typed-store owned-object index.
    version: u64,
}

/// Typed-store tables for the owned-object index.
#[derive(DBMapUtils)]
struct OwnedObjectIndexTables {
    /// Initialization marker and schema version.
    meta: DBMap<(), OwnedObjectIndexMetadata>,
    /// Owner-ordered rows used by `list_owned_objects` and RPC pagination.
    objects: DBMap<OwnedObjectIndexKey, SequenceNumber>,
}

/// DBMap-backed owned-object index.
///
/// Held by value inside `DataStore`, which already shares all of its state through a single
/// `Arc`, so this type needs neither its own `Arc` wrapper nor a `Clone` impl.
pub(crate) struct OwnedObjectIndexStore {
    tables: OwnedObjectIndexTables,
}

impl OwnedObjectIndexKey {
    fn from_object(object: &Object) -> Option<Self> {
        let owner = match object.owner() {
            Owner::AddressOwner(owner) | Owner::ConsensusAddressOwner { owner, .. } => *owner,
            _ => return None,
        };
        let object_type = object.struct_tag()?;
        let inverted_balance = object.as_coin_maybe().map(|coin| !coin.value());

        Some(Self {
            owner,
            object_type,
            inverted_balance,
            object_id: object.id(),
        })
    }

    /// Rebuild the exact index key encoded in a v2 owned-object page token.
    ///
    /// Pagination is inclusive, so the next scan must resume from the full row key: type and
    /// inverted balance are part of the ordering, not just metadata returned to the client.
    fn from_cursor(cursor: OwnedObjectInfo) -> Self {
        Self {
            owner: cursor.owner,
            object_type: cursor.object_type,
            inverted_balance: cursor.balance.map(std::ops::Not::not),
            object_id: cursor.object_id,
        }
    }

    /// Construct the first possible key for an owner scan when there is no cursor.
    ///
    /// The owner index is ordered by `(owner, object_type, inverted_balance, object_id)`. A scan
    /// therefore needs a concrete type in its lower bound: exact type filters start at that type,
    /// while unfiltered scans use the smallest valid struct tag so every type for the owner is
    /// reachable.
    fn lower_bound(owner: SuiAddress, object_type: Option<&StructTag>) -> Self {
        Self {
            owner,
            object_type: object_type.cloned().unwrap_or_else(min_struct_tag),
            inverted_balance: None,
            object_id: ObjectID::ZERO,
        }
    }

    /// Convert an index row back to the RPC cursor/result shape.
    ///
    /// The key stores `!balance` for coin ordering; the API exposes the original balance.
    fn into_owned_object_info(self, version: SequenceNumber) -> OwnedObjectInfo {
        OwnedObjectInfo {
            owner: self.owner,
            object_type: self.object_type,
            balance: self.inverted_balance.map(std::ops::Not::not),
            object_id: self.object_id,
            version,
        }
    }
}

impl OwnedObjectIndexStore {
    /// Open the owned-object index tables under `<root>/indices/owned_objects_db`.
    ///
    /// typed-store starts Tokio-backed metrics tasks while opening RocksDB tables, so synchronous
    /// call sites need a temporary runtime until construction can move fully under Tokio.
    pub(crate) fn open(root: &Path) -> Self {
        let path = root.join(INDICES_DIR).join(OWNED_OBJECTS_INDEX_DB_DIR);
        if tokio::runtime::Handle::try_current().is_ok() {
            return Self::open_at(path);
        }

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("tokio runtime should build for typed-store open");
        runtime.block_on(async move { Self::open_at(path) })
    }

    /// Open the typed-store table set at a concrete path.
    fn open_at(path: PathBuf) -> Self {
        Self {
            tables: OwnedObjectIndexTables::open_tables_read_write(
                path,
                Self::metric_conf(),
                None,
                None,
            ),
        }
    }

    /// Use a stable DB name while disabling interval-based sampling for synchronous tests.
    fn metric_conf() -> MetricConf {
        MetricConf {
            db_name: "sui-fork-owned-index".to_owned(),
            read_sample_interval: SamplingInterval::new(Duration::ZERO, 0),
            write_sample_interval: SamplingInterval::new(Duration::ZERO, 0),
            iter_sample_interval: SamplingInterval::new(Duration::ZERO, 0),
        }
    }

    /// Read all object infos from the owned-object index.
    pub(crate) fn get_owned_object_infos(&self) -> anyhow::Result<Vec<OwnedObjectInfo>> {
        self.tables
            .objects
            .safe_iter()
            .map(|entry| {
                entry
                    .map(|(key, version)| key.into_owned_object_info(version))
                    .map_err(Into::into)
            })
            .collect()
    }

    /// Read object infos for one owner.
    ///
    /// The cursor is an inclusive full-key lower bound because the v2 RPC page token stores the
    /// first not-yet-returned object, not the last returned object.
    pub(crate) fn scan_owner(
        &self,
        owner: SuiAddress,
        object_type: Option<&StructTag>,
        cursor: Option<OwnedObjectInfo>,
    ) -> anyhow::Result<Vec<OwnedObjectInfo>> {
        let lower = cursor
            .map(OwnedObjectIndexKey::from_cursor)
            .unwrap_or_else(|| OwnedObjectIndexKey::lower_bound(owner, object_type));

        self.tables
            .objects
            .safe_iter_with_bounds(Some(lower), None)
            .take_while(|entry| {
                let Ok((key, _)) = entry else {
                    return true;
                };
                key.owner == owner
                    && object_type
                        .is_none_or(|filter| struct_tag_filter_matches(filter, &key.object_type))
            })
            .map(|entry| {
                entry
                    .map(|(key, version)| key.into_owned_object_info(version))
                    .map_err(Into::into)
            })
            .collect()
    }

    /// Apply local execution ownership changes to the owned-object index.
    pub(crate) fn apply_owned_object_index_updates<'a>(
        &self,
        old_objects: impl IntoIterator<Item = &'a Object>,
        new_objects: impl IntoIterator<Item = &'a Object>,
    ) -> anyhow::Result<()> {
        let mut batch = self.tables.objects.batch();

        self.delete_owned_objects(&mut batch, old_objects)?;
        self.insert_owned_objects(&mut batch, new_objects)?;

        self.mark_owned_object_index_initialized(&mut batch)?;
        batch.write()?;
        Ok(())
    }

    /// Initialize the owned-object index from a complete object snapshot.
    ///
    /// This replaces any existing rows and writes the metadata marker in the same batch, including
    /// when `entries` is empty. Do not use for updating the index. Use
    /// `apply_owned_object_index_updates` instead.
    pub(crate) fn replace_from_objects<'a>(
        &self,
        objects: impl IntoIterator<Item = &'a Object>,
    ) -> anyhow::Result<()> {
        let mut batch = self.tables.objects.batch();

        for entry in self.tables.objects.safe_iter() {
            let (key, _) = entry?;
            batch.delete_batch(&self.tables.objects, [key])?;
        }

        self.insert_owned_objects(&mut batch, objects)?;

        self.mark_owned_object_index_initialized(&mut batch)?;
        batch.write()?;
        Ok(())
    }

    /// Return whether the owned-object index has already been initialized.
    pub(crate) fn owned_object_index_exists(&self) -> anyhow::Result<bool> {
        match self.tables.meta.get(&())? {
            Some(metadata) if metadata.version == OWNED_OBJECT_INDEX_VERSION => Ok(true),
            Some(metadata) => bail!(
                "unsupported owned-object index version: {}",
                metadata.version
            ),
            None => Ok(false),
        }
    }

    /// Mark the owned-object index initialized in the same batch as the index rows.
    ///
    /// This marker is written for empty indexes too, so readers can tell an initialized-empty
    /// index from one that still needs lazy seed initialization.
    fn mark_owned_object_index_initialized(&self, batch: &mut DBBatch) -> anyhow::Result<()> {
        batch.insert_batch(
            &self.tables.meta,
            [(
                (),
                OwnedObjectIndexMetadata {
                    version: OWNED_OBJECT_INDEX_VERSION,
                },
            )],
        )?;
        Ok(())
    }

    /// Remove prior keys for address-owned, indexable objects.
    fn delete_owned_objects<'a>(
        &self,
        batch: &mut DBBatch,
        objects: impl IntoIterator<Item = &'a Object>,
    ) -> anyhow::Result<()> {
        for object in objects {
            if let Some(key) = OwnedObjectIndexKey::from_object(object) {
                batch.delete_batch(&self.tables.objects, [key])?;
            }
        }
        Ok(())
    }

    /// Insert current keys for address-owned, indexable objects.
    fn insert_owned_objects<'a>(
        &self,
        batch: &mut DBBatch,
        objects: impl IntoIterator<Item = &'a Object>,
    ) -> anyhow::Result<()> {
        for object in objects {
            if let Some(key) = OwnedObjectIndexKey::from_object(object) {
                batch.insert_batch(&self.tables.objects, [(key, object.version())])?;
            }
        }
        Ok(())
    }
}

/// `DataStore`-level orchestration of the owned-object index: lazy seed initialization and the
/// owner-listing reads that back the v2 RPC owned-object iterator.
impl DataStore {
    /// Initialize the owned-object index from the seed manifest the first time it is needed.
    pub(crate) fn ensure_owned_object_index_initialized(&self) -> anyhow::Result<()> {
        if self.owned_index().owned_object_index_exists()? {
            return Ok(());
        }

        let _local_snapshot_guard = self.write_local_snapshot()?;
        if self.owned_index().owned_object_index_exists()? {
            return Ok(());
        }

        if let Some(checkpoint) = self.local().get_highest_verified_checkpoint()?
            && checkpoint.data().sequence_number > self.forked_at_checkpoint()
        {
            bail!(
                "owned-object index is missing while local checkpoints have advanced past the fork checkpoint; refusing to rebuild stale seed state",
            );
        }

        let mut indexed_objects = BTreeMap::new();
        if self.local().seed_manifest_exists() {
            let manifest = self.local().read_seed_manifest()?;
            if manifest.checkpoint != self.forked_at_checkpoint() {
                bail!(
                    "Seed manifest checkpoint {} does not match requested checkpoint {}. Use a different --data-dir.",
                    manifest.checkpoint,
                    self.forked_at_checkpoint(),
                );
            }

            let keys: Vec<_> = manifest
                .entries
                .iter()
                .map(|entry| ObjectKey {
                    object_id: entry.object_ref.0,
                    version_query: VersionQuery::VersionAtCheckpoint {
                        version: entry.object_ref.1.value(),
                        checkpoint: self.forked_at_checkpoint(),
                    },
                })
                .collect();
            let objects = self
                .gql()
                .get_objects(&keys)
                .context("failed to fetch seeded objects for owned-object index")?;

            for (seed_entry, object) in manifest.entries.iter().zip_eq(objects) {
                let Some((object, _)) = object else {
                    bail!(
                        "seeded object {} version {} was not found at fork checkpoint {}",
                        seed_entry.object_ref.0,
                        seed_entry.object_ref.1.value(),
                        self.forked_at_checkpoint(),
                    );
                };
                if OwnedObjectIndexKey::from_object(&object).is_none() {
                    bail!(
                        "seeded object {} is not an address-owned Move object",
                        seed_entry.object_ref.0,
                    );
                }
                let object_ref = object.compute_object_reference();
                if object_ref != seed_entry.object_ref {
                    bail!(
                        "seeded object {} metadata does not match fetched object at fork checkpoint {}",
                        seed_entry.object_ref.0,
                        self.forked_at_checkpoint(),
                    );
                }

                self.local().write_object(&object)?;
                indexed_objects.insert(object_ref.0, object);
            }
        }

        // Insert into DB the objects for index
        self.owned_index()
            .replace_from_objects(indexed_objects.values())
    }

    /// Get owned objects for an address, optionally filtered by object type and paginated with a
    /// cursor.
    pub(crate) fn get_owned_objects(
        &self,
        owner: SuiAddress,
        object_type: Option<StructTag>,
        cursor: Option<OwnedObjectInfo>,
    ) -> StorageResult<Vec<OwnedObjectInfo>> {
        self.ensure_owned_object_index_initialized()
            .map_err(|e| StorageError::custom(e.to_string()))?;
        let _local_snapshot_guard = self.read_local_snapshot()?;
        self.owned_index()
            .scan_owner(owner, object_type.as_ref(), cursor)
            .map_err(|e| StorageError::custom(e.to_string()))
    }
}

/// Smallest valid struct tag used to begin owner-wide scans.
///
/// `StructTag` does not have a natural `MIN` constant, but DB scans need a concrete lower-bound
/// key. This sentinel sorts before all framework and user package types because it uses address
/// zero and the shortest valid identifiers.
fn min_struct_tag() -> StructTag {
    "0x0::A::A"
        .parse()
        .expect("minimum struct tag literal should parse")
}

/// Check if these two `StructTag`s match for the purposes of owned-object filtering. The filter
/// may have empty type parameters, in which case they are ignored and only the address, module, and
/// name are compared.
///
/// This allows a wildcard filter like `0x2::coin::Coin` to match all versions of the `Coin` struct,
/// regardless of the type parameter (e.g., `0x2::coin::Coin<0x1::sui::SUI>`).
pub(crate) fn struct_tag_filter_matches(filter: &StructTag, candidate: &StructTag) -> bool {
    filter.address == candidate.address
        && filter.module.as_ident_str() == candidate.module.as_ident_str()
        && filter.name.as_ident_str() == candidate.name.as_ident_str()
        && (filter.type_params.is_empty()
            || filter.type_params.as_slice() == candidate.type_params.as_slice())
}

#[cfg(test)]
#[path = "tests/owned_object_index.rs"]
mod tests;
