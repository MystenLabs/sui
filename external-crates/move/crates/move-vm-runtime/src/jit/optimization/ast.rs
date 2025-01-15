// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::shared::types::{PackageStorageId, RuntimePackageId};

use move_binary_format::{
    file_format::{
        FunctionDefinitionIndex, ConstantPoolIndex, LocalIndex, FunctionHandleIndex, FunctionInstantiationIndex, StructDefinitionIndex, StructDefInstantiationIndex, FieldHandleIndex, FieldInstantiationIndex, SignatureIndex, VariantHandleIndex, VariantInstantiationHandleIndex, VariantJumpTableIndex,
    },
    CompiledModule,
};
use move_core_types::{
    language_storage::ModuleId,
    resolver::TypeOrigin,
};
use std::collections::BTreeMap;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

/// An optimized package
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Package {
    pub(crate) runtime_id: RuntimePackageId,
    pub(crate) storage_id: PackageStorageId,
    pub(crate) modules: BTreeMap<ModuleId, Module>,
    pub(crate) type_origin_table: Vec<TypeOrigin>,
    pub(crate) linkage_table: BTreeMap<RuntimePackageId, PackageStorageId>,
}

/// An optimized module
#[derive(Debug, Clone)]
pub struct Module {
    pub(crate) value: CompiledModule,
    /// Optimized versions of the functions defined in the module.
    pub(crate) functions: BTreeMap<FunctionDefinitionIndex, Option<Code>>,
}

pub(crate) type Label = u16;

/// Optimized Function Code
#[derive(Debug, Clone)]
pub struct Code {
    pub(crate) code: BTreeMap<Label, Vec<Bytecode>>
}

/// Optimized Bytecode
#[derive(Debug, Clone)]
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
