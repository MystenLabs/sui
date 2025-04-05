// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::shared::types::{OriginalId, VersionId};

use move_binary_format::{
    file_format::{
        ConstantPoolIndex, FieldHandleIndex, FieldInstantiationIndex, FunctionDefinitionIndex,
        FunctionHandleIndex, FunctionInstantiationIndex, JumpTableInner, LocalIndex,
        SignatureIndex, StructDefInstantiationIndex, StructDefinitionIndex, VariantHandleIndex,
        VariantInstantiationHandleIndex, VariantJumpTable, VariantJumpTableIndex,
    },
    CompiledModule,
};
use move_core_types::{language_storage::ModuleId, resolver::TypeOrigin};
use std::collections::BTreeMap;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

/// An optimized package
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Package {
    pub(crate) original_id: OriginalId,
    pub(crate) version_id: VersionId,
    pub(crate) modules: BTreeMap<ModuleId, Module>,
    pub(crate) type_origin_table: Vec<TypeOrigin>,
    pub(crate) linkage_table: BTreeMap<OriginalId, VersionId>,
}

/// An optimized module
#[derive(Debug, Clone)]
pub struct Module {
    pub(crate) compiled_module: CompiledModule,
    /// Optimized versions of the functions defined in the module.
    pub(crate) functions: BTreeMap<FunctionDefinitionIndex, Function>,
}

/// Representation of functions being optimized
#[derive(Debug, Clone)]
pub struct Function {
    /// Original index in the compiled module
    #[allow(unused)]
    pub(crate) ndx: FunctionDefinitionIndex,
    /// Optimized code
    pub(crate) code: Option<Code>,
}

pub(crate) type Label = u16;

/// Optimized Function Code
#[derive(Debug, Clone)]
pub struct Code {
    pub(crate) jump_tables: Vec<VariantJumpTable>,
    pub(crate) code: BTreeMap<Label, Vec<Bytecode>>,
}

/// Optimized Bytecode
#[derive(Clone)]
pub enum Bytecode {
    Pop,
    Ret,
    BrTrue(Label),
    BrFalse(Label),
    Branch(Label),
    LdU8(u8),
    LdU64(u64),
    LdU128(Box<u128>),
    CastU8,
    CastU64,
    CastU128,
    LdConst(ConstantPoolIndex),
    LdTrue,
    LdFalse,
    CopyLoc(LocalIndex),
    MoveLoc(LocalIndex),
    StLoc(LocalIndex),
    Call(FunctionHandleIndex),
    CallGeneric(FunctionInstantiationIndex),
    Pack(StructDefinitionIndex),
    PackGeneric(StructDefInstantiationIndex),
    Unpack(StructDefinitionIndex),
    UnpackGeneric(StructDefInstantiationIndex),
    ReadRef,
    WriteRef,
    FreezeRef,
    MutBorrowLoc(LocalIndex),
    ImmBorrowLoc(LocalIndex),
    MutBorrowField(FieldHandleIndex),
    MutBorrowFieldGeneric(FieldInstantiationIndex),
    ImmBorrowField(FieldHandleIndex),
    ImmBorrowFieldGeneric(FieldInstantiationIndex),
    Add,
    Sub,
    Mul,
    Mod,
    Div,
    BitOr,
    BitAnd,
    Xor,
    Or,
    And,
    Not,
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
    Abort,
    Nop,
    Shl,
    Shr,
    VecPack(SignatureIndex, u64),
    VecLen(SignatureIndex),
    VecImmBorrow(SignatureIndex),
    VecMutBorrow(SignatureIndex),
    VecPushBack(SignatureIndex),
    VecPopBack(SignatureIndex),
    VecUnpack(SignatureIndex, u64),
    VecSwap(SignatureIndex),
    LdU16(u16),
    LdU32(u32),
    LdU256(Box<move_core_types::u256::U256>),
    CastU16,
    CastU32,
    CastU256,
    PackVariant(VariantHandleIndex),
    PackVariantGeneric(VariantInstantiationHandleIndex),
    UnpackVariant(VariantHandleIndex),
    UnpackVariantImmRef(VariantHandleIndex),
    UnpackVariantMutRef(VariantHandleIndex),
    UnpackVariantGeneric(VariantInstantiationHandleIndex),
    UnpackVariantGenericImmRef(VariantInstantiationHandleIndex),
    UnpackVariantGenericMutRef(VariantInstantiationHandleIndex),
    VariantSwitch(VariantJumpTableIndex),
}

impl ::std::fmt::Debug for Bytecode {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match self {
            Bytecode::Pop => write!(f, "Pop"),
            Bytecode::Ret => write!(f, "Ret"),
            Bytecode::BrTrue(a) => write!(f, "BrTrue({})", a),
            Bytecode::BrFalse(a) => write!(f, "BrFalse({})", a),
            Bytecode::Branch(a) => write!(f, "Branch({})", a),
            Bytecode::LdU8(a) => write!(f, "LdU8({})", a),
            Bytecode::LdU16(a) => write!(f, "LdU16({})", a),
            Bytecode::LdU32(a) => write!(f, "LdU32({})", a),
            Bytecode::LdU64(a) => write!(f, "LdU64({})", a),
            Bytecode::LdU128(a) => write!(f, "LdU128({})", a),
            Bytecode::LdU256(a) => write!(f, "LdU256({})", a),
            Bytecode::CastU8 => write!(f, "CastU8"),
            Bytecode::CastU16 => write!(f, "CastU16"),
            Bytecode::CastU32 => write!(f, "CastU32"),
            Bytecode::CastU64 => write!(f, "CastU64"),
            Bytecode::CastU128 => write!(f, "CastU128"),
            Bytecode::CastU256 => write!(f, "CastU256"),
            Bytecode::LdConst(a) => write!(f, "LdConst({})", a),
            Bytecode::LdTrue => write!(f, "LdTrue"),
            Bytecode::LdFalse => write!(f, "LdFalse"),
            Bytecode::CopyLoc(a) => write!(f, "CopyLoc({})", a),
            Bytecode::MoveLoc(a) => write!(f, "MoveLoc({})", a),
            Bytecode::StLoc(a) => write!(f, "StLoc({})", a),
            Bytecode::Call(fun) => write!(f, "Call({})", fun),
            Bytecode::CallGeneric(ndx) => write!(f, "CallGeneric({})", ndx),
            Bytecode::Pack(a) => write!(f, "Pack({})", a),
            Bytecode::PackGeneric(a) => write!(f, "PackGeneric({})", a),
            Bytecode::Unpack(a) => write!(f, "Unpack({})", a),
            Bytecode::UnpackGeneric(a) => write!(f, "UnpackGeneric({})", a),
            Bytecode::ReadRef => write!(f, "ReadRef"),
            Bytecode::WriteRef => write!(f, "WriteRef"),
            Bytecode::FreezeRef => write!(f, "FreezeRef"),
            Bytecode::MutBorrowLoc(a) => write!(f, "MutBorrowLoc({})", a),
            Bytecode::ImmBorrowLoc(a) => write!(f, "ImmBorrowLoc({})", a),
            Bytecode::MutBorrowField(a) => write!(f, "MutBorrowField({:?})", a),
            Bytecode::MutBorrowFieldGeneric(a) => write!(f, "MutBorrowFieldGeneric({:?})", a),
            Bytecode::ImmBorrowField(a) => write!(f, "ImmBorrowField({:?})", a),
            Bytecode::ImmBorrowFieldGeneric(a) => write!(f, "ImmBorrowFieldGeneric({:?})", a),
            Bytecode::Add => write!(f, "Add"),
            Bytecode::Sub => write!(f, "Sub"),
            Bytecode::Mul => write!(f, "Mul"),
            Bytecode::Mod => write!(f, "Mod"),
            Bytecode::Div => write!(f, "Div"),
            Bytecode::BitOr => write!(f, "BitOr"),
            Bytecode::BitAnd => write!(f, "BitAnd"),
            Bytecode::Xor => write!(f, "Xor"),
            Bytecode::Shl => write!(f, "Shl"),
            Bytecode::Shr => write!(f, "Shr"),
            Bytecode::Or => write!(f, "Or"),
            Bytecode::And => write!(f, "And"),
            Bytecode::Not => write!(f, "Not"),
            Bytecode::Eq => write!(f, "Eq"),
            Bytecode::Neq => write!(f, "Neq"),
            Bytecode::Lt => write!(f, "Lt"),
            Bytecode::Gt => write!(f, "Gt"),
            Bytecode::Le => write!(f, "Le"),
            Bytecode::Ge => write!(f, "Ge"),
            Bytecode::Abort => write!(f, "Abort"),
            Bytecode::Nop => write!(f, "Nop"),
            Bytecode::VecPack(a, n) => write!(f, "VecPack({}, {})", a, n),
            Bytecode::VecLen(a) => write!(f, "VecLen({})", a),
            Bytecode::VecImmBorrow(a) => write!(f, "VecImmBorrow({})", a),
            Bytecode::VecMutBorrow(a) => write!(f, "VecMutBorrow({})", a),
            Bytecode::VecPushBack(a) => write!(f, "VecPushBack({})", a),
            Bytecode::VecPopBack(a) => write!(f, "VecPopBack({})", a),
            Bytecode::VecUnpack(a, n) => write!(f, "VecUnpack({}, {})", a, n),
            Bytecode::VecSwap(a) => write!(f, "VecSwap({})", a),
            Bytecode::PackVariant(handle) => {
                write!(f, "PackVariant({:?})", handle)
            }
            Bytecode::PackVariantGeneric(handle) => write!(f, "PackVariantGeneric({:?})", handle),
            Bytecode::UnpackVariant(handle) => write!(f, "UnpackVariant({:?})", handle),
            Bytecode::UnpackVariantGeneric(handle) => {
                write!(f, "UnpackVariantGeneric({:?})", handle)
            }
            Bytecode::UnpackVariantImmRef(handle) => {
                write!(f, "UnpackVariantImmRef({:?})", handle)
            }
            Bytecode::UnpackVariantGenericImmRef(handle) => {
                write!(f, "UnpackVariantGenericImmRef({:?})", handle)
            }
            Bytecode::UnpackVariantMutRef(handle) => {
                write!(f, "UnpackVariantMutRef({:?})", handle)
            }
            Bytecode::UnpackVariantGenericMutRef(handle) => {
                write!(f, "UnpackVariantGenericMutRef({:?})", handle)
            }
            Bytecode::VariantSwitch(jt) => write!(f, "VariantSwitch({:?})", jt),
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl Bytecode {
    pub fn branch_target(&self, tables: &[VariantJumpTable]) -> Option<Vec<Label>> {
        match self {
            Bytecode::BrTrue(target) | Bytecode::BrFalse(target) | Bytecode::Branch(target) => {
                Some(vec![*target as Label])
            }
            Bytecode::VariantSwitch(table) => {
                let jump_table: &JumpTableInner = &tables.get(table.0 as usize)?.jump_table;
                match jump_table {
                    JumpTableInner::Full(vec) => {
                        Some(vec.iter().map(|ndx| *ndx as Label).collect())
                    }
                }
            }
            Bytecode::Pop
            | Bytecode::Ret
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
            | Bytecode::UnpackVariantGenericMutRef(_) => None,
        }
    }

    pub fn is_unconditional_branch(&self) -> bool {
        match self {
            Bytecode::Branch(_) | Bytecode::Abort | Bytecode::Ret => true,
            // True because verifier insists these are exhaustive
            Bytecode::VariantSwitch(_) => true,
            Bytecode::Pop
            | Bytecode::BrTrue(_)
            | Bytecode::BrFalse(_)
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
            | Bytecode::UnpackVariantGenericMutRef(_) => false,
        }
    }
}
