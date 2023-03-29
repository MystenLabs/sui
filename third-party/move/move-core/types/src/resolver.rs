// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    account_address::AccountAddress,
    identifier::IdentStr,
    language_storage::{ModuleId, StructTag},
};
use std::fmt::Debug;

/// Traits for resolving Move modules and resources from persistent storage

/// An execution context that remaps the modules referred to at runtime according to a linkage
/// table, allowing the same module in storage to be run against different dependencies.
///
/// Default implementation does no re-linking (Module IDs are unchanged by relocation and the
/// link context is a constant value).
pub trait LinkageResolver {
    type Error: Debug;

    /// The link context identifies the mapping from runtime `ModuleId`s to the `ModuleId`s in
    /// storage that they are loaded from as returned by `relocate`.
    fn link_context(&self) -> AccountAddress {
        AccountAddress::ZERO
    }

    /// Translate the runtime `module_id` to the on-chain `ModuleId` that it should be loaded from.
    fn relocate(&self, module_id: &ModuleId) -> Result<ModuleId, Self::Error> {
        Ok(module_id.clone())
    }

    /// Translate the runtime fully-qualified struct name to the on-chain `ModuleId` that originally
    /// defined that type.
    fn defining_module(
        &self,
        module_id: &ModuleId,
        _struct: &IdentStr,
    ) -> Result<ModuleId, Self::Error> {
        Ok(module_id.clone())
    }
}

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

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error>;
}

/// A persistent storage backend that can resolve resources by address + type
/// Storage backends should return
///   - Ok(Some(..)) if the data exists
///   - Ok(None)     if the data does not exist
///   - Err(..)      only when something really wrong happens, for example
///                    - invariants are broken and observable from the storage side
///                      (this is not currently possible as ModuleId and StructTag
///                       are always structurally valid)
///                    - storage encounters internal error
pub trait ResourceResolver {
    type Error: Debug;

    fn get_resource(
        &self,
        address: &AccountAddress,
        typ: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error>;
}

/// A persistent storage implementation that can resolve both resources and modules
pub trait MoveResolver:
    LinkageResolver<Error = Self::Err>
    + ModuleResolver<Error = Self::Err>
    + ResourceResolver<Error = Self::Err>
{
    type Err: Debug;
}

impl<
        E: Debug,
        T: LinkageResolver<Error = E>
            + ModuleResolver<Error = E>
            + ResourceResolver<Error = E>
            + ?Sized,
    > MoveResolver for T
{
    type Err = E;
}

impl<T: ResourceResolver + ?Sized> ResourceResolver for &T {
    type Error = T::Error;

    fn get_resource(
        &self,
        address: &AccountAddress,
        tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        (**self).get_resource(address, tag)
    }
}

impl<T: ModuleResolver + ?Sized> ModuleResolver for &T {
    type Error = T::Error;
    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        (**self).get_module(module_id)
    }
}
