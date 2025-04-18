// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    programmable_transactions::execution::{bcs_argument_validate, PrimitiveArgumentLayout},
    static_programmable_transactions::env::datatype_qualified_ident,
};

use crate::static_programmable_transactions::{env::Env, typing::ast as T};
use move_vm_types::loaded_data::runtime_types::{CachedDatatype, Type};
use sui_types::{
    base_types::{RESOLVED_ASCII_STR, RESOLVED_STD_OPTION, RESOLVED_UTF8_STR},
    error::{command_argument_error, ExecutionError, ExecutionErrorKind},
    execution_status::CommandArgumentError,
    id::RESOLVED_SUI_ID,
    transaction::{CallArg, ObjectArg},
    transfer::RESOLVED_RECEIVING_STRUCT,
};

pub fn verify(env: &Env, txn: &T::Transaction) -> Result<(), ExecutionError> {
    let T::Transaction {
        inputs,
        commands: _,
    } = txn;
    for (arg, ty) in inputs {
        match ty {
            T::InputType::Bytes(constraints) => {
                for (constraint, &(command_idx, arg_idx)) in constraints {
                    check_constraint(env, arg_idx, arg, constraint)
                        .map_err(|e| e.with_command_index(command_idx as usize))?;
                }
            }
            T::InputType::Fixed(_) => (),
        }
    }
    Ok(())
}

fn check_constraint(
    env: &Env,
    command_arg_idx: u16,
    arg: &CallArg,
    constraint: &Type,
) -> Result<(), ExecutionError> {
    match arg {
        CallArg::Pure(bytes) => check_pure_bytes(env, command_arg_idx, bytes, constraint),
        CallArg::Object(ObjectArg::Receiving(_)) => {
            check_receiving(env, command_arg_idx, constraint)
        }
        CallArg::Object(ObjectArg::ImmOrOwnedObject(_) | ObjectArg::SharedObject { .. }) => {
            invariant_violation!("Object inputs should be Fixed")
        }
    }
}

fn check_pure_bytes(
    env: &Env,
    command_arg_idx: u16,
    bytes: &[u8],
    constraint: &Type,
) -> Result<(), ExecutionError> {
    let Some(layout) = primitive_serialization_layout(env, constraint)? else {
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
    env: &Env,
    param_ty: &Type,
) -> Result<Option<PrimitiveArgumentLayout>, ExecutionError> {
    Ok(match param_ty {
        Type::Signer => return Ok(None),
        Type::TyParam(_) => {
            invariant_violation!("TyParam should be instantiated and never added as a constraint")
        }
        Type::Reference(_) | Type::MutableReference(_) => {
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

        Type::Vector(inner) => {
            let info_opt = primitive_serialization_layout(env, inner)?;
            info_opt.map(|layout| PrimitiveArgumentLayout::Vector(Box::new(layout)))
        }
        Type::DatatypeInstantiation(inst) => {
            let (idx, targs) = &**inst;
            let datatype = env.datatype(*idx)?;
            let datatype: &CachedDatatype = datatype.as_ref();
            let resolved = datatype_qualified_ident(datatype);
            // is option of a string
            if resolved == RESOLVED_STD_OPTION && targs.len() == 1 {
                let info_opt = primitive_serialization_layout(env, &targs[0])?;
                info_opt.map(|layout| PrimitiveArgumentLayout::Option(Box::new(layout)))
            } else {
                None
            }
        }
        Type::Datatype(idx) => {
            let datatype = env.datatype(*idx)?;
            let datatype: &CachedDatatype = datatype.as_ref();
            let resolved = datatype_qualified_ident(datatype);
            if resolved == RESOLVED_SUI_ID {
                Some(PrimitiveArgumentLayout::Address)
            } else if resolved == RESOLVED_ASCII_STR {
                Some(PrimitiveArgumentLayout::Ascii)
            } else if resolved == RESOLVED_UTF8_STR {
                Some(PrimitiveArgumentLayout::UTF8)
            } else {
                None
            }
        }
    })
}

fn check_receiving(
    env: &Env,
    command_arg_idx: u16,
    constraint: &Type,
) -> Result<(), ExecutionError> {
    let is_receiving = match constraint {
        Type::DatatypeInstantiation(inst) => {
            let (idx, targs) = &**inst;
            let datatype = env.datatype(*idx)?;
            let datatype: &CachedDatatype = datatype.as_ref();
            let resolved = datatype_qualified_ident(datatype);
            resolved == RESOLVED_RECEIVING_STRUCT && targs.len() == 1
        }
        _ => false,
    };
    if is_receiving {
        Ok(())
    } else {
        Err(command_argument_error(
            CommandArgumentError::TypeMismatch,
            command_arg_idx as usize,
        ))
    }
}
