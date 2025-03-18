// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::execution_value::SuiResolver;
use crate::linkage_resolution::UnifiedLinkage;
use crate::programmable_transactions::context::vm_for_struct_tags;
use crate::programmable_transactions::datastore::SuiDataStore;
use move_core_types::annotated_value as A;
use move_core_types::language_storage::StructTag;
use move_vm_runtime::runtime::MoveRuntime;
use sui_types::base_types::ObjectID;
use sui_types::error::SuiResult;
use sui_types::execution::TypeLayoutStore;
use sui_types::storage::{BackingPackageStore, PackageObject};
use sui_types::TypeTag;
use sui_types::{error::SuiError, layout_resolver::LayoutResolver};

/// Retrieve a `MoveStructLayout` from a `Type`.
pub struct TypeLayoutResolver<'state, 'vm> {
    vm: &'vm MoveRuntime,
    resolver: NullSuiResolver<'state>,
    // Doesn't matter what we have here as long as it implements the `LinkageResolver` trait.
    linkage_resolver: UnifiedLinkage,
}

/// Implements SuiResolver traits by providing null implementations for module and resource
/// resolution and delegating backing package resolution to the trait object.
struct NullSuiResolver<'state>(Box<dyn TypeLayoutStore + 'state>);

impl<'state, 'vm> TypeLayoutResolver<'state, 'vm> {
    pub fn new(vm: &'vm MoveRuntime, state_view: Box<dyn TypeLayoutStore + 'state>) -> Self {
        let resolver = NullSuiResolver(state_view);
        // Since we do not include system packages by default, we can set this to false as no
        // loading will be done when creating the linkage resolver.
        let always_include_system_packages = false;
        let Some(linkage_resolver) = UnifiedLinkage::new(
            always_include_system_packages,
            vm.vm_config().binary_config.clone(),
            &resolver,
        )
        .ok() else {
            unreachable!()
        };
        Self {
            vm,
            resolver,
            linkage_resolver,
        }
    }
}

impl LayoutResolver for TypeLayoutResolver<'_, '_> {
    fn get_annotated_layout(
        &mut self,
        struct_tag: &StructTag,
    ) -> Result<A::MoveDatatypeLayout, SuiError> {
        let data_store = SuiDataStore::new(self.resolver.0.as_backing_package_store(), &[]);
        let Ok(vm) = vm_for_struct_tags(
            &mut self.linkage_resolver,
            self.vm,
            [struct_tag],
            &data_store,
        ) else {
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
