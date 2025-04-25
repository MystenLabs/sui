// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::programmable_transactions::execution::{bcs_argument_validate, PrimitiveArgumentLayout};

use crate::static_programmable_transactions::{
    env::Env,
    loading::ast::Type,
    typing::ast::{self as T, InputArg, ObjectArg},
};
use sui_types::{
    base_types::{RESOLVED_ASCII_STR, RESOLVED_STD_OPTION, RESOLVED_UTF8_STR},
    error::{command_argument_error, ExecutionError, ExecutionErrorKind},
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
/// 2. That any `Object` arguments are used validly. This means mutable references are taken only
///    on mutable objects. And that the gas coin is only taken by value in transfer objects
pub fn verify(_env: &Env, txn: &T::Transaction) -> Result<(), ExecutionError> {
    let T::Transaction { inputs, commands } = txn;
    for (arg, ty) in inputs {
        match ty {
            T::InputType::Bytes(constraints) => {
                for (constraint, &(command_idx, arg_idx)) in constraints {
                    check_constraint(arg_idx, arg, constraint)
                        .map_err(|e| e.with_command_index(command_idx as usize))?;
                }
            }
            T::InputType::Fixed(_) => (),
        }
    }
    let context = &mut Context::new(inputs);
    for (i, (c, _t)) in commands.iter().enumerate() {
        command(context, c).map_err(|e| e.with_command_index(i))?;
    }
    Ok(())
}

//**************************************************************************************************
// Pure bytes
//**************************************************************************************************

fn check_constraint(
    command_arg_idx: u16,
    arg: &InputArg,
    constraint: &Type,
) -> Result<(), ExecutionError> {
    match arg {
        InputArg::Pure(bytes) => check_pure_bytes(command_arg_idx, bytes, constraint),
        InputArg::Receiving(_) => check_receiving(command_arg_idx, constraint),
        InputArg::Object(
            ObjectArg::ImmObject(_) | ObjectArg::OwnedObject(_) | ObjectArg::SharedObject { .. },
        ) => {
            invariant_violation!("Object inputs should be Fixed")
        }
    }
}

fn check_pure_bytes(
    command_arg_idx: u16,
    bytes: &[u8],
    constraint: &Type,
) -> Result<(), ExecutionError> {
    debug_assert!(false, "TODO implement mode for arbitrary values");
    let Some(layout) = primitive_serialization_layout(constraint)? else {
        let msg = format!(
            "Non-primitive argument at index {command_arg_idx}. If it is an object, it must be \
            populated by an object",
        );
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::command_argument_error(
                CommandArgumentError::InvalidUsageOfPureArg,
                command_arg_idx,
            ),
            msg,
        ));
    };
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
    let is_receiving = matches!(constraint ,
        Type::Datatype(dt) if
            dt.qualified_ident() == RESOLVED_RECEIVING_STRUCT && dt.type_arguments.len() == 1
    );
    if is_receiving {
        Ok(())
    } else {
        Err(command_argument_error(
            CommandArgumentError::TypeMismatch,
            command_arg_idx as usize,
        ))
    }
}

//**************************************************************************************************
// Object usage
//**************************************************************************************************

fn command(context: &mut Context, command: &T::Command) -> Result<(), ExecutionError> {
    match command {
        T::Command::MoveCall(mc) => {
            check_obj_usages(context, 0, &mc.arguments)?;
            check_gas_by_values(0, &mc.arguments)?;
        }
        T::Command::TransferObjects(objects, recipient) => {
            check_obj_usages(context, 0, objects)?;
            check_obj_usage(context, objects.len(), recipient)?;
            // gas can be used by value in TransferObjects
        }
        T::Command::SplitCoins(_, coin, amounts) => {
            check_obj_usage(context, 0, coin)?;
            check_obj_usages(context, 1, amounts)?;
            check_gas_by_value(0, coin)?;
            check_gas_by_values(1, amounts)?;
        }
        T::Command::MergeCoins(_, target, coins) => {
            check_obj_usage(context, 0, target)?;
            check_obj_usages(context, 1, coins)?;
            check_gas_by_value(0, target)?;
            check_gas_by_values(1, coins)?;
        }
        T::Command::MakeMoveVec(_, xs) => {
            check_obj_usages(context, 0, xs)?;
            check_gas_by_values(0, xs)?;
        }
        T::Command::Publish(_, _) => (),
        T::Command::Upgrade(_, _, _, x) => {
            check_obj_usage(context, 0, x)?;
            check_gas_by_value(0, x)?;
        }
    }
    Ok(())
}

// Checks for valid by-mut-ref and by-value usage of input objects
fn check_obj_usages(
    context: &mut Context,
    start: usize,
    arguments: &[T::Argument],
) -> Result<(), ExecutionError> {
    for (i, arg) in arguments.iter().enumerate() {
        check_obj_usage(context, start + i, arg)?;
    }
    Ok(())
}

fn check_obj_usage(
    context: &mut Context,
    arg_idx: usize,
    arg: &T::Argument,
) -> Result<(), ExecutionError> {
    match &arg.0 {
        T::Argument_::Borrow(true, l) => check_obj_by_mut_ref(context, arg_idx, l),
        T::Argument_::Use(T::Usage::Move(l)) => check_by_value_ref(context, arg_idx, l),
        T::Argument_::Borrow(false, _)
        | T::Argument_::Use(T::Usage::Copy { .. })
        | T::Argument_::Read(_) => Ok(()),
    }
}

// Checks for valid by-mut-ref usage of input objects
fn check_obj_by_mut_ref(
    context: &mut Context,
    arg_idx: usize,
    location: &T::Location,
) -> Result<(), ExecutionError> {
    match location {
        T::Location::GasCoin | T::Location::Result(_, _) => Ok(()),
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
                arg_idx,
            )),
        },
    }
}

// Checks for valid by-value usage of input objects
fn check_by_value_ref(
    context: &mut Context,
    arg_idx: usize,
    location: &T::Location,
) -> Result<(), ExecutionError> {
    match location {
        T::Location::GasCoin | T::Location::Result(_, _) => Ok(()),
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
                arg_idx,
            )),
        },
    }
}

// Checks for no by value usage of gas
fn check_gas_by_values(start: usize, arguments: &[T::Argument]) -> Result<(), ExecutionError> {
    for (i, arg) in arguments.iter().enumerate() {
        check_gas_by_value(start + i, arg)?;
    }
    Ok(())
}

fn check_gas_by_value(arg_idx: usize, arg: &T::Argument) -> Result<(), ExecutionError> {
    match &arg.0 {
        T::Argument_::Use(T::Usage::Move(l)) => check_gas_by_value_loc(arg_idx, l),
        T::Argument_::Borrow(_, _)
        | T::Argument_::Use(T::Usage::Copy { .. })
        | T::Argument_::Read(_) => Ok(()),
    }
}

fn check_gas_by_value_loc(arg_idx: usize, location: &T::Location) -> Result<(), ExecutionError> {
    match location {
        T::Location::GasCoin => Err(command_argument_error(
            CommandArgumentError::InvalidGasCoinUsage,
            arg_idx,
        )),
        T::Location::Input(_) | T::Location::Result(_, _) => Ok(()),
    }
}
