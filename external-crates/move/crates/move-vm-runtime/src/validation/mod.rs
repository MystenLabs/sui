// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod deserialization;
pub mod verification;

use crate::{
    dbg_println,
    natives::functions::NativeFunctions,
    shared::types::{OriginalId, VersionId},
    validation::verification::linkage::verify_linkage_and_cyclic_checks_for_publication,
};

use std::collections::BTreeMap;

use move_binary_format::{
    IndexKind,
    errors::{Location, PartialVMError, VMResult, verification_error},
};
use move_core_types::{resolver::SerializedPackage, vm_status::StatusCode};
use move_vm_config::runtime::VMConfig;

use self::verification::linkage::verify_linkage_and_cyclic_checks;

// -------------------------------------------------------------------------------------------------
// Entry Points
// -------------------------------------------------------------------------------------------------

/// Verify a package for publication, including ensuring it is valid against its dependencies for
/// linkage.
pub fn validate_for_publish(
    natives: &NativeFunctions,
    vm_config: &VMConfig,
    original_id: OriginalId,
    package: SerializedPackage,
    dependencies: BTreeMap<VersionId, &verification::ast::Package>,
) -> VMResult<verification::ast::Package> {
    dbg_println!(
        "doing verification with linkage context {:#?}\nand type origins {:#?}",
        package.linkage_table,
        package.type_origin_table,
    );

    let validated_package = validate_package(natives, vm_config, package)?;

    // Make sure all modules' self addresses match the `runtime_package_id`.
    for module in validated_package.as_modules().into_iter() {
        if module.value.address() != &original_id {
            return Err(verification_error(
                StatusCode::MISMATCHED_MODULE_IDS_IN_PACKAGE,
                IndexKind::AddressIdentifier,
                module.value.self_handle_idx().0,
            )
            .finish(Location::Package(validated_package.version_id)));
        }
    }

    // Now verify linking on-the-spot to make sure that the current package links correctly in
    // the supplied linking context.
    verify_linkage_and_cyclic_checks_for_publication(&validated_package, &dependencies)?;
    Ok(validated_package)
}

/// Verify a set of packages for VM execution, ensuring linkage is correct and there are no cycles.
pub fn validate_for_vm_execution(
    packages: BTreeMap<VersionId, &verification::ast::Package>,
) -> VMResult<()> {
    verify_linkage_and_cyclic_checks(&packages)
}

/// Deserialize and internally verify the package.
/// NB: Does not perform cyclic dependency verification or linkage checking.
pub fn validate_package(
    natives: &NativeFunctions,
    vm_config: &VMConfig,
    package: SerializedPackage,
) -> VMResult<verification::ast::Package> {
    let pkg = deserialization::translate::package(vm_config, package)?;
    // Packages must be non-empty
    if pkg.modules.is_empty() {
        return Err(PartialVMError::new(StatusCode::EMPTY_PACKAGE)
            .with_message("Empty packages are not allowed.".to_string())
            .finish(Location::Package(pkg.version_id)));
    }
    // NB: We don't check for cycles inside of the package just yet since we may need to load
    // further packages.
    let pkg = verification::translate::package(natives, vm_config, pkg)?;
    Ok(pkg)
}
