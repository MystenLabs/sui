// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// The type system as we know it today.
/// Each element of the type system is mapped into a struct in this module.
use crate::model::global_env::GlobalEnv;
use move_binary_format::file_format::{
    AbilitySet, CodeOffset, CompiledModule, ConstantPoolIndex, FunctionDefinitionIndex, LocalIndex,
    MemberCount, StructDefinitionIndex, StructTypeParameter, TypeParameterIndex, Visibility,
};
use move_core_types::{language_storage::ModuleId, u256::U256};
use std::collections::{BTreeMap, BTreeSet};
use sui_types::{base_types::ObjectID, move_package::MovePackage};

// An index in one of the pools
pub type PackageIndex = usize;
pub type ModuleIndex = usize;
pub type StructIndex = usize;
pub type FunctionIndex = usize;
pub type IdentifierIndex = usize;

/// A package as known in the GlobalEnv.
/// Wraps a MovePackage and directly exposes some of its fields.
#[derive(Debug)]
pub struct Package {
    // metadata
    pub self_idx: PackageIndex,
    pub id: ObjectID, // The package id as known to Sui (DB, blockchain)
    pub version: u64,

    // version info.
    // `root_version` is the first version of the package.
    // `versions` are all versions of the package, including the root_version.
    // `versions is only loaded for the root package.
    pub root_version: Option<PackageIndex>,
    pub versions: Vec<PackageIndex>,

    pub type_origin: BTreeMap<(String, String), ObjectID>,

    // dependencies info, all dependencies and direct dependencies
    pub linkage_table: BTreeMap<PackageIndex, PackageIndex>,
    pub dependencies: BTreeSet<PackageIndex>,
    pub direct_dependencies: BTreeSet<PackageIndex>,

    // List of modules in this package as indices in the GlobalEnv.modules pool
    pub modules: Vec<ModuleIndex>,

    // original Move package
    pub package: Option<MovePackage>,
}

impl Package {
    pub fn struct_count(&self, env: &GlobalEnv) -> usize {
        self.modules
            .iter()
            .map(|idx| env.modules[*idx].structs.len())
            .sum()
    }

    pub fn function_count(&self, env: &GlobalEnv) -> usize {
        self.modules
            .iter()
            .map(|idx| env.modules[*idx].functions.len())
            .sum()
    }
}

/// A Move module
#[derive(Debug)]
pub struct Module {
    // metadata
    pub self_idx: ModuleIndex,
    pub package: PackageIndex,
    pub name: IdentifierIndex,
    pub module_id: ModuleId,
    pub dependencies: BTreeSet<ObjectID>,

    // list of types, functions and constants in this module
    pub structs: Vec<StructIndex>,
    pub functions: Vec<FunctionIndex>,
    pub constants: Vec<Constant>,

    // original Move module
    pub module: Option<CompiledModule>,
}

/// A Move struct
#[derive(Debug)]
pub struct Struct {
    // metadata
    pub self_idx: StructIndex,
    pub package: PackageIndex,
    pub module: ModuleIndex,
    pub name: IdentifierIndex,
    // ability/generic
    pub abilities: AbilitySet,
    pub type_parameters: Vec<StructTypeParameter>,

    // list of fields
    pub fields: Vec<Field>,

    // original Move struct
    pub def_idx: StructDefinitionIndex,
}

// A constant
#[derive(Debug)]
pub struct Constant {
    pub type_: Type,
    // refer to the value in the `CompiledModule`
    pub constant: ConstantPoolIndex,
}

// A field, embedded in the struct it belongs to
#[derive(Debug)]
pub struct Field {
    pub name: IdentifierIndex,
    pub type_: Type,
}

//. Field reference used in bytecodes
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct FieldRef {
    pub struct_idx: StructIndex,
    pub field_idx: MemberCount,
}

/// A type in the Move type system. All types are made unique by interning them in the GlobalEnv.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Type {
    /// Boolean, `true` or `false`.
    Bool,
    /// Unsigned integers, 8 bits length.
    U8,
    /// Unsigned integers, 16 bits length.
    U16,
    /// Unsigned integers, 32 bits length.
    U32,
    /// Unsigned integers, 64 bits length.
    U64,
    /// Unsigned integers, 128 bits length.
    U128,
    /// Unsigned integers, 256 bits length.
    U256,
    /// Address, a 16 bytes immutable type.
    Address,
    /// Vector
    Vector(Box<Type>),
    /// User defined type
    Struct(StructIndex),
    StructInstantiation(Box<(StructIndex, Vec<Type>)>),
    /// Reference to a type.
    Reference(Box<Type>),
    /// Mutable reference to a type.
    MutableReference(Box<Type>),
    /// Type parameter.
    TypeParameter(TypeParameterIndex),
}

/// A Move function
#[derive(Debug)]
pub struct Function {
    pub self_idx: FunctionIndex,
    pub package: PackageIndex,
    pub module: ModuleIndex,
    pub name: IdentifierIndex,
    pub def_idx: FunctionDefinitionIndex,
    pub type_parameters: Vec<AbilitySet>,
    pub parameters: Vec<Type>,
    pub returns: Vec<Type>,
    pub visibility: Visibility,
    pub is_entry: bool,
    pub code: Option<Code>,
}

/// A Move function body
#[derive(Debug)]
pub struct Code {
    pub locals: Vec<Type>,
    pub code: Vec<Bytecode>,
}

/// Bytecode normalized to the global environment.
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum Bytecode {
    Nop,
    Pop,
    Ret,
    BrTrue(CodeOffset),
    BrFalse(CodeOffset),
    Branch(CodeOffset),
    LdConst(ConstantPoolIndex),
    LdTrue,
    LdFalse,
    LdU8(u8),
    LdU16(u16),
    LdU32(u32),
    LdU64(u64),
    LdU128(Box<u128>),
    LdU256(Box<U256>),
    CastU8,
    CastU16,
    CastU32,
    CastU64,
    CastU128,
    CastU256,
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
    Shl,
    Shr,
    Abort,
    CopyLoc(LocalIndex),
    MoveLoc(LocalIndex),
    StLoc(LocalIndex),
    Call(FunctionIndex),
    CallGeneric(FunctionIndex, Vec<Type>),
    Pack(StructIndex),
    PackGeneric(StructIndex, Vec<Type>),
    Unpack(StructIndex),
    UnpackGeneric(StructIndex, Vec<Type>),
    MutBorrowLoc(LocalIndex),
    ImmBorrowLoc(LocalIndex),
    MutBorrowField(FieldRef),
    MutBorrowFieldGeneric(FieldRef, Vec<Type>),
    ImmBorrowField(FieldRef),
    ImmBorrowFieldGeneric(FieldRef, Vec<Type>),
    ReadRef,
    WriteRef,
    FreezeRef,
    VecPack(Type, u64),
    VecLen(Type),
    VecImmBorrow(Type),
    VecMutBorrow(Type),
    VecPushBack(Type),
    VecPopBack(Type),
    VecUnpack(Type, u64),
    VecSwap(Type),
}
