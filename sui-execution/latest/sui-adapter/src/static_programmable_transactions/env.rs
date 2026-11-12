// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module defines the shared environment, `Env`, used for the compilation/translation and
//! execution of programmable transactions. While the "context" for each pass might be different,
//! the `Env` provides consistent access to shared components such as the VM or the protocol config.

use crate::{
    data_store::{VerifiedPackageStore, cached_package_store::CachedPackageStore},
    execution_mode::ExecutionMode,
    execution_value::ExecutionState,
    static_programmable_transactions::{
        linkage::{config::ResolutionConfig, resolved_linkage::ExecutableLinkage},
        loading::ast::{self as L, Datatype, LoadedFunction, LoadedFunctionInstantiation, Type},
    },
};
use move_binary_format::{
    errors::VMError,
    file_format::{Ability, AbilitySet, TypeParameterIndex},
};
use move_core_types::{
    annotated_value,
    identifier::IdentStr,
    language_storage::{ModuleId, StructTag},
    runtime_value::{self, MoveTypeLayout},
    vm_status::StatusCode,
};
use move_vm_runtime::{
    execution::{self as vm_runtime, vm::MoveVM},
    runtime::MoveRuntime,
};
use std::{cell::OnceCell, marker::PhantomData, rc::Rc};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    Identifier, SUI_FRAMEWORK_PACKAGE_ID, TypeTag,
    allowance::RESOLVED_ALLOWANCE_WITHDRAWAL_STRUCT,
    balance::RESOLVED_BALANCE_STRUCT,
    base_types::{ObjectID, TxContext},
    coin::RESOLVED_COIN_STRUCT,
    error::ExecutionErrorTrait,
    execution_status::{ExecutionErrorKind, TypeArgumentError},
    funds_accumulator::RESOLVED_WITHDRAWAL_STRUCT,
    gas_coin::GasCoin,
    move_package::{UpgradeCap, UpgradeReceipt, UpgradeTicket},
    object::Object,
};

pub struct Env<'pc, 'vm, 'state, 'linkage, 'extensions, Mode>
where
    Mode: ExecutionMode,
{
    pub protocol_config: &'pc ProtocolConfig,
    pub vm: &'vm MoveRuntime,
    pub state_view: &'state mut dyn ExecutionState,
    pub linkable_store: &'linkage CachedPackageStore<'state, 'vm>,
    pub resolution_config: &'linkage ResolutionConfig,
    gas_coin_type: OnceCell<Type>,
    upgrade_ticket_type: OnceCell<Type>,
    upgrade_receipt_type: OnceCell<Type>,
    upgrade_cap_type: OnceCell<Type>,
    tx_context_type: OnceCell<Type>,
    pub unified_linkage: &'linkage ExecutableLinkage,
    // Loading and execution share linkage, but execution needs its own native extensions.
    //
    // This VM is used for type resolution of input types and types statically present in the PTB
    // and for "compilation"/"querying" only. This VM should not be used for execution or final
    // serialization of values.
    loading_vm: &'linkage MoveVM<'extensions>,
    _mode: PhantomData<fn() -> Mode>,
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

impl<'pc, 'vm, 'state, 'linkage, 'extensions, Mode>
    Env<'pc, 'vm, 'state, 'linkage, 'extensions, Mode>
where
    Mode: ExecutionMode,
{
    pub fn new(
        protocol_config: &'pc ProtocolConfig,
        vm: &'vm MoveRuntime,
        state_view: &'state mut dyn ExecutionState,
        linkable_store: &'linkage CachedPackageStore<'state, 'vm>,
        resolution_config: &'linkage ResolutionConfig,
        loading_vm: &'linkage MoveVM<'extensions>,
        unified_linkage: &'linkage ExecutableLinkage,
    ) -> Self {
        Self {
            protocol_config,
            vm,
            state_view,
            linkable_store,
            resolution_config,
            gas_coin_type: OnceCell::new(),
            upgrade_ticket_type: OnceCell::new(),
            upgrade_receipt_type: OnceCell::new(),
            upgrade_cap_type: OnceCell::new(),
            tx_context_type: OnceCell::new(),
            loading_vm,
            unified_linkage,
            _mode: PhantomData,
        }
    }

    pub fn convert_linked_vm_error(&self, e: VMError, linkage: &ExecutableLinkage) -> Mode::Error {
        convert_vm_error(e, self.linkable_store, Some(linkage), self.protocol_config)
    }

    pub fn convert_vm_error(&self, e: VMError) -> Mode::Error {
        convert_vm_error(e, self.linkable_store, None, self.protocol_config)
    }

    pub fn convert_type_argument_error(&self, idx: usize, e: VMError) -> Mode::Error {
        use move_core_types::vm_status::StatusCode;
        let argument_idx = match checked_as!(idx, TypeParameterIndex) {
            Err(e) => return e.into(),
            Ok(v) => v,
        };
        match e.major_status() {
            StatusCode::NUMBER_OF_TYPE_ARGUMENTS_MISMATCH => {
                Mode::Error::from_kind(ExecutionErrorKind::TypeArityMismatch)
            }
            StatusCode::EXTERNAL_RESOLUTION_REQUEST_ERROR => {
                Mode::Error::from_kind(ExecutionErrorKind::TypeArgumentError {
                    argument_idx,
                    kind: TypeArgumentError::TypeNotFound,
                })
            }
            StatusCode::CONSTRAINT_NOT_SATISFIED => {
                Mode::Error::from_kind(ExecutionErrorKind::TypeArgumentError {
                    argument_idx,
                    kind: TypeArgumentError::ConstraintNotSatisfied,
                })
            }
            _ => self.convert_linked_vm_error(e, self.unified_linkage),
        }
    }

    pub fn fully_annotated_layout(
        &self,
        ty: &Type,
    ) -> Result<annotated_value::MoveTypeLayout, Mode::Error> {
        let tag: TypeTag = ty.clone().try_into().map_err(|s| {
            Mode::Error::new_with_source(ExecutionErrorKind::VMInvariantViolation, s)
        })?;
        self.loading_vm
            .annotated_type_layout(&tag)
            .map_err(|e| self.convert_linked_vm_error(e, self.unified_linkage))
    }

    pub fn runtime_layout(&self, ty: &Type) -> Result<runtime_value::MoveTypeLayout, Mode::Error> {
        let tag: TypeTag = ty.clone().try_into().map_err(|s| {
            Mode::Error::new_with_source(ExecutionErrorKind::VMInvariantViolation, s)
        })?;
        self.loading_vm
            .runtime_type_layout(&tag)
            .map_err(|e| self.convert_linked_vm_error(e, self.unified_linkage))
    }

    pub fn load_framework_function(
        &self,
        module: &IdentStr,
        function: &IdentStr,
        type_arguments: Vec<Type>,
    ) -> Result<LoadedFunction, Mode::Error> {
        // SAFETY: Normal/dev-inspect linkage seeds the framework, and the type arguments are
        // present in the tx inputs and therefore are included in the unified linkage. Therefore
        // all packages needed are present for this load. These synthesized calls are for
        // withdrawal compatibility; system/genesis callers do not request those conversions.
        self.load_function(
            SUI_FRAMEWORK_PACKAGE_ID,
            module.to_owned(),
            function.to_owned(),
            type_arguments,
        )
    }

    pub fn load_function(
        &self,
        package: ObjectID,
        module: Identifier,
        name: Identifier,
        type_arguments: Vec<Type>,
    ) -> Result<LoadedFunction, Mode::Error> {
        let linkage = self.unified_linkage;

        let Some(original_id) = linkage.0.resolve_to_original_id(&package) else {
            invariant_violation!(
                "Package ID {:?} is not found in linkage generated for that package",
                package
            );
        };
        let version_mid = ModuleId::new(package.into(), module.clone());
        let original_mid = ModuleId::new(original_id.into(), module);
        let loaded_type_arguments = type_arguments
            .iter()
            .enumerate()
            .map(|(idx, ty)| self.load_vm_type_argument_from_adapter_type(idx, ty))
            .collect::<Result<Vec<_>, _>>()?;
        let vm = self.loading_vm;
        let runtime_signature = vm
            .function_information(&original_mid, name.as_ident_str(), &loaded_type_arguments)
            .map_err(|e| {
                if e.major_status() == StatusCode::EXTERNAL_RESOLUTION_REQUEST_ERROR {
                    Mode::Error::new_with_source(
                        ExecutionErrorKind::FunctionNotFound,
                        format!(
                            "Could not resolve function '{}' in module '{}'",
                            name, version_mid,
                        ),
                    )
                } else {
                    self.convert_linked_vm_error(e, linkage)
                }
            })?;
        let parameters = runtime_signature
            .parameters
            .into_iter()
            .map(|ty| self.adapter_type_from_vm_type(vm, &ty))
            .collect::<Result<Vec<_>, _>>()?;
        let return_ = runtime_signature
            .return_
            .into_iter()
            .map(|ty| self.adapter_type_from_vm_type(vm, &ty))
            .collect::<Result<Vec<_>, _>>()?;
        let signature = LoadedFunctionInstantiation {
            parameters,
            return_,
        };
        Ok(LoadedFunction {
            version_mid,
            original_mid,
            name,
            type_arguments,
            signature,
            linkage: linkage.clone(),
            instruction_length: runtime_signature.instruction_count,
            definition_index: runtime_signature.index,
            visibility: runtime_signature.visibility,
            is_entry: runtime_signature.is_entry,
            is_native: runtime_signature.is_native,
        })
    }

    pub fn load_type_tag(&self, idx: usize, ty: &TypeTag) -> Result<Type, Mode::Error> {
        let vm_type = self.load_vm_type_from_type_tag(Some(idx), ty)?;
        self.adapter_type_from_vm_type(self.loading_vm, &vm_type)
    }

    /// We verify that all types in the `StructTag` are defining ID-based types.
    pub fn load_type_from_struct(&self, tag: &StructTag) -> Result<Type, Mode::Error> {
        let vm_type =
            self.load_vm_type_from_type_tag(None, &TypeTag::Struct(Box::new(tag.clone())))?;
        self.adapter_type_from_vm_type(self.loading_vm, &vm_type)
    }

    pub fn type_layout_for_struct(&self, tag: &StructTag) -> Result<MoveTypeLayout, Mode::Error> {
        let ty: Type = self.load_type_from_struct(tag)?;
        self.runtime_layout(&ty)
    }

    pub fn gas_coin_type(&self) -> Result<Type, Mode::Error> {
        get_or_init_ty!(self, gas_coin_type, GasCoin::type_())
    }

    pub fn upgrade_ticket_type(&self) -> Result<Type, Mode::Error> {
        get_or_init_ty!(self, upgrade_ticket_type, UpgradeTicket::type_())
    }

    pub fn upgrade_receipt_type(&self) -> Result<Type, Mode::Error> {
        get_or_init_ty!(self, upgrade_receipt_type, UpgradeReceipt::type_())
    }

    pub fn upgrade_cap_type(&self) -> Result<Type, Mode::Error> {
        get_or_init_ty!(self, upgrade_cap_type, UpgradeCap::type_())
    }

    pub fn tx_context_type(&self) -> Result<Type, Mode::Error> {
        get_or_init_ty!(self, tx_context_type, TxContext::type_())
    }

    pub fn coin_type(&self, inner_type: Type) -> Result<Type, Mode::Error> {
        const COIN_ABILITIES: AbilitySet =
            AbilitySet::singleton(Ability::Key).union(AbilitySet::singleton(Ability::Store));
        let (a, m, n) = RESOLVED_COIN_STRUCT;
        let module = ModuleId::new(*a, m.to_owned());
        Ok(Type::Datatype(Rc::new(Datatype {
            abilities: COIN_ABILITIES,
            module,
            name: n.to_owned(),
            type_arguments: vec![inner_type],
        })))
    }

    pub fn balance_type(&self, inner_type: Type) -> Result<Type, Mode::Error> {
        const BALANCE_ABILITIES: AbilitySet = AbilitySet::singleton(Ability::Store);
        let (a, m, n) = RESOLVED_BALANCE_STRUCT;
        let module = ModuleId::new(*a, m.to_owned());
        Ok(Type::Datatype(Rc::new(Datatype {
            abilities: BALANCE_ABILITIES,
            module,
            name: n.to_owned(),
            type_arguments: vec![inner_type],
        })))
    }

    pub fn withdrawal_type(&self, inner_type: Type) -> Result<Type, Mode::Error> {
        const WITHDRAWAL_ABILITIES: AbilitySet = AbilitySet::singleton(Ability::Drop);
        let (a, m, n) = RESOLVED_WITHDRAWAL_STRUCT;
        let module = ModuleId::new(*a, m.to_owned());
        Ok(Type::Datatype(Rc::new(Datatype {
            abilities: WITHDRAWAL_ABILITIES,
            module,
            name: n.to_owned(),
            type_arguments: vec![inner_type],
        })))
    }

    pub fn allowance_withdrawal_type(&self, inner_type: Type) -> Result<Type, Mode::Error> {
        const ALLOWANCE_WITHDRAWAL_ABILITIES: AbilitySet = AbilitySet::singleton(Ability::Drop);
        let (a, m, n) = RESOLVED_ALLOWANCE_WITHDRAWAL_STRUCT;
        let module = ModuleId::new(*a, m.to_owned());
        Ok(Type::Datatype(Rc::new(Datatype {
            abilities: ALLOWANCE_WITHDRAWAL_ABILITIES,
            module,
            name: n.to_owned(),
            type_arguments: vec![inner_type],
        })))
    }

    /// Either `Withdrawal` or `AllowanceWithdrawal` depending on the source
    pub fn withdrawal_type_for_source(
        &self,
        source: &L::WithdrawalSource,
        funds_type: Type,
    ) -> Result<Type, Mode::Error> {
        match source {
            L::WithdrawalSource::Direct { .. } => self.withdrawal_type(funds_type),
            L::WithdrawalSource::Allowance { .. } => self.allowance_withdrawal_type(funds_type),
        }
    }

    pub fn vector_type(&self, element_type: Type) -> Result<Type, Mode::Error> {
        let abilities = AbilitySet::polymorphic_abilities(
            AbilitySet::VECTOR,
            [false],
            [element_type.abilities()],
        )
        .map_err(|e| {
            Mode::Error::new_with_source(ExecutionErrorKind::VMInvariantViolation, e.to_string())
        })?;
        Ok(Type::Vector(Rc::new(L::Vector {
            abilities,
            element_type,
        })))
    }

    pub fn read_object(&self, id: &ObjectID) -> Result<&Object, Mode::Error> {
        let Some(obj) = self.state_view.read_object(id) else {
            // protected by transaction input checker
            invariant_violation!("Object {:?} does not exist", id);
        };
        Ok(obj)
    }

    /// Load an adapter type using the transaction linkage.
    pub fn load_vm_type_argument_from_adapter_type(
        &self,
        idx: usize,
        ty: &Type,
    ) -> Result<vm_runtime::Type, Mode::Error> {
        self.load_vm_type_from_adapter_type(Some(idx), ty)
    }

    fn load_vm_type_from_adapter_type(
        &self,
        type_arg_idx: Option<usize>,
        ty: &Type,
    ) -> Result<vm_runtime::Type, Mode::Error> {
        let tag: TypeTag = ty.clone().try_into().map_err(|s| {
            Mode::Error::new_with_source(ExecutionErrorKind::VMInvariantViolation, s)
        })?;
        self.load_vm_type_from_type_tag(type_arg_idx, &tag)
    }

    /// Load a defining-ID-based type tag using the transaction linkage.
    fn load_vm_type_from_type_tag(
        &self,
        type_arg_idx: Option<usize>,
        tag: &TypeTag,
    ) -> Result<vm_runtime::Type, Mode::Error> {
        fn execution_error<Mode: ExecutionMode>(
            env: &Env<Mode>,
            type_arg_idx: Option<usize>,
            e: VMError,
        ) -> Mode::Error {
            if let Some(idx) = type_arg_idx {
                env.convert_type_argument_error(idx, e)
            } else {
                env.convert_linked_vm_error(e, env.unified_linkage)
            }
        }

        let ty = self
            .loading_vm
            .load_type(tag)
            .map_err(|e| execution_error(self, type_arg_idx, e))?;
        Ok(ty)
    }

    /// Converts a VM runtime Type to an adapter Type.
    pub(crate) fn adapter_type_from_vm_type(
        &self,
        vm: &MoveVM,
        vm_type: &vm_runtime::Type,
    ) -> Result<Type, Mode::Error> {
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
                let inner_ty = self.adapter_type_from_vm_type(vm, ref_ty)?;
                Type::Reference(false, Rc::new(inner_ty))
            }
            VRT::Type::MutableReference(ref_ty) => {
                let inner_ty = self.adapter_type_from_vm_type(vm, ref_ty)?;
                Type::Reference(true, Rc::new(inner_ty))
            }

            VRT::Type::Vector(inner) => {
                let element_type = self.adapter_type_from_vm_type(vm, inner)?;
                self.vector_type(element_type)?
            }
            VRT::Type::Datatype(_) => {
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
                    .map(|t| self.adapter_type_from_vm_type(vm, t))
                    .collect::<Result<Vec<_>, _>>()?;

                Type::Datatype(Rc::new(Datatype {
                    abilities,
                    module,
                    name,
                    type_arguments,
                }))
            }
        })
    }
}

pub(crate) fn convert_vm_error<E: ExecutionErrorTrait>(
    error: VMError,
    store: &VerifiedPackageStore<'_>,
    linkage: Option<&ExecutableLinkage>,
    _protocol_config: &ProtocolConfig,
) -> E {
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
                let version_id = linkage
                    .0
                    .linkage
                    .get(&(*id.address()).into())
                    .cloned()
                    .unwrap_or_else(|| ObjectID::from_address(*id.address()));
                store.get_package(&version_id).ok().flatten().and_then(|p| {
                    p.modules().get(id).map(|module| {
                        let module = module.compiled_module();
                        let fdef = module.function_def_at(function);
                        let fhandle = module.function_handle_at(fdef.function);
                        module.identifier_at(fhandle.name).to_string()
                    })
                })
            })
        },
    )
    .into()
}
