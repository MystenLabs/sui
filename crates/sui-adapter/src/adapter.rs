// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
};

use anyhow::Result;
use leb128;
use move_binary_format::{
    access::ModuleAccess,
    binary_views::BinaryIndexedView,
    errors::VMError,
    file_format::{
        CompiledModule, LocalIndex, SignatureToken, StructHandleIndex, TypeParameterIndex,
    },
};
use move_bytecode_verifier::VerifierConfig;
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, StructTag, TypeTag},
    resolver::{ModuleResolver, ResourceResolver},
    value::{MoveStruct, MoveTypeLayout, MoveValue},
};
pub use move_vm_runtime::move_vm::MoveVM;
use move_vm_runtime::{
    config::{VMConfig, VMRuntimeLimitsConfig},
    native_extensions::NativeContextExtensions,
    native_functions::NativeFunctionTable,
    session::Session,
};

use crate::execution_mode::ExecutionMode;
use sui_framework::natives::{object_runtime::ObjectRuntime, NativesCostTable};
use sui_json::primitive_type;
use sui_protocol_config::ProtocolConfig;
use sui_types::error::convert_vm_error;
use sui_types::{
    base_types::*,
    error::ExecutionError,
    error::{ExecutionErrorKind, SuiError},
    messages::{CallArg, EntryArgumentErrorKind, InputObjectKind, ObjectArg},
    object::{self, Data, Object, Owner},
    storage::ChildObjectResolver,
};
use sui_verifier::entry_points_verifier::{
    is_tx_context, TxContextKind, RESOLVED_ASCII_STR, RESOLVED_UTF8_STR,
};

pub fn new_move_vm(
    natives: NativeFunctionTable,
    protocol_config: &ProtocolConfig,
) -> Result<MoveVM, SuiError> {
    MoveVM::new_with_config(
        natives,
        VMConfig {
            verifier: VerifierConfig {
                max_loop_depth: Some(protocol_config.max_loop_depth()),
                max_generic_instantiation_length: Some(
                    protocol_config.max_generic_instantiation_length(),
                ),
                max_function_parameters: Some(protocol_config.max_function_parameters()),
                max_basic_blocks: Some(protocol_config.max_basic_blocks()),
                max_value_stack_size: protocol_config.max_value_stack_size(),
                max_type_nodes: Some(protocol_config.max_type_nodes()),
                max_push_size: Some(protocol_config.max_push_size()),
                max_dependency_depth: Some(protocol_config.max_dependency_depth()),
                max_fields_in_struct: Some(protocol_config.max_fields_in_struct()),
                max_function_definitions: Some(protocol_config.max_function_definitions()),
                max_struct_definitions: Some(protocol_config.max_struct_definitions()),
                max_constant_vector_len: protocol_config.max_move_vector_len(),
            },
            max_binary_format_version: protocol_config.move_binary_format_version(),
            paranoid_type_checks: false,
            runtime_limits_config: VMRuntimeLimitsConfig {
                vector_len_max: protocol_config.max_move_vector_len(),
            },
        },
    )
    .map_err(|_| SuiError::ExecutionInvariantViolation)
}

pub fn new_session<
    'v,
    'r,
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + ChildObjectResolver,
>(
    vm: &'v MoveVM,
    state_view: &'r S,
    input_objects: BTreeMap<ObjectID, Owner>,
    is_metered: bool,
    protocol_config: &ProtocolConfig,
) -> Session<'r, 'v, S> {
    let mut extensions = NativeContextExtensions::default();
    extensions.add(ObjectRuntime::new(
        Box::new(state_view),
        input_objects,
        is_metered,
        protocol_config,
    ));
    extensions.add(NativesCostTable::from_protocol_config(protocol_config));
    vm.new_session_with_extensions(state_view, extensions)
}

/// Given a list of `modules`, use `ctx` to generate a fresh ID for the new packages.
/// If `is_framework` is true, then the modules can have arbitrary user-defined address,
/// otherwise their addresses must be 0.
/// Mutate each module's self ID to the appropriate fresh ID and update its module handle tables
/// to reflect the new ID's of its dependencies.
/// Returns the newly created package ID.
pub fn generate_package_id(
    modules: &mut [CompiledModule],
    ctx: &mut TxContext,
) -> Result<ObjectID, ExecutionError> {
    let package_id = ctx.fresh_id();
    let new_address = AccountAddress::from(package_id);

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

    Ok(package_id)
}

pub struct TypeCheckSuccess {
    pub module_id: ModuleId,
    pub object_data: BTreeMap<ObjectID, (object::Owner, SequenceNumber)>,
    pub by_value_objects: BTreeSet<ObjectID>,
    pub mutable_ref_objects: BTreeMap<LocalIndex, ObjectID>,
    pub args: Vec<Vec<u8>>,
    /// is TxContext included in the arguments? If so, is it mutable?
    pub tx_ctx_kind: TxContextKind,
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
    args: Vec<CallArg>,
    is_genesis: bool,
) -> Result<TypeCheckSuccess, ExecutionError> {
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
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::FunctionNotFound,
                format!(
                    "Could not resolve function '{}' in module {}",
                    function, &module_id,
                ),
            ));
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
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::NonEntryFunctionInvoked,
            "Can only call `entry` functions",
        ));
    }
    let fhandle = module.function_handle_at(fdef.function);

    // check arity of type and value arguments
    if fhandle.type_parameters.len() != type_args.len() {
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::EntryTypeArityMismatch,
            format!(
                "Expected {:?} type arguments, but found {:?}",
                fhandle.type_parameters.len(),
                type_args.len()
            ),
        ));
    }

    // total number of args is (|objects| + |pure_args|) + 1 for the the `TxContext` object
    let parameters = &module.signature_at(fhandle.parameters).0;
    let tx_ctx_kind = parameters
        .last()
        .map(|t| is_tx_context(view, t))
        .unwrap_or(TxContextKind::None);

    let num_args = if tx_ctx_kind != TxContextKind::None {
        args.len() + 1
    } else {
        args.len()
    };
    if parameters.len() != num_args {
        let idx = std::cmp::min(parameters.len(), num_args) as LocalIndex;
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::entry_argument_error(idx, EntryArgumentErrorKind::ArityMismatch),
            format!(
                "Expected {:?} arguments calling function '{}', but found {:?}",
                parameters.len(),
                function,
                num_args
            ),
        ));
    }

    // type check object arguments passed in by value and by reference
    let mut object_data = BTreeMap::new();
    let mut mutable_ref_objects = BTreeMap::new();
    let mut by_value_objects = BTreeSet::new();

    // Track the mapping from each input object to its Move type.
    // This will be needed latter in `check_child_object_of_shared_object`.
    let mut object_type_map = BTreeMap::new();
    let bcs_args = args
        .into_iter()
        .enumerate()
        .map(|(idx, arg)| {
            let param_type = &parameters[idx];
            let idx = idx as LocalIndex;
            let object_arg = match arg {
                // dev-inspect does not make state changes and just a developer aid, so let through
                // any BCS bytes (they will be checked later by the VM)
                CallArg::Pure(arg) if Mode::allow_arbitrary_values() => return Ok(arg),
                CallArg::Pure(arg) => {
                    let (is_primitive, type_layout_opt) =
                        primitive_type(view, type_args, param_type);
                    if !is_primitive {
                        let msg = format!(
                            "Non-primitive argument at index {}. If it is an object, it must be \
                            populated by an object ID",
                            idx,
                        );
                        return Err(ExecutionError::new_with_source(
                            ExecutionErrorKind::entry_argument_error(
                                idx,
                                EntryArgumentErrorKind::UnsupportedPureArg,
                            ),
                            msg,
                        ));
                    }
                    validate_primitive_arg(view, &arg, idx, param_type, type_layout_opt)?;
                    return Ok(arg);
                }
                CallArg::Object(ObjectArg::ImmOrOwnedObject(ref_)) => {
                    let (o, arg_type, param_type) = serialize_object(
                        InputObjectKind::ImmOrOwnedMoveObject(ref_),
                        idx,
                        param_type,
                        objects,
                        &mut object_data,
                        &mut mutable_ref_objects,
                        &mut by_value_objects,
                        &mut object_type_map,
                    )?;
                    type_check_struct(view, type_args, idx, arg_type, param_type)?;
                    o
                }
                CallArg::Object(ObjectArg::SharedObject {
                    id,
                    initial_shared_version,
                    mutable,
                }) => {
                    let (o, arg_type, param_type) = serialize_object(
                        InputObjectKind::SharedMoveObject {
                            id,
                            initial_shared_version,
                            mutable,
                        },
                        idx,
                        param_type,
                        objects,
                        &mut object_data,
                        &mut mutable_ref_objects,
                        &mut by_value_objects,
                        &mut object_type_map,
                    )?;
                    type_check_struct(view, type_args, idx, arg_type, param_type)?;
                    o
                }
                CallArg::ObjVec(vec) => {
                    if vec.is_empty() {
                        // bcs representation of the empty vector
                        return Ok(vec![0]);
                    }
                    // write length of the vector as uleb128 as it is encoded in BCS and then append
                    // all (already serialized) object content data
                    let mut res = vec![];
                    leb128::write::unsigned(&mut res, vec.len() as u64).unwrap();
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
                        let (o, arg_type, param_type) = serialize_object(
                            object_kind,
                            idx,
                            param_type,
                            objects,
                            &mut object_data,
                            &mut mutable_ref_objects,
                            &mut by_value_objects,
                            &mut object_type_map,
                        )?;
                        type_check_struct(view, type_args, idx, arg_type, param_type)?;
                        res.extend(o);
                    }
                    res
                }
            };

            Ok(object_arg)
        })
        .collect::<Result<Vec<_>, _>>()?;

    check_shared_object_rules(
        objects,
        &by_value_objects,
        &object_type_map,
        module.self_id(),
    )?;

    Ok(TypeCheckSuccess {
        module_id,
        object_data,
        by_value_objects,
        mutable_ref_objects,
        args: bcs_args,
        tx_ctx_kind,
    })
}

// Validates a primitive argument
fn validate_primitive_arg(
    view: &BinaryIndexedView,
    arg: &[u8],
    idx: LocalIndex,
    param_type: &SignatureToken,
    type_layout: Option<MoveTypeLayout>,
) -> Result<(), ExecutionError> {
    // at this point we only check validity of string arguments (ascii and utf8)
    let string_arg_opt = string_arg(param_type, view);
    if string_arg_opt.is_none() {
        return Ok(());
    }
    let string_struct = string_arg_opt.unwrap();

    // we already checked the type above and struct layout for this type is guaranteed to exist
    let string_struct_layout = type_layout.unwrap();
    validate_primitive_arg_string(arg, idx, string_struct, string_struct_layout)
}

pub fn validate_primitive_arg_string(
    arg: &[u8],
    idx: LocalIndex,
    string_struct: (&AccountAddress, &IdentStr, &IdentStr),
    string_struct_layout: MoveTypeLayout,
) -> Result<(), ExecutionError> {
    let string_move_value =
        MoveValue::simple_deserialize(arg, &string_struct_layout).map_err(|_| {
            ExecutionError::new_with_source(
                ExecutionErrorKind::entry_argument_error(idx, EntryArgumentErrorKind::TypeMismatch),
                format!(
                "Function expects {}::{}::{} struct but provided argument's value does not match",
                string_struct.0, string_struct.1, string_struct.2,
            ),
            )
        })?;
    validate_string_move_value(&string_move_value, idx, string_struct.1)
}

// Given a MoveValue representing an string argument (string itself or arbitrarily nested vector of
// strings), validate that the content of the data represents a valid string
fn validate_string_move_value(
    value: &MoveValue,
    idx: LocalIndex,
    module: &IdentStr,
) -> Result<(), ExecutionError> {
    match value {
        MoveValue::Vector(vec) => {
            for v in vec {
                validate_string_move_value(v, idx, module)?;
            }
            Ok(())
        }
        MoveValue::Struct(MoveStruct::Runtime(vec)) => {
            // deserialization process validates the structure of this MoveValue (one struct field
            // in string structs containing a vector of u8 values)
            debug_assert!(vec.len() == 1);
            if let MoveValue::Vector(u8_vec) = &vec[0] {
                validate_string(&move_values_to_u8(u8_vec), idx, module)
            } else {
                debug_assert!(false);
                Ok(())
            }
        }
        _ => {
            debug_assert!(false);
            Ok(())
        }
    }
}

// Converts a Vec<MoveValue::U8> to a Vec<U8>
fn move_values_to_u8(values: &[MoveValue]) -> Vec<u8> {
    let res: Vec<u8> = values
        .iter()
        .filter_map(|v| {
            // deserialization process validates the structure of this MoveValue (u8 stored in a
            // vector of a string struct)
            if let MoveValue::U8(b) = v {
                Some(b)
            } else {
                None
            }
        })
        .cloned()
        .collect();
    debug_assert!(res.len() == values.len());
    res
}

// Validates that Vec<u8> represents a valid string
fn validate_string(bytes: &[u8], idx: LocalIndex, module: &IdentStr) -> Result<(), ExecutionError> {
    if module == STD_ASCII_MODULE_NAME {
        for b in bytes {
            if *b > 0x7F {
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::entry_argument_error(
                        idx,
                        EntryArgumentErrorKind::TypeMismatch,
                    ),
                    format!("Unexpected non-ASCII value (outside of ASCII character range) in argument {}", idx),
                ));
            }
        }
    } else {
        debug_assert!(module == STD_UTF8_MODULE_NAME);
        if std::str::from_utf8(bytes).is_err() {
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::entry_argument_error(idx, EntryArgumentErrorKind::TypeMismatch),
                format!(
                    "Unexpected non-UTF8 value (outside of UTF8 character range) in argument {}",
                    idx
                ),
            ));
        }
    }
    Ok(())
}

/// Check if a given argument is a string (or vector of strings) and, if so, return it
fn string_arg<'a>(
    param_type: &SignatureToken,
    view: &'a BinaryIndexedView,
) -> Option<(&'a AccountAddress, &'a IdentStr, &'a IdentStr)> {
    match param_type {
        SignatureToken::Struct(struct_handle_idx) => {
            let resolved_struct = sui_verifier::resolve_struct(view, *struct_handle_idx);
            if resolved_struct == RESOLVED_ASCII_STR || resolved_struct == RESOLVED_UTF8_STR {
                Some(resolved_struct)
            } else {
                None
            }
        }
        SignatureToken::Vector(el_token) => string_arg(el_token, view),
        _ => None,
    }
}

/// Serialize object with ID encoded in object_kind and also verify if various object properties are
/// correct.
fn serialize_object<'a>(
    object_kind: InputObjectKind,
    idx: LocalIndex,
    param_type: &'a SignatureToken,
    objects: &'a BTreeMap<ObjectID, impl Borrow<Object>>,
    object_data: &mut BTreeMap<ObjectID, (Owner, SequenceNumber)>,
    mutable_ref_objects: &mut BTreeMap<u8, ObjectID>,
    by_value_objects: &mut BTreeSet<ObjectID>,
    object_type_map: &mut BTreeMap<ObjectID, ModuleId>,
) -> Result<(Vec<u8>, &'a StructTag, &'a SignatureToken), ExecutionError> {
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
            let error = format!(
                "Argument at index {} populated with shared object id {} \
                        but an immutable or owned object was expected",
                idx, object_id
            );
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::entry_argument_error(
                    idx,
                    EntryArgumentErrorKind::ObjectKindMismatch,
                ),
                error,
            ));
        }
        InputObjectKind::SharedMoveObject { mutable, .. } => {
            if !object.is_shared() {
                let error = format!(
                    "Argument at index {} populated with an immutable or owned object id {} \
                            but an shared object was expected",
                    idx, object_id
                );
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::entry_argument_error(
                        idx,
                        EntryArgumentErrorKind::ObjectKindMismatch,
                    ),
                    error,
                ));
            }
            // Immutable shared object can only pass as immutable reference to move call
            if !mutable {
                match param_type {
                    SignatureToken::Reference(_) => {} // ok
                    SignatureToken::MutableReference(_) => {
                        let error = format!(
                            "Argument at index {} populated with an immutable shared object id {} \
                            but move call takes mutable object reference",
                            idx, object_id
                        );
                        return Err(ExecutionError::new_with_source(
                            ExecutionErrorKind::entry_argument_error(
                                idx,
                                EntryArgumentErrorKind::ObjectMutabilityMismatch,
                            ),
                            error,
                        ));
                    }
                    _ => {
                        return Err(ExecutionError::new_with_source(
                            ExecutionErrorKind::entry_argument_error(
                                idx,
                                EntryArgumentErrorKind::InvalidObjectByValue,
                            ),
                            format!(
                                "Shared objects cannot be passed by-value, \
                                    violation found in argument {}",
                                idx
                            ),
                        ));
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
            let error = format!(
                "Found module {} argument, but function expects {:?}",
                if for_vector { "element in vector" } else { "" },
                param_type
            );
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::entry_argument_error(idx, EntryArgumentErrorKind::TypeMismatch),
                error,
            ));
        }
    };

    // check that m.type_ matches the parameter types of the function
    let inner_param_type = inner_param_type(
        object,
        object_id,
        idx,
        param_type,
        &move_object.type_,
        mutable_ref_objects,
        by_value_objects,
    )?;

    object_type_map.insert(object_id, move_object.type_.module_id());
    object_data.insert(object_id, (object.owner, object.version()));
    Ok((
        move_object.contents().to_vec(),
        &move_object.type_,
        inner_param_type,
    ))
}

/// Get "inner" type of an object passed as argument (e.g., an inner type of a reference or of a
/// vector) and also verify if various object properties are correct.
fn inner_param_type<'a>(
    object: &Object,
    object_id: ObjectID,
    idx: LocalIndex,
    param_type: &'a SignatureToken,
    arg_type: &StructTag,
    mutable_ref_objects: &mut BTreeMap<u8, ObjectID>,
    by_value_objects: &mut BTreeSet<ObjectID>,
) -> Result<&'a SignatureToken, ExecutionError> {
    if let Owner::ObjectOwner(parent) = &object.owner {
        return Err(ExecutionErrorKind::invalid_child_object_argument(object_id, *parent).into());
    }
    match &param_type {
        SignatureToken::Reference(inner_t) => Ok(&**inner_t),
        SignatureToken::MutableReference(inner_t) => {
            if object.is_immutable() {
                let error = format!(
                    "Argument {} is expected to be mutable, immutable object found",
                    idx
                );
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::entry_argument_error(
                        idx,
                        EntryArgumentErrorKind::InvalidObjectByMuteRef,
                    ),
                    error,
                ));
            }
            mutable_ref_objects.insert(idx as LocalIndex, object_id);
            Ok(&**inner_t)
        }
        SignatureToken::Vector(inner_t) => inner_param_type(
            object,
            object_id,
            idx,
            inner_t,
            arg_type,
            mutable_ref_objects,
            by_value_objects,
        ),
        t @ SignatureToken::Struct(_)
        | t @ SignatureToken::StructInstantiation(_, _)
        | t @ SignatureToken::TypeParameter(_) => {
            match &object.owner {
                Owner::AddressOwner(_) | Owner::ObjectOwner(_) => (),
                Owner::Shared { .. } | Owner::Immutable => {
                    return Err(ExecutionError::new_with_source(
                        ExecutionErrorKind::entry_argument_error(
                            idx,
                            EntryArgumentErrorKind::InvalidObjectByValue,
                        ),
                        format!(
                            "Immutable and shared objects cannot be passed by-value, \
                                    violation found in argument {}",
                            idx
                        ),
                    ));
                }
            }
            by_value_objects.insert(object_id);
            Ok(t)
        }
        t => Err(ExecutionError::new_with_source(
            ExecutionErrorKind::entry_argument_error(idx, EntryArgumentErrorKind::TypeMismatch),
            format!(
                "Found object argument {}, but function expects {:?}",
                arg_type, t
            ),
        )),
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
    idx: LocalIndex,
    arg_type: &StructTag,
    param_type: &SignatureToken,
) -> Result<(), ExecutionError> {
    if !struct_tag_equals_sig_token(view, function_type_arguments, arg_type, param_type) {
        Err(ExecutionError::new_with_source(
            ExecutionErrorKind::entry_argument_error(idx, EntryArgumentErrorKind::TypeMismatch),
            format!(
                "Expected argument of type {}, but found type {}",
                sui_verifier::format_signature_token(view, param_type),
                arg_type
            ),
        ))
    } else {
        Ok(())
    }
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

fn struct_tag_equals_struct_inst(
    view: &BinaryIndexedView,
    function_type_arguments: &[TypeTag],
    arg_type: &StructTag,
    param_type: StructHandleIndex,
    param_type_arguments: &[SignatureToken],
) -> bool {
    let (address, module_name, struct_name) = sui_verifier::resolve_struct(view, param_type);

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

pub fn convert_type_argument_error<
    'r,
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E>,
>(
    idx: usize,
    error: VMError,
    vm: &'r MoveVM,
    state_view: &'r S,
) -> ExecutionError {
    use move_core_types::vm_status::StatusCode;
    use sui_types::messages::EntryTypeArgumentErrorKind;
    let kind = match error.major_status() {
        StatusCode::TYPE_RESOLUTION_FAILURE => EntryTypeArgumentErrorKind::TypeNotFound,
        StatusCode::NUMBER_OF_TYPE_ARGUMENTS_MISMATCH => EntryTypeArgumentErrorKind::ArityMismatch,
        StatusCode::CONSTRAINT_NOT_SATISFIED => EntryTypeArgumentErrorKind::ConstraintNotSatisfied,
        _ => return convert_vm_error(error, vm, state_view),
    };
    ExecutionErrorKind::entry_type_argument_error(idx as TypeParameterIndex, kind).into()
}
