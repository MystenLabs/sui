// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution::vm::MoveVM,
    natives::extensions::NativeContextExtensions,
    runtime::{telemetry::MoveRuntimeTelemetry, MoveRuntime},
    shared::{
        linkage_context::LinkageContext,
        types::{DefiningTypeId, OriginalId},
    },
    validation::verification::ast as verif_ast,
};
use move_binary_format::{errors::VMResult, file_format::CompiledModule};
use move_core_types::resolver::{ModuleResolver, SerializedPackage};

// FIXME(cswords): support gas

/// A VM Test Adaptor holds storage and a VM, and can handle publishing packages and executing
/// functions. Based on its needs, it may also provide ways to generate linkage contexts.
pub trait VMTestAdapter<Storage: ModuleResolver + Sync + Send> {
    /// Verify a package for publication, and receive a VM that can be used to call functions
    /// inside of it (such as ).
    fn verify_package<'extensions>(
        &mut self,
        original_id: OriginalId,
        package: SerializedPackage,
    ) -> VMResult<(verif_ast::Package, MoveVM<'extensions>)>;

    /// Perform a publication. THIS DOES NOT PERFORM PACKAGE VERIFICATION.
    /// This publishes the package to the adapter so that it is available for subsequent calls.
    fn publish_verified_package(
        &mut self,
        original_id: OriginalId,
        package: verif_ast::Package,
    ) -> VMResult<()>;

    /// Perform package verification and publication.
    /// This publishes the package to the adapter so that it is available for subsequent calls.
    fn publish_package(
        &mut self,
        original_id: OriginalId,
        package: SerializedPackage,
    ) -> VMResult<()> {
        let (verif_pkg, _vm) = self.verify_package(original_id, package)?;
        self.publish_verified_package(original_id, verif_pkg)
    }

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

    /// Retrieve the telmetry report for the execution runtime
    fn get_telemetry_report(&self) -> MoveRuntimeTelemetry;

    /// Generate a linkage context for a given version ID, original ID, and list of compiled modules.
    /// This must include all of the transitive dependencies of the provided modules in the linkage
    /// context. This may produce an error if the adapter cannot find the relevant dependencies in
    /// its storage.
    fn generate_linkage_context(
        &self,
        original_id: OriginalId,
        version_id: DefiningTypeId,
        modules: &[CompiledModule],
    ) -> VMResult<LinkageContext>;

    /// Retrieve the linkage context for the given package in `Storage`.
    fn get_linkage_context(&self, version_id: DefiningTypeId) -> VMResult<LinkageContext> {
        let pkg = self.get_package_from_store(&version_id)?;
        Ok(LinkageContext::new(pkg.linkage_table))
    }

    fn get_package_from_store(&self, version_id: &DefiningTypeId) -> VMResult<SerializedPackage>;

    /// Get the move runtime associated with the adapter.
    fn runtime(&mut self) -> &mut MoveRuntime;

    /// Get the storage data cache associated with the adapter.
    fn storage(&self) -> &Storage;

    /// Get the storage data cache associated with the adapter.
    fn storage_mut(&mut self) -> &mut Storage;
}
