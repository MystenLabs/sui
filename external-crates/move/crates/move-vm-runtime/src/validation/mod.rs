// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod deserialization;
pub mod verification;

use crate::{
    dbg_println,
    natives::functions::NativeFunctions,
    shared::{
        linkage_context::LinkageContext,
        types::{OriginalId, VersionId},
    },
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
    link_context: &LinkageContext,
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

    validate_against_link_context(/* publish */ true, &dependencies, link_context)?;

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
    linkage_context: &LinkageContext,
) -> VMResult<()> {
    validate_against_link_context(/* publish */ false, &packages, linkage_context)?;
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

/// Validates a bijection between the resolved `packages` and the `link_context`'s linkage table.
///
/// The linkage table maps `OriginalId -> VersionId`, and `LinkageContext::new` already enforces
/// that version IDs are unique (i.e., the map is injective). This function checks two things:
///
/// 1. **Cardinality**: The number of resolved packages equals the number of linkage table entries.
///    During publication, the to-be-published package is not yet in `packages` but has an entry in
///    the linkage table, so we account for that with `+1`.
///
/// 2. **Mapping consistency**: For every resolved package, the linkage table maps its `original_id`
///    to its `version_id`.
///
/// Together with the injectivity guaranteed by `LinkageContext::new`, these two checks establish a
/// bijection: every linkage table entry corresponds to exactly one resolved package and vice versa.
/// This ensures no packages are missing from the resolved set and no extraneous entries exist in the
/// linkage table.
fn validate_against_link_context(
    publish: bool,
    packages: &BTreeMap<VersionId, &verification::ast::Package>,
    link_context: &LinkageContext,
) -> VMResult<()> {
    let expected_len = if publish {
        packages.len().saturating_add(1)
    } else {
        packages.len()
    };
    if expected_len != link_context.linkage_table().len() {
        return Err(partial_vm_error!(
            UNKNOWN_INVARIANT_VIOLATION_ERROR,
            "Linkage context contains {} entries, but {} were expected based on resolved packages",
            link_context.linkage_table().len(),
            expected_len,
        )
        .finish(Location::Undefined));
    }

    for (version_id, pkg) in packages {
        if link_context.linkage_table().get(&pkg.original_id) != Some(version_id) {
            return Err(partial_vm_error!(
                UNKNOWN_INVARIANT_VIOLATION_ERROR,
                "Linkage context does not match package store: linkage context maps original ID '{}' \
                to version ID '{}', but package store has version ID '{:?}' for that original ID",
                pkg.original_id,
                version_id,
                link_context.linkage_table().get(&pkg.original_id)
            )
            .finish(Location::Package(*version_id)));
        }
    }
    Ok(())
}
