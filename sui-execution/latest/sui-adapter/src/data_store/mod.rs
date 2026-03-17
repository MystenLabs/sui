// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod cached_package_store;
pub mod transaction_package_store;

use move_core_types::identifier::IdentStr;
use move_vm_runtime::validation::verification::ast::Package as VerifiedPackage;
use std::sync::Arc;
use sui_types::{base_types::ObjectID, error::SuiResult};

// A unifying trait that allows us to resolve a type to its defining ID as well as load packages.
// Some move packages that can be "loaded" via this may not be objects just yet (e.g., if
// they were published in the current transaction). Note that this needs to load `MovePackage`s and
// not `MovePackageObject`s because of this.
pub trait PackageStore {
    fn get_package(&self, id: &ObjectID) -> SuiResult<Option<Arc<VerifiedPackage>>>;

    fn resolve_type_to_defining_id(
        &self,
        module_address: ObjectID,
        module_name: &IdentStr,
        type_name: &IdentStr,
    ) -> SuiResult<Option<ObjectID>>;
}
