// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    sync::Arc,
};

use anyhow::Result;
use move_binary_format::{
    access::ModuleAccess,
    binary_views::BinaryIndexedView,
    file_format::{AbilitySet, CompiledModule, LocalIndex, SignatureToken, StructHandleIndex},
};
use move_bytecode_utils::{format_signature_token, resolve_struct};
use move_bytecode_verifier::{verify_module_with_config, VerifierConfig};
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{ModuleId, StructTag, TypeTag},
    vm_status::StatusCode,
};
pub use move_vm_runtime::move_vm::MoveVM;
use move_vm_runtime::{
    config::{VMConfig, VMRuntimeLimitsConfig},
    native_extensions::NativeContextExtensions,
    native_functions::NativeFunctionTable,
};
use tracing::instrument;

use crate::{
    execution_mode::ExecutionMode,
    programmable_transactions::execution::{bcs_argument_validate, PrimitiveArgumentLayout},
};
use sui_move_natives::{object_runtime::ObjectRuntime, NativesCostTable};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::*,
    error::ExecutionError,
    error::{ExecutionErrorKind, SuiError},
    id::RESOLVED_SUI_ID,
    is_primitive,
    messages::{InputObjectKind, ObjectArg},
    metrics::LimitsMetrics,
    object::{Data, Object, Owner},
    storage::ChildObjectResolver,
};

sui_macros::checked_arithmetic! {

pub fn default_verifier_config(
    protocol_config: &ProtocolConfig,
    is_metered: bool,
) -> VerifierConfig {
    let (
        max_back_edges_per_function,
        max_back_edges_per_module,
        max_per_fun_meter_units,
        max_per_mod_meter_units,
    ) = if is_metered {
        (
            Some(protocol_config.max_back_edges_per_function() as usize),
            Some(protocol_config.max_back_edges_per_module() as usize),
            Some(protocol_config.max_verifier_meter_ticks_per_function() as u128),
            Some(protocol_config.max_meter_ticks_per_module() as u128),
        )
    } else {
        (None, None, None, None)
    };

    VerifierConfig {
        max_loop_depth: Some(protocol_config.max_loop_depth() as usize),
        max_generic_instantiation_length: Some(
            protocol_config.max_generic_instantiation_length() as usize
        ),
        max_function_parameters: Some(protocol_config.max_function_parameters() as usize),
        max_basic_blocks: Some(protocol_config.max_basic_blocks() as usize),
        max_value_stack_size: protocol_config.max_value_stack_size() as usize,
        max_type_nodes: Some(protocol_config.max_type_nodes() as usize),
        max_push_size: Some(protocol_config.max_push_size() as usize),
        max_dependency_depth: Some(protocol_config.max_dependency_depth() as usize),
        max_fields_in_struct: Some(protocol_config.max_fields_in_struct() as usize),
        max_function_definitions: Some(protocol_config.max_function_definitions() as usize),
        max_struct_definitions: Some(protocol_config.max_struct_definitions() as usize),
        max_constant_vector_len: Some(protocol_config.max_move_vector_len()),
        max_back_edges_per_function,
        max_back_edges_per_module,
        max_basic_blocks_in_script: None,
        max_per_fun_meter_units,
        max_per_mod_meter_units,
    }
}

pub fn new_move_vm(
    natives: NativeFunctionTable,
    protocol_config: &ProtocolConfig,
    paranoid_type_checks: bool,
) -> Result<MoveVM, SuiError> {
    MoveVM::new_with_config(
        natives,
        VMConfig {
            verifier: default_verifier_config(
                protocol_config,
                false, /* we do not enable metering in execution*/
            ),
            max_binary_format_version: protocol_config.move_binary_format_version(),
            paranoid_type_checks,
            runtime_limits_config: VMRuntimeLimitsConfig {
                vector_len_max: protocol_config.max_move_vector_len(),
            },
            enable_invariant_violation_check_in_swap_loc:
                !protocol_config.disable_invariant_violation_check_in_swap_loc(),
        },
    )
    .map_err(|_| SuiError::ExecutionInvariantViolation)
}

pub fn new_native_extensions<'r>(
    child_resolver: &'r impl ChildObjectResolver,
    input_objects: BTreeMap<ObjectID, Owner>,
    is_metered: bool,
    protocol_config: &ProtocolConfig,
    metrics: Arc<LimitsMetrics>,
) -> NativeContextExtensions<'r> {
    let mut extensions = NativeContextExtensions::default();
    extensions.add(ObjectRuntime::new(
        Box::new(child_resolver),
        input_objects,
        is_metered,
        protocol_config,
        metrics,
    ));
    extensions.add(NativesCostTable::from_protocol_config(protocol_config));
    extensions
}

/// Given a list of `modules` and an `object_id`, mutate each module's self ID (which must be
/// 0x0) to be `object_id`.
pub fn substitute_package_id(
    modules: &mut [CompiledModule],
    object_id: ObjectID,
) -> Result<(), ExecutionError> {
    let new_address = AccountAddress::from(object_id);

    for module in modules.iter_mut() {
        let self_handle = module.self_handle().clone();
        let self_address_idx = self_handle.address;

        let addrs = &mut module.address_identifiers;
        let Some(address_mut) = addrs.get_mut(self_address_idx.0 as usize) else {
            let name = module.identifier_at(self_handle.name);
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::PublishErrorNonZeroAddress,
                format!("Publishing module {name} with invalid address index"),
            ));
        };

        if *address_mut != AccountAddress::ZERO {
            let name = module.identifier_at(self_handle.name);
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::PublishErrorNonZeroAddress,
                format!("Publishing module {name} with non-zero address is not allowed"),
            ));
        };

        *address_mut = new_address;
    }

    Ok(())
}

/// Small enum used to type check function calls for external tools. ObjVec does not exist
/// for Programmable Transactions, but it effectively does via the command MakeMoveVec
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckCallArg {
    /// contains no structs or objects
    Pure(Vec<u8>),
    /// an object
    Object(ObjectArg),
    /// a vector of objects
    ObjVec(Vec<ObjectArg>),
}

/// - Check that `package_object`, `module` and `function` are valid
/// - Check that the the signature of `function` is well-typed w.r.t `type_args`, `object_args`, and `pure_args`
/// - Return the ID of the resolved module, a vector of BCS encoded arguments to pass to the VM, and a partitioning
/// of the input objects into objects passed by value vs by mutable reference
pub fn resolve_and_type_check<Mode: ExecutionMode>(
    objects: &BTreeMap<ObjectID, impl Borrow<Object>>,
    module: &CompiledModule,
    function: &Identifier,
    type_args: &[TypeTag],
    args: Vec<CheckCallArg>,
    is_genesis: bool,
) -> anyhow::Result<()> {
    // Resolve the function we are calling
    let view = &BinaryIndexedView::Module(module);
    let function_str = function.as_ident_str();
    let module_id = module.self_id();
    let fdef_opt = module.function_defs.iter().find(|fdef| {
        module.identifier_at(module.function_handle_at(fdef.function).name) == function_str
    });
    let fdef = match fdef_opt {
        Some(fdef) => fdef,
        None => {
            anyhow::bail!(
                "Could not resolve function '{}' in module {}",
                function,
                &module_id,
            )
        }
    };
    // Check for entry modifier, but ignore for genesis or dev-inspect.
    // Genesis calls non-entry, private functions, and bypasses this rule. This is helpful for
    // ensuring the functions are not called again later.
    // In other words, this is an implementation detail that we are using `execute` for genesis
    // functions, and as such need to bypass this check.
    // Similarly, we will bypass this check for dev-inspect, as the mode does not make state changes
    // and is just for developers to check the result of Move functions. This mode is flagged by
    // allow_arbitrary_function_calls
    if !fdef.is_entry && !is_genesis && !Mode::allow_arbitrary_function_calls() {
        anyhow::bail!("Can only call `entry` functions",)
    }
    let fhandle = module.function_handle_at(fdef.function);

    // check arity of type and value arguments
    if fhandle.type_parameters.len() != type_args.len() {
        anyhow::bail!(
            "Expected {:?} type arguments, but found {:?}",
            fhandle.type_parameters.len(),
            type_args.len()
        );
    }

    // total number of args is (|objects| + |pure_args|) + 1 for the the `TxContext` object
    let parameters = &module.signature_at(fhandle.parameters).0;
    let tx_ctx_kind = parameters
        .last()
        .map(|t| TxContext::kind(view, t))
        .unwrap_or(TxContextKind::None);

    let num_args = if tx_ctx_kind != TxContextKind::None {
        args.len() + 1
    } else {
        args.len()
    };
    if parameters.len() != num_args {
        anyhow::bail!(
            "Expected {:?} arguments calling function '{}', but found {:?}",
            parameters.len(),
            function,
            num_args
        )
    }

    // type check object arguments passed in by value and by reference
    let mut by_value_objects = BTreeSet::new();

    // Track the mapping from each input object to its Move type.
    // This will be needed latter in `check_child_object_of_shared_object`.
    let mut object_type_map = BTreeMap::new();
    for (idx, arg) in args.into_iter().enumerate() {
        let param_type = &parameters[idx];
        let idx = idx as LocalIndex;
        match arg {
            // dev-inspect does not make state changes and just a developer aid, so let through
            // any BCS bytes (they will be checked later by the VM)
            CheckCallArg::Pure(_) if Mode::allow_arbitrary_values() => break,
            CheckCallArg::Pure(arg) => {
                // optimistically assume all type args are objects
                // actually checking this requires loading other modules
                let type_arg_abilities = &type_args
                    .iter()
                    .map(|_| AbilitySet::PRIMITIVES)
                    .collect::<Vec<_>>();
                if !is_primitive(view, type_arg_abilities, param_type) {
                    anyhow::bail!(
                        "Non-primitive argument at index {}. If it is an object, it must be \
                        populated by an object ID",
                        idx,
                    );
                }
                if let Some(layout) = additional_validation_layout(view, param_type) {
                    bcs_argument_validate(&arg, idx as u16, layout)?;
                }
            }
            CheckCallArg::Object(ObjectArg::ImmOrOwnedObject(ref_)) => {
                let (arg_type, param_type) = serialize_object(
                    InputObjectKind::ImmOrOwnedMoveObject(ref_),
                    idx,
                    param_type,
                    objects,
                    &mut by_value_objects,
                    &mut object_type_map,
                )?;
                type_check_struct(view, type_args, arg_type, param_type)?;
            }
            CheckCallArg::Object(ObjectArg::SharedObject {
                id,
                initial_shared_version,
                mutable,
            }) => {
                let (arg_type, param_type) = serialize_object(
                    InputObjectKind::SharedMoveObject {
                        id,
                        initial_shared_version,
                        mutable,
                    },
                    idx,
                    param_type,
                    objects,
                    &mut by_value_objects,
                    &mut object_type_map,
                )?;
                type_check_struct(view, type_args, arg_type, param_type)?;
            }
            CheckCallArg::ObjVec(vec) => {
                for arg in vec {
                    let object_kind = match arg {
                        ObjectArg::ImmOrOwnedObject(ref_) => {
                            InputObjectKind::ImmOrOwnedMoveObject(ref_)
                        }
                        ObjectArg::SharedObject {
                            id,
                            initial_shared_version,
                            mutable,
                        } => InputObjectKind::SharedMoveObject {
                            id,
                            initial_shared_version,
                            mutable,
                        },
                    };
                    let (arg_type, param_type) = serialize_object(
                        object_kind,
                        idx,
                        param_type,
                        objects,
                        &mut by_value_objects,
                        &mut object_type_map,
                    )?;
                    type_check_struct(view, type_args, arg_type, param_type)?;
                }
            }
        };
    }

    check_shared_object_rules(
        objects,
        &by_value_objects,
        &object_type_map,
        module.self_id(),
    )?;

    Ok(())
}

fn additional_validation_layout(
    view: &BinaryIndexedView,
    param_type: &SignatureToken,
) -> Option<PrimitiveArgumentLayout> {
    match param_type {
        // should be ruled out above
        SignatureToken::Reference(_)
        | SignatureToken::MutableReference(_)
        | SignatureToken::Signer => None,
        // optimistically assume does not need validation (but it might)
        // actually checking this requires a VM instance to load the type arguments
        SignatureToken::TypeParameter(_) => None,
        // primitives
        SignatureToken::Bool => Some(PrimitiveArgumentLayout::Bool),
        SignatureToken::U8 => Some(PrimitiveArgumentLayout::U8),
        SignatureToken::U16 => Some(PrimitiveArgumentLayout::U16),
        SignatureToken::U32 => Some(PrimitiveArgumentLayout::U32),
        SignatureToken::U64 => Some(PrimitiveArgumentLayout::U64),
        SignatureToken::U128 => Some(PrimitiveArgumentLayout::U128),
        SignatureToken::U256 => Some(PrimitiveArgumentLayout::U256),
        SignatureToken::Address => Some(PrimitiveArgumentLayout::Address),

        SignatureToken::Vector(inner) => additional_validation_layout(view, inner)
            .map(|layout| PrimitiveArgumentLayout::Vector(Box::new(layout))),
        SignatureToken::StructInstantiation(idx, targs) => {
            let resolved_struct = resolve_struct(view, *idx);
            if resolved_struct == RESOLVED_STD_OPTION && targs.len() == 1 {
                additional_validation_layout(view, &targs[0])
                    .map(|layout| PrimitiveArgumentLayout::Option(Box::new(layout)))
            } else {
                None
            }
        }
        SignatureToken::Struct(idx) => {
            let resolved_struct = resolve_struct(view, *idx);
            if resolved_struct == RESOLVED_SUI_ID {
                Some(PrimitiveArgumentLayout::Address)
            } else if resolved_struct == RESOLVED_ASCII_STR {
                Some(PrimitiveArgumentLayout::Ascii)
            } else if resolved_struct == RESOLVED_UTF8_STR {
                Some(PrimitiveArgumentLayout::UTF8)
            } else {
                None
            }
        }
    }
}

/// Serialize object with ID encoded in object_kind and also verify if various object properties are
/// correct.
fn serialize_object<'a>(
    object_kind: InputObjectKind,
    idx: LocalIndex,
    param_type: &'a SignatureToken,
    objects: &'a BTreeMap<ObjectID, impl Borrow<Object>>,
    by_value_objects: &mut BTreeSet<ObjectID>,
    object_type_map: &mut BTreeMap<ObjectID, ModuleId>,
) -> anyhow::Result<(&'a MoveObjectType, &'a SignatureToken)> {
    let object_id = object_kind.object_id();
    let object = match objects.get(&object_id) {
        Some(object) => object.borrow(),
        None => {
            debug_assert!(
                false,
                "Object map not populated for arg {} with id {}",
                idx, object_id
            );
            return Err(ExecutionErrorKind::InvariantViolation.into());
        }
    };
    match object_kind {
        InputObjectKind::ImmOrOwnedMoveObject(_) if object.is_shared() => {
            anyhow::bail!(
                "Argument at index {} populated with shared object id {} \
                        but an immutable or owned object was expected",
                idx,
                object_id
            );
        }
        InputObjectKind::SharedMoveObject { mutable, .. } => {
            if !object.is_shared() {
                anyhow::bail!(
                    "Argument at index {} populated with an immutable or owned object id {} \
                            but an shared object was expected",
                    idx,
                    object_id
                )
            }
            // Immutable shared object can only pass as immutable reference to move call
            if !mutable {
                match param_type {
                    SignatureToken::Reference(_) => {} // ok
                    SignatureToken::MutableReference(_) => {
                        anyhow::bail!(
                            "Argument at index {} populated with an immutable shared object id {} \
                            but move call takes mutable object reference",
                            idx,
                            object_id
                        )
                    }
                    _ => {
                        anyhow::bail!(
                            "Shared objects cannot be passed by-value, \
                                    violation found in argument {}",
                            idx
                        );
                    }
                }
            }
        }
        _ => (),
    }

    let move_object = match &object.data {
        Data::Move(m) => m,
        Data::Package(_) => {
            let for_vector = matches!(param_type, SignatureToken::Vector { .. });
            anyhow::bail!(
                "Found module {} argument, but function expects {:?}",
                if for_vector { "element in vector" } else { "" },
                param_type
            );
        }
    };

    // check that m.type_ matches the parameter types of the function
    let inner_param_type = inner_param_type(
        object,
        object_id,
        idx,
        param_type,
        move_object.type_(),
        by_value_objects,
    )?;

    object_type_map.insert(object_id, move_object.type_().module_id());
    Ok((move_object.type_(), inner_param_type))
}

/// Get "inner" type of an object passed as argument (e.g., an inner type of a reference or of a
/// vector) and also verify if various object properties are correct.
fn inner_param_type<'a>(
    object: &Object,
    object_id: ObjectID,
    idx: LocalIndex,
    param_type: &'a SignatureToken,
    arg_type: &MoveObjectType,
    by_value_objects: &mut BTreeSet<ObjectID>,
) -> anyhow::Result<&'a SignatureToken> {
    if let Owner::ObjectOwner(parent) = &object.owner {
        anyhow::bail!(
            "Cannot take a child object as an argyment. Object {object_id} is a child of {}",
            *parent
        )
    }
    match &param_type {
        SignatureToken::Reference(inner_t) => Ok(&**inner_t),
        SignatureToken::MutableReference(inner_t) => {
            if object.is_immutable() {
                anyhow::bail!(
                    "Argument {} is expected to be mutable, immutable object found",
                    idx
                )
            }
            Ok(&**inner_t)
        }
        SignatureToken::Vector(inner_t) => {
            inner_param_type(object, object_id, idx, inner_t, arg_type, by_value_objects)
        }
        t @ SignatureToken::Struct(_)
        | t @ SignatureToken::StructInstantiation(_, _)
        | t @ SignatureToken::TypeParameter(_) => {
            match &object.owner {
                Owner::AddressOwner(_) | Owner::ObjectOwner(_) => (),
                Owner::Shared { .. } | Owner::Immutable => {
                    anyhow::bail!(
                        "Immutable and shared objects cannot be passed by-value, \
                                    violation found in argument {}",
                        idx
                    )
                }
            }
            by_value_objects.insert(object_id);
            Ok(t)
        }
        t => anyhow::bail!(
            "Found object argument {}, but function expects {:?}",
            arg_type,
            t
        ),
    }
}

/// Check rules for shared object + by-value child rules and rules for by-value shared object rules:
/// - For each pair of a shared object and a descendant of it (through object ownership), if the
///   descendant is used by-value, at least one of the types of the shared object and the descendant
///   must be defined in the same module as the entry function being called
///   (somewhat similar to Rust's orphan rule where a trait impl must be defined in the same crate
///   as the type implementing the trait or the trait itself).
/// - For each shared object used by-value, the type of the shared object must be defined in the
///   same module as the entry function being called.
fn check_shared_object_rules(
    _objects: &BTreeMap<ObjectID, impl Borrow<Object>>,
    _by_value_objects: &BTreeSet<ObjectID>,
    _object_type_map: &BTreeMap<ObjectID, ModuleId>,
    _current_module: ModuleId,
) -> Result<(), ExecutionError> {
    // TODO not yet supported
    // // check shared object by value rule
    // let by_value_shared_object = object_owner_map
    //     .iter()
    //     .filter(|(id, owner)| matches!(owner, Owner::Shared) && by_value_objects.contains(id))
    //     .map(|(id, _)| *id);
    // for shared_object_id in by_value_shared_object {
    //     let shared_object_module = object_type_map.get(&shared_object_id).unwrap();
    //     if shared_object_module != &current_module {
    //         return Err(ExecutionError::new_with_source(
    //             ExecutionErrorKind::invalid_shared_by_value(shared_object_id),
    //             format!(
    //     "When a shared object is passed as an owned Move value in an entry function, either the \
    //     the shared object's type must be defined in the same module as the called function. The \
    //     shared object {shared_object_id} (defined in module '{shared_object_module}') is not \
    //     defined in this module '{current_module}'",
    //             ),
    //         ));
    //     }
    // }
    Ok(())
}

fn type_check_struct(
    view: &BinaryIndexedView,
    function_type_arguments: &[TypeTag],
    arg_type: &MoveObjectType,
    param_type: &SignatureToken,
) -> anyhow::Result<()> {
    if !move_type_equals_sig_token(view, function_type_arguments, arg_type, param_type) {
        anyhow::bail!(
            "Expected argument of type {}, but found type {}",
            format_signature_token(view, param_type),
            arg_type
        )
    }
    Ok(())
}

fn type_tag_equals_sig_token(
    view: &BinaryIndexedView,
    function_type_arguments: &[TypeTag],
    arg_type: &TypeTag,
    param_type: &SignatureToken,
) -> bool {
    match (arg_type, param_type) {
        (TypeTag::Bool, SignatureToken::Bool)
        | (TypeTag::U8, SignatureToken::U8)
        | (TypeTag::U64, SignatureToken::U64)
        | (TypeTag::U128, SignatureToken::U128)
        | (TypeTag::Address, SignatureToken::Address)
        | (TypeTag::Signer, SignatureToken::Signer) => true,

        (TypeTag::Vector(inner_arg_type), SignatureToken::Vector(inner_param_type)) => {
            type_tag_equals_sig_token(
                view,
                function_type_arguments,
                inner_arg_type,
                inner_param_type,
            )
        }

        (TypeTag::Struct(arg_struct), SignatureToken::Struct(_))
        | (TypeTag::Struct(arg_struct), SignatureToken::StructInstantiation(_, _)) => {
            struct_tag_equals_sig_token(view, function_type_arguments, arg_struct, param_type)
        }

        (_, SignatureToken::TypeParameter(idx)) => {
            arg_type == &function_type_arguments[*idx as usize]
        }
        _ => false,
    }
}

fn move_type_equals_sig_token(
    view: &BinaryIndexedView,
    function_type_arguments: &[TypeTag],
    arg_type: &MoveObjectType,
    param_type: &SignatureToken,
) -> bool {
    match param_type {
        SignatureToken::Struct(idx) => {
            move_type_equals_struct_inst(view, function_type_arguments, arg_type, *idx, &[])
        }
        SignatureToken::StructInstantiation(idx, args) => {
            move_type_equals_struct_inst(view, function_type_arguments, arg_type, *idx, args)
        }
        SignatureToken::TypeParameter(idx) => match &function_type_arguments[*idx as usize] {
            TypeTag::Struct(s) => arg_type.is(s),
            _ => false,
        },
        _ => false,
    }
}

fn struct_tag_equals_sig_token(
    view: &BinaryIndexedView,
    function_type_arguments: &[TypeTag],
    arg_type: &StructTag,
    param_type: &SignatureToken,
) -> bool {
    match param_type {
        SignatureToken::Struct(idx) => {
            struct_tag_equals_struct_inst(view, function_type_arguments, arg_type, *idx, &[])
        }
        SignatureToken::StructInstantiation(idx, args) => {
            struct_tag_equals_struct_inst(view, function_type_arguments, arg_type, *idx, args)
        }
        SignatureToken::TypeParameter(idx) => match &function_type_arguments[*idx as usize] {
            TypeTag::Struct(s) => arg_type == &**s,
            _ => false,
        },
        _ => false,
    }
}

fn move_type_equals_struct_inst(
    view: &BinaryIndexedView,
    function_type_arguments: &[TypeTag],
    arg_type: &MoveObjectType,
    param_type: StructHandleIndex,
    param_type_arguments: &[SignatureToken],
) -> bool {
    let (address, module_name, struct_name) = resolve_struct(view, param_type);
    let arg_type_params = arg_type.type_params();

    // same address, module, name, and type parameters
    &arg_type.address() == address
        && arg_type.module() == module_name
        && arg_type.name() == struct_name
        && arg_type_params.len() == param_type_arguments.len()
        && arg_type_params
            .iter()
            .zip(param_type_arguments)
            .all(|(arg_type_arg, param_type_arg)| {
                type_tag_equals_sig_token(
                    view,
                    function_type_arguments,
                    arg_type_arg,
                    param_type_arg,
                )
            })
}

fn struct_tag_equals_struct_inst(
    view: &BinaryIndexedView,
    function_type_arguments: &[TypeTag],
    arg_type: &StructTag,
    param_type: StructHandleIndex,
    param_type_arguments: &[SignatureToken],
) -> bool {
    let (address, module_name, struct_name) = resolve_struct(view, param_type);

    // same address, module, name, and type parameters
    &arg_type.address == address
        && arg_type.module.as_ident_str() == module_name
        && arg_type.name.as_ident_str() == struct_name
        && arg_type.type_params.len() == param_type_arguments.len()
        && arg_type.type_params.iter().zip(param_type_arguments).all(
            |(arg_type_arg, param_type_arg)| {
                type_tag_equals_sig_token(
                    view,
                    function_type_arguments,
                    arg_type_arg,
                    param_type_arg,
                )
            },
        )
}

pub fn missing_unwrapped_msg(id: &ObjectID) -> String {
    format!(
        "Unable to unwrap object {}. Was unable to retrieve last known version in the parent sync",
        id
    )
}

/// Run the bytecode verifier with a meter limit
/// This function only fails if the verification does not complete within the limit
#[instrument(level = "trace", skip_all)]
pub fn run_metered_move_bytecode_verifier(
    module_bytes: &[Vec<u8>],
    protocol_config: &ProtocolConfig,
) -> Result<(), SuiError> {
    let modules_stat = module_bytes
        .iter()
        .map(|b| {
            CompiledModule::deserialize(b)
                .map_err(|e| e.finish(move_binary_format::errors::Location::Undefined))
        })
        .collect::<move_binary_format::errors::VMResult<Vec<CompiledModule>>>();
    let modules = if let Ok(m) = modules_stat {
        m
    } else {
        // Although we failed, we dont care since it failed withing the timeout
        return Ok(());
    };

    // We use a custom config with metering enabled
    let metered_verifier_config =
        default_verifier_config(protocol_config, true /* enable metering */);

    run_metered_move_bytecode_verifier_impl(&modules, &metered_verifier_config)
}

pub fn run_metered_move_bytecode_verifier_impl(
    modules: &[CompiledModule],
    verifier_config: &VerifierConfig,
) -> Result<(), SuiError> {
    // run the Move verifier
    for module in modules.iter() {
        if let Err(e) = verify_module_with_config(verifier_config, module) {
            // Check that the status indicates mtering timeout
            // TODO: currently the Move verifier emits `CONSTRAINT_NOT_SATISFIED` for various failures including metering timeout
            // We need to change the VM error code to be more specific when timedout for metering
            if [
                StatusCode::CONSTRAINT_NOT_SATISFIED,
                StatusCode::TOO_MANY_BACK_EDGES,
            ]
            .contains(&e.major_status())
            {
                return Err(SuiError::ModuleVerificationFailure {
                    error: "Verification timedout".to_string(),
                });
            };
        }
    }
    Ok(())
}

}
