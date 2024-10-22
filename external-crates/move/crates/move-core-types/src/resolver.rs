// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{account_address::AccountAddress, identifier::Identifier};
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
    pub modules: Vec<Vec<u8>>,
    /// The storage ID of this package. This is a unique identifier for this particular package.
    pub storage_id: AccountAddress,
    /// For each dependency (including transitive dependencies), maps runtime package ID to the
    /// storage ID of the package that is to be used for the linkage rooted at this package.
    pub linkage_table: BTreeMap<AccountAddress, AccountAddress>,
    /// The type origin table for the package. Every type in the package must have an entry in this
    /// table.
    pub type_origin_table: Vec<TypeOrigin>,
}

impl SerializedPackage {
    /// TODO(vm-rewrite): This is a shim to use as we move over to storing metadata and then
    /// reading that back in when loading a package. Remove this when no longer needed.
    pub fn raw_package(modules: Vec<Vec<u8>>, storage_id: AccountAddress) -> Self {
        Self {
            modules,
            storage_id,
            linkage_table: BTreeMap::new(),
            type_origin_table: vec![],
        }
    }

    pub fn empty(stroage_id: AccountAddress) -> Self {
        Self {
            modules: vec![],
            storage_id: stroage_id,
            linkage_table: BTreeMap::new(),
            type_origin_table: vec![],
        }
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
}

/// A persistent storage implementation that can resolve both resources and modules
/// TODO(vm-rewrite): Remove this in favor of using the `ModuleResolver` trait directly.
pub trait MoveResolver: ModuleResolver<Error = Self::Err> {
    type Err: Debug;
}

impl<E: Debug, T: ModuleResolver<Error = E> + ?Sized> MoveResolver for T {
    type Err = E;
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
}
