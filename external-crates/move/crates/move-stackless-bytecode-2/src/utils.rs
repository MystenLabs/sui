// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::normalized::Bytecode as NB;
use move_symbol_pool::Symbol;

pub(crate) fn comma_separated<T: std::fmt::Display>(items: &[T]) -> String {
    items
        .iter()
        .map(|item| format!("{}", item))
        .collect::<Vec<_>>()
        .join(", ")
}


pub(crate) fn debug_fun(op: &NB<Symbol>, pc: usize, function: &move_binary_format::normalized::Function<Symbol>) -> String {
    format!(
        "Bytecode: {:?} at pc: {} in function: {}\nCode: {:#}\nLocals: {:?}",
        op,
        pc,
        function.name,
        print_code(function.code()),
        function.locals
    )
}

pub(crate) fn print_code(code: &[NB<Symbol>]) -> String {
    code.iter()
        .enumerate()
        .map(|(pc, bytecode)| format!("{}: {}", pc, print_bytecode(bytecode)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn print_bytecode(bytecode: &NB<Symbol>) -> String {
    match bytecode {
        NB::Pop => "Pop".to_string(),

        NB::Ret => "Ret".to_string(),

        NB::BrTrue(label) => {
            format!("BrTrue [{}]", label)
        }

        NB::BrFalse(label) => {
            format!("BrFalse [{}]", label)
        }

        NB::Branch(label) => {
            format!("Branch [{}]", label)
        }

        NB::LdU8(value) => {
            format!("LdU8({})", value)
        }

        NB::LdU16(value) => {
            format!("LdU16({})", value)
        }

        NB::LdU32(value) => {
            format!("LdU32({})", value)
        }

        NB::LdU64(value) => {
            format!("LdU64({})", value)
        }

        NB::LdU128(value) => {
            format!("LdU128({})", value)
        }

        NB::LdU256(value) => {
            format!("LdU256({})", value)
        }

        NB::CastU8 => "CastU8".to_string(),

        NB::CastU16 => "CastU16".to_string(),

        NB::CastU32 => "CastU32".to_string(),

        NB::CastU64 => "CastU64".to_string(),

        NB::CastU128 => "CastU128".to_string(),

        NB::CastU256 => "CastU256".to_string(),

        NB::LdConst(value) => {
            format!("LdConst({:?})", value.data)
        }

        NB::LdTrue => "LdTrue".to_string(),

        NB::LdFalse => "LdFalse".to_string(),

        NB::CopyLoc(loc) => {
            format!("CopyLoc [{}]", loc)
        }

        NB::MoveLoc(loc) => {
            format!("MoveLoc [{}]", loc)
        }

        NB::StLoc(loc) => {
            format!("StLoc [{}]", loc)
        }

        NB::Call(function_ref) => {
            format!("Call::{}", function_ref.function)
        }

        NB::Pack(struct_ref) => {
            format!("Pack<{}>", struct_ref.struct_.name)
        }

        NB::Unpack(struct_ref) => {
            format!("Unpack<{}>", struct_ref.struct_.name)
        }

        NB::ReadRef => "ReadRef".to_string(),

        NB::WriteRef => "WriteRef".to_string(),

        NB::FreezeRef => "FreezeRef".to_string(),

        NB::MutBorrowLoc(loc) => {
            format!("MutBorrowLoc [{}]", loc)
        }

        NB::ImmBorrowLoc(loc) => {
            format!("ImmBorrowLoc [{}]", loc)
        }

        NB::ImmBorrowField(field_ref) => {
            format!("ImmBorrowField<{}> ", field_ref.field.type_)
        }

        NB::MutBorrowField(field_ref) => {
            format!("MutBorrowField<{}> ", field_ref.field.type_)
        }

        NB::Add => "Add".to_string(),

        NB::Sub => "Sub".to_string(),

        NB::Mul => "Mul".to_string(),

        NB::Div => "Div".to_string(),

        NB::Mod => "Mod".to_string(),

        NB::BitOr => "BitOr".to_string(),

        NB::BitAnd => "BitAnd".to_string(),

        NB::Xor => "Xor".to_string(),

        NB::Or => "Or".to_string(),

        NB::And => "And".to_string(),

        NB::Shl => "Shl".to_string(),

        NB::Shr => "Shr".to_string(),

        NB::Not => "Not".to_string(),

        NB::Eq => "Eq".to_string(),

        NB::Neq => "Neq".to_string(),

        NB::Lt => "Lt".to_string(),

        NB::Gt => "Gt".to_string(),

        NB::Le => "Le".to_string(),

        NB::Ge => "Ge".to_string(),

        NB::Abort => "Abort".to_string(),

        NB::Nop => "Nop".to_string(),

        NB::VecPack(vec_ref) => {
            format!("VecPack<{}>", vec_ref.as_ref().0)
        }

        NB::VecLen(vec_ref) => {
            format!("VecLen<{}>", vec_ref)
        }

        NB::VecImmBorrow(vec_ref) => {
            format!("VecImmBorrow<{}>", vec_ref)
        }

        NB::VecMutBorrow(vec_ref) => {
            format!("VecMutBorrow<{}>", vec_ref)
        }

        NB::VecPushBack(vec_ref) => {
            format!("VecPushBack<{}>", vec_ref)
        }

        NB::VecPopBack(vec_ref) => {
            format!("VecPopBack<{}>", vec_ref)
        }

        NB::VecUnpack(vec_ref) => {
            format!("VecUnpack<{}>", vec_ref.as_ref().0)
        }

        NB::VecSwap(vec_ref) => {
            format!("VecSwap<{}>", vec_ref)
        }

        NB::PackVariant(variant_ref) => {
            format!("PackVariant<{}>", variant_ref.variant.name)
        }

        NB::UnpackVariant(variant_ref) => {
            format!("UnpackVariant<{}>", variant_ref.variant.name)
        }

        NB::UnpackVariantImmRef(variant_ref) => {
            format!("UnpackVariantImmRef<{}>", variant_ref.variant.name)
        }

        NB::UnpackVariantMutRef(variant_ref) => {
            format!("UnpackVariantMutRef<{}>", variant_ref.variant.name)
        }

        NB::VariantSwitch(jt) => {
            format!("VariantSwitch [{}]", jt.enum_.name)
        }

        NB::ExistsDeprecated(_)
        | NB::MoveToDeprecated(_)
        | NB::MoveFromDeprecated(_)
        | NB::ImmBorrowGlobalDeprecated(_)
        | NB::MutBorrowGlobalDeprecated(_) => "Deprecated operation".to_string(),
    }
}
