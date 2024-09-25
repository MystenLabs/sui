// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::linkage_context::LinkageContext, on_chain::ast::PackageStorageId, vm::vm::VirtualMachine,
};

use move_binary_format::errors::VMResult;
use move_binary_format::file_format::CompiledModule;

use move_core_types::language_storage::ModuleId;
use move_core_types::{identifier::Identifier, resolver::MoveResolver};

/// A VM Test Adaptor holds storage and a VM, and can handle publishing packages and executing
/// functions. Based on its needs, it may also provide ways to generate linkage contexts.
pub trait VMTestAdapter<Storage: MoveResolver> {
    /// Perform a publication, including package verification and updating the relevant storage in
    /// the test adapter so that it is available for subsequent calls.
    fn publish_package(
        &mut self,
        linkage_context: LinkageContext,
        storage_id: PackageStorageId,
        modules: Vec<CompiledModule>,
    ) -> VMResult<()>;

    /// Execute a move function in the provided linkage context.
    /// TODO: type arguments, normal arguments.
    fn execute_function(
        &mut self,
        linkage_context: LinkageContext,
        _module: ModuleId,
        _function: Identifier,
    );

    /// Get the virtual machine associated with the test adapter.
    fn vm(&mut self) -> &mut VirtualMachine;

    /// Get the storage data cache associated with the test adapter.
    fn storage(&self) -> &Storage;

    /// Get the storage data cache associated with the test adapter.
    fn storage_mut(&mut self) -> &mut Storage;
}
