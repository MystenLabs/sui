// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;

use crate::{account_address::AccountAddress, identifier::Identifier, language_storage::ModuleId};
use std::{collections::BTreeMap, fmt::Debug, sync::Arc};

/// The `IntraPackageName` struct holds the module name and type name of a type within a package.
/// This is used as a key in the `type_origin_table` of a `SerializedPackage` and other package
/// related data structures.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IntraPackageName {
    pub module_name: Identifier,
    pub type_name: Identifier,
}

/// The `SerializedPackage` struct holds the serialized modules of a package, the storage ID of the
/// package, and the linkage table that maps the runtime ID found within the package to their
/// storage IDs.
#[derive(Debug, Clone)]
pub struct SerializedPackage {
    pub modules: BTreeMap<Identifier, Vec<u8>>,
    /// The version ID of this package. This is a unique identifier for this particular package.
    pub version_id: AccountAddress,
    /// The original ID of the package. This is the ID that is used to refer to the package in the
    /// VM, and is constant across all versions of the package.
    pub original_id: AccountAddress,
    /// For each dependency (including transitive dependencies), maps runtime package ID to the
    /// storage ID of the package that is to be used for the linkage rooted at this package.
    ///
    /// NB: The linkage table for a `SerializedPackage` must include the "self" linkage mapping the
    /// current package's runtime ID to its storage ID.
    pub linkage_table: BTreeMap<AccountAddress, AccountAddress>,
    /// The type origin table for the package. Every type in the package must have an entry in this
    /// table.
    /// This is a mapping of (module name, type name) to the defining ID of the type -- this is the
    /// version ID of the package where the type was first defined.
    pub type_origin_table: IndexMap<IntraPackageName, AccountAddress>,
    /// The version number of this package. This is a monotonically increasing number that is used
    /// to track the version of the package. This is not used for linkage or identification, but
    /// can be used for other purposes such as displaying the version of the package or determining
    /// relative ordering of versions of the same package.
    pub version: u64,
}

impl SerializedPackage {
    pub fn get_module_by_name(&self, name: &Identifier) -> Option<&Vec<u8>> {
        self.modules.get(name)
    }
}

/// # Traits for resolving Move modules and resources from persistent storage
///
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
