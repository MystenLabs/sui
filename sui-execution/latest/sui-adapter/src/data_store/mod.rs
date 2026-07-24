// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod cached_package_store;
pub mod transaction_package_store;

use move_core_types::identifier::IdentStr;
use move_vm_runtime::{
    shared::types::{OriginalId, VersionId},
    validation::verification::ast::Package as VerifiedPackage,
};
use std::{collections::BTreeMap, sync::Arc};
use sui_types::{base_types::ObjectID, error::SuiResult};

/// The VM-independent package metadata required for linkage analysis.
pub trait PackageMetadata {
    fn version(&self) -> u64;
    fn version_id(&self) -> ObjectID;
    fn original_id(&self) -> ObjectID;
    fn linkage_table(&self) -> BTreeMap<OriginalId, VersionId>;
}

/// Access to package information needed for linkage analysis.
pub trait PackageStore {
    type Package: PackageMetadata;

    fn get_package(&self, id: &ObjectID) -> SuiResult<Option<Self::Package>>;

    fn resolve_type_to_defining_id(
        &self,
        module_address: ObjectID,
        module_name: &IdentStr,
        type_name: &IdentStr,
    ) -> SuiResult<Option<ObjectID>>;
}

/// A package store whose loaded packages have been verified by the Move VM.
///
/// Some move packages that can be loaded through this store may not be objects yet (for example,
/// packages published in the current transaction).
pub type VerifiedPackageStore<'a> = dyn PackageStore<Package = Arc<VerifiedPackage>> + 'a;

impl PackageMetadata for Arc<VerifiedPackage> {
    fn version(&self) -> u64 {
        VerifiedPackage::version(self.as_ref())
    }

    fn version_id(&self) -> ObjectID {
        VerifiedPackage::version_id(self.as_ref()).into()
    }

    fn original_id(&self) -> ObjectID {
        VerifiedPackage::original_id(self.as_ref()).into()
    }

    fn linkage_table(&self) -> BTreeMap<OriginalId, VersionId> {
        VerifiedPackage::linkage_table(self.as_ref()).clone()
    }
}
