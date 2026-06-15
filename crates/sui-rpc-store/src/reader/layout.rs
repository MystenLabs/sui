// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Move type-layout resolver.
//!
//! [`RpcStateReader::get_struct_layout_with_overlay`] resolves the
//! [`MoveTypeLayout`] of a [`StructTag`] using a Move type
//! resolver. The validator's perpetual store pulls every piece
//! from its [`AuthorityState`]; this adapter assembles them from
//! the typed [`RpcStoreSchema`] CFs instead:
//!
//! 1. A [`BackingPackageStore`] (here:
//!    [`PackageStoreOverObjects`]) that returns package objects
//!    by id, looking them up through `live_objects` + `objects`.
//!    Packages are immutable, so a single lookup is enough.
//! 2. An overlay wrapping the caller-supplied [`ObjectSet`] on top
//!    of the backing store.
//! 3. The live [`ProtocolConfig`] — sourced from the latest
//!    epoch's [`SuiSystemState`] (protocol version) plus the
//!    chain id recorded by any pipeline's first checkpoint.
//! 4. An [`Executor`] for that protocol config; its
//!    `type_layout_resolver` is what actually performs the
//!    layout build.
//!
//! [`AuthorityState`]: https://docs.rs/sui-core/latest/sui_core/authority/struct.AuthorityState.html
//! [`Executor`]: sui_execution::Executor
//! [`ProtocolConfig`]: sui_protocol_config::ProtocolConfig
//! [`BackingPackageStore`]: sui_types::storage::BackingPackageStore
//! [`RpcStateReader`]: sui_types::storage::RpcStateReader
//! [`SuiSystemState`]: sui_types::sui_system_state::SuiSystemState

use move_core_types::annotated_value::MoveTypeLayout;
use move_core_types::language_storage::StructTag;
use sui_consistent_store::FrameworkSchema;
use sui_consistent_store::reader::Reader;
use sui_protocol_config::ProtocolConfig;
use sui_protocol_config::ProtocolVersion;
use sui_types::base_types::ObjectID;
use sui_types::digests::ChainIdentifier;
use sui_types::digests::CheckpointDigest;
use sui_types::error::SuiResult;
use sui_types::full_checkpoint_content::ObjectSet;
use sui_types::storage::BackingPackageStore;
use sui_types::storage::ObjectStore;
use sui_types::storage::OverlayBackingPackageStore;
use sui_types::storage::PackageObject;
use sui_types::storage::error::Error as StorageError;
use sui_types::storage::error::Result as StorageResult;
use sui_types::sui_system_state::SuiSystemStateTrait;

use crate::RpcStoreSchema;
use crate::reader::RpcStoreReader;

impl<R: Reader + Send + Sync> RpcStoreReader<R> {
    /// Resolve the [`MoveTypeLayout`] for a given [`StructTag`],
    /// optionally seeded with extra objects in `overlay`.
    ///
    /// Returns `Ok(None)` when the store doesn't have enough
    /// context to build an executor (no checkpoints observed yet,
    /// no chain id recorded). Surfaces resolver failures as
    /// [`StorageError::custom`].
    pub fn resolve_struct_layout(
        &self,
        struct_tag: &StructTag,
        overlay: &ObjectSet,
    ) -> StorageResult<Option<MoveTypeLayout>> {
        let Some(protocol_config) = self.live_protocol_config()? else {
            return Ok(None);
        };

        let executor = sui_execution::executor(&protocol_config, /* silent */ true)
            .map_err(StorageError::custom)?;

        let backing = PackageStoreOverObjects {
            schema: self.schema(),
        };
        let overlay_store = OverlayBackingPackageStore::new(overlay, &backing);

        let layout = executor
            .type_layout_resolver(&protocol_config, Box::new(overlay_store))
            .get_annotated_layout(struct_tag)
            .map_err(StorageError::custom)?;
        Ok(Some(layout.into_layout()))
    }

    /// Resolve the live [`ProtocolConfig`], combining the protocol
    /// version recorded in the latest epoch's [`SuiSystemState`]
    /// with the chain id recorded in the framework `__chain_id`
    /// CF. Returns `Ok(None)` when either side is missing —
    /// callers treat the absence as "no layout available" rather
    /// than as an error.
    fn live_protocol_config(&self) -> StorageResult<Option<ProtocolConfig>> {
        let Some(chain) = self.read_chain_identifier()? else {
            return Ok(None);
        };
        let Some(protocol_version) = self.latest_protocol_version()? else {
            return Ok(None);
        };
        Ok(ProtocolConfig::get_for_version_if_supported(
            protocol_version,
            chain.chain(),
        ))
    }

    /// Read the chain identifier from any framework `__chain_id`
    /// row. Every pipeline records the same value on first
    /// contact, so any entry is authoritative.
    fn read_chain_identifier(&self) -> StorageResult<Option<ChainIdentifier>> {
        let framework = FrameworkSchema::new(self.db().clone());
        let first = framework
            .chain_ids
            .iter(..)
            .map_err(StorageError::custom)?
            .next();
        let Some(entry) = first else {
            return Ok(None);
        };
        let (_, chain_id) = entry.map_err(StorageError::custom)?;
        Ok(Some(ChainIdentifier::from(CheckpointDigest::new(
            chain_id.0,
        ))))
    }

    /// Read the protocol version from the latest epoch's
    /// [`SuiSystemState`].
    fn latest_protocol_version(&self) -> StorageResult<Option<ProtocolVersion>> {
        let epochs = &self.schema().epochs;
        let latest = epochs.iter_rev(..).map_err(StorageError::custom)?.next();
        let Some(entry) = latest else {
            return Ok(None);
        };
        let (epoch_id, _) = entry.map_err(StorageError::custom)?;
        let Some(info) = self
            .schema()
            .get_epoch(epoch_id.0)
            .map_err(StorageError::custom)?
        else {
            return Ok(None);
        };
        let Some(state) = info.system_state else {
            return Ok(None);
        };
        Ok(Some(ProtocolVersion::new(state.protocol_version())))
    }
}

/// [`BackingPackageStore`] backed by the typed
/// [`RpcStoreSchema`] CFs. Looks each package up by its storage
/// id through `get_object` (which composes `live_objects` →
/// `objects`); packages are immutable, so the latest live row
/// IS the entirety of the package's on-chain state.
struct PackageStoreOverObjects<'a, R: Reader> {
    schema: &'a RpcStoreSchema<R>,
}

impl<R: Reader> BackingPackageStore for PackageStoreOverObjects<'_, R> {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        let Some(object) = ObjectStore::get_object(&self.schema_object_store(), package_id) else {
            return Ok(None);
        };
        if !object.is_package() {
            return Ok(None);
        }
        Ok(Some(PackageObject::new(object)))
    }
}

impl<R: Reader> PackageStoreOverObjects<'_, R> {
    /// Adapt the schema's inherent `get_object` helper into a
    /// small struct that implements [`ObjectStore`], so we can
    /// route `BackingPackageStore` lookups through the existing
    /// trait surface without re-implementing the `live_objects →
    /// objects` composition.
    fn schema_object_store(&self) -> SchemaObjectStore<'_, R> {
        SchemaObjectStore {
            schema: self.schema,
        }
    }
}

struct SchemaObjectStore<'a, R: Reader> {
    schema: &'a RpcStoreSchema<R>,
}

impl<R: Reader> ObjectStore for SchemaObjectStore<'_, R> {
    fn get_object(&self, object_id: &ObjectID) -> Option<sui_types::object::Object> {
        self.schema.get_object(*object_id).ok().flatten()
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> Option<sui_types::object::Object> {
        self.schema
            .get_object_by_key(*object_id, version)
            .ok()
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_types::full_checkpoint_content::ObjectSet;

    use super::*;
    use crate::reader::RpcStoreReader;

    /// On a fresh store there's no chain id and no epoch row, so
    /// `resolve_struct_layout` falls through to `Ok(None)` rather
    /// than failing or constructing an executor against bogus
    /// state.
    #[test]
    fn resolve_returns_none_when_no_chain_id_or_epoch() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        let reader = RpcStoreReader::new(db, Arc::new(schema));

        let tag = StructTag {
            address: move_core_types::account_address::AccountAddress::new([1u8; 32]),
            module: move_core_types::identifier::Identifier::new("foo").unwrap(),
            name: move_core_types::identifier::Identifier::new("Bar").unwrap(),
            type_params: vec![],
        };
        let overlay = ObjectSet::default();
        assert!(
            reader
                .resolve_struct_layout(&tag, &overlay)
                .unwrap()
                .is_none()
        );
    }
}
