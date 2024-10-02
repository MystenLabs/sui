// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::verification_failure;
use move_binary_format::file_format::{Bytecode, CompiledModule};
use sui_types::error::ExecutionError;

pub fn verify_module(module: &CompiledModule) -> Result<(), ExecutionError> {
    verify_global_storage_access(module)
}

/// Global storage in sui is handled by sui instead of within Move.
/// Hence we want to forbid any global storage access in Move.
fn verify_global_storage_access(module: &CompiledModule) -> Result<(), ExecutionError> {
    for func_def in &module.function_defs {
        if func_def.code.is_none() {
            continue;
        }
        let code = &func_def.code.as_ref().unwrap().code;
        let mut invalid_bytecode = vec![];
        for bytecode in code {
            match bytecode {
                Bytecode::MoveFromDeprecated(_)
                | Bytecode::MoveFromGenericDeprecated(_)
                | Bytecode::MoveToDeprecated(_)
                | Bytecode::MoveToGenericDeprecated(_)
                | Bytecode::ImmBorrowGlobalDeprecated(_)
                | Bytecode::MutBorrowGlobalDeprecated(_)
                | Bytecode::ImmBorrowGlobalGenericDeprecated(_)
                | Bytecode::MutBorrowGlobalGenericDeprecated(_)
                | Bytecode::ExistsDeprecated(_)
                | Bytecode::ExistsGenericDeprecated(_) => {
                    invalid_bytecode.push(bytecode);
                }
                Bytecode::Pop
                | Bytecode::Ret
                | Bytecode::BrTrue(_)
                | Bytecode::BrFalse(_)
                | Bytecode::Branch(_)
                | Bytecode::LdU8(_)
                | Bytecode::LdU16(_)
                | Bytecode::LdU32(_)
                | Bytecode::LdU64(_)
                | Bytecode::LdU128(_)
                | Bytecode::LdU256(_)
                | Bytecode::CastU8
                | Bytecode::CastU16
                | Bytecode::CastU32
                | Bytecode::CastU64
                | Bytecode::CastU128
                | Bytecode::CastU256
                | Bytecode::LdConst(_)
                | Bytecode::LdTrue
                | Bytecode::LdFalse
                | Bytecode::CopyLoc(_)
                | Bytecode::MoveLoc(_)
                | Bytecode::StLoc(_)
                | Bytecode::Call(_)
                | Bytecode::CallGeneric(_)
                | Bytecode::Pack(_)
                | Bytecode::PackGeneric(_)
                | Bytecode::Unpack(_)
                | Bytecode::UnpackGeneric(_)
                | Bytecode::ReadRef
                | Bytecode::WriteRef
                | Bytecode::FreezeRef
                | Bytecode::MutBorrowLoc(_)
                | Bytecode::ImmBorrowLoc(_)
                | Bytecode::MutBorrowField(_)
                | Bytecode::MutBorrowFieldGeneric(_)
                | Bytecode::ImmBorrowField(_)
                | Bytecode::ImmBorrowFieldGeneric(_)
                | Bytecode::Add
                | Bytecode::Sub
                | Bytecode::Mul
                | Bytecode::Mod
                | Bytecode::Div
                | Bytecode::BitOr
                | Bytecode::BitAnd
                | Bytecode::Xor
                | Bytecode::Shl
                | Bytecode::Shr
                | Bytecode::Or
                | Bytecode::And
                | Bytecode::Not
                | Bytecode::Eq
                | Bytecode::Neq
                | Bytecode::Lt
                | Bytecode::Gt
                | Bytecode::Le
                | Bytecode::Ge
                | Bytecode::Abort
                | Bytecode::Nop
                | Bytecode::VecPack(_, _)
                | Bytecode::VecLen(_)
                | Bytecode::VecImmBorrow(_)
                | Bytecode::VecMutBorrow(_)
                | Bytecode::VecPushBack(_)
                | Bytecode::VecPopBack(_)
                | Bytecode::VecUnpack(_, _)
                | Bytecode::VecSwap(_) => {}
                Bytecode::PackVariant(_)
                | Bytecode::PackVariantGeneric(_)
                | Bytecode::UnpackVariant(_)
                | Bytecode::UnpackVariantImmRef(_)
                | Bytecode::UnpackVariantMutRef(_)
                | Bytecode::UnpackVariantGeneric(_)
                | Bytecode::UnpackVariantGenericImmRef(_)
                | Bytecode::UnpackVariantGenericMutRef(_)
                | Bytecode::VariantSwitch(_) => {
                    unreachable!("variant bytecodes should never occur in v0")
                }
            }
        }
        if !invalid_bytecode.is_empty() {
            return Err(verification_failure(format!(
                "Access to Move global storage is not allowed. Found in function {}: {:?}",
                module.identifier_at(module.function_handle_at(func_def.function).name),
                invalid_bytecode,
            )));
        }
    }
    Ok(())
}
