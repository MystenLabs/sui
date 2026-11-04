// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Package source for response-local layout resolution.
//!
//! [`RpcStateReader::layout_package_store`] supplies packages from the typed
//! [`RpcStoreSchema`] CFs. RPC rendering adds its overlay and parsed-package cache.
//!
//! [`RpcStateReader`]: sui_types::storage::RpcStateReader

use sui_consistent_store::reader::Reader;
use sui_types::base_types::ObjectID;
use sui_types::error::SuiResult;
use sui_types::storage::BackingPackageStore;
use sui_types::storage::ObjectStore;
use sui_types::storage::PackageObject;

use crate::RpcStoreSchema;

/// [`BackingPackageStore`] backed by the typed
/// [`RpcStoreSchema`] CFs. Looks each package up by its storage
/// id through `get_object` (a reverse scan of `objects`); packages
/// are immutable, so the latest live row IS the entirety of the
/// package's on-chain state.
pub(super) struct PackageStoreOverObjects<'a, R: Reader> {
    pub(super) schema: &'a RpcStoreSchema<R>,
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
    /// trait surface without re-implementing the `objects` reverse
    /// scan.
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
    use sui_types::storage::RpcStateReader;

    use super::*;
    use crate::reader::RpcStoreReader;

    #[tokio::test]
    async fn package_source_returns_none_when_no_chain_id_or_epoch() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        let reader = RpcStoreReader::new(db, Arc::new(schema));

        assert!(
            reader
                .layout_package_store()
                .get_package_object(&ObjectID::new([1u8; 32]))
                .unwrap()
                .is_none()
        );
    }
}
