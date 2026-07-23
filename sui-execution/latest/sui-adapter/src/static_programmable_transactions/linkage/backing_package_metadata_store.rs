// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{CompiledModule, binary_config::BinaryConfig};
use move_core_types::identifier::IdentStr;
use move_vm_runtime::shared::types::{OriginalId, VersionId};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    error::{SuiError, SuiResult},
    move_package::MovePackage,
    object::OBJECT_START_VERSION,
    storage::BackingPackageStore,
};

use crate::data_store::{PackageMetadata, PackageStore};

/// A package store for linkage analysis that does not require a Move VM.
///
/// Package objects are cached after the first lookup. Modules are deserialized lazily and cached
/// separately, since most linkage operations only need package metadata.
pub(crate) struct BackingPackageMetadataStore<'a> {
    backing: &'a dyn BackingPackageStore,
    binary_config: BinaryConfig,
    packages: std::cell::RefCell<BTreeMap<ObjectID, Rc<BackingPackageMetadata>>>,
}

impl<'a> BackingPackageMetadataStore<'a> {
    pub(crate) fn new(
        protocol_config: &ProtocolConfig,
        backing: &'a dyn BackingPackageStore,
    ) -> Self {
        Self {
            backing,
            binary_config: protocol_config.binary_config(None).clone(),
            packages: std::cell::RefCell::new(BTreeMap::new()),
        }
    }

    pub(crate) fn get_package(
        &self,
        package_id: &ObjectID,
    ) -> SuiResult<Option<Rc<BackingPackageMetadata>>> {
        if let Some(package) = self.packages.borrow().get(package_id) {
            return Ok(Some(package.clone()));
        }

        let Some(package_object) = self.backing.get_package_object(package_id)? else {
            return Ok(None);
        };
        let package = Rc::new(BackingPackageMetadata::new(
            package_object.move_package().clone(),
            self.binary_config.clone(),
        )?);
        self.packages
            .borrow_mut()
            .insert(*package_id, package.clone());
        Ok(Some(package))
    }

    pub(crate) fn deserialize_modules(
        &self,
        serialized_modules: &[Vec<u8>],
    ) -> SuiResult<Vec<CompiledModule>> {
        serialized_modules
            .iter()
            .map(|bytes| {
                CompiledModule::deserialize_with_config(bytes, &self.binary_config)
                    .map_err(|error| error.to_string().into())
            })
            .collect()
    }
}

impl<'a> PackageStore for BackingPackageMetadataStore<'a> {
    type Package = Rc<BackingPackageMetadata>;

    fn get_package(&self, id: &ObjectID) -> SuiResult<Option<Self::Package>> {
        self.get_package(id)
    }

    fn resolve_type_to_defining_id(
        &self,
        module_address: ObjectID,
        module_name: &IdentStr,
        type_name: &IdentStr,
    ) -> SuiResult<Option<ObjectID>> {
        let Some(package) = self.get_package(&module_address)? else {
            return Ok(None);
        };
        Ok(package
            .type_origins()
            .get(&(module_name.to_string(), type_name.to_string()))
            .copied())
    }
}

impl PackageMetadata for Rc<BackingPackageMetadata> {
    fn version(&self) -> u64 {
        BackingPackageMetadata::version(self.as_ref()).into()
    }

    fn version_id(&self) -> ObjectID {
        BackingPackageMetadata::id(self.as_ref())
    }

    fn original_id(&self) -> ObjectID {
        BackingPackageMetadata::original_id(self.as_ref())
    }

    fn linkage_table(&self) -> BTreeMap<OriginalId, VersionId> {
        BackingPackageMetadata::linkage_table(self.as_ref())
    }
}

pub(crate) struct BackingPackageMetadata {
    package: MovePackage,
    original_id: ObjectID,
    modules: RefCell<Option<Rc<BTreeMap<String, CompiledModule>>>>,
    binary_config: BinaryConfig,
}

impl BackingPackageMetadata {
    fn new(package: MovePackage, binary_config: BinaryConfig) -> SuiResult<Self> {
        let original_id = if package.version() == OBJECT_START_VERSION {
            package.id()
        } else {
            let bytes = package
                .serialized_module_map()
                .values()
                .next()
                .ok_or_else(|| SuiError::from("Move package has no modules"))?;
            let module = CompiledModule::deserialize_with_config(bytes, &binary_config)
                .map_err(|error| SuiError::from(error.to_string()))?;
            (*module.address()).into()
        };
        Ok(Self {
            package,
            original_id,
            modules: RefCell::new(None),
            binary_config,
        })
    }

    pub(crate) fn id(&self) -> ObjectID {
        self.package.id()
    }

    pub(crate) fn version(&self) -> SequenceNumber {
        self.package.version()
    }

    pub(crate) fn original_id(&self) -> ObjectID {
        self.original_id
    }

    pub(crate) fn linkage_table(&self) -> BTreeMap<OriginalId, VersionId> {
        self.package
            .linkage_table()
            .iter()
            .map(|(original_id, info)| ((*original_id).into(), info.upgraded_id.into()))
            .collect()
    }

    pub(crate) fn type_origins(&self) -> BTreeMap<(String, String), ObjectID> {
        self.package.type_origin_map()
    }

    pub(crate) fn modules(&self) -> SuiResult<Rc<BTreeMap<String, CompiledModule>>> {
        if let Some(modules) = self.modules.borrow().as_ref() {
            return Ok(modules.clone());
        }

        let modules = self
            .package
            .serialized_module_map()
            .iter()
            .map(|(name, bytes)| {
                CompiledModule::deserialize_with_config(bytes, &self.binary_config)
                    .map(|module| (name.clone(), module))
                    .map_err(|error| error.to_string().into())
            })
            .collect::<SuiResult<BTreeMap<_, _>>>()?;
        let modules = Rc::new(modules);
        *self.modules.borrow_mut() = Some(modules.clone());
        Ok(modules)
    }
}
