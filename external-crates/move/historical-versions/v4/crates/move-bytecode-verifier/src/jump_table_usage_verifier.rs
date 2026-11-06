// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::absint::FunctionContext;
use move_binary_format::{
    errors::PartialVMResult,
    file_format::{Bytecode, CompiledModule},
    partial_vm_error,
};
use move_bytecode_verifier_meter::Meter;
use move_vm_config::verifier::VerifierConfig;
use std::collections::BTreeSet;

// Verifies that all jump tables defined in the function are used by a `VariantSwitch` instruction.
// This is a sanity check to ensure that the function does not contain dead jump tables, which
// while not unsound, are likely indicative of an issue in the compiler or a "smelly" input module
// and we should reject it.
pub(crate) fn verify(
    _config: &VerifierConfig,
    _module: &CompiledModule,
    function_context: &FunctionContext,
    _meter: &mut (impl Meter + ?Sized),
) -> PartialVMResult<()> {
    let mut seen = BTreeSet::new();
    for bytecode in function_context.code().code.iter() {
        match bytecode {
            Bytecode::VariantSwitch(variant_jump_table_index) => {
                seen.insert(*variant_jump_table_index);
            }
            Bytecode::Pop
            | Bytecode::Ret
            | Bytecode::BrTrue(_)
            | Bytecode::BrFalse(_)
            | Bytecode::Branch(_)
            | Bytecode::LdU8(_)
            | Bytecode::LdU64(_)
            | Bytecode::LdU128(_)
            | Bytecode::CastU8
            | Bytecode::CastU64
            | Bytecode::CastU128
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
            | Bytecode::Shl
            | Bytecode::Shr
            | Bytecode::VecPack(_, _)
            | Bytecode::VecLen(_)
            | Bytecode::VecImmBorrow(_)
            | Bytecode::VecMutBorrow(_)
            | Bytecode::VecPushBack(_)
            | Bytecode::VecPopBack(_)
            | Bytecode::VecUnpack(_, _)
            | Bytecode::VecSwap(_)
            | Bytecode::LdU16(_)
            | Bytecode::LdU32(_)
            | Bytecode::LdU256(_)
            | Bytecode::CastU16
            | Bytecode::CastU32
            | Bytecode::CastU256
            | Bytecode::PackVariant(_)
            | Bytecode::PackVariantGeneric(_)
            | Bytecode::UnpackVariant(_)
            | Bytecode::UnpackVariantImmRef(_)
            | Bytecode::UnpackVariantMutRef(_)
            | Bytecode::UnpackVariantGeneric(_)
            | Bytecode::UnpackVariantGenericImmRef(_)
            | Bytecode::UnpackVariantGenericMutRef(_)
            | Bytecode::ExistsDeprecated(_)
            | Bytecode::ExistsGenericDeprecated(_)
            | Bytecode::MoveFromDeprecated(_)
            | Bytecode::MoveFromGenericDeprecated(_)
            | Bytecode::MoveToDeprecated(_)
            | Bytecode::MoveToGenericDeprecated(_)
            | Bytecode::MutBorrowGlobalDeprecated(_)
            | Bytecode::MutBorrowGlobalGenericDeprecated(_)
            | Bytecode::ImmBorrowGlobalDeprecated(_)
            | Bytecode::ImmBorrowGlobalGenericDeprecated(_) => (),
        }
    }

    if seen.len() != function_context.code().jump_tables.len() {
        return Err(partial_vm_error!(INVALID_ENUM_SWITCH));
    }

    Ok(())
}
