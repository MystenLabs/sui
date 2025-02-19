// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{account_address::AccountAddress, identifier::Identifier, language_storage::ModuleId};
use std::{collections::BTreeMap, fmt::Debug, sync::Arc};

/// The `TypeOrigin` struct holds the first storage ID that the type `module_name::type_name` in
/// the current package was first defined in.
/// The `origin_id` provides:
/// 1. A stable and unique representation of the fully-qualified type; and
/// 2. The ability to fetch the package at the given origin ID and no that the type will exist in
///    that package.
#[derive(Debug, Clone)]
pub struct TypeOrigin {
    /// The module name of the type
    pub module_name: Identifier,
    /// The type name
    pub type_name: Identifier,
    /// The package storaage ID of the package that first defined this type
    pub origin_id: AccountAddress,
}

/// The `SerializedPackage` struct holds the serialized modules of a package, the storage ID of the
/// package, and the linkage table that maps the runtime ID found within the package to their
/// storage IDs.
#[derive(Debug, Clone)]
pub struct SerializedPackage {
    pub modules: BTreeMap<Identifier, Vec<u8>>,
    /// The storage ID of this package. This is a unique identifier for this particular package.
    pub storage_id: AccountAddress,
    /// The runtime ID of the package. This is the ID that is used to refer to the package in the
    /// VM, and is constant across all versions of the package.
    pub runtime_id: AccountAddress,
    /// For each dependency (including transitive dependencies), maps runtime package ID to the
    /// storage ID of the package that is to be used for the linkage rooted at this package.
    ///
    /// NB: The linkage table for a `SerializedPackage` must include the "self" linkage mapping the
    /// current package's runtime ID to its storage ID.
    pub linkage_table: BTreeMap<AccountAddress, AccountAddress>,
    /// The type origin table for the package. Every type in the package must have an entry in this
    /// table.
    pub type_origin_table: Vec<TypeOrigin>,
}

impl SerializedPackage {
    /// TODO(vm-rewrite): This is a shim to use as we move over to storing metadata and then
    /// reading that back in when loading a package. Remove this when no longer needed.
    pub fn raw_package(
        modules: BTreeMap<Identifier, Vec<u8>>,
        runtime_id: AccountAddress,
        storage_id: AccountAddress,
    ) -> Self {
        Self {
            modules,
            runtime_id,
            storage_id,
            linkage_table: BTreeMap::new(),
            type_origin_table: vec![],
        }
    }

    pub fn empty(runtime_id: AccountAddress, storage_id: AccountAddress) -> Self {
        Self {
            modules: BTreeMap::new(),
            storage_id,
            runtime_id,
            linkage_table: BTreeMap::new(),
            type_origin_table: vec![],
        }
    }

    pub fn get_module_by_name(&self, name: &Identifier) -> Option<&Vec<u8>> {
        self.modules.get(name)
    }
}

/// # Traits for resolving Move modules and resources from persistent storage

/// A persistent storage backend that can resolve modules by address + name.
/// Storage backends should return
///   - Ok(Some(..)) if the data exists
///   - Ok(None)     if the data does not exist
///   - Err(..)      only when something really wrong happens, for example
///                    - invariants are broken and observable from the storage side
///                      (this is not currently possible as ModuleId and StructTag
///                       are always structurally valid)
///                    - storage encounters internal error
pub trait ModuleResolver {
    type Error: Debug;

    /// Given a list of storage IDs where the number is statically known, return the `SerializedPackage` for
    /// each ID. A result is returned for every ID requested. `None` if the package did not exist.
    fn get_packages_static<const N: usize>(
        &self,
        ids: [AccountAddress; N],
    ) -> Result<[Option<SerializedPackage>; N], Self::Error>;

    /// Given a list of storage IDs for a package, return the `SerializedPackage` for each ID.
    /// A result is returned for every ID requested. `None` if the package did not exist, and
    /// `Some(..)` if the package was found.
    fn get_packages(
        &self,
        ids: &[AccountAddress],
    ) -> Result<Vec<Option<SerializedPackage>>, Self::Error>;

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        let [package] = self.get_packages_static([*id.address()])?;
        Ok(package.and_then(|p| p.get_module_by_name(&id.name().to_owned()).cloned()))
    }
}

impl<T: ModuleResolver + ?Sized> ModuleResolver for &T {
    type Error = T::Error;
    fn get_packages_static<const N: usize>(
        &self,
        ids: [AccountAddress; N],
    ) -> Result<[Option<SerializedPackage>; N], Self::Error> {
        (**self).get_packages_static(ids)
    }

    fn get_packages(
        &self,
        ids: &[AccountAddress],
    ) -> Result<Vec<Option<SerializedPackage>>, Self::Error> {
        (**self).get_packages(ids)
    }

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        (**self).get_module(id)
    }
}

impl<T: ModuleResolver + ?Sized> ModuleResolver for Arc<T> {
    type Error = T::Error;

    fn get_packages_static<const N: usize>(
        &self,
        ids: [AccountAddress; N],
    ) -> Result<[Option<SerializedPackage>; N], Self::Error> {
        (**self).get_packages_static(ids)
    }

    fn get_packages(
        &self,
        ids: &[AccountAddress],
    ) -> Result<Vec<Option<SerializedPackage>>, Self::Error> {
        (**self).get_packages(ids)
    }

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        (**self).get_module(id)
    }
}
