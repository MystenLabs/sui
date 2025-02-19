// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::{PartialVMResult, VMResult};
use move_core_types::{
    account_address::AccountAddress, identifier::IdentStr, language_storage::ModuleId,
    resolver::ModuleResolver,
};
use std::fmt::Debug;

/// Provide an implementation for bytecodes related to data with a given data store.
///
/// The `DataStore` is a generic concept that includes both data and events.
/// A default implementation of the `DataStore` is `TransactionDataCache` which provides
/// an in memory cache for a given transaction and the atomic transactional changes
/// proper of a script execution (transaction).
pub trait DataStore {
    /// The link context identifies the mapping from runtime `ModuleId`s to the `ModuleId`s in
    /// storage that they are loaded from as returned by `relocate`.  Implementors of `DataStore`
    /// are required to keep the link context stable for the duration of
    /// `Interpreter::execute_main`.
    fn link_context(&self) -> AccountAddress;

    /// Translate the runtime `module_id` to the on-chain `ModuleId` that it should be loaded from.
    fn relocate(&self, module_id: &ModuleId) -> PartialVMResult<ModuleId>;

    /// Translate the runtime fully-qualified struct name to the on-chain `ModuleId` that originally
    /// defined that type.
    fn defining_module(
        &self,
        module_id: &ModuleId,
        struct_: &IdentStr,
    ) -> PartialVMResult<ModuleId>;

    /// Get the serialized format of a `CompiledModule` given a `ModuleId`.
    fn load_module(&self, module_id: &ModuleId) -> VMResult<Vec<u8>>;

    /// Publish a module.
    fn publish_module(&mut self, module_id: &ModuleId, blob: Vec<u8>) -> VMResult<()>;
}

/// A persistent storage implementation that can resolve both resources and modules
pub trait MoveResolver:
    LinkageResolver<Error = Self::Err> + ModuleResolver<Error = Self::Err>
{
    type Err: Debug;
}

impl<E: Debug, T: LinkageResolver<Error = E> + ModuleResolver<Error = E> + ?Sized> MoveResolver
    for T
{
    type Err = E;
}

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

impl<T: LinkageResolver + ?Sized> LinkageResolver for &T {
    type Error = T::Error;

    fn link_context(&self) -> AccountAddress {
        (**self).link_context()
    }

    fn relocate(&self, module_id: &ModuleId) -> Result<ModuleId, Self::Error> {
        (**self).relocate(module_id)
    }

    fn defining_module(
        &self,
        module_id: &ModuleId,
        struct_: &IdentStr,
    ) -> Result<ModuleId, Self::Error> {
        (**self).defining_module(module_id, struct_)
    }
}
