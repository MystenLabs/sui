// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::PackageStore,
    static_programmable_transactions::linkage::resolution::{
        ResolutionTable, VersionConstraint, add_and_unify, get_package,
    },
};
use move_binary_format::binary_config::BinaryConfig;
use sui_types::{
    MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID, SUI_SYSTEM_PACKAGE_ID, base_types::ObjectID,
    error::ExecutionError,
};

/// These are the set of native packages in Sui -- importantly they can be used implicitly by
/// different parts of the system and are not required to be explicitly imported always.
/// Additionally, there is no versioning concerns around these as they are "stable" for a given
/// epoch, and are the special packages that are always available, and updated in-place.
const NATIVE_PACKAGE_IDS: &[ObjectID] = &[
    SUI_FRAMEWORK_PACKAGE_ID,
    SUI_SYSTEM_PACKAGE_ID,
    MOVE_STDLIB_PACKAGE_ID,
];

/// Metadata and shared operations for the PTB linkage analysis.
#[derive(Debug)]
pub struct ResolutionConfig {
    /// Config to use for the linkage analysis.
    pub linkage_config: LinkageConfig,
    /// Config to use for the binary analysis (needed for deserialization to determine if a
    /// function is a non-public entry function).
    pub binary_config: BinaryConfig,
}

/// Configuration for the linkage analysis.
#[derive(Debug)]
pub struct LinkageConfig {
    /// Whether system packages should always be included as a member in the generated linkage.
    /// This is almost always true except for system transactions and genesis transactions.
    pub always_include_system_packages: bool,
}

impl ResolutionConfig {
    pub fn new(linkage_config: LinkageConfig, binary_config: BinaryConfig) -> Self {
        Self {
            linkage_config,
            binary_config,
        }
    }
}

impl LinkageConfig {
    pub fn legacy_linkage_settings(always_include_system_packages: bool) -> Self {
        Self {
            always_include_system_packages,
        }
    }

    pub(crate) fn resolution_table_with_native_packages(
        &self,
        store: &dyn PackageStore,
    ) -> Result<ResolutionTable, ExecutionError> {
        let mut resolution_table = ResolutionTable::empty();
        if self.always_include_system_packages {
            for id in NATIVE_PACKAGE_IDS {
                #[cfg(debug_assertions)]
                {
                    let package = get_package(id, store)?;
                    debug_assert_eq!(package.id(), *id);
                    debug_assert_eq!(package.original_package_id(), *id);
                }
                add_and_unify(id, store, &mut resolution_table, VersionConstraint::exact)?;
            }
        }

        Ok(resolution_table)
    }
}
