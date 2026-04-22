// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, rc::Rc, sync::Arc};

use crate::{
    data_store::PackageStore,
    static_programmable_transactions::linkage::resolution::{
        ResolutionTable, VersionConstraint, add_and_unify,
    },
};
use move_binary_format::binary_config::BinaryConfig;
use move_vm_runtime::{
    shared::types::{OriginalId, VersionId},
    validation::verification::ast::Package as VerifiedPackage,
};
use sui_protocol_config::Amendments;
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
pub struct ResolutionConfig_ {
    /// Config to use for the linkage analysis.
    linkage_config: LinkageConfig,
    /// Config to use for the binary analysis (needed for deserialization to determine if a
    /// function is a non-public entry function).
    binary_config: BinaryConfig,
}

#[derive(Debug, Clone)]
pub struct ResolutionConfig(Rc<ResolutionConfig_>);

/// Configuration for the linkage analysis.
#[derive(Debug, Clone)]
pub struct LinkageConfig {
    /// Whether system packages should always be included as a member in the generated linkage.
    /// This is almost always true except for system transactions and genesis transactions.
    pub always_include_system_packages: bool,
    /// If special amendments should be included in the generated linkage.
    pub include_special_amendments: Option<Arc<Amendments>>,
}

impl ResolutionConfig {
    pub fn new(linkage_config: LinkageConfig, binary_config: BinaryConfig) -> Self {
        Self(Rc::new(ResolutionConfig_ {
            linkage_config,
            binary_config,
        }))
    }

    pub fn linkage_config(&self) -> &LinkageConfig {
        &self.0.linkage_config
    }

    pub fn binary_config(&self) -> &BinaryConfig {
        &self.0.binary_config
    }

    pub(crate) fn resolution_table_with_native_packages(
        &self,
        store: &dyn PackageStore,
    ) -> Result<ResolutionTable, ExecutionError> {
        let mut resolution_table = ResolutionTable::empty(self.clone());
        if self.0.linkage_config.always_include_system_packages {
            for id in NATIVE_PACKAGE_IDS {
                #[cfg(debug_assertions)]
                {
                    use crate::static_programmable_transactions::linkage::resolution::get_package;
                    let package = get_package(id, store)?;
                    debug_assert_eq!(package.version_id(), **id);
                    debug_assert_eq!(package.original_id(), **id);
                }
                add_and_unify(id, store, &mut resolution_table, VersionConstraint::exact)?;
            }
        }

        Ok(resolution_table)
    }

    pub(crate) fn linkage_table(&self, pkg: &VerifiedPackage) -> BTreeMap<OriginalId, VersionId> {
        let linkage_table = pkg.linkage_table().clone();
        self.linkage_config()
            .apply_linkage_amendments(pkg.version_id(), linkage_table)
    }
}

impl LinkageConfig {
    pub fn new(
        include_special_amendments: Option<Arc<Amendments>>,
        always_include_system_packages: bool,
    ) -> Self {
        Self {
            include_special_amendments,
            always_include_system_packages,
        }
    }

    fn apply_linkage_amendments(
        &self,
        root: VersionId,
        mut linkage: BTreeMap<OriginalId, VersionId>,
    ) -> BTreeMap<OriginalId, VersionId> {
        let Some(amendments) = &self.include_special_amendments else {
            return linkage;
        };

        if let Some(amendments_for_root) = amendments.get(&root) {
            for (orig_id, upgraded_id) in amendments_for_root.iter() {
                // Upgrade linkage. This can either an insert or override.
                linkage.insert(*orig_id, *upgraded_id);
            }
        }
        linkage
    }
}
