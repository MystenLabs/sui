// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module defines the shared environment, `Env`, used for the compilation/translation and
//! execution of programmable transactions. While the "context" for each pass might be different,
//! the `Env` provides consistent access to shared components such as the VM or the protocol config.

use crate::{
    data_store::{PackageStore, cached_package_store::CachedPackageStore},
    execution_value::ExecutionState,
    static_programmable_transactions::{
        env::cache::{LoadedFunctionKey, PerTxCache, TypeLinkageCacheKey},
        execution::context::subst_signature,
        linkage::{analysis::LinkageAnalyzer, resolved_linkage::ExecutableLinkage},
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
    resolver::IntraPackageName,
    runtime_value::{self, MoveTypeLayout},
    vm_status::StatusCode,
};
use move_vm_runtime::{
    execution::{self as vm_runtime, vm::MoveVM},
    runtime::MoveRuntime,
};
use std::{rc::Rc, sync::LazyLock};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    Identifier, SUI_FRAMEWORK_PACKAGE_ID, TypeTag,
    balance::RESOLVED_BALANCE_STRUCT,
    base_types::{ObjectID, TxContext},
    coin::RESOLVED_COIN_STRUCT,
    error::ExecutionError,
    execution_status::{ExecutionErrorKind, TypeArgumentError},
    funds_accumulator::RESOLVED_WITHDRAWAL_STRUCT,
    gas_coin::GasCoin,
    move_package::{UpgradeCap, UpgradeReceipt, UpgradeTicket},
    object::Object,
    type_input::{StructInput, TypeInput},
};

mod cache;

static GAS_COIN_TYPE: LazyLock<StructTag> = LazyLock::new(GasCoin::type_);
static UPGRADE_TICKET_TYPE: LazyLock<StructTag> = LazyLock::new(UpgradeTicket::type_);
static UPGRADE_RECEIPT_TYPE: LazyLock<StructTag> = LazyLock::new(UpgradeReceipt::type_);
static UPGRADE_CAP_TYPE: LazyLock<StructTag> = LazyLock::new(UpgradeCap::type_);
static TX_CONTEXT_TYPE: LazyLock<StructTag> = LazyLock::new(TxContext::type_);

pub struct Env<'pc, 'vm, 'state, 'linkage, 'extensions> {
    pub protocol_config: &'pc ProtocolConfig,
    pub vm: &'vm MoveRuntime,
    pub state_view: &'state mut dyn ExecutionState,
    pub linkable_store: &'linkage CachedPackageStore<'state, 'vm>,
    pub linkage_analysis: &'linkage LinkageAnalyzer,
    // The VM used for type resolution of input types (and types statically present in the PTB)
    // only. This VM should only be used for resolution of input types, but should not be used for
    // resolution around function calls, execution, or final serialization of execution values.
    input_type_resolution_vm: &'linkage MoveVM<'extensions>,
    per_tx_cache: PerTxCache<'pc>,
}

impl<'pc, 'vm, 'state, 'linkage, 'extensions> Env<'pc, 'vm, 'state, 'linkage, 'extensions> {
    pub fn new(
        protocol_config: &'pc ProtocolConfig,
        vm: &'vm MoveRuntime,
        state_view: &'state mut dyn ExecutionState,
        linkable_store: &'linkage CachedPackageStore<'state, 'vm>,
        linkage_analysis: &'linkage LinkageAnalyzer,
        input_type_resolution_vm: &'linkage MoveVM<'extensions>,
    ) -> Self {
        Self {
            protocol_config,
            vm,
            state_view,
            linkable_store,
            linkage_analysis,
            input_type_resolution_vm,
            per_tx_cache: PerTxCache::new(protocol_config),
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
        let argument_idx = match checked_as!(idx, TypeParameterIndex) {
            Err(e) => return e,
            Ok(v) => v,
        };
        match e.major_status() {
            StatusCode::NUMBER_OF_TYPE_ARGUMENTS_MISMATCH => {
                ExecutionErrorKind::TypeArityMismatch.into()
            }
            StatusCode::EXTERNAL_RESOLUTION_REQUEST_ERROR => {
                ExecutionErrorKind::TypeArgumentError {
                    argument_idx,
                    kind: TypeArgumentError::TypeNotFound,
                }
                .into()
            }
            StatusCode::CONSTRAINT_NOT_SATISFIED => ExecutionErrorKind::TypeArgumentError {
                argument_idx,
                kind: TypeArgumentError::ConstraintNotSatisfied,
            }
            .into(),
            _ => self.convert_linked_vm_error(e, linkage),
        }
    }

    /// Resolve an adapter `Type` to its `TypeTag`, consulting (and populating) the per-tx cache.
    fn tag_from_type(&self, ty: &Type) -> Result<Rc<TypeTag>, ExecutionError> {
        if let Some(rc) = self.per_tx_cache.lookup_tag_by_type(ty)? {
            return Ok(rc);
        }
        let tag: TypeTag = ty.clone().try_into().map_err(|s| {
            ExecutionError::new_with_source(ExecutionErrorKind::VMInvariantViolation, s)
        })?;
        let (rc, _) = self.per_tx_cache.insert_tag_type_pair(tag, ty.clone())?;
        Ok(rc)
    }

    pub fn fully_annotated_layout(
        &self,
        ty: &Type,
    ) -> Result<annotated_value::MoveTypeLayout, ExecutionError> {
        let tag = self.tag_from_type(ty)?;
        let tag_linkage = self.get_type_linkage(&tag)?;
        self.input_type_resolution_vm
            .annotated_type_layout(&tag)
            .map_err(|e| self.convert_linked_vm_error(e, &tag_linkage))
    }

    pub fn runtime_layout(
        &self,
        ty: &Type,
    ) -> Result<runtime_value::MoveTypeLayout, ExecutionError> {
        let tag = self.tag_from_type(ty)?;
        let tag_linkage = self.get_type_linkage(&tag)?;
        self.input_type_resolution_vm
            .runtime_type_layout(&tag)
            .map_err(|e| self.convert_linked_vm_error(e, &tag_linkage))
    }

    pub fn load_framework_function(
        &self,
        module: &IdentStr,
        function: &IdentStr,
        type_arguments: Vec<Type>,
    ) -> Result<Rc<LoadedFunction>, ExecutionError> {
        self.load_function(
            SUI_FRAMEWORK_PACKAGE_ID,
            module.to_string(),
            function.to_string(),
            type_arguments,
        )
    }

    pub fn load_function(
        &self,
        package: ObjectID,
        module: String,
        function: String,
        type_arguments: Vec<Type>,
    ) -> Result<Rc<LoadedFunction>, ExecutionError> {
        let module = to_identifier(module)?;
        let name = to_identifier(function)?;

        let cache_key = LoadedFunctionKey::new(
            package,
            module.clone(),
            name.clone(),
            type_arguments.clone(),
        );
        if let Some(cached) = self.per_tx_cache.lookup_function(&cache_key)? {
            return Ok(cached);
        }

        let linkage = self.linkage_analysis.compute_call_linkage(
            &package,
            module.as_ident_str(),
            name.as_ident_str(),
            &type_arguments,
            self.linkable_store,
        )?;

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
        // NB: We cannot use the resolution VM here because the linkage for that unifies up, and if
        // this is a private entry function, it may have been removed in future versions of the
        // package.
        let vm = self
            .vm
            .make_vm(
                &self.linkable_store.package_store,
                linkage.linkage_context()?,
            )
            .map_err(|e| self.convert_linked_vm_error(e, &linkage))?;
        let runtime_signature = vm
            .function_information(&original_mid, name.as_ident_str(), &loaded_type_arguments)
            .map_err(|e| {
                if e.major_status() == StatusCode::EXTERNAL_RESOLUTION_REQUEST_ERROR {
                    ExecutionError::new_with_source(
                        ExecutionErrorKind::FunctionNotFound,
                        format!(
                            "Could not resolve function '{}' in module '{}'",
                            name, &version_mid,
                        ),
                    )
                } else {
                    self.convert_linked_vm_error(e, &linkage)
                }
            })?;
        let runtime_signature = subst_signature(runtime_signature, &loaded_type_arguments)
            .map_err(|e| self.convert_linked_vm_error(e, &linkage))?;
        let parameters = runtime_signature
            .parameters
            .into_iter()
            .map(|ty| self.adapter_type_from_vm_type(&vm, &ty))
            .collect::<Result<Vec<_>, _>>()?;
        let return_ = runtime_signature
            .return_
            .into_iter()
            .map(|ty| self.adapter_type_from_vm_type(&vm, &ty))
            .collect::<Result<Vec<_>, _>>()?;
        let signature = LoadedFunctionInstantiation {
            parameters,
            return_,
        };
        let loaded = Rc::new(LoadedFunction {
            version_mid,
            original_mid,
            name,
            type_arguments,
            signature,
            linkage,
            instruction_length: runtime_signature.instruction_count,
            definition_index: runtime_signature.index,
            visibility: runtime_signature.visibility,
            is_entry: runtime_signature.is_entry,
            is_native: runtime_signature.is_native,
        });
        self.per_tx_cache
            .insert_function(cache_key, loaded.clone())?;
        Ok(loaded)
    }

    pub fn load_type_input(&self, idx: usize, ty: TypeInput) -> Result<Type, ExecutionError> {
        if let Some(cached) = self.per_tx_cache.lookup_type_by_input(&ty)? {
            return Ok(cached);
        }
        let vm_type = self.load_vm_type_from_type_input(idx, ty.clone())?;
        let adapter = self.adapter_type_from_vm_type(self.input_type_resolution_vm, &vm_type)?;
        self.per_tx_cache.insert_type_input(ty, adapter.clone())?;
        Ok(adapter)
    }

    pub fn load_type_tag(&self, idx: usize, ty: &TypeTag) -> Result<Type, ExecutionError> {
        if let Some(cached) = self.per_tx_cache.lookup_type_by_tag(ty)? {
            return Ok(cached);
        }
        let vm_type = self.load_vm_type_from_type_tag(Some(idx), ty)?;
        let adapter = self.adapter_type_from_vm_type(self.input_type_resolution_vm, &vm_type)?;
        self.per_tx_cache
            .insert_tag_type_pair(ty.clone(), adapter.clone())?;
        Ok(adapter)
    }

    /// We verify that all types in the `StructTag` are defining ID-based types.
    pub fn load_type_from_struct(&self, tag: &StructTag) -> Result<Type, ExecutionError> {
        let tag = TypeTag::Struct(Box::new(tag.clone()));
        if let Some(cached) = self.per_tx_cache.lookup_type_by_tag(&tag)? {
            return Ok(cached);
        }
        let vm_type = self.load_vm_type_from_type_tag(None, &tag)?;
        let adapter = self.adapter_type_from_vm_type(self.input_type_resolution_vm, &vm_type)?;
        self.per_tx_cache
            .insert_tag_type_pair(tag, adapter.clone())?;
        Ok(adapter)
    }

    pub fn type_layout_for_struct(
        &self,
        tag: &StructTag,
    ) -> Result<MoveTypeLayout, ExecutionError> {
        let ty: Type = self.load_type_from_struct(tag)?;
        self.runtime_layout(&ty)
    }

    pub fn gas_coin_type(&self) -> Result<Type, ExecutionError> {
        self.load_type_from_struct(&GAS_COIN_TYPE)
    }

    pub fn upgrade_ticket_type(&self) -> Result<Type, ExecutionError> {
        self.load_type_from_struct(&UPGRADE_TICKET_TYPE)
    }

    pub fn upgrade_receipt_type(&self) -> Result<Type, ExecutionError> {
        self.load_type_from_struct(&UPGRADE_RECEIPT_TYPE)
    }

    pub fn upgrade_cap_type(&self) -> Result<Type, ExecutionError> {
        self.load_type_from_struct(&UPGRADE_CAP_TYPE)
    }

    pub fn tx_context_type(&self) -> Result<Type, ExecutionError> {
        self.load_type_from_struct(&TX_CONTEXT_TYPE)
    }

    pub fn coin_type(&self, inner_type: Type) -> Result<Type, ExecutionError> {
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

    pub fn balance_type(&self, inner_type: Type) -> Result<Type, ExecutionError> {
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

    pub fn withdrawal_type(&self, inner_type: Type) -> Result<Type, ExecutionError> {
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
        if let Some(cached) = self.per_tx_cache.lookup_vm_type_by_type(ty)? {
            return Ok((*cached).clone());
        }
        let tag = self.tag_from_type(ty)?;
        let vm_type = self.load_vm_type_from_type_tag(type_arg_idx, &tag)?;
        self.per_tx_cache
            .insert_vm_type_pair(vm_type.clone(), ty.clone())?;
        Ok(vm_type)
    }

    /// Take a type tag and returns a VM runtime Type and the linkage for it.
    fn load_vm_type_from_type_tag(
        &self,
        type_arg_idx: Option<usize>,
        tag: &TypeTag,
    ) -> Result<vm_runtime::Type, ExecutionError> {
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

        let tag_linkage = self.get_type_linkage(tag)?;
        let ty = self
            .input_type_resolution_vm
            .load_type(tag)
            .map_err(|e| execution_error(self, type_arg_idx, e, &tag_linkage))?;
        Ok(ty)
    }

    fn get_type_linkage(&self, tag: &TypeTag) -> Result<ExecutableLinkage, ExecutionError> {
        let root_ids = tag.all_addresses();
        let key = TypeLinkageCacheKey::new(&root_ids);
        if let Some(cached) = self.per_tx_cache.lookup_type_linkage(&key)? {
            return Ok(cached);
        }
        let linkage = ExecutableLinkage::type_linkage(
            self.linkage_analysis.config().clone(),
            root_ids.into_iter().map(ObjectID::from),
            self.linkable_store,
        )?;
        self.per_tx_cache
            .insert_type_linkage(key, linkage.clone())?;
        Ok(linkage)
    }

    /// Converts a VM runtime Type to an adapter Type.
    pub(crate) fn adapter_type_from_vm_type(
        &self,
        vm: &MoveVM,
        vm_type: &vm_runtime::Type,
    ) -> Result<Type, ExecutionError> {
        use vm_runtime as VRT;

        if let Some(cached) = self.per_tx_cache.lookup_type_by_vm_type(vm_type)? {
            return Ok(cached);
        }

        let ty = match vm_type {
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

            VRT::Type::TyParam(_) => {
                invariant_violation!(
                    "Unexpected type parameter in VM type: {:?}. This should not happen as we should \
                     have resolved all type parameters before this point.",
                    vm_type
                );
            }
        };
        self.per_tx_cache
            .insert_vm_type_pair(vm_type.clone(), ty.clone())?;
        Ok(ty)
    }

    /// Load a `TypeInput` into a VM runtime `Type` and its `Linkage`. Loading into the VM ensures
    /// that any adapter type or type tag that results from this is properly output with defining
    /// IDs.
    fn load_vm_type_from_type_input(
        &self,
        type_arg_idx: usize,
        ty: TypeInput,
    ) -> Result<vm_runtime::Type, ExecutionError> {
        fn to_type_tag_internal(
            env: &Env,
            type_arg_idx: usize,
            ty: &TypeInput,
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
                    let inner = to_type_tag_internal(env, type_arg_idx, type_input)?;
                    TypeTag::Vector(Box::new(inner))
                }
                TypeInput::Struct(struct_input) => {
                    let StructInput {
                        address,
                        module,
                        name,
                        type_params,
                    } = &**struct_input;

                    let pkg = env
                        .linkable_store
                        .get_package(&ObjectID::from(*address))
                        .ok()
                        .flatten()
                        .ok_or_else(|| {
                            let argument_idx = match checked_as!(type_arg_idx, u16) {
                                Err(e) => return e,
                                Ok(v) => v,
                            };
                            ExecutionError::from_kind(ExecutionErrorKind::TypeArgumentError {
                                argument_idx,
                                kind: TypeArgumentError::TypeNotFound,
                            })
                        })?;
                    let module = to_identifier(module.clone())?;
                    let name = to_identifier(name.clone())?;
                    let tid = IntraPackageName {
                        module_name: module,
                        type_name: name,
                    };
                    let Some(resolved_address) = pkg.type_origin_table().get(&tid).cloned() else {
                        return Err(ExecutionError::from_kind(
                            ExecutionErrorKind::TypeArgumentError {
                                argument_idx: checked_as!(type_arg_idx, u16)?,
                                kind: TypeArgumentError::TypeNotFound,
                            },
                        ));
                    };

                    let tys = type_params
                        .iter()
                        .map(|tp| to_type_tag_internal(env, type_arg_idx, tp))
                        .collect::<Result<Vec<_>, _>>()?;
                    TypeTag::Struct(Box::new(StructTag {
                        address: resolved_address,
                        module: tid.module_name,
                        name: tid.type_name,
                        type_params: tys,
                    }))
                }
            })
        }
        let tag = to_type_tag_internal(self, type_arg_idx, &ty)?;
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
    _protocol_config: &ProtocolConfig,
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
}
