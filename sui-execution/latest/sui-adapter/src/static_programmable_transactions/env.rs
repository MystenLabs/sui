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
        execution::context::subst_signature,
        linkage::{
            analysis::{LinkageAnalysis, type_linkage},
            resolved_linkage::ExecutableLinkage,
        },
        loading::ast::{self as L, Datatype, LoadedFunction, LoadedFunctionInstantiation, Type},
    },
};
use move_binary_format::{
    CompiledModule,
    errors::VMError,
    file_format::{AbilitySet, TypeParameterIndex},
};
use move_core_types::{
    annotated_value,
    language_storage::{ModuleId, StructTag},
    resolver::ModuleResolver,
    runtime_value::{self, MoveTypeLayout},
    vm_status::StatusCode,
};
use move_vm_runtime::{
    execution::{self as vm_runtime, vm::MoveVM},
    runtime::MoveRuntime,
};
use std::{cell::OnceCell, rc::Rc};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    Identifier, TypeTag,
    base_types::{ObjectID, TxContext},
    error::{ExecutionError, ExecutionErrorKind},
    execution_config_utils::to_binary_config,
    execution_status::TypeArgumentError,
    gas_coin::GasCoin,
    move_package::{UpgradeCap, UpgradeReceipt, UpgradeTicket},
    object::Object,
    type_input::{StructInput, TypeInput},
};

pub struct Env<'pc, 'vm, 'state, 'linkage> {
    pub protocol_config: &'pc ProtocolConfig,
    pub vm: &'vm MoveRuntime,
    pub state_view: &'state mut dyn ExecutionState,
    pub linkable_store: &'linkage CachedPackageStore<'state>,
    pub linkage_analysis: &'linkage dyn LinkageAnalysis,
    gas_coin_type: OnceCell<Type>,
    upgrade_ticket_type: OnceCell<Type>,
    upgrade_receipt_type: OnceCell<Type>,
    upgrade_cap_type: OnceCell<Type>,
    tx_context_type: OnceCell<Type>,
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
        vm: &'vm MoveRuntime,
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
            tx_context_type: OnceCell::new(),
        }
    }

    pub fn convert_linked_vm_error(
        &self,
        e: VMError,
        linkage: &ExecutableLinkage,
    ) -> ExecutionError {
        convert_vm_error(e, self.linkable_store, Some(linkage), self.protocol_config)
    }

    pub fn convert_vm_error(&self, e: VMError) -> ExecutionError {
        convert_vm_error(e, self.linkable_store, None, self.protocol_config)
    }

    pub fn convert_type_argument_error(
        &self,
        idx: usize,
        e: VMError,
        linkage: &ExecutableLinkage,
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

    pub fn fully_annotated_layout(
        &self,
        ty: &Type,
    ) -> Result<annotated_value::MoveTypeLayout, ExecutionError> {
        let tag: TypeTag = ty.clone().try_into().map_err(|s| {
            ExecutionError::new_with_source(ExecutionErrorKind::VMInvariantViolation, s)
        })?;
        use annotated_value as AV;
        fn annotated_type_layout(
            env: &Env,
            tag: &TypeTag,
        ) -> Result<AV::MoveTypeLayout, ExecutionError> {
            Ok(match tag {
                TypeTag::Bool => AV::MoveTypeLayout::Bool,
                TypeTag::U8 => AV::MoveTypeLayout::U8,
                TypeTag::U16 => AV::MoveTypeLayout::U16,
                TypeTag::U32 => AV::MoveTypeLayout::U32,
                TypeTag::U64 => AV::MoveTypeLayout::U64,
                TypeTag::U128 => AV::MoveTypeLayout::U128,
                TypeTag::U256 => AV::MoveTypeLayout::U256,
                TypeTag::Address => AV::MoveTypeLayout::Address,
                TypeTag::Signer => AV::MoveTypeLayout::Signer,
                TypeTag::Vector(type_tag) => {
                    AV::MoveTypeLayout::Vector(Box::new(annotated_type_layout(env, type_tag)?))
                }
                TypeTag::Struct(struct_tag) => {
                    let objects = struct_tag.all_addresses();
                    let tag_linkage = type_linkage(
                        &objects.iter().map(|a| (*a).into()).collect::<Vec<_>>(),
                        env.linkable_store,
                    )?;
                    let linkage_context = tag_linkage.linkage_context();
                    let linked_store = LinkedDataStore::new(&tag_linkage, env.linkable_store);
                    let vm = env
                        .vm
                        .make_vm(&linked_store, linkage_context)
                        .map_err(|e| env.convert_linked_vm_error(e, &tag_linkage))?;
                    vm.annotated_type_layout(tag)
                        .map_err(|e| env.convert_linked_vm_error(e, &tag_linkage))?
                }
            })
        }
        annotated_type_layout(self, &tag)
    }

    pub fn runtime_layout(
        &self,
        ty: &Type,
    ) -> Result<runtime_value::MoveTypeLayout, ExecutionError> {
        let tag: TypeTag = ty.clone().try_into().map_err(|s| {
            ExecutionError::new_with_source(ExecutionErrorKind::VMInvariantViolation, s)
        })?;
        use runtime_value as RV;
        fn runtime_type_layout(
            env: &Env,
            tag: &TypeTag,
        ) -> Result<RV::MoveTypeLayout, ExecutionError> {
            Ok(match tag {
                TypeTag::Bool => RV::MoveTypeLayout::Bool,
                TypeTag::U8 => RV::MoveTypeLayout::U8,
                TypeTag::U16 => RV::MoveTypeLayout::U16,
                TypeTag::U32 => RV::MoveTypeLayout::U32,
                TypeTag::U64 => RV::MoveTypeLayout::U64,
                TypeTag::U128 => RV::MoveTypeLayout::U128,
                TypeTag::U256 => RV::MoveTypeLayout::U256,
                TypeTag::Address => RV::MoveTypeLayout::Address,
                TypeTag::Signer => RV::MoveTypeLayout::Signer,
                TypeTag::Vector(type_tag) => {
                    RV::MoveTypeLayout::Vector(Box::new(runtime_type_layout(env, type_tag)?))
                }
                TypeTag::Struct(struct_tag) => {
                    let objects = struct_tag.all_addresses();
                    let tag_linkage = type_linkage(
                        &objects.iter().map(|a| (*a).into()).collect::<Vec<_>>(),
                        env.linkable_store,
                    )?;
                    let linkage_context = tag_linkage.linkage_context();
                    let linked_store = LinkedDataStore::new(&tag_linkage, env.linkable_store);
                    let vm = env
                        .vm
                        .make_vm(&linked_store, linkage_context)
                        .map_err(|e| env.convert_linked_vm_error(e, &tag_linkage))?;
                    vm.runtime_type_layout(tag)
                        .map_err(|e| env.convert_linked_vm_error(e, &tag_linkage))?
                }
            })
        }
        runtime_type_layout(self, &tag)
    }

    pub fn load_function(
        &self,
        package: ObjectID,
        module: String,
        function: String,
        type_arguments: Vec<Type>,
        linkage: ExecutableLinkage,
    ) -> Result<LoadedFunction, ExecutionError> {
        let Some(original_id) = linkage.0.resolve_to_original_id(&package) else {
            invariant_violation!(
                "Package ID {:?} is not found in linkage generated for that package",
                package
            );
        };
        let module = to_identifier(module)?;
        let name = to_identifier(function)?;
        let storage_id = ModuleId::new(package.into(), module.clone());
        let runtime_id = ModuleId::new(original_id.into(), module);
        let data_store = LinkedDataStore::new(&linkage, self.linkable_store);
        let linkage_context = linkage.linkage_context();
        let loaded_type_arguments = type_arguments
            .iter()
            .enumerate()
            .map(|(idx, ty)| self.load_vm_type_argument_from_adapter_type(idx, ty))
            .collect::<Result<Vec<_>, _>>()?;
        let vm = self
            .vm
            .make_vm(&data_store, linkage_context)
            .map_err(|e| self.convert_linked_vm_error(e, &linkage))?;
        let runtime_signature = vm
            .function_information(&runtime_id, name.as_ident_str(), &loaded_type_arguments)
            .map_err(|e| {
                if e.major_status() == StatusCode::EXTERNAL_RESOLUTION_REQUEST_ERROR {
                    ExecutionError::new_with_source(
                        ExecutionErrorKind::FunctionNotFound,
                        format!(
                            "Could not resolve function '{}' in module {}",
                            name, &storage_id,
                        ),
                    )
                } else {
                    self.convert_linked_vm_error(e, &linkage)
                }
            })?;
        let runtime_signature = subst_signature(runtime_signature, &loaded_type_arguments)
            .map_err(|e| self.convert_linked_vm_error(e, &linkage))?;
        let vm_opt = Some(vm);
        let parameters = runtime_signature
            .parameters
            .into_iter()
            .map(|ty| self.adapter_type_from_vm_type(&vm_opt, &ty))
            .collect::<Result<Vec<_>, _>>()?;
        let return_ = runtime_signature
            .return_
            .into_iter()
            .map(|ty| self.adapter_type_from_vm_type(&vm_opt, &ty))
            .collect::<Result<Vec<_>, _>>()?;
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
            linkage,
            instruction_length: runtime_signature.instruction_count,
            definition_index: runtime_signature.index,
            visibility: runtime_signature.visibility,
            is_entry: runtime_signature.is_entry,
            is_native: runtime_signature.is_native,
        })
    }

    pub fn load_type_input(&self, idx: usize, ty: TypeInput) -> Result<Type, ExecutionError> {
        let (vm_type, vm_opt) = self.load_vm_type_from_type_input(idx, ty)?;
        self.adapter_type_from_vm_type(&vm_opt, &vm_type)
    }

    /// We verify that all types in the `StructTag` are defining ID-based types.
    pub fn load_type_from_struct(&self, tag: &StructTag) -> Result<Type, ExecutionError> {
        let (vm_type, vm_opt) =
            self.load_vm_type_from_type_tag(None, &TypeTag::Struct(Box::new(tag.clone())))?;
        self.adapter_type_from_vm_type(&vm_opt, &vm_type)
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

    pub fn tx_context_type(&self) -> Result<Type, ExecutionError> {
        get_or_init_ty!(self, tx_context_type, TxContext::type_())
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
    pub fn load_vm_type_argument_from_adapter_type(
        &self,
        idx: usize,
        ty: &Type,
    ) -> Result<vm_runtime::Type, ExecutionError> {
        self.load_vm_type_from_adapter_type(Some(idx), ty)
    }

    fn load_vm_type_from_adapter_type(
        &self,
        type_arg_idx: Option<usize>,
        ty: &Type,
    ) -> Result<vm_runtime::Type, ExecutionError> {
        let tag: TypeTag = ty.clone().try_into().map_err(|s| {
            ExecutionError::new_with_source(ExecutionErrorKind::VMInvariantViolation, s)
        })?;
        self.load_vm_type_from_type_tag(type_arg_idx, &tag)
            .map(|(ty, _)| ty)
    }

    /// Take a type tag and returns a VM runtime Type and the linkage for it.
    fn load_vm_type_from_type_tag(
        &self,
        type_arg_idx: Option<usize>,
        tag: &TypeTag,
    ) -> Result<(vm_runtime::Type, Option<MoveVM>), ExecutionError> {
        use vm_runtime as VMR;
        fn execution_error(
            env: &Env,
            type_arg_idx: Option<usize>,
            e: VMError,
            linkage: &ExecutableLinkage,
        ) -> ExecutionError {
            if let Some(idx) = type_arg_idx {
                env.convert_type_argument_error(idx, e, linkage)
            } else {
                env.convert_linked_vm_error(e, linkage)
            }
        }

        let vm_opt_linkage = {
            let addresses = tag
                .all_addresses()
                .into_iter()
                .map(|a| a.into())
                .collect::<Vec<_>>();
            if addresses.is_empty() {
                None
            } else {
                let tag_linkage = type_linkage(&addresses, self.linkable_store)?;
                let link_context = tag_linkage.linkage_context();
                let linked_store = LinkedDataStore::new(&tag_linkage, self.linkable_store);
                Some((
                    self.vm
                        .make_vm(&linked_store, link_context)
                        .map_err(|e| execution_error(self, type_arg_idx, e, &tag_linkage))?,
                    tag_linkage,
                ))
            }
        };

        fn load_type_tag(
            env: &Env,
            type_arg_idx: Option<usize>,
            tag: &TypeTag,
            vm_opt_linkage: &Option<(MoveVM, ExecutableLinkage)>,
        ) -> Result<VMR::Type, ExecutionError> {
            Ok(match tag {
                TypeTag::Bool => VMR::Type::Bool,
                TypeTag::U8 => VMR::Type::U8,
                TypeTag::U16 => VMR::Type::U16,
                TypeTag::U32 => VMR::Type::U32,
                TypeTag::U64 => VMR::Type::U64,
                TypeTag::U128 => VMR::Type::U128,
                TypeTag::U256 => VMR::Type::U256,
                TypeTag::Address => VMR::Type::Address,
                TypeTag::Signer => VMR::Type::Signer,

                TypeTag::Vector(inner) => VMR::Type::Vector(Box::new(load_type_tag(
                    env,
                    type_arg_idx,
                    inner,
                    vm_opt_linkage,
                )?)),
                TypeTag::Struct(_) => {
                    let Some((vm, linkage)) = vm_opt_linkage else {
                        invariant_violation!(
                            "Expected VM in load_struct_tag for non-primitive tag {:?}",
                            tag
                        )
                    };
                    vm.load_type(tag)
                        .map_err(|e| execution_error(env, type_arg_idx, e, linkage))?
                }
            })
        }

        Ok((
            load_type_tag(self, type_arg_idx, tag, &vm_opt_linkage)?,
            vm_opt_linkage.map(|(vm, _)| vm),
        ))
    }

    /// Converts a VM runtime Type to an adapter Type.
    fn adapter_type_from_vm_type(
        &self,
        vm_opt: &Option<MoveVM>,
        vm_type: &vm_runtime::Type,
    ) -> Result<Type, ExecutionError> {
        use vm_runtime as VRT;

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
                let inner_ty = self.adapter_type_from_vm_type(vm_opt, ref_ty)?;
                Type::Reference(false, Rc::new(inner_ty))
            }
            VRT::Type::MutableReference(ref_ty) => {
                let inner_ty = self.adapter_type_from_vm_type(vm_opt, ref_ty)?;
                Type::Reference(true, Rc::new(inner_ty))
            }

            VRT::Type::Vector(inner) => {
                let element_type = self.adapter_type_from_vm_type(vm_opt, inner)?;
                self.vector_type(element_type)?
            }
            VRT::Type::Datatype(_) => {
                let Some(vm) = vm_opt else {
                    invariant_violation!(
                        "Expected VM in adapter_type_from_vm_type for non-primitive type {:?}",
                        vm_type
                    )
                };

                let type_information = vm
                    .type_information(vm_type)
                    .map_err(|e| self.convert_vm_error(e))?;
                let Some(data_type_info) = type_information.datatype_info else {
                    invariant_violation!("Expected datatype info for datatype type {:?}", vm_type);
                };
                let datatype = Datatype {
                    abilities: type_information.abilities,
                    module: ModuleId::new(data_type_info.defining_id, data_type_info.module_name),
                    name: data_type_info.type_name,
                    type_arguments: vec![],
                };
                Type::Datatype(Rc::new(datatype))
            }
            ty @ VRT::Type::DatatypeInstantiation(inst) => {
                let (_, type_arguments) = &**inst;
                let Some(vm) = vm_opt else {
                    invariant_violation!(
                        "Expected VM in adapter_type_from_vm_type for non-primitive type {:?}",
                        vm_type
                    )
                };
                let type_information = vm
                    .type_information(ty)
                    .map_err(|e| self.convert_vm_error(e))?;
                let Some(data_type_info) = type_information.datatype_info else {
                    invariant_violation!("Expected datatype info for datatype type {:?}", vm_type);
                };

                let abilities = type_information.abilities;
                let module = ModuleId::new(data_type_info.defining_id, data_type_info.module_name);
                let name = data_type_info.type_name;
                let type_arguments = type_arguments
                    .iter()
                    .map(|t| self.adapter_type_from_vm_type(vm_opt, t))
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
        type_arg_idx: usize,
        ty: TypeInput,
    ) -> Result<(vm_runtime::Type, Option<MoveVM>), ExecutionError> {
        fn to_type_tag_internal(
            env: &Env,
            type_arg_idx: usize,
            ty: TypeInput,
        ) -> Result<TypeTag, ExecutionError> {
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
                    let inner = to_type_tag_internal(env, type_arg_idx, *type_input)?;
                    TypeTag::Vector(Box::new(inner))
                }
                TypeInput::Struct(struct_input) => {
                    let StructInput {
                        address,
                        module,
                        name,
                        type_params,
                    } = *struct_input;

                    let pkg = env
                        .linkable_store
                        .get_package(&address.into())
                        .ok()
                        .flatten()
                        .ok_or_else(|| {
                            ExecutionError::from_kind(ExecutionErrorKind::TypeArgumentError {
                                argument_idx: type_arg_idx as u16,
                                kind: TypeArgumentError::TypeNotFound,
                            })
                        })?;
                    let Some(resolved_address) = pkg
                        .type_origin_map()
                        .get(&(module.clone(), name.clone()))
                        .cloned()
                    else {
                        return Err(ExecutionError::from_kind(
                            ExecutionErrorKind::TypeArgumentError {
                                argument_idx: type_arg_idx as u16,
                                kind: TypeArgumentError::TypeNotFound,
                            },
                        ));
                    };

                    let module = to_identifier(module)?;
                    let name = to_identifier(name)?;
                    let tys = type_params
                        .into_iter()
                        .map(|tp| to_type_tag_internal(env, type_arg_idx, tp))
                        .collect::<Result<Vec<_>, _>>()?;
                    TypeTag::Struct(Box::new(StructTag {
                        address: *resolved_address,
                        module,
                        name,
                        type_params: tys,
                    }))
                }
            })
        }
        let tag = to_type_tag_internal(self, type_arg_idx, ty)?;
        self.load_vm_type_from_type_tag(Some(type_arg_idx), &tag)
    }
}

fn to_identifier(name: String) -> Result<Identifier, ExecutionError> {
    Identifier::new(name).map_err(|e| {
        ExecutionError::new_with_source(ExecutionErrorKind::VMInvariantViolation, e.to_string())
    })
}

fn convert_vm_error(
    error: VMError,
    store: &dyn PackageStore,
    linkage: Option<&ExecutableLinkage>,
    protocol_config: &ProtocolConfig,
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
                .and_then(|linkage| {
                    linkage
                        .0
                        .linkage
                        .get(&(*id.address()).into())
                        .map(|new_id| ModuleId::new((*new_id).into(), id.name().to_owned()))
                })
                .unwrap_or_else(|| id.clone())
        },
        // NB: the `id` here is the original ID (and hence _not_ relocated).
        &|id, function| {
            debug_assert!(
                linkage.is_some(),
                "Linkage should be set anywhere where runtime errors may occur in order to resolve abort locations to package IDs"
            );
            linkage.and_then(|linkage| {
                let remapped_id = linkage
                    .0
                    .linkage
                    .get(&(*id.address()).into())
                    .map(|new_id| ModuleId::new((*new_id).into(), id.name().to_owned()))?;
                let state_view = LinkedDataStore::new(linkage, store);
                state_view.get_module(&remapped_id).ok().and_then(|module| {
                    let binary_config = to_binary_config(protocol_config);
                    let module =
                        CompiledModule::deserialize_with_config(&module?, &binary_config).ok()?;
                    let fdef = module.function_def_at(function);
                    let fhandle = module.function_handle_at(fdef.function);
                    Some(module.identifier_at(fhandle.name).to_string())
                })
            })
        },
    )
}
