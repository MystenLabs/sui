// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Utilities for the Move model
use crate::model::{
    global_env::GlobalEnv,
    move_model::{Bytecode, Type},
};

pub fn bytecode_to_string(bytecode: &Bytecode) -> String {
    match bytecode {
        Bytecode::Nop => "Nop",
        Bytecode::Pop => "Pop",
        Bytecode::Ret => "Ret",
        Bytecode::BrTrue(_) => "BrTrue",
        Bytecode::BrFalse(_) => "BrFalse",
        Bytecode::Branch(_) => "Branch",
        Bytecode::LdConst(_) => "LdConst",
        Bytecode::LdTrue => "LdTrue",
        Bytecode::LdFalse => "LdFalse",
        Bytecode::LdU8(_) => "LdU8",
        Bytecode::LdU16(_) => "LdU16",
        Bytecode::LdU32(_) => "LdU32",
        Bytecode::LdU64(_) => "LdU64",
        Bytecode::LdU128(_) => "LdU128",
        Bytecode::LdU256(_) => "LdU256",
        Bytecode::CastU8 => "CastU8",
        Bytecode::CastU16 => "CastU16",
        Bytecode::CastU32 => "CastU32",
        Bytecode::CastU64 => "CastU64",
        Bytecode::CastU128 => "CastU128",
        Bytecode::CastU256 => "CastU256",
        Bytecode::Add => "Add",
        Bytecode::Sub => "Sub",
        Bytecode::Mul => "Mul",
        Bytecode::Mod => "Mod",
        Bytecode::Div => "Div",
        Bytecode::BitOr => "BitOr",
        Bytecode::BitAnd => "BitAnd",
        Bytecode::Xor => "Xor",
        Bytecode::Or => "Or",
        Bytecode::And => "And",
        Bytecode::Not => "Not",
        Bytecode::Eq => "Eq",
        Bytecode::Neq => "Neq",
        Bytecode::Lt => "Lt",
        Bytecode::Gt => "Gt",
        Bytecode::Le => "Le",
        Bytecode::Ge => "Ge",
        Bytecode::Shl => "Shl",
        Bytecode::Shr => "Shr",
        Bytecode::Abort => "Abort",
        Bytecode::CopyLoc(_) => "CopyLoc",
        Bytecode::MoveLoc(_) => "MoveLoc",
        Bytecode::StLoc(_) => "StLoc",
        Bytecode::Call(_) => "Call",
        Bytecode::CallGeneric(_, _) => "CallGeneric",
        Bytecode::Pack(_) => "Pack",
        Bytecode::PackGeneric(_, _) => "PackGeneric",
        Bytecode::Unpack(_) => "Unpack",
        Bytecode::UnpackGeneric(_, _) => "UnpackGeneric",
        Bytecode::MutBorrowLoc(_) => "MutBorrowLoc",
        Bytecode::ImmBorrowLoc(_) => "ImmBorrowLoc",
        Bytecode::MutBorrowField(_) => "MutBorrowField",
        Bytecode::MutBorrowFieldGeneric(_, _) => "MutBorrowFieldGeneric",
        Bytecode::ImmBorrowField(_) => "ImmBorrowField",
        Bytecode::ImmBorrowFieldGeneric(_, _) => "ImmBorrowFieldGeneric",
        Bytecode::ReadRef => "ReadRef",
        Bytecode::WriteRef => "WriteRef",
        Bytecode::FreezeRef => "FreezeRef",
        Bytecode::VecPack(_, _) => "VecPack",
        Bytecode::VecLen(_) => "VecLen",
        Bytecode::VecImmBorrow(_) => "VecImmBorrow",
        Bytecode::VecMutBorrow(_) => "VecMutBorrow",
        Bytecode::VecPushBack(_) => "VecPushBack",
        Bytecode::VecPopBack(_) => "VecPopBack",
        Bytecode::VecUnpack(_, _) => "VecUnpack",
        Bytecode::VecSwap(_) => "VecSwap",
    }
    .to_string()
}

pub fn type_name(env: &GlobalEnv, type_: &Type) -> String {
    match type_ {
        Type::Bool => "bool".to_string(),
        Type::U8 => "u8".to_string(),
        Type::U16 => "u16".to_string(),
        Type::U32 => "u32".to_string(),
        Type::U64 => "u64".to_string(),
        Type::U128 => "u128".to_string(),
        Type::U256 => "u256".to_string(),
        Type::Address => "address".to_string(),
        Type::Vector(inner) => {
            format!("vector<{}>", type_name(env, inner))
        }
        Type::Struct(struct_handle_idx) => env.struct_name_from_idx(*struct_handle_idx),
        Type::StructInstantiation(struct_inst) => {
            let (struct_handle_idx, type_arguments) = &**struct_inst;
            let type_arg = type_arguments
                .iter()
                .map(|type_| type_name(env, type_))
                .collect::<Vec<_>>();
            format!(
                "{}<{}>",
                env.struct_name_from_idx(*struct_handle_idx),
                type_arg.join(", ")
            )
        }
        Type::Reference(inner) => {
            format!("&{}", type_name(env, inner))
        }
        Type::MutableReference(inner) => {
            format!("&mut {}", type_name(env, inner))
        }
        Type::TypeParameter(idx) => format!("{}", *idx),
    }
}
