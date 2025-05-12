// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod cached_package_store;
pub mod legacy;
pub mod linked_data_store;

use crate::{
    data_store::linked_data_store::LinkedDataStore,
    linkage::{Linkage, analysis::type_linkage},
};
use move_core_types::language_storage::StructTag;
use std::rc::Rc;
use sui_types::{
    base_types::{MoveObjectType, ObjectID},
    error::{ExecutionError, SuiResult},
    move_package::MovePackage,
    storage::BackingPackageStore,
};

// A unifying trait that allows us to load move packages that may not be objects just yet (e.g., if
// they were published in the current transaction). Note that this needs to load `MovePackage`s and
// not `MovePackageObject`s.
pub trait PackageStore {
    fn get_package(&self, id: &ObjectID) -> SuiResult<Option<Rc<MovePackage>>>;
}

/// A trait that allows us to resolve a type to its defining ID as well as load packages.
/// TODO: Examine rolling this into `PackageStore` in the near future as we incorporate this into
/// the new PTB runtime.
pub trait ResolvablePackageStore: PackageStore {
    // TODO: Remove this once we start using Rust 1.86
    fn as_package_store(&self) -> &dyn PackageStore;

    fn resolve_type_to_defining_id(
        &self,
        module_address: ObjectID,
        module_name: String,
        type_name: String,
    ) -> SuiResult<Option<ObjectID>>;
}

impl<T: BackingPackageStore> PackageStore for T {
    fn get_package(&self, id: &ObjectID) -> SuiResult<Option<Rc<MovePackage>>> {
        Ok(self
            .get_package_object(id)?
            .map(|x| Rc::new(x.move_package().clone())))
    }
}

/// Create a new `LinkedDataStore` for the given `Linkage`.
pub fn linked_data_store_for_linkage<'b>(
    store: &'b dyn ResolvablePackageStore,
    linkage: &'b Linkage,
) -> LinkedDataStore<'b> {
    LinkedDataStore::new(linkage, store)
}

/// Compute the `Linkage` for a `MoveObjectType`. All `MoveObjectType`s are expected to be
/// defining-id based.
pub fn linkage_for_object_type(
    store: &dyn ResolvablePackageStore,
    object_type: MoveObjectType,
) -> Result<Linkage, ExecutionError> {
    linkage_for_struct_tag(store, &StructTag::from(object_type))
}

/// Compute the `Linkage` for a `StructTag`. All `StructTag`s are expected to be
/// defining-id based.
pub fn linkage_for_struct_tag(
    store: &dyn ResolvablePackageStore,
    struct_tag: &StructTag,
) -> Result<Linkage, ExecutionError> {
    let link_context = struct_tag.address;
    let ids: Vec<_> = struct_tag
        .all_addresses()
        .into_iter()
        .map(ObjectID::from)
        .collect();
    let resolved_linkage = Rc::new(type_linkage(ids.as_slice(), store.as_package_store())?);
    Ok(Linkage {
        link_context,
        resolved_linkage,
    })
}
