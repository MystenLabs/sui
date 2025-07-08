// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution_mode::ExecutionMode,
    programmable_transactions::execution::{PrimitiveArgumentLayout, bcs_argument_validate},
    sp,
    static_programmable_transactions::{
        env::Env,
        loading::ast::Type,
        typing::ast::{self as T, BytesConstraint, BytesUsage, InputArg, ObjectArg},
    },
};
use sui_types::{
    base_types::{RESOLVED_ASCII_STR, RESOLVED_STD_OPTION, RESOLVED_UTF8_STR},
    error::{ExecutionError, ExecutionErrorKind, command_argument_error},
    execution_status::CommandArgumentError,
    id::RESOLVED_SUI_ID,
    transfer::RESOLVED_RECEIVING_STRUCT,
};

struct ObjectUsage {
    allow_by_value: bool,
    allow_by_mut_ref: bool,
}

struct Context {
    inputs: Vec<Option<ObjectUsage>>,
}

impl Context {
    fn new(inputs: &T::Inputs) -> Self {
        let inputs = inputs
            .iter()
            .map(|(arg, _)| {
                Some(match arg {
                    InputArg::Pure(_) | InputArg::Receiving(_) => return None,
                    InputArg::Object(ObjectArg::ImmObject(_)) => ObjectUsage {
                        allow_by_value: false,
                        allow_by_mut_ref: false,
                    },
                    InputArg::Object(ObjectArg::OwnedObject(_)) => ObjectUsage {
                        allow_by_value: true,
                        allow_by_mut_ref: true,
                    },
                    InputArg::Object(ObjectArg::SharedObject { mutable, .. }) => ObjectUsage {
                        allow_by_value: *mutable,
                        allow_by_mut_ref: *mutable,
                    },
                })
            })
            .collect();
        Self { inputs }
    }
}

/// Verifies two properties for input objects:
/// 1. That the `Pure` inputs can be serialized to the type inferred and that the type is
///    permissible
///    - Can be relaxed under certain execution modes
/// 2. That any `Object` arguments are used validly. This means mutable references are taken only
///    on mutable objects. And that the gas coin is only taken by value in transfer objects
pub fn verify<Mode: ExecutionMode>(_env: &Env, txn: &T::Transaction) -> Result<(), ExecutionError> {
    let T::Transaction { inputs, commands } = txn;
    for (arg, ty) in inputs {
        match ty {
            T::InputType::Bytes(constraints) => {
                for (constraint, constraint_info) in constraints {
                    let BytesConstraint {
                        command,
                        argument,
                        usage,
                    } = *constraint_info;
                    check_constraint::<Mode>(argument, arg, constraint, usage)
                        .map_err(|e| e.with_command_index(command as usize))?;
                }
            }
            T::InputType::Fixed(_) => (),
        }
    }
    let context = &mut Context::new(inputs);
    for (c, _t) in commands {
        command(context, c).map_err(|e| e.with_command_index(c.idx as usize))?;
    }
    Ok(())
}

//**************************************************************************************************
// Pure bytes
//**************************************************************************************************

fn check_constraint<Mode: ExecutionMode>(
    command_arg_idx: u16,
    arg: &InputArg,
    constraint: &Type,
    usage: BytesUsage,
) -> Result<(), ExecutionError> {
    match arg {
        InputArg::Pure(bytes) => {
            check_pure_bytes::<Mode>(command_arg_idx, bytes, constraint, usage)
        }
        InputArg::Receiving(_) => check_receiving(command_arg_idx, constraint),
        InputArg::Object(
            ObjectArg::ImmObject(_) | ObjectArg::OwnedObject(_) | ObjectArg::SharedObject { .. },
        ) => {
            invariant_violation!("Object inputs should be Fixed")
        }
    }
}

fn check_pure_bytes<Mode: ExecutionMode>(
    command_arg_idx: u16,
    bytes: &[u8],
    constraint: &Type,
    usage: BytesUsage,
) -> Result<(), ExecutionError> {
    assert_invariant!(
        !matches!(constraint, Type::Reference(_, _)),
        "references should not be added as a constraint"
    );
    if Mode::allow_arbitrary_values() {
        return Ok(());
    }
    let Some(layout) = primitive_serialization_layout(constraint)? else {
        let msg = format!(
            "Invalid usage of `Pure` argument for a non-primitive argument type at index {command_arg_idx}.",
        );
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::command_argument_error(
                CommandArgumentError::InvalidUsageOfPureArg,
                command_arg_idx,
            ),
            msg,
        ));
    };
    match usage {
        BytesUsage::Copied => assert_invariant!(
            constraint.abilities().has_copy(),
            "non-fixed bytes used by-value should be copyable. For type {constraint:?}"
        ),
        BytesUsage::ByImmRef => assert_invariant!(
            constraint.abilities().has_drop(),
            "non-fixed bytes used by-immutable-ref should be droppable since they will be \
            deserialized, borrowed, but ultimately dropped. For type {constraint:?}"
        ),
        BytesUsage::ByMutRef => (),
    }
    bcs_argument_validate(bytes, command_arg_idx, layout)?;
    Ok(())
}

fn primitive_serialization_layout(
    param_ty: &Type,
) -> Result<Option<PrimitiveArgumentLayout>, ExecutionError> {
    Ok(match param_ty {
        Type::Signer => return Ok(None),
        Type::Reference(_, _) => {
            invariant_violation!("references should not be added as a constraint")
        }
        Type::Bool => Some(PrimitiveArgumentLayout::Bool),
        Type::U8 => Some(PrimitiveArgumentLayout::U8),
        Type::U16 => Some(PrimitiveArgumentLayout::U16),
        Type::U32 => Some(PrimitiveArgumentLayout::U32),
        Type::U64 => Some(PrimitiveArgumentLayout::U64),
        Type::U128 => Some(PrimitiveArgumentLayout::U128),
        Type::U256 => Some(PrimitiveArgumentLayout::U256),
        Type::Address => Some(PrimitiveArgumentLayout::Address),

        Type::Vector(v) => {
            let info_opt = primitive_serialization_layout(&v.element_type)?;
            info_opt.map(|layout| PrimitiveArgumentLayout::Vector(Box::new(layout)))
        }
        Type::Datatype(dt) => {
            let resolved = dt.qualified_ident();
            // is option of a string
            if resolved == RESOLVED_STD_OPTION && dt.type_arguments.len() == 1 {
                let info_opt = primitive_serialization_layout(&dt.type_arguments[0])?;
                info_opt.map(|layout| PrimitiveArgumentLayout::Option(Box::new(layout)))
            } else if dt.type_arguments.is_empty() {
                if resolved == RESOLVED_SUI_ID {
                    Some(PrimitiveArgumentLayout::Address)
                } else if resolved == RESOLVED_ASCII_STR {
                    Some(PrimitiveArgumentLayout::Ascii)
                } else if resolved == RESOLVED_UTF8_STR {
                    Some(PrimitiveArgumentLayout::UTF8)
                } else {
                    None
                }
            } else {
                None
            }
        }
    })
}

fn check_receiving(command_arg_idx: u16, constraint: &Type) -> Result<(), ExecutionError> {
    if is_valid_receiving(constraint) {
        Ok(())
    } else {
        Err(command_argument_error(
            CommandArgumentError::TypeMismatch,
            command_arg_idx as usize,
        ))
    }
}

pub fn is_valid_pure_type(constraint: &Type) -> Result<bool, ExecutionError> {
    Ok(primitive_serialization_layout(constraint)?.is_some())
}

/// Returns true if a type is a `Receiving<t>` where `t` has `key`
pub fn is_valid_receiving(constraint: &Type) -> bool {
    let Type::Datatype(dt) = constraint else {
        return false;
    };
    dt.qualified_ident() == RESOLVED_RECEIVING_STRUCT
        && dt.type_arguments.len() == 1
        && dt.type_arguments[0].abilities().has_key()
}

//**************************************************************************************************
// Object usage
//**************************************************************************************************

fn command(context: &mut Context, sp!(_, command): &T::Command) -> Result<(), ExecutionError> {
    match command {
        T::Command_::MoveCall(mc) => {
            check_obj_usages(context, &mc.arguments)?;
            check_gas_by_values(&mc.arguments)?;
        }
        T::Command_::TransferObjects(objects, recipient) => {
            check_obj_usages(context, objects)?;
            check_obj_usage(context, recipient)?;
            // gas can be used by value in TransferObjects
        }
        T::Command_::SplitCoins(_, coin, amounts) => {
            check_obj_usage(context, coin)?;
            check_obj_usages(context, amounts)?;
            check_gas_by_value(coin)?;
            check_gas_by_values(amounts)?;
        }
        T::Command_::MergeCoins(_, target, coins) => {
            check_obj_usage(context, target)?;
            check_obj_usages(context, coins)?;
            check_gas_by_value(target)?;
            check_gas_by_values(coins)?;
        }
        T::Command_::MakeMoveVec(_, xs) => {
            check_obj_usages(context, xs)?;
            check_gas_by_values(xs)?;
        }
        T::Command_::Publish(_, _, _) => (),
        T::Command_::Upgrade(_, _, _, x, _) => {
            check_obj_usage(context, x)?;
            check_gas_by_value(x)?;
        }
    }
    Ok(())
}

// Checks for valid by-mut-ref and by-value usage of input objects
fn check_obj_usages(
    context: &mut Context,
    arguments: &[T::Argument],
) -> Result<(), ExecutionError> {
    for arg in arguments {
        check_obj_usage(context, arg)?;
    }
    Ok(())
}

fn check_obj_usage(context: &mut Context, arg: &T::Argument) -> Result<(), ExecutionError> {
    match &arg.value.0 {
        T::Argument__::Borrow(true, l) => check_obj_by_mut_ref(context, arg.idx, l),
        T::Argument__::Use(T::Usage::Move(l)) => check_by_value(context, arg.idx, l),
        // We do not care about
        // - immutable object borrowing
        // - copying/read ref (since you cannot copy objects)
        // - freeze (since an input object cannot be a reference without a borrow)
        T::Argument__::Borrow(false, _)
        | T::Argument__::Use(T::Usage::Copy { .. })
        | T::Argument__::Read(_)
        | T::Argument__::Freeze(_) => Ok(()),
    }
}

// Checks for valid by-mut-ref usage of input objects
fn check_obj_by_mut_ref(
    context: &mut Context,
    arg_idx: u16,
    location: &T::Location,
) -> Result<(), ExecutionError> {
    match location {
        T::Location::GasCoin | T::Location::Result(_, _) | T::Location::TxContext => Ok(()),
        T::Location::Input(idx) => match &context.inputs[*idx as usize] {
            None
            | Some(ObjectUsage {
                allow_by_mut_ref: true,
                ..
            }) => Ok(()),
            Some(ObjectUsage {
                allow_by_mut_ref: false,
                ..
            }) => Err(command_argument_error(
                CommandArgumentError::InvalidObjectByMutRef,
                arg_idx as usize,
            )),
        },
    }
}

// Checks for valid by-value usage of input objects
fn check_by_value(
    context: &mut Context,
    arg_idx: u16,
    location: &T::Location,
) -> Result<(), ExecutionError> {
    match location {
        T::Location::TxContext | T::Location::GasCoin | T::Location::Result(_, _) => Ok(()),
        T::Location::Input(idx) => match &context.inputs[*idx as usize] {
            None
            | Some(ObjectUsage {
                allow_by_value: true,
                ..
            }) => Ok(()),
            Some(ObjectUsage {
                allow_by_value: false,
                ..
            }) => Err(command_argument_error(
                CommandArgumentError::InvalidObjectByValue,
                arg_idx as usize,
            )),
        },
    }
}

// Checks for no by value usage of gas
fn check_gas_by_values(arguments: &[T::Argument]) -> Result<(), ExecutionError> {
    for arg in arguments {
        check_gas_by_value(arg)?;
    }
    Ok(())
}

fn check_gas_by_value(arg: &T::Argument) -> Result<(), ExecutionError> {
    match &arg.value.0 {
        T::Argument__::Use(T::Usage::Move(l)) => check_gas_by_value_loc(arg.idx, l),
        // We do not care about the read/freeze case since they cannot move an object input
        T::Argument__::Borrow(_, _)
        | T::Argument__::Use(T::Usage::Copy { .. })
        | T::Argument__::Read(_)
        | T::Argument__::Freeze(_) => Ok(()),
    }
}

fn check_gas_by_value_loc(idx: u16, location: &T::Location) -> Result<(), ExecutionError> {
    match location {
        T::Location::GasCoin => Err(command_argument_error(
            CommandArgumentError::InvalidGasCoinUsage,
            idx as usize,
        )),
        T::Location::TxContext | T::Location::Input(_) | T::Location::Result(_, _) => Ok(()),
    }
}
