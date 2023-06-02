// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::programmable_transactions::context::new_session_for_linkage;
use crate::programmable_transactions::{
    context::load_type,
    linkage_view::{LinkageInfo, LinkageView},
};
use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::{ModuleId, StructTag, TypeTag};
use move_core_types::resolver::{ModuleResolver, ResourceResolver};
use move_core_types::value::{MoveStructLayout, MoveTypeLayout};
use move_vm_runtime::{move_vm::MoveVM, session::Session};
use sui_types::base_types::ObjectID;
use sui_types::error::SuiResult;
use sui_types::object::Object;
use sui_types::storage::BackingPackageStore;
use sui_types::{
    error::SuiError,
    object::{MoveObject, ObjectFormatOptions},
    type_resolver::LayoutResolver,
};

/// Retrieve a `MoveStructLayout` from a `Type`.
/// Invocation into the `Session` to leverage the `LinkageView` implementation
/// common to the runtime.
pub struct TypeLayoutResolver<
    'state,
    'vm,
    S: BackingPackageStore + ModuleResolver<Error = SuiError>,
> {
    session: Session<'state, 'vm, LinkageView<NullSuiResolver<S>>>,
}

/// Implements SuiResolver traits by providing null implementations for module and resource
/// resolution and delegating backing package resolution to the wrapped type.
struct NullSuiResolver<S: BackingPackageStore + ModuleResolver<Error = SuiError>>(S);

impl<'state, 'vm, S: BackingPackageStore + ModuleResolver<Error = SuiError>>
    TypeLayoutResolver<'state, 'vm, S>
{
    pub fn new(vm: &'vm MoveVM, state_view: S) -> Self {
        let session = new_session_for_linkage(
            vm,
            LinkageView::new(NullSuiResolver(state_view), LinkageInfo::Unset),
        );
        Self { session }
    }
}

impl<'state, 'vm, S: BackingPackageStore + ModuleResolver<Error = SuiError>> LayoutResolver
    for TypeLayoutResolver<'state, 'vm, S>
{
    fn get_layout(
        &mut self,
        object: &MoveObject,
        format: ObjectFormatOptions,
    ) -> Result<MoveStructLayout, SuiError> {
        let struct_tag: StructTag = object.type_().clone().into();
        let type_tag: TypeTag = TypeTag::from(struct_tag.clone());
        let Ok(ty) = load_type(&mut self.session, &type_tag) else {
            return Err(SuiError::FailObjectLayout {
                st: format!("{}", struct_tag),
            });
        };
        let layout = if format.include_types() {
            self.session.type_to_fully_annotated_layout(&ty)
        } else {
            self.session.type_to_type_layout(&ty)
        };
        let Ok(MoveTypeLayout::Struct(layout)) = layout else {
            return Err(SuiError::FailObjectLayout {
                st: format!("{}", struct_tag),
            })
        };
        Ok(layout)
    }
}

impl<S: BackingPackageStore + ModuleResolver<Error = SuiError>> BackingPackageStore
    for NullSuiResolver<S>
{
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        self.0.get_package_object(package_id)
    }
}

impl<S: BackingPackageStore + ModuleResolver<Error = SuiError>> ModuleResolver
    for NullSuiResolver<S>
{
    type Error = SuiError;

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.0.get_module(id)
    }
}

impl<S: BackingPackageStore + ModuleResolver<Error = SuiError>> ResourceResolver
    for NullSuiResolver<S>
{
    type Error = SuiError;

    fn get_resource(
        &self,
        _address: &AccountAddress,
        _typ: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(None)
    }
}
