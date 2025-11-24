// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod cached_package_store;
pub mod legacy;
pub mod linked_data_store;

use move_core_types::identifier::IdentStr;
use std::rc::Rc;
use sui_types::{base_types::ObjectID, error::SuiResult, move_package::MovePackage};

// A unifying trait that allows us to resolve a type to its defining ID as well as load packages.
// Some move packages that can be "loaded" via this may not be objects just yet (e.g., if
// they were published in the current transaction). Note that this needs to load `MovePackage`s and
// not `MovePackageObject`s because of this.
pub trait PackageStore {
    fn get_package(&self, id: &ObjectID) -> SuiResult<Option<Rc<MovePackage>>>;

    fn resolve_type_to_defining_id(
        &self,
        module_address: ObjectID,
        module_name: &IdentStr,
        type_name: &IdentStr,
    ) -> SuiResult<Option<ObjectID>>;
}
