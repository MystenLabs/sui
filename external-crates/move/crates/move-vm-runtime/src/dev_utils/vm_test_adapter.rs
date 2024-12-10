// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution::vm::MoveVM,
    natives::extensions::NativeContextExtensions,
    runtime::MoveRuntime,
    shared::{
        linkage_context::LinkageContext,
        types::{PackageStorageId, RuntimePackageId},
    },
};
use move_binary_format::{errors::VMResult, file_format::CompiledModule};
use move_core_types::resolver::{MoveResolver, SerializedPackage};
use std::collections::HashMap;

// FIXME(cswords): support gas

/// A VM Test Adaptor holds storage and a VM, and can handle publishing packages and executing
/// functions. Based on its needs, it may also provide ways to generate linkage contexts.
pub trait VMTestAdapter<Storage: MoveResolver + Sync + Send> {
    /// Perform a publication, including package verification and updating the relevant storage in
    /// the test adapter so that it is available for subsequent calls.
    fn publish_package(
        &mut self,
        runtime_id: RuntimePackageId,
        package: SerializedPackage,
    ) -> VMResult<()>;

    /// Generate a VM instance which holds the relevant virtual tables for the provided linkage
    /// context.
    fn make_vm<'extensions>(
        &self,
        linkage_context: LinkageContext,
    ) -> VMResult<MoveVM<'extensions>>;

    /// Generate a VM instance which holds the relevant virtual tables for the provided linkage
    /// context, and set that instance's native extensions to those provided.
    fn make_vm_with_native_extensions<'extensions>(
        &self,
        linkage_context: LinkageContext,
        native_extensions: NativeContextExtensions<'extensions>,
    ) -> VMResult<MoveVM<'extensions>>;

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

    /// Retrieve the linkage context for the given package in `Storage`.
    fn get_linkage_context(&self, package_id: PackageStorageId) -> VMResult<LinkageContext> {
        let pkg = self.get_package_from_store(&package_id)?;
        Ok(LinkageContext::new(
            package_id,
            HashMap::from_iter(pkg.linkage_table),
        ))
    }

    fn get_package_from_store(&self, package_id: &PackageStorageId) -> VMResult<SerializedPackage>;

    /// Get the move runtime associated with the adapter.
    fn runtime(&mut self) -> &mut MoveRuntime;

    /// Get the storage data cache associated with the adapter.
    fn storage(&self) -> &Storage;

    /// Get the storage data cache associated with the adapter.
    fn storage_mut(&mut self) -> &mut Storage;
}
