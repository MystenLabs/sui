// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt};

use move_binary_format::{
    access::ModuleAccess,
    file_format::{AbilitySet, LocalIndex, Visibility},
    CompiledModule,
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::IdentStr,
    language_storage::{ModuleId, StructTag, TypeTag},
    value::{MoveStructLayout, MoveTypeLayout},
};
use move_vm_runtime::{
    move_vm::MoveVM,
    session::{LoadedFunctionInstantiation, SerializedReturnValues},
};
use move_vm_types::loaded_data::runtime_types::{StructType, Type};
use sui_cost_tables::bytecode_tables::GasStatus;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    balance::Balance,
    base_types::{ObjectID, SuiAddress, TxContext, TX_CONTEXT_MODULE_NAME, TX_CONTEXT_STRUCT_NAME},
    coin::Coin,
    error::{ExecutionError, ExecutionErrorKind},
    id::UID,
    messages::{
        Argument, Command, EntryArgumentErrorKind, ProgrammableMoveCall, ProgrammableTransaction,
    },
    object::Owner,
    SUI_FRAMEWORK_ADDRESS,
};
use sui_verifier::{
    entry_points_verifier::{
        TxContextKind, RESOLVED_ASCII_STR, RESOLVED_STD_OPTION, RESOLVED_SUI_ID, RESOLVED_UTF8_STR,
    },
    INIT_FN_NAME,
};

use crate::adapter::{
    convert_type_argument_error, generate_package_id, validate_primitive_arg_string,
};

use super::{context::*, types::*};

pub fn execute<E: fmt::Debug, S: StorageView<E>>(
    protocol_config: &ProtocolConfig,
    vm: &MoveVM,
    state_view: &mut S,
    ctx: &mut TxContext,
    gas_status: &mut GasStatus,
    gas_coin: ObjectID,
    pt: ProgrammableTransaction,
) -> Result<(), ExecutionError> {
    let ProgrammableTransaction { inputs, commands } = pt;
    let mut context = ExecutionContext::new(
        protocol_config,
        vm,
        state_view,
        ctx,
        gas_status,
        gas_coin,
        inputs,
    )?;
    for command in commands {
        execute_command(&mut context, command)?;
    }
    Ok(())
}

/// Execute a single command
fn execute_command<E: fmt::Debug, S: StorageView<E>>(
    context: &mut ExecutionContext<E, S>,
    command: Command,
) -> Result<(), ExecutionError> {
    let results = match command {
        Command::TransferObjects(objs, addr_arg) => {
            let objs: Vec<ObjectValue> = objs
                .into_iter()
                .enumerate()
                .map(|(idx, arg)| context.take_arg(CommandKind::TransferObjects, idx, arg))
                .collect::<Result<_, _>>()?;
            let addr: SuiAddress = context.clone_arg(objs.len(), addr_arg)?;
            for obj in objs {
                obj.ensure_public_transfer_eligible()?;
                context.transfer_object(obj, addr)?;
            }
            vec![]
        }
        Command::SplitCoin(coin_arg, amount_arg) => {
            let mut obj: ObjectValue = context.borrow_arg_mut(0, coin_arg)?;
            let ObjectContents::Coin(coin) = &mut obj.contents else {
                panic!("not a coin")
            };
            let amount: u64 = context.clone_arg(1, amount_arg)?;
            let new_coin_id = context.fresh_id()?;
            let new_coin = coin.split_coin(amount, UID::new(new_coin_id))?;
            let coin_type = obj.type_.clone();
            context.restore_arg(coin_arg, Value::Object(obj))?;
            vec![Value::Object(ObjectValue::coin(coin_type, new_coin)?)]
        }
        Command::MergeCoins(target_arg, coin_args) => {
            let mut target: ObjectValue = context.borrow_arg_mut(0, target_arg)?;
            let ObjectContents::Coin(target_coin) = &mut target.contents else {
                panic!("not a coin")
            };
            let coins: Vec<ObjectValue> = coin_args
                .into_iter()
                .enumerate()
                .map(|(idx, arg)| context.take_arg(CommandKind::MergeCoins, idx + 1, arg))
                .collect::<Result<_, _>>()?;
            for coin in coins {
                let ObjectContents::Coin(Coin { id, balance }) = coin.contents else {
                    panic!("not a coin")
                };
                context.delete_id(*id.object_id())?;
                let Some(new_value) = target_coin.balance.value().checked_add(balance.value())
                    else {
                        panic!("coin overflow")
                    };
                target_coin.balance = Balance::new(new_value);
            }
            context.restore_arg(target_arg, Value::Object(target))?;
            vec![]
        }
        Command::MoveCall(move_call) => {
            let ProgrammableMoveCall {
                package,
                module,
                function,
                type_arguments,
                arguments,
            } = *move_call;
            let module_id = ModuleId::new(package.into(), module);
            execute_move_call(
                context,
                &module_id,
                &function,
                type_arguments,
                arguments,
                /* is_init */ false,
            )?
        }
        Command::Publish(modules) => {
            execute_move_publish(context, modules)?;
            vec![]
        }
    };
    context.push_command_results(results)?;
    Ok(())
}

/// Execute a single Move call
fn execute_move_call<E: fmt::Debug, S: StorageView<E>>(
    context: &mut ExecutionContext<E, S>,
    module_id: &ModuleId,
    function: &IdentStr,
    type_arguments: Vec<TypeTag>,
    arguments: Vec<Argument>,
    is_init: bool,
) -> Result<Vec<Value>, ExecutionError> {
    // check that the function is either an entry function or a valid public function
    let (function_kind, signature, return_value_kinds) =
        check_visibility_and_signature(context, module_id, function, &type_arguments, is_init)?;
    // build the arguments, storing meta data about by-mut-ref args
    let (tx_context_kind, by_mut_ref, serialized_arguments) = build_move_args(
        context,
        module_id,
        function,
        function_kind,
        &signature,
        &arguments,
    )?;
    // invoke the VM
    let SerializedReturnValues {
        mutable_reference_outputs,
        return_values,
    } = vm_move_call(
        context,
        module_id,
        function,
        type_arguments,
        tx_context_kind,
        serialized_arguments,
    )?;
    assert_invariant!(
        by_mut_ref.len() == mutable_reference_outputs.len(),
        "lost mutable input"
    );
    // write back mutable inputs
    for ((i1, bytes, _layout), (i2, value_info)) in
        mutable_reference_outputs.into_iter().zip(by_mut_ref)
    {
        assert_invariant!(i1 == i2, "lost mutable input");
        let arg_idx = i1 as usize;
        let value = make_value(value_info, bytes, /* return value */ false)?;
        context.restore_arg(arguments[arg_idx], value)?;
    }
    // taint arguments if this function is not an entry function (i.e. just some public function)
    // &mut on primitive, non-object values will already have been tainted when updating the value
    if function_kind == FunctionKind::NonEntry {
        for arg in &arguments {
            context.mark_used_in_non_entry_move_call(*arg);
        }
    }

    assert_invariant!(
        return_value_kinds.len() == return_values.len(),
        "lost return value"
    );
    return_value_kinds
        .into_iter()
        .zip(return_values)
        .map(|(value_info, (bytes, _layout))| {
            make_value(value_info, bytes, /* return value */ true)
        })
        .collect()
}

fn make_value(
    value_info: ValueKind,
    bytes: Vec<u8>,
    is_return_value: bool,
) -> Result<Value, ExecutionError> {
    Ok(match value_info {
        ValueKind::Object {
            owner,
            type_,
            has_public_transfer,
        } => Value::Object(ObjectValue::new(
            owner,
            type_,
            has_public_transfer,
            is_return_value,
            &bytes,
        )?),
        ValueKind::Raw(ty, abilities) => Value::Raw(ValueType::Loaded { ty, abilities }, bytes),
    })
}

/// Publish Move modules and call the init functions
fn execute_move_publish<E: fmt::Debug, S: StorageView<E>>(
    context: &mut ExecutionContext<E, S>,
    module_bytes: Vec<Vec<u8>>,
) -> Result<(), ExecutionError> {
    let modules = publish_and_verify_modules(context, module_bytes)?;
    let modules_to_init = modules
        .iter()
        .filter_map(|module| {
            for fdef in &module.function_defs {
                let fhandle = module.function_handle_at(fdef.function);
                let fname = module.identifier_at(fhandle.name);
                if fname == INIT_FN_NAME {
                    return Some(module.self_id());
                }
            }
            None
        })
        .collect::<Vec<_>>();

    context.new_package(modules)?;
    for module_id in &modules_to_init {
        let return_values = execute_move_call(
            context,
            module_id,
            INIT_FN_NAME,
            vec![],
            vec![],
            /* is_int */ true,
        )?;
        assert_invariant!(
            return_values.is_empty(),
            "init should not have return values"
        )
    }
    Ok(())
}

/***************************************************************************************************
 * Move execution
 **************************************************************************************************/

fn vm_move_call<E: fmt::Debug, S: StorageView<E>>(
    context: &mut ExecutionContext<E, S>,
    module_id: &ModuleId,
    function: &IdentStr,
    type_arguments: Vec<TypeTag>,
    tx_context_kind: TxContextKind,
    mut serialized_arguments: Vec<Vec<u8>>,
) -> Result<SerializedReturnValues, ExecutionError> {
    match tx_context_kind {
        TxContextKind::None => (),
        TxContextKind::Mutable | TxContextKind::Immutable => {
            serialized_arguments.push(context.tx_context.to_vec());
        }
    }
    // script visibility checked manually for entry points
    let mut result = context
        .session
        .execute_function_bypass_visibility(
            module_id,
            function,
            type_arguments,
            serialized_arguments,
            context.gas_status,
        )
        .map_err(|e| context.convert_vm_error(e))?;

    // When this function is used during publishing, it
    // may be executed several times, with objects being
    // created in the Move VM in each Move call. In such
    // case, we need to update TxContext value so that it
    // reflects what happened each time we call into the
    // Move VM (e.g. to account for the number of created
    // objects).
    if tx_context_kind == TxContextKind::Mutable {
        let (_, ctx_bytes, _) = result.mutable_reference_outputs.pop().unwrap();
        let updated_ctx: TxContext = bcs::from_bytes(&ctx_bytes).unwrap();
        context.tx_context.update_state(updated_ctx)?;
    }
    Ok(result)
}

/// - Deserializes the modules
/// - Publishes them into the VM, which invokes the Move verifier
/// - Run the Sui Verifier
fn publish_and_verify_modules<E: fmt::Debug, S: StorageView<E>>(
    context: &mut ExecutionContext<E, S>,
    module_bytes: Vec<Vec<u8>>,
) -> Result<Vec<CompiledModule>, ExecutionError> {
    let mut modules = module_bytes
        .iter()
        .map(|b| {
            CompiledModule::deserialize(b)
                .map_err(|e| e.finish(move_binary_format::errors::Location::Undefined))
        })
        .collect::<move_binary_format::errors::VMResult<Vec<CompiledModule>>>()
        .map_err(|e| context.convert_vm_error(e))?;

    if modules.is_empty() {
        return Err(ExecutionErrorKind::PublishErrorEmptyPackage.into());
    }

    // It should be fine that this does not go through context.fresh_id since the Move runtime
    // does not to know about new packages created, since Move objects and Move packages cannot
    // interact
    let package_id = generate_package_id(&mut modules, context.tx_context)?;
    // TODO(https://github.com/MystenLabs/sui/issues/69): avoid this redundant serialization by exposing VM API that allows us to run the linker directly on `Vec<CompiledModule>`
    let new_module_bytes: Vec<_> = modules
        .iter()
        .map(|m| {
            let mut bytes = Vec::new();
            m.serialize(&mut bytes).unwrap();
            bytes
        })
        .collect();
    context
        .session
        .publish_module_bundle(
            new_module_bytes,
            AccountAddress::from(package_id),
            // TODO: publish_module_bundle() currently doesn't charge gas.
            // Do we want to charge there?
            context.gas_status,
        )
        .map_err(|e| context.convert_vm_error(e))?;

    // run the Sui verifier
    for module in &modules {
        // Run Sui bytecode verifier, which runs some additional checks that assume the Move
        // bytecode verifier has passed.
        sui_verifier::verifier::verify_module(module, &BTreeMap::new())?;
    }
    Ok(modules)
}

/***************************************************************************************************
 * Move signatures
 **************************************************************************************************/

/// Helper marking what function we are invoking
#[derive(PartialEq, Eq, Clone, Copy)]
enum FunctionKind {
    PrivateEntry,
    PublicEntry,
    NonEntry,
    Init,
}

/// Used to remember type information about a type when resolving the signature
enum ValueKind {
    Object {
        owner: Option<Owner>,
        type_: StructTag,
        has_public_transfer: bool,
    },
    Raw(Type, AbilitySet),
}

/// Checks that the function to be called is either
/// - an entry function
/// - a public function that does not return references
/// - module init (only internal usage)
fn check_visibility_and_signature<E: fmt::Debug, S: StorageView<E>>(
    context: &mut ExecutionContext<E, S>,
    module_id: &ModuleId,
    function: &IdentStr,
    type_arguments: &[TypeTag],
    from_init: bool,
) -> Result<(FunctionKind, LoadedFunctionInstantiation, Vec<ValueKind>), ExecutionError> {
    for (idx, ty) in type_arguments.iter().enumerate() {
        context
            .session
            .load_type(ty)
            .map_err(|e| convert_type_argument_error(idx, e, context.vm, context.state_view))?;
    }
    let function_kind = {
        let module = context
            .vm
            .load_module(module_id, context.state_view)
            .map_err(|e| context.convert_vm_error(e))?;
        let module_id = module.self_id();
        let Some(fdef) = module.function_defs.iter().find(|fdef| {
            module.identifier_at(module.function_handle_at(fdef.function).name) == function
        }) else {
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::FunctionNotFound,
                format!(
                    "Could not resolve function '{}' in module {}",
                    function, &module_id,
                ),
            ));
        };
        // TODO dev inspect
        match (fdef.visibility, fdef.is_entry) {
            (Visibility::Private | Visibility::Friend, true) => FunctionKind::PrivateEntry,
            (Visibility::Public, true) => FunctionKind::PublicEntry,
            (Visibility::Public, false) => FunctionKind::NonEntry,
            (Visibility::Private, false) if from_init => {
                assert_invariant!(
                    function == INIT_FN_NAME,
                    "module init specified non-init function"
                );
                FunctionKind::Init
            }
            (Visibility::Private | Visibility::Friend, false) => {
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::NonEntryFunctionInvoked,
                    "Can only call `entry` or `public` functions",
                ));
            }
        }
    };
    let signature = context
        .session
        .load_function(module_id, function, type_arguments)
        .map_err(|e| context.convert_vm_error(e))?;
    let return_value_kinds = match function_kind {
        FunctionKind::PrivateEntry | FunctionKind::PublicEntry | FunctionKind::Init => {
            assert_invariant!(
                signature.return_.is_empty(),
                "entry functions must have no return values"
            );
            vec![]
        }
        FunctionKind::NonEntry => {
            check_non_entry_signature(context, module_id, function, &signature)?
        }
    };
    Ok((function_kind, signature, return_value_kinds))
}

/// Checks that the non-entry function does not return references. And marks the return values
/// as object or non-object return values
fn check_non_entry_signature<E: fmt::Debug, S: StorageView<E>>(
    context: &mut ExecutionContext<E, S>,
    _module_id: &ModuleId,
    _function: &IdentStr,
    signature: &LoadedFunctionInstantiation,
) -> Result<Vec<ValueKind>, ExecutionError> {
    signature
        .return_
        .iter()
        .map(|return_type| {
            if let Type::Reference(_) | Type::MutableReference(_) = return_type {
                panic!("references not supported")
            };
            let abilities = context
                .session
                .get_type_abilities(return_type)
                .map_err(|e| context.convert_vm_error(e))?;
            Ok(match return_type {
                Type::MutableReference(_) | Type::Reference(_) => unreachable!(),
                Type::TyParam(_) => invariant_violation!("TyParam should have been substituted"),
                Type::Struct(_) | Type::StructInstantiation(_, _) if abilities.has_key() => {
                    let type_tag = context
                        .session
                        .get_type_tag(return_type)
                        .map_err(|e| context.convert_vm_error(e))?;
                    let TypeTag::Struct(struct_tag) = type_tag else {
                        invariant_violation!("Struct type make a non struct type tag")
                    };
                    ValueKind::Object {
                        owner: None,
                        type_: *struct_tag,
                        has_public_transfer: abilities.has_store(),
                    }
                }
                Type::Struct(_)
                | Type::StructInstantiation(_, _)
                | Type::Bool
                | Type::U8
                | Type::U64
                | Type::U128
                | Type::Address
                | Type::Signer
                | Type::Vector(_)
                | Type::U16
                | Type::U32
                | Type::U256 => ValueKind::Raw(return_type.clone(), abilities),
            })
        })
        .collect()
}

type ArgInfo = (
    TxContextKind,
    /* mut ref */
    Vec<(LocalIndex, ValueKind)>,
    Vec<Vec<u8>>,
);

/// Serializes the arguments into BCS values for Move. Performs the necessary type checking for
/// each value
fn build_move_args<E: fmt::Debug, S: StorageView<E>>(
    context: &mut ExecutionContext<E, S>,
    module_id: &ModuleId,
    function: &IdentStr,
    function_kind: FunctionKind,
    signature: &LoadedFunctionInstantiation,
    args: &[Argument],
) -> Result<ArgInfo, ExecutionError> {
    // check the arity
    let parameters = &signature.parameters;
    let tx_ctx_kind = match parameters.last() {
        Some(t) => is_tx_context(context, t)?,
        None => TxContextKind::None,
    };
    // an init function can have one or two arguments, with the last one always being of type
    // &mut TxContext and the additional (first) one representing a one time witness type (see
    // one_time_witness verifier pass for additional explanation)
    let has_one_time_witness = function_kind == FunctionKind::Init && parameters.len() == 2;
    let has_tx_context = tx_ctx_kind != TxContextKind::None;
    let num_args = args.len() + (has_one_time_witness as usize) + (has_tx_context as usize);
    if num_args != parameters.len() {
        let idx = std::cmp::min(parameters.len(), num_args) as LocalIndex;
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::entry_argument_error(idx, EntryArgumentErrorKind::ArityMismatch),
            format!(
                "Expected {:?} argument{} calling function '{}', but found {:?}",
                parameters.len(),
                if parameters.len() == 1 { "" } else { "s" },
                function,
                num_args
            ),
        ));
    }

    // check the types and remember which are by mutable ref
    let mut by_mut_ref = vec![];
    let mut serialized_args = Vec::with_capacity(num_args);
    let command_kind = CommandKind::MoveCall {
        package: (*module_id.address()).into(),
        module: module_id.name(),
        function,
    };
    // an init function can have one or two arguments, with the last one always being of type
    // &mut TxContext and the additional (first) one representing a one time witness type (see
    // one_time_witness verifier pass for additional explanation)
    if has_one_time_witness {
        // one time witness type is a struct with a single bool filed which in bcs is encoded as
        // 0x01
        let bcs_true_value = vec![0x01];
        serialized_args.push(bcs_true_value)
    }
    for ((idx, arg), param_ty) in args.iter().copied().enumerate().zip(parameters) {
        let (value, non_ref_param_ty): (Value, &Type) = match param_ty {
            Type::MutableReference(inner) => {
                let value = context.borrow_arg_mut(idx, arg)?;
                let object_info = if let Value::Object(ObjectValue {
                    owner,
                    type_,
                    has_public_transfer,
                    ..
                }) = &value
                {
                    ValueKind::Object {
                        owner: *owner,
                        type_: type_.clone(),
                        has_public_transfer: *has_public_transfer,
                    }
                } else {
                    let abilities = context
                        .session
                        .get_type_abilities(inner)
                        .map_err(|e| context.convert_vm_error(e))?;
                    ValueKind::Raw((**inner).clone(), abilities)
                };
                by_mut_ref.push((idx as LocalIndex, object_info));
                (value, inner)
            }
            Type::Reference(inner) => (context.borrow_arg(idx, arg)?, inner),
            t => {
                let abilities = context
                    .session
                    .get_type_abilities(t)
                    .map_err(|e| context.convert_vm_error(e))?;
                let value = if abilities.has_copy() {
                    context.clone_arg(idx, arg)?
                } else {
                    context.take_arg(command_kind, idx, arg)?
                };
                (value, t)
            }
        };
        if matches!(
            function_kind,
            FunctionKind::PrivateEntry | FunctionKind::Init
        ) && value.was_used_in_non_entry_move_call()
        {
            panic!("private entry taint failed")
        }
        check_param_type(context, idx, &value, non_ref_param_ty)?;
        let bytes = value.to_bcs_bytes();
        // Any means this was just some bytes passed in as an argument (as opposed to being
        // generated from a Move function). Meaning we will need to run validation
        if matches!(value, Value::Raw(ValueType::Any, _)) {
            if let Some((string_struct, string_struct_layout)) = is_string_arg(context, param_ty)? {
                validate_primitive_arg_string(
                    &bytes,
                    idx as LocalIndex,
                    string_struct,
                    string_struct_layout,
                )?;
            }
        }
        serialized_args.push(bytes);
    }
    Ok((tx_ctx_kind, by_mut_ref, serialized_args))
}

/// checks that the value is compatible with the specified type
fn check_param_type<E: fmt::Debug, S: StorageView<E>>(
    context: &mut ExecutionContext<E, S>,
    idx: usize,
    value: &Value,
    param_ty: &Type,
) -> Result<(), ExecutionError> {
    let obj_ty;
    let ty = match value {
        // TODO dev inspect
        Value::Raw(ValueType::Any, _) => {
            if !is_entry_primitive_type(context, param_ty)? {
                let msg = format!(
                    "Non-primitive argument at index {}. If it is an object, it must be \
                    populated by an object ID",
                    idx,
                );
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::entry_argument_error(
                        idx as LocalIndex,
                        EntryArgumentErrorKind::UnsupportedPureArg,
                    ),
                    msg,
                ));
            } else {
                return Ok(());
            }
        }
        Value::Raw(ValueType::Loaded { ty, abilities }, _) => {
            assert_invariant!(!abilities.has_key(), "Raw value should never be an object");
            ty
        }
        Value::Object(obj) => {
            obj_ty = context
                .session
                .load_type(&TypeTag::Struct(Box::new(obj.type_.clone())))
                .map_err(|e| context.convert_vm_error(e))?;
            &obj_ty
        }
    };
    if ty != param_ty {
        panic!("type mismatch")
    } else {
        Ok(())
    }
}

/// If the type is a string, returns the name of the string type and the layout
/// Otherwise, returns None
fn is_string_arg<E: fmt::Debug, S: StorageView<E>>(
    context: &mut ExecutionContext<E, S>,
    param_ty: &Type,
) -> Result<Option<StringInfo>, ExecutionError> {
    let Type::Struct(idx) = param_ty else { return Ok(None) };
    let Some(s) = context.session.get_struct_type(*idx) else {
        invariant_violation!("Loaded struct not found")
    };
    let resolved_struct = get_struct_ident(&s);
    let string_name = if resolved_struct == RESOLVED_ASCII_STR {
        RESOLVED_ASCII_STR
    } else if resolved_struct == RESOLVED_UTF8_STR {
        RESOLVED_UTF8_STR
    } else {
        return Ok(None);
    };
    let layout = MoveTypeLayout::Struct(MoveStructLayout::Runtime(vec![MoveTypeLayout::Vector(
        Box::new(MoveTypeLayout::U8),
    )]));
    Ok(Some((string_name, layout)))
}
type StringInfo = (
    (
        &'static AccountAddress,
        &'static IdentStr,
        &'static IdentStr,
    ),
    MoveTypeLayout,
);

// Returns Some(kind) if the type is a reference to the TxnContext. kind being Mutable with
// a MutableReference, and Immutable otherwise.
// Returns None for all other types
pub fn is_tx_context<E: fmt::Debug, S: StorageView<E>>(
    context: &mut ExecutionContext<E, S>,
    t: &Type,
) -> Result<TxContextKind, ExecutionError> {
    let (is_mut, inner) = match t {
        Type::MutableReference(inner) => (true, inner),
        Type::Reference(inner) => (false, inner),
        _ => return Ok(TxContextKind::None),
    };
    let Type::Struct(idx) = &**inner else { return Ok(TxContextKind::None) };
    let Some(s) = context.session.get_struct_type(*idx) else {
        invariant_violation!("Loaded struct not found")
    };
    let (module_addr, module_name, struct_name) = get_struct_ident(&s);
    let is_tx_context_type = module_addr == &SUI_FRAMEWORK_ADDRESS
        && module_name == TX_CONTEXT_MODULE_NAME
        && struct_name == TX_CONTEXT_STRUCT_NAME;
    Ok(if is_tx_context_type {
        if is_mut {
            TxContextKind::Mutable
        } else {
            TxContextKind::Immutable
        }
    } else {
        TxContextKind::None
    })
}

/// Returns true iff it is a primitive, an ID, a String, or an option/vector of a valid type
fn is_entry_primitive_type<E: fmt::Debug, S: StorageView<E>>(
    context: &mut ExecutionContext<E, S>,
    param_ty: &Type,
) -> Result<bool, ExecutionError> {
    let mut stack = vec![param_ty];
    while let Some(cur) = stack.pop() {
        match cur {
            Type::Signer => return Ok(false),
            Type::Reference(_) | Type::MutableReference(_) | Type::TyParam(_) => return Ok(false),
            Type::Bool
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::U128
            | Type::U256
            | Type::Address => (),
            Type::Vector(inner) => stack.push(&**inner),
            Type::Struct(idx) => {
                let Some(s) = context.session.get_struct_type(*idx) else {
                    invariant_violation!("Loaded struct not found")
                };
                let resolved_struct = get_struct_ident(&s);
                if ![RESOLVED_SUI_ID, RESOLVED_ASCII_STR, RESOLVED_UTF8_STR]
                    .contains(&resolved_struct)
                {
                    return Ok(false);
                }
            }
            Type::StructInstantiation(idx, targs) => {
                let Some(s) = context.session.get_struct_type(*idx) else {
                    invariant_violation!("Loaded struct not found")
                };
                let resolved_struct = get_struct_ident(&s);
                // is option of a primitive
                let is_valid = resolved_struct == RESOLVED_STD_OPTION && targs.len() == 1;
                if !is_valid {
                    return Ok(false);
                }
                stack.extend(targs)
            }
        }
    }
    Ok(true)
}

fn get_struct_ident(s: &StructType) -> (&AccountAddress, &IdentStr, &IdentStr) {
    let module_id = &s.module;
    let struct_name = &s.name;
    (
        module_id.address(),
        module_id.name(),
        struct_name.as_ident_str(),
    )
}
