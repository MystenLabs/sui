// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::data_store::cached_package_store::CachedPackageStore;
use crate::data_store::linked_data_store::LinkedDataStore;
use crate::static_programmable_transactions::linkage::analysis::type_linkage;
use move_core_types::annotated_value as A;
use move_core_types::language_storage::StructTag;
use move_vm_runtime::runtime::MoveRuntime;
use sui_types::TypeTag;
use sui_types::base_types::ObjectID;
use sui_types::error::SuiResult;
use sui_types::execution::TypeLayoutStore;
use sui_types::storage::{BackingPackageStore, PackageObject};
use sui_types::{error::SuiError, layout_resolver::LayoutResolver};

/// Retrieve a `MoveStructLayout` from a `Type`.
pub struct TypeLayoutResolver<'state, 'vm> {
    vm: &'vm MoveRuntime,
    resolver: CachedPackageStore<'state>,
}

/// Implements SuiResolver traits by providing null implementations for module and resource
/// resolution and delegating backing package resolution to the trait object.
struct NullSuiResolver<'state>(Box<dyn TypeLayoutStore + 'state>);

impl<'state, 'vm> TypeLayoutResolver<'state, 'vm> {
    pub fn new(vm: &'vm MoveRuntime, state_view: Box<dyn TypeLayoutStore + 'state>) -> Self {
        let resolver = CachedPackageStore::new(Box::new(NullSuiResolver(state_view)));
        Self { vm, resolver }
    }
}

impl LayoutResolver for TypeLayoutResolver<'_, '_> {
    fn get_annotated_layout(
        &mut self,
        struct_tag: &StructTag,
    ) -> Result<A::MoveDatatypeLayout, SuiError> {
        let ids = struct_tag
            .all_addresses()
            .into_iter()
            .map(|a| a.into())
            .collect::<Vec<_>>();
        let tag_linkage = type_linkage(&ids, &self.resolver)?;
        let link_context = tag_linkage.linkage_context();
        let data_store = LinkedDataStore::new(&tag_linkage, &self.resolver);
        let Ok(vm) = self.vm.make_vm(data_store, link_context) else {
            return Err(SuiError::FailObjectLayout {
                st: format!("{}", struct_tag),
            });
        };

        let type_tag = TypeTag::Struct(Box::new(struct_tag.clone()));
        match vm.annotated_type_layout(&type_tag) {
            Ok(A::MoveTypeLayout::Struct(s)) => Ok(A::MoveDatatypeLayout::Struct(s)),
            Ok(A::MoveTypeLayout::Enum(e)) => Ok(A::MoveDatatypeLayout::Enum(e)),
            _ => Err(SuiError::FailObjectLayout {
                st: format!("{}", struct_tag),
            }),
        }
    }
}

impl BackingPackageStore for NullSuiResolver<'_> {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        self.0.get_package_object(package_id)
    }
}
