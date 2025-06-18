// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module defines the shared environment, `Env`, used for the compilation/translation and
//! execution of programmable transactions. While the "context" for each pass might be different,
//! the `Env` provides consistent access to shared components such as the VM or the protocol config.

use crate::{
    data_store::{
        PackageStore, cached_package_store::CachedPackageStore, linked_data_store::LinkedDataStore,
    },
    execution_value::ExecutionState,
    static_programmable_transactions::{
        linkage::{
            analysis::{LinkageAnalysis, type_linkage},
            resolved_linkage::RootedLinkage,
        },
        loading::ast::{
            self as L, Datatype, LoadedFunction, LoadedFunctionInstantiation, Type, Vector,
        },
    },
};
use indexmap::IndexSet;
use move_binary_format::{
    CompiledModule,
    errors::VMError,
    file_format::{AbilitySet, TypeParameterIndex},
};
use move_core_types::{
    account_address::AccountAddress,
    annotated_value,
    language_storage::{ModuleId, StructTag},
    runtime_value::{self, MoveTypeLayout},
};
use move_vm_runtime::move_vm::MoveVM;
use move_vm_types::{data_store::DataStore, loaded_data::runtime_types as vm_runtime_type};
use std::{cell::OnceCell, rc::Rc, sync::Arc};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    Identifier, TypeTag,
    base_types::{ObjectID, TxContextKind},
    error::{ExecutionError, ExecutionErrorKind},
    execution_status::TypeArgumentError,
    gas_coin::GasCoin,
    move_package::{UpgradeCap, UpgradeReceipt, UpgradeTicket},
    object::Object,
    type_input::{StructInput, TypeInput},
};

pub struct Env<'pc, 'vm, 'state, 'linkage> {
    pub protocol_config: &'pc ProtocolConfig,
    pub vm: &'vm MoveVM,
    pub state_view: &'state mut dyn ExecutionState,
    pub linkable_store: &'linkage CachedPackageStore<'state>,
    pub linkage_analysis: &'linkage dyn LinkageAnalysis,
    gas_coin_type: OnceCell<Type>,
    upgrade_ticket_type: OnceCell<Type>,
    upgrade_receipt_type: OnceCell<Type>,
    upgrade_cap_type: OnceCell<Type>,
}

macro_rules! get_or_init_ty {
    ($env:expr, $ident:ident, $tag:expr) => {{
        let env = $env;
        if env.$ident.get().is_none() {
            let tag = $tag;
            let ty = env.load_type_from_struct(&tag)?;
            env.$ident.set(ty.clone()).unwrap();
        }
        Ok(env.$ident.get().unwrap().clone())
    }};
}

impl<'pc, 'vm, 'state, 'linkage> Env<'pc, 'vm, 'state, 'linkage> {
    pub fn new(
        protocol_config: &'pc ProtocolConfig,
        vm: &'vm MoveVM,
        state_view: &'state mut dyn ExecutionState,
        linkable_store: &'linkage CachedPackageStore<'state>,
        linkage_analysis: &'linkage dyn LinkageAnalysis,
    ) -> Self {
        Self {
            protocol_config,
            vm,
            state_view,
            linkable_store,
            linkage_analysis,
            gas_coin_type: OnceCell::new(),
            upgrade_ticket_type: OnceCell::new(),
            upgrade_receipt_type: OnceCell::new(),
            upgrade_cap_type: OnceCell::new(),
        }
    }

    pub fn convert_linked_vm_error(&self, e: VMError, linkage: &RootedLinkage) -> ExecutionError {
        convert_vm_error(e, self.vm, self.linkable_store, Some(linkage))
    }

    pub fn convert_vm_error(&self, e: VMError) -> ExecutionError {
        convert_vm_error(e, self.vm, self.linkable_store, None)
    }

    pub fn convert_type_argument_error(
        &self,
        idx: usize,
        e: VMError,
        linkage: &RootedLinkage,
    ) -> ExecutionError {
        use move_core_types::vm_status::StatusCode;
        match e.major_status() {
            StatusCode::NUMBER_OF_TYPE_ARGUMENTS_MISMATCH => {
                ExecutionErrorKind::TypeArityMismatch.into()
            }
            StatusCode::TYPE_RESOLUTION_FAILURE => ExecutionErrorKind::TypeArgumentError {
                argument_idx: idx as TypeParameterIndex,
                kind: TypeArgumentError::TypeNotFound,
            }
            .into(),
            StatusCode::CONSTRAINT_NOT_SATISFIED => ExecutionErrorKind::TypeArgumentError {
                argument_idx: idx as TypeParameterIndex,
                kind: TypeArgumentError::ConstraintNotSatisfied,
            }
            .into(),
            _ => self.convert_linked_vm_error(e, linkage),
        }
    }

    pub fn module_definition(
        &self,
        module_id: &ModuleId,
        linkage: &RootedLinkage,
    ) -> Result<Arc<CompiledModule>, ExecutionError> {
        let linked_data_store = LinkedDataStore::new(linkage, self.linkable_store);
        self.vm
            .get_runtime()
            .load_module(module_id, &linked_data_store)
            .map_err(|e| self.convert_linked_vm_error(e, linkage))
    }

    pub fn fully_annotated_layout(
        &self,
        ty: &Type,
    ) -> Result<annotated_value::MoveTypeLayout, ExecutionError> {
        let (ty, linkage) = self.load_vm_type_from_adapter_type(ty)?;
        self.vm
            .get_runtime()
            .type_to_fully_annotated_layout(&ty)
            .map_err(|e| self.convert_linked_vm_error(e, &linkage))
    }

    pub fn runtime_layout(
        &self,
        ty: &Type,
    ) -> Result<runtime_value::MoveTypeLayout, ExecutionError> {
        let (ty, linkage) = self.load_vm_type_from_adapter_type(ty)?;
        self.vm
            .get_runtime()
            .type_to_type_layout(&ty)
            .map_err(|e| self.convert_linked_vm_error(e, &linkage))
    }

    pub fn load_function(
        &self,
        package: ObjectID,
        module: String,
        function: String,
        type_arguments: Vec<Type>,
        linkage: RootedLinkage,
    ) -> Result<LoadedFunction, ExecutionError> {
        let Some(original_id) = linkage.resolved_linkage.resolve_to_original_id(&package) else {
            invariant_violation!(
                "Package ID {:?} is not found in linkage generated for that package",
                package
            );
        };
        let module = to_identifier(module)?;
        let name = to_identifier(function)?;
        let storage_id = ModuleId::new(package.into(), module.clone());
        let runtime_id = ModuleId::new(original_id.into(), module);
        let mut data_store = LinkedDataStore::new(&linkage, self.linkable_store);
        let loaded_type_arguments = type_arguments
            .iter()
            .map(|ty| self.load_vm_type_from_adapter_type(ty).map(|(ty, _)| ty))
            .collect::<Result<Vec<_>, _>>()?;
        let runtime_signature = self
            .vm
            .get_runtime()
            .load_function(
                &runtime_id,
                name.as_ident_str(),
                &loaded_type_arguments,
                &mut data_store,
            )
            .map_err(|e| self.convert_linked_vm_error(e, &linkage))?;
        let parameters = runtime_signature
            .parameters
            .into_iter()
            .map(|ty| self.adapter_type_from_vm_type(&ty, &linkage))
            .collect::<Result<Vec<_>, _>>()?;
        let return_ = runtime_signature
            .return_
            .into_iter()
            .map(|ty| self.adapter_type_from_vm_type(&ty, &linkage))
            .collect::<Result<Vec<_>, _>>()?;
        let tx_context = match parameters.last() {
            Some(adapter_ty) => adapter_ty.is_tx_context(),
            None => TxContextKind::None,
        };
        let signature = LoadedFunctionInstantiation {
            parameters,
            return_,
        };
        Ok(LoadedFunction {
            storage_id,
            runtime_id,
            name,
            type_arguments,
            signature,
            tx_context,
            linkage,
            instruction_length: runtime_signature.instruction_length,
            definition_index: runtime_signature.definition_index,
        })
    }

    pub fn load_type_input(&self, _idx: usize, ty: TypeInput) -> Result<Type, ExecutionError> {
        let (runtime_type, linkage) = self.load_vm_type_from_type_input(ty)?;
        self.adapter_type_from_vm_type(&runtime_type, &linkage)
    }

    /// We verify that all types in the `StructTag` are defining ID-based types.
    pub fn load_type_from_struct(&self, tag: &StructTag) -> Result<Type, ExecutionError> {
        let (vm_type, linkage) =
            self.load_vm_type_from_type_tag(&TypeTag::Struct(Box::new(tag.clone())))?;
        self.adapter_type_from_vm_type(&vm_type, &linkage)
    }

    pub fn type_layout_for_struct(
        &self,
        tag: &StructTag,
    ) -> Result<MoveTypeLayout, ExecutionError> {
        let ty: Type = self.load_type_from_struct(tag)?;
        self.runtime_layout(&ty)
    }

    pub fn gas_coin_type(&self) -> Result<Type, ExecutionError> {
        get_or_init_ty!(self, gas_coin_type, GasCoin::type_())
    }

    pub fn upgrade_ticket_type(&self) -> Result<Type, ExecutionError> {
        get_or_init_ty!(self, upgrade_ticket_type, UpgradeTicket::type_())
    }

    pub fn upgrade_receipt_type(&self) -> Result<Type, ExecutionError> {
        get_or_init_ty!(self, upgrade_receipt_type, UpgradeReceipt::type_())
    }

    pub fn upgrade_cap_type(&self) -> Result<Type, ExecutionError> {
        get_or_init_ty!(self, upgrade_cap_type, UpgradeCap::type_())
    }

    pub fn vector_type(&self, element_type: Type) -> Result<Type, ExecutionError> {
        let abilities = AbilitySet::polymorphic_abilities(
            AbilitySet::VECTOR,
            [false],
            [element_type.abilities()],
        )
        .map_err(|e| {
            ExecutionError::new_with_source(ExecutionErrorKind::VMInvariantViolation, e.to_string())
        })?;
        Ok(Type::Vector(Rc::new(L::Vector {
            abilities,
            element_type,
        })))
    }

    pub fn read_object(&self, id: &ObjectID) -> Result<&Object, ExecutionError> {
        let Some(obj) = self.state_view.read_object(id) else {
            // protected by transaction input checker
            invariant_violation!("Object {:?} does not exist", id);
        };
        Ok(obj)
    }

    /// Takes an adapter Type and returns a VM runtime Type and the linkage for it.
    pub fn load_vm_type_from_adapter_type(
        &self,
        ty: &Type,
    ) -> Result<(vm_runtime_type::Type, RootedLinkage), ExecutionError> {
        let tag: TypeTag = ty.clone().try_into().map_err(|s| {
            ExecutionError::new_with_source(ExecutionErrorKind::VMInvariantViolation, s)
        })?;
        self.load_vm_type_from_type_tag(&tag)
    }

    /// Take a type tag and returns a VM runtime Type and the linkage for it.
    fn load_vm_type_from_type_tag(
        &self,
        tag: &TypeTag,
    ) -> Result<(vm_runtime_type::Type, RootedLinkage), ExecutionError> {
        let (addresses, link_context) = link_info_for_type_tag(tag)?;
        let ids: Vec<_> = addresses.into_iter().map(ObjectID::from).collect();
        let linkage = type_linkage(&ids, self.linkable_store)?;
        let linkage = RootedLinkage::new(link_context, linkage);
        let linked_store = LinkedDataStore::new(&linkage, self.linkable_store);
        self.vm
            .get_runtime()
            .load_type_tag(tag, &linked_store)
            .map_err(|e| self.convert_linked_vm_error(e, &linkage))
            .map(|runtime_ty| (runtime_ty, linkage))
    }

    /// Converts a VM runtime Type to an adapter Type.
    fn adapter_type_from_vm_type(
        &self,
        vm_type: &vm_runtime_type::Type,
        linkage: &RootedLinkage,
    ) -> Result<Type, ExecutionError> {
        use vm_runtime_type as VRT;

        Ok(match vm_type {
            VRT::Type::Bool => Type::Bool,
            VRT::Type::U8 => Type::U8,
            VRT::Type::U16 => Type::U16,
            VRT::Type::U32 => Type::U32,
            VRT::Type::U64 => Type::U64,
            VRT::Type::U128 => Type::U128,
            VRT::Type::U256 => Type::U256,
            VRT::Type::Address => Type::Address,
            VRT::Type::Signer => Type::Signer,

            VRT::Type::Reference(ref_ty) => {
                let inner_ty = self.adapter_type_from_vm_type(ref_ty, linkage)?;
                Type::Reference(false, Rc::new(inner_ty))
            }
            VRT::Type::MutableReference(ref_ty) => {
                let inner_ty = self.adapter_type_from_vm_type(ref_ty, linkage)?;
                Type::Reference(true, Rc::new(inner_ty))
            }

            VRT::Type::Vector(inner) => {
                let element_type = self.adapter_type_from_vm_type(inner, linkage)?;
                let abilities = self
                    .vm
                    .get_runtime()
                    .get_type_abilities(vm_type)
                    .map_err(|e| self.convert_linked_vm_error(e, linkage))?;
                let vector_ty = Vector {
                    abilities,
                    element_type,
                };
                Type::Vector(Rc::new(vector_ty))
            }
            VRT::Type::Datatype(cached_type_index) => {
                let runtime = self.vm.get_runtime();
                let Some(cached_info) = runtime.get_type(*cached_type_index) else {
                    invariant_violation!(
                        "Unable to find cached type info for {:?}. This should not happen as we have \
                         a loaded VM type in-hand.",
                        vm_type
                    )
                };
                let datatype = Datatype {
                    abilities: cached_info.abilities,
                    module: cached_info.defining_id.clone(),
                    name: cached_info.name.clone(),
                    type_arguments: vec![],
                };
                Type::Datatype(Rc::new(datatype))
            }
            ty @ VRT::Type::DatatypeInstantiation(inst) => {
                let (cached_type_index, type_arguments) = &**inst;
                let runtime = self.vm.get_runtime();
                let Some(cached_info) = runtime.get_type(*cached_type_index) else {
                    invariant_violation!(
                        "Unable to find cached type info for {:?}. This should not happen as we have \
                         a loaded VM type in-hand.",
                        vm_type
                    )
                };

                let abilities = runtime
                    .get_type_abilities(ty)
                    .map_err(|e| self.convert_linked_vm_error(e, linkage))?;
                let module = cached_info.defining_id.clone();
                let name = cached_info.name.clone();
                let type_arguments = type_arguments
                    .iter()
                    .map(|t| self.adapter_type_from_vm_type(t, linkage))
                    .collect::<Result<Vec<_>, _>>()?;

                Type::Datatype(Rc::new(Datatype {
                    abilities,
                    module,
                    name,
                    type_arguments,
                }))
            }

            VRT::Type::TyParam(_) => {
                invariant_violation!(
                    "Unexpected type parameter in VM type: {:?}. This should not happen as we should \
                     have resolved all type parameters before this point.",
                    vm_type
                );
            }
        })
    }

    /// Load a `TypeInput` into a VM runtime `Type` and its `Linkage`. Loading into the VM ensures
    /// that any adapter type or type tag that results from this is properly output with defining
    /// IDs.
    fn load_vm_type_from_type_input(
        &self,
        ty: TypeInput,
    ) -> Result<(vm_runtime_type::Type, RootedLinkage), ExecutionError> {
        fn to_type_tag_internal(ty: TypeInput) -> Result<TypeTag, ExecutionError> {
            Ok(match ty {
                TypeInput::Bool => TypeTag::Bool,
                TypeInput::U8 => TypeTag::U8,
                TypeInput::U16 => TypeTag::U16,
                TypeInput::U32 => TypeTag::U32,
                TypeInput::U64 => TypeTag::U64,
                TypeInput::U128 => TypeTag::U128,
                TypeInput::U256 => TypeTag::U256,
                TypeInput::Address => TypeTag::Address,
                TypeInput::Signer => TypeTag::Signer,
                TypeInput::Vector(type_input) => {
                    let inner = to_type_tag_internal(*type_input)?;
                    TypeTag::Vector(Box::new(inner))
                }
                TypeInput::Struct(struct_input) => {
                    let StructInput {
                        address,
                        module,
                        name,
                        type_params,
                    } = *struct_input;
                    let module = to_identifier(module)?;
                    let name = to_identifier(name)?;
                    let tys = type_params
                        .into_iter()
                        .map(to_type_tag_internal)
                        .collect::<Result<Vec<_>, _>>()?;
                    TypeTag::Struct(Box::new(StructTag {
                        address,
                        module,
                        name,
                        type_params: tys,
                    }))
                }
            })
        }
        let tag = to_type_tag_internal(ty)?;
        self.load_vm_type_from_type_tag(&tag)
    }
}

#[allow(unused)]
fn to_identifier(name: String) -> Result<Identifier, ExecutionError> {
    Identifier::new(name).map_err(|e| {
        ExecutionError::new_with_source(ExecutionErrorKind::VMInvariantViolation, e.to_string())
    })
}

/// Given a `TypeTag` returns the set of addresses (IDs) within the type, and the link context for
/// the type.
///
/// The Link context for a type is the first address encountered in the type tag in a pre-order
/// traversal. If no address is encountered in the type tag that we can use, we default to using
/// the `AccountAddress::ZERO` link context.
fn link_info_for_type_tag(
    tag: &TypeTag,
) -> Result<(IndexSet<AccountAddress>, AccountAddress), ExecutionError> {
    let addresses = tag.all_addresses();
    // The link context is the first address encountered in the type tag.
    // If no address is encountered in the type tag that we can use, we default to
    // AccountAddress::ZERO.
    let link_context = addresses.first().cloned().unwrap_or(AccountAddress::ZERO);
    Ok((addresses, link_context))
}

fn convert_vm_error(
    error: VMError,
    vm: &MoveVM,
    store: &dyn PackageStore,
    linkage: Option<&RootedLinkage>,
) -> ExecutionError {
    use crate::error::convert_vm_error_impl;
    convert_vm_error_impl(
        error,
        &|id| {
            debug_assert!(
                linkage.is_some(),
                "Linkage should be set anywhere where runtime errors may occur in order to resolve abort locations to package IDs"
            );
            linkage
                .and_then(|linkage| LinkedDataStore::new(linkage, store).relocate(id).ok())
                .unwrap_or_else(|| id.clone())
        },
        &|id, function| {
            debug_assert!(
                linkage.is_some(),
                "Linkage should be set anywhere where runtime errors may occur in order to resolve abort locations to package IDs"
            );
            linkage.and_then(|linkage| {
                let state_view = LinkedDataStore::new(linkage, store);
                vm.load_module(id, state_view).ok().map(|module| {
                    let fdef = module.function_def_at(function);
                    let fhandle = module.function_handle_at(fdef.function);
                    module.identifier_at(fhandle.name).to_string()
                })
            })
        },
    )
}
