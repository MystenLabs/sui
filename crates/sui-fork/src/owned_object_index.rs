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
use sui_types::base_types::ObjectRef;
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
use sui_types::base_types::SequenceNumber;

/// Typed-store owned-object index directory under `indices`.
const OWNED_OBJECTS_INDEX_DB_DIR: &str = "owned_objects_db";
/// Current owned-object index schema version.
const OWNED_OBJECT_INDEX_VERSION: u64 = 1;

/// Index entry for a live address-owned object.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) struct OwnedObjectEntry {
    pub(crate) owner: SuiAddress,
    pub(crate) object_ref: ObjectRef,
    pub(crate) object_type: StructTag,
    pub(crate) balance: Option<u64>,
}

impl OwnedObjectEntry {
    /// Build index metadata for live address-owned Move objects.
    pub(crate) fn from_object(object: &Object) -> Option<Self> {
        let owner = match &object.owner {
            Owner::AddressOwner(owner) | Owner::ConsensusAddressOwner { owner, .. } => *owner,
            _ => return None,
        };
        let object_type = object.struct_tag()?;
        Some(Self {
            owner,
            object_ref: object.compute_object_reference(),
            object_type,
            balance: object.as_coin_maybe().map(|coin| coin.value()),
        })
    }
}

/// Ordered owner index key. Scans over this table group rows by owner, then by object ID, which
/// lets RPC pagination seek directly to `(owner, cursor_object_id)`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Deserialize, serde::Serialize)]
struct OwnedObjectOwnerKey {
    /// Address whose object list contains `object_id`.
    owner: SuiAddress,
    /// Object ID used for deterministic ordering and cursor lower bounds within one owner.
    object_id: ObjectID,
    /// Object type for filtering by type without a separate lookup.
    object_type: StructTag,
}

impl From<&OwnedObjectEntry> for OwnedObjectOwnerKey {
    fn from(entry: &OwnedObjectEntry) -> Self {
        Self {
            owner: entry.owner,
            object_id: entry.object_ref.0,
        }
    }
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
    /// Canonical row per object ID, used when update paths only know the object ID.
    by_object: DBMap<ObjectID, SequenceNumber>,
    /// Owner-ordered rows used by `list_owned_objects` and RPC pagination.
    by_owner: DBMap<OwnedObjectOwnerKey, OwnedObjectEntry>,
}

/// DBMap-backed owned-object index.
///
/// Held by value inside `DataStore`, which already shares all of its state through a single
/// `Arc`, so this type needs neither its own `Arc` wrapper nor a `Clone` impl.
pub(crate) struct OwnedObjectIndexStore {
    tables: OwnedObjectIndexTables,
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

    /// Read all entries from the owned-object index.
    pub(crate) fn get_owned_object_entries(&self) -> anyhow::Result<Vec<OwnedObjectEntry>> {
        self.tables
            .by_object
            .safe_iter()
            .map(|entry| entry.map(|(_, entry)| entry).map_err(Into::into))
            .collect()
    }

    /// Read entries for one owner.
    ///
    /// The cursor is an inclusive object-ID lower bound because the v2 RPC page token stores the
    /// first not-yet-returned object, not the last returned object.
    pub(crate) fn get_owned_object_entries_for_owner(
        &self,
        owner: SuiAddress,
        cursor: Option<ObjectID>,
    ) -> anyhow::Result<Vec<OwnedObjectEntry>> {
        let lower = OwnedObjectOwnerKey {
            owner,
            object_id: cursor.unwrap_or(ObjectID::ZERO),
        };

        self.tables
            .by_owner
            .safe_iter_with_bounds(Some(lower), None)
            .take_while(|entry| {
                let Ok((key, _)) = entry else {
                    return true;
                };
                key.owner == owner
            })
            .map(|entry| entry.map(|(_, entry)| entry).map_err(Into::into))
            .collect()
    }

    /// Apply local execution ownership changes to the owned-object index.
    pub(crate) fn apply_owned_object_index_updates<'a>(
        &self,
        removed_object_ids: &[ObjectID],
        written_objects: impl IntoIterator<Item = &'a Object>,
    ) -> anyhow::Result<()> {
        let mut batch = self.tables.by_object.batch();

        for object_id in removed_object_ids {
            self.delete_owned_entry(&mut batch, *object_id)?;
        }

        for object in written_objects {
            match OwnedObjectEntry::from_object(object) {
                Some(entry) => self.upsert_owned_entry(&mut batch, entry)?,
                None => self.delete_owned_entry(&mut batch, object.id())?,
            }
        }

        self.mark_owned_object_index_initialized(&mut batch)?;
        batch.write()?;
        Ok(())
    }

    /// Initialize the owned-object index from a complete snapshot.
    ///
    /// This replaces any existing rows and writes the metadata marker in the same batch, including
    /// when `entries` is empty. Do not use for updating the index. Use
    /// `apply_owned_object_index_updates` instead.
    pub(crate) fn write_owned_object_entries_for_initialization(
        &self,
        entries: &[OwnedObjectEntry],
    ) -> anyhow::Result<()> {
        let mut batch = self.tables.by_object.batch();

        for existing in self.get_owned_object_entries()? {
            self.delete_owned_entry(&mut batch, existing.object_ref.0)?;
        }

        for entry in entries {
            self.upsert_owned_entry(&mut batch, entry.clone())?;
        }

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

    /// Remove an object's rows from both lookup tables.
    ///
    /// Removal starts from `by_object` because execution diffs often know only the object ID; the
    /// stored entry provides the previous owner key needed to clean up `by_owner`.
    fn delete_owned_entry(&self, batch: &mut DBBatch, object_id: ObjectID) -> anyhow::Result<()> {
        if let Some(existing) = self.tables.by_object.get(&object_id)? {
            batch.delete_batch(&self.tables.by_object, [object_id])?;
            batch.delete_batch(
                &self.tables.by_owner,
                [OwnedObjectOwnerKey::from(&existing)],
            )?;
        }
        Ok(())
    }

    /// Insert or replace an owned-object row in both lookup tables.
    ///
    /// Deleting the prior row first handles owner transfers, where the old `by_owner` key is not
    /// derivable from the new entry.
    fn upsert_owned_entry(
        &self,
        batch: &mut DBBatch,
        entry: OwnedObjectEntry,
    ) -> anyhow::Result<()> {
        self.delete_owned_entry(batch, entry.object_ref.0)?;
        batch.insert_batch(
            &self.tables.by_object,
            [(entry.object_ref.0, entry.clone())],
        )?;
        batch.insert_batch(
            &self.tables.by_owner,
            [(OwnedObjectOwnerKey::from(&entry), entry)],
        )?;
        Ok(())
    }
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

        let mut entries = BTreeMap::new();
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
                let entry = OwnedObjectEntry::from_object(&object).with_context(|| {
                    format!(
                        "seeded object {} is not an address-owned Move object",
                        seed_entry.object_ref.0,
                    )
                })?;
                if entry.object_ref != seed_entry.object_ref {
                    bail!(
                        "seeded object {} metadata does not match fetched object at fork checkpoint {}",
                        seed_entry.object_ref.0,
                        self.forked_at_checkpoint(),
                    );
                }

                self.local().write_object(&object)?;
                entries.insert(entry.object_ref.0, entry);
            }
        }

        let entries: Vec<_> = entries.into_values().collect();
        self.owned_index()
            .write_owned_object_entries_for_initialization(&entries)
    }

    /// Get owned objects for an address, optionally filtered by object type and paginated with a
    /// cursor.
    pub(crate) fn get_owned_objects(
        &self,
        owner: SuiAddress,
        object_type: Option<StructTag>,
        cursor: Option<OwnedObjectInfo>,
    ) -> StorageResult<Vec<OwnedObjectInfo>> {
        self.get_owned_object_infos(owner, object_type, cursor)
    }

    /// Initialize the owned-object index when needed, then read complete indexed RPC metadata.
    pub(crate) fn get_owned_object_infos(
        &self,
        owner: SuiAddress,
        object_type: Option<StructTag>,
        cursor: Option<OwnedObjectInfo>,
    ) -> StorageResult<Vec<OwnedObjectInfo>> {
        self.ensure_owned_object_index_initialized()
            .map_err(|e| StorageError::custom(e.to_string()))?;
        let cursor_object_id = cursor.map(|cursor| cursor.object_id);
        let entries = self
            .owned_index()
            .get_owned_object_entries_for_owner(owner, cursor_object_id)
            .map_err(|e| StorageError::custom(e.to_string()))?;

        Ok(entries
            .into_iter()
            .filter(|entry| {
                object_type
                    .as_ref()
                    .is_none_or(|filter| struct_tag_filter_matches(filter, &entry.object_type))
            })
            .map(|entry| OwnedObjectInfo {
                owner: entry.owner,
                object_type: entry.object_type,
                balance: entry.balance,
                object_id: entry.object_ref.0,
                version: entry.object_ref.1,
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "tests/owned_object_index.rs"]
mod tests;
