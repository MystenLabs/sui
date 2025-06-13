// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod cached_package_store;
pub mod legacy;
pub mod linked_data_store;

use move_core_types::{identifier::IdentStr, language_storage::StructTag};
use std::rc::Rc;
use sui_types::{
    base_types::{MoveObjectType, ObjectID},
    error::{ExecutionError, SuiResult},
    move_package::MovePackage,
};

use crate::static_programmable_transactions::linkage::{
    analysis::type_linkage, resolved_linkage::RootedLinkage,
};

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

/// Compute the `Linkage` for a `MoveObjectType`. All `MoveObjectType`s are expected to be
/// defining-id based.
pub fn linkage_for_object_type(
    store: &dyn PackageStore,
    object_type: MoveObjectType,
) -> Result<RootedLinkage, ExecutionError> {
    linkage_for_struct_tag(store, &StructTag::from(object_type))
}

/// Compute the `Linkage` for a `StructTag`. All `StructTag`s are expected to be
/// defining-id based.
pub fn linkage_for_struct_tag(
    store: &dyn PackageStore,
    struct_tag: &StructTag,
) -> Result<RootedLinkage, ExecutionError> {
    let link_context = struct_tag.address;
    let ids: Vec<_> = struct_tag
        .all_addresses()
        .into_iter()
        .map(ObjectID::from)
        .collect();
    let resolved_linkage = type_linkage(ids.as_slice(), store)?;
    Ok(RootedLinkage::new(link_context, resolved_linkage))
}
