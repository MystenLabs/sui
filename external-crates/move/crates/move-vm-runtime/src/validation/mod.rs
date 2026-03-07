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
    errors::{Location, VMResult},
    partial_vm_error,
};
use move_core_types::resolver::SerializedPackage;
use move_vm_config::runtime::VMConfig;
use tracing::instrument;

use self::verification::linkage::verify_linkage_and_cyclic_checks;

// -------------------------------------------------------------------------------------------------
// Entry Points
// -------------------------------------------------------------------------------------------------

/// Verify a package for publication, including ensuring it is valid against its dependencies for
/// linkage.
#[instrument(level = "trace", skip_all)]
pub(crate) fn validate_for_publish(
    natives: &NativeFunctions,
    vm_config: &VMConfig,
    original_id: OriginalId,
    package: SerializedPackage,
    dependencies: BTreeMap<VersionId, &verification::ast::Package>,
) -> VMResult<verification::ast::Package> {
    tracing::trace!(
        original_id = %original_id,
        "validating package for publication"
    );
    dbg_println!(
        "doing verification with linkage context {:#?}\nand type origins {:#?}",
        package.linkage_table,
        package.type_origin_table,
    );

    let validated_package = validate_package(natives, vm_config, package)?;

    if validated_package.original_id != original_id {
        return Err(partial_vm_error!(
            UNKNOWN_INVARIANT_VIOLATION_ERROR,
            "Mismatched original package IDs: given '{}', found '{}'",
            original_id,
            validated_package.original_id
        )
        .finish(Location::Package(validated_package.version_id)));
    }

    // Now verify linking on-the-spot to make sure that the current package links correctly in
    // the supplied linking context.
    verify_linkage_and_cyclic_checks_for_publication(&validated_package, &dependencies)?;
    Ok(validated_package)
}

/// Verify a set of packages for VM execution, ensuring linkage is correct and there are no cycles.
#[instrument(level = "trace", skip_all, ret)]
pub(crate) fn validate_for_vm_execution(
    packages: BTreeMap<VersionId, &verification::ast::Package>,
) -> VMResult<()> {
    verify_linkage_and_cyclic_checks(&packages)
}

/// Deserialize and internally verify the package.
/// NB: Does not perform cyclic dependency verification or linkage checking.
#[instrument(level = "trace", skip_all)]
pub fn validate_package(
    natives: &NativeFunctions,
    vm_config: &VMConfig,
    package: SerializedPackage,
) -> VMResult<verification::ast::Package> {
    tracing::trace!(
        version_id = %package.version_id,
        original_id = %package.original_id,
        version = %package.version,
        "validating package"
    );
    let pkg = deserialization::translate::package(vm_config, package)?;

    // NB: We don't check for cycles inside of the package just yet since we may need to load
    // further packages.
    let pkg = verification::translate::package(natives, vm_config, pkg)?;
    Ok(pkg)
}
