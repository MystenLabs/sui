// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{SequenceNumber, VersionDigest};
use crate::effects::TransactionEvents;
use crate::error::SuiResult;
use crate::execution::DynamicallyLoadedObjectMetadata;
use crate::storage::BackingPackageStore;
use crate::storage::PackageObject;
use crate::{
    base_types::ObjectID,
    object::{Object, Owner},
};
use move_binary_format::binary_config::BinaryConfig;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::language_storage::ModuleId;
use std::collections::BTreeMap;
use std::sync::Arc;

pub type WrittenObjects = BTreeMap<ObjectID, Object>;
pub type ObjectMap = BTreeMap<ObjectID, Object>;
pub type TxCoins = (ObjectMap, WrittenObjects);

#[derive(Debug, Clone)]
pub struct InnerTemporaryStore {
    pub input_objects: ObjectMap,
    pub stream_ended_consensus_objects: BTreeMap<ObjectID, SequenceNumber /* start_version */>,
    pub mutable_inputs: BTreeMap<ObjectID, (VersionDigest, Owner)>,
    // All the written objects' sequence number should have been updated to the lamport version.
    pub written: WrittenObjects,
    pub loaded_runtime_objects: BTreeMap<ObjectID, DynamicallyLoadedObjectMetadata>,
    pub events: TransactionEvents,
    pub binary_config: BinaryConfig,
    pub runtime_packages_loaded_from_db: BTreeMap<ObjectID, PackageObject>,
    pub lamport_version: SequenceNumber,
}

pub struct TemporaryModuleResolver<'a, R> {
    temp_store: &'a InnerTemporaryStore,
    fallback: R,
}

impl<'a, R> TemporaryModuleResolver<'a, R> {
    pub fn new(temp_store: &'a InnerTemporaryStore, fallback: R) -> Self {
        Self {
            temp_store,
            fallback,
        }
    }
}

impl<R> GetModule for TemporaryModuleResolver<'_, R>
where
    R: GetModule<Item = Arc<CompiledModule>, Error = anyhow::Error>,
{
    type Error = anyhow::Error;
    type Item = Arc<CompiledModule>;

    fn get_module_by_id(&self, id: &ModuleId) -> anyhow::Result<Option<Self::Item>, Self::Error> {
        let obj = self.temp_store.written.get(&ObjectID::from(*id.address()));
        if let Some(o) = obj {
            if let Some(p) = o.data.try_as_package() {
                return Ok(Some(Arc::new(p.deserialize_module(
                    &id.name().into(),
                    &self.temp_store.binary_config,
                )?)));
            }
        }
        self.fallback.get_module_by_id(id)
    }
}

impl BackingPackageStore for InnerTemporaryStore {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        Ok(self
            .written
            .get(package_id)
            .cloned()
            .map(PackageObject::new))
    }
}

pub struct PackageStoreWithFallback<P, F> {
    primary: P,
    fallback: F,
}

impl<P, F> PackageStoreWithFallback<P, F> {
    pub fn new(primary: P, fallback: F) -> Self {
        Self { primary, fallback }
    }
}

impl<P, F> BackingPackageStore for PackageStoreWithFallback<P, F>
where
    P: BackingPackageStore,
    F: BackingPackageStore,
{
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        if let Some(package) = self.primary.get_package_object(package_id)? {
            Ok(Some(package))
        } else {
            self.fallback.get_package_object(package_id)
        }
    }
}
