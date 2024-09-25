// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::on_chain::ast::RuntimePackageId;
use crate::vm::vm_instance::VirtualMachineExecutionInstance;
use crate::{
    cache::linkage_context::LinkageContext, on_chain::ast::PackageStorageId, vm::vm::VirtualMachine,
};

use move_binary_format::errors::VMResult;
use move_binary_format::file_format::CompiledModule;

use move_core_types::resolver::MoveResolver;

// FIXME(cswords): support gas

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

    fn make_vm_instance<'extensions>(
        &mut self,
        linkage_context: LinkageContext,
    ) -> VMResult<VirtualMachineExecutionInstance<'extensions, &Storage>>;

    /// Generate a linkage context for a given runtime ID, storage ID, and list of compiled modules.
    /// This must include all of the transitive dependencies of the provided modules in the linkage
    /// context. This may produce an error if the adapter cannot find the relevant dependencies in
    /// its storage.
    fn generate_linkage_context(
        &self,
        runtime_package_id: RuntimePackageId,
        storage_id: PackageStorageId,
        modules: &[CompiledModule],
    ) -> VMResult<LinkageContext>;

    /// Generate a "default" linkage for an account address. This assumes its publication and
    /// runtime ID are the same, and computes dependencies by retrieving the compiled modules from
    /// `get_compild_modules_from_storage` and handing all of that into `generate_linkage_context`.
    fn generate_default_linkage(&self, package_id: PackageStorageId) -> VMResult<LinkageContext> {
        let modules = self.get_compiled_modules_from_storage(&package_id)?;
        self.generate_linkage_context(package_id, package_id, &modules)
    }

    fn get_compiled_modules_from_storage(
        &self,
        package_id: &PackageStorageId,
    ) -> VMResult<Vec<CompiledModule>>;

    /// Get the virtual machine associated with the test adapter.
    fn vm(&mut self) -> &mut VirtualMachine;

    /// Get the storage data cache associated with the test adapter.
    fn storage(&self) -> &Storage;

    /// Get the storage data cache associated with the test adapter.
    fn storage_mut(&mut self) -> &mut Storage;
}
