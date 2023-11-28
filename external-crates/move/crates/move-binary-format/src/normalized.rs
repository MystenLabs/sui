// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    access::ModuleAccess,
    file_format::{
        AbilitySet, Bytecode as FBytecode, CodeOffset, CompiledModule, DatatypeTyParameter,
        EnumDefInstantiation, EnumDefInstantiationIndex, EnumDefinition, FieldDefinition,
        FieldHandle, FieldHandleIndex, FieldInstantiation, FieldInstantiationIndex,
        FunctionDefinition, FunctionHandle, FunctionHandleIndex, FunctionInstantiation, LocalIndex,
        SignatureIndex, SignatureToken, StructDefInstantiation, StructDefInstantiationIndex,
        StructDefinition, StructDefinitionIndex, StructFieldInformation, TypeParameterIndex,
        VariantDefinition, VariantJumpTable as FFVariantJumpTable, VariantTag, Visibility,
    },
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, StructTag, TypeTag},
};
use move_proc_macros::test_variant_order;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Defines normalized representations of Move types, fields, kinds, structs, functions, and
/// modules. These representations are useful in situations that require require comparing
/// functions, resources, and types across modules. This arises in linking, compatibility checks
/// (e.g., "is it safe to deploy this new module without updating its dependents and/or restarting
/// genesis?"), defining schemas for resources stored on-chain, and (possibly in the future)
/// allowing module updates transactions.

/// A normalized version of `SignatureToken`, a type expression appearing in struct or function
/// declarations. Unlike `SignatureToken`s, `normalized::Type`s from different modules can safely be
/// compared.
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
#[test_variant_order(src/unit_tests/staged_enum_variant_order/type.yaml)]
pub enum Type {
    #[serde(rename = "bool")]
    Bool,
    #[serde(rename = "u8")]
    U8,
    #[serde(rename = "u64")]
    U64,
    #[serde(rename = "u128")]
    U128,
    #[serde(rename = "address")]
    Address,
    #[serde(rename = "signer")]
    Signer,
    Struct {
        address: AccountAddress,
        module: Identifier,
        name: Identifier,
        type_arguments: Vec<Type>,
    },
    #[serde(rename = "vector")]
    Vector(Box<Type>),
    TypeParameter(TypeParameterIndex),
    Reference(Box<Type>),
    MutableReference(Box<Type>),
    // NOTE: Added in bytecode version v6, do not reorder!
    #[serde(rename = "u16")]
    U16,
    #[serde(rename = "u32")]
    U32,
    #[serde(rename = "u256")]
    U256,
}

/// Normalized version of a `FieldDefinition`. The `name` is included even though it is
/// metadata that it is ignored by the VM. The reason: names are important to clients. We would
/// want a change from `Account { bal: u64, seq: u64 }` to `Account { seq: u64, bal: u64 }` to be
/// marked as incompatible. Not safe to compare without an enclosing `Struct`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Field {
    pub name: Identifier,
    pub type_: Type,
}

/// Normalized version of a `Constant`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Constant {
    pub type_: Type,
    pub data: Vec<u8>,
}

/// Normalized version of a `StructDefinition`. Not safe to compare without an associated
/// `ModuleId` or `Module`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Struct {
    pub abilities: AbilitySet,
    pub type_parameters: Vec<DatatypeTyParameter>,
    pub fields: Vec<Field>,
}

/// Normalized version of a `FunctionDefinition`. Not safe to compare without an associated
/// `ModuleId` or `Module`.
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct Function {
    pub visibility: Visibility,
    pub is_entry: bool,
    pub type_parameters: Vec<AbilitySet>,
    pub parameters: Vec<Type>,
    pub return_: Vec<Type>,
    pub code: Vec<Bytecode>,
}

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct FieldRef {
    pub struct_name: Identifier,
    pub field_index: u16,
}

/// Normalized version of a `EnumDefinition`. Not safe to compare without an associated
/// `ModuleId` or `Module`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Enum {
    pub abilities: AbilitySet,
    pub type_parameters: Vec<DatatypeTyParameter>,
    pub variants: Vec<Variant>,
}

/// Normalized version of a `VariantDefinition`. Not safe to compare without an associated
/// `ModuleId` or `Module`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Variant {
    pub name: Identifier,
    pub fields: Vec<Field>,
}

/// Normalized version of a `VariantJumpTable`. Not safe to compare without an associated
/// `ModuleId` or `Module`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct VariantJumpTable {
    pub enum_name: Identifier,
    pub jump_table: Vec<CodeOffset>,
}

// Functions can reference external modules. We don't track the exact type parameters and the like
// since we know they can't change, or don't matter since:
// * Either we allow compatible upgrades in which case the changing of the call parameters/types
//   doesn't matter since this will align with the callee signature, and that callee must go through
//   the compatibility checker for any upgrades.
// * We are in an inclusion scenario. In which case either:
//   - The callee is in the same package as this call, in which case the callee couldn't have changed; or
//   - The callee was in a different package and therefore public, and therefore the API of that
//   function must not have changed by compatibility rules.
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct FunctionRef {
    pub module_id: ModuleId,
    pub function_ident: Identifier,
}

/// Normalized representation of bytecode.
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum Bytecode {
    Pop,
    Ret,
    BrTrue(CodeOffset),
    BrFalse(CodeOffset),
    Branch(CodeOffset),
    LdU8(u8),
    LdU64(u64),
    LdU128(u128),
    CastU8,
    CastU64,
    CastU128,
    LdConst(Constant),
    LdTrue,
    LdFalse,
    CopyLoc(LocalIndex),
    MoveLoc(LocalIndex),
    StLoc(LocalIndex),
    Call(FunctionRef),
    CallGeneric((FunctionRef, Vec<Type>)),
    Pack(Identifier),
    PackGeneric((Identifier, Vec<Type>)),
    Unpack(Identifier),
    UnpackGeneric((Identifier, Vec<Type>)),
    ReadRef,
    WriteRef,
    FreezeRef,
    MutBorrowLoc(LocalIndex),
    ImmBorrowLoc(LocalIndex),
    MutBorrowField(FieldRef),
    MutBorrowFieldGeneric((FieldRef, Vec<Type>)),
    ImmBorrowField(FieldRef),
    ImmBorrowFieldGeneric((FieldRef, Vec<Type>)),
    MutBorrowGlobal(Identifier),
    MutBorrowGlobalGeneric((Identifier, Vec<Type>)),
    ImmBorrowGlobal(Identifier),
    ImmBorrowGlobalGeneric((Identifier, Vec<Type>)),
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
    Exists(Identifier),
    ExistsGeneric((Identifier, Vec<Type>)),
    MoveFrom(Identifier),
    MoveFromGeneric((Identifier, Vec<Type>)),
    MoveTo(Identifier),
    MoveToGeneric((Identifier, Vec<Type>)),
    Shl,
    Shr,
    VecPack(Type, u64),
    VecLen(Type),
    VecImmBorrow(Type),
    VecMutBorrow(Type),
    VecPushBack(Type),
    VecPopBack(Type),
    VecUnpack(Type, u64),
    VecSwap(Type),
    LdU16(u16),
    LdU32(u32),
    LdU256(move_core_types::u256::U256),
    CastU16,
    CastU32,
    CastU256,
    PackVariant(Identifier, VariantTag),
    PackVariantGeneric((Identifier, Vec<Type>), VariantTag),
    UnpackVariant(Identifier, VariantTag),
    UnpackVariantImmRef(Identifier, VariantTag),
    UnpackVariantMutRef(Identifier, VariantTag),
    UnpackVariantGeneric((Identifier, Vec<Type>), VariantTag),
    UnpackVariantGenericImmRef((Identifier, Vec<Type>), VariantTag),
    UnpackVariantGenericMutRef((Identifier, Vec<Type>), VariantTag),
    VariantSwitch(VariantJumpTable),
}

impl Constant {
    pub fn new(m: &CompiledModule, constant: &crate::file_format::Constant) -> Self {
        Self {
            type_: Type::new(m, &constant.type_),
            data: constant.data.clone(),
        }
    }
}

/// Normalized version of a `CompiledModule`: its address, name, struct declarations, and public
/// function declarations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Module {
    pub file_format_version: u32,
    pub address: AccountAddress,
    pub name: Identifier,
    pub dependencies: Vec<ModuleId>,
    pub friends: Vec<ModuleId>,
    pub structs: BTreeMap<Identifier, Struct>,
    pub enums: BTreeMap<Identifier, Enum>,
    pub functions: BTreeMap<Identifier, Function>,
    pub constants: Vec<Constant>,
}

impl Module {
    /// Extract a normalized module from a `CompiledModule`. The module `m` should be verified.
    /// Nothing will break here if that is not the case, but there is little point in computing a
    /// normalized representation of a module that won't verify (since it can't be published).
    pub fn new(m: &CompiledModule) -> Self {
        let friends = m.immediate_friends();
        let structs = m.struct_defs().iter().map(|d| Struct::new(m, d)).collect();
        let enums = m.enum_defs().iter().map(|d| Enum::new(m, d)).collect();
        let dependencies = m.immediate_dependencies();
        let constants = m
            .constant_pool()
            .iter()
            .map(|constant| Constant::new(m, constant))
            .collect();
        let functions = m
            .function_defs()
            .iter()
            .map(|func_def| Function::new(m, func_def))
            .collect();
        Self {
            file_format_version: m.version(),
            address: *m.address(),
            name: m.name().to_owned(),
            friends,
            structs,
            enums,
            functions,
            dependencies,
            constants,
        }
    }

    pub fn module_id(&self) -> ModuleId {
        ModuleId::new(self.address, self.name.clone())
    }
}

impl Type {
    /// Create a normalized `Type` for `SignatureToken` `s` in module `m`.
    pub fn new(m: &CompiledModule, s: &SignatureToken) -> Self {
        use SignatureToken::*;
        match s {
            Datatype(shi) => {
                let s_handle = m.datatype_handle_at(*shi);
                assert!(s_handle.type_parameters.is_empty(), "A struct with N type parameters should be encoded as StructModuleInstantiation with type_arguments = [TypeParameter(1), ..., TypeParameter(N)]");
                let m_handle = m.module_handle_at(s_handle.module);
                Type::Struct {
                    address: *m.address_identifier_at(m_handle.address),
                    module: m.identifier_at(m_handle.name).to_owned(),
                    name: m.identifier_at(s_handle.name).to_owned(),
                    type_arguments: Vec::new(),
                }
            }
            DatatypeInstantiation(shi, type_actuals) => {
                let s_handle = m.datatype_handle_at(*shi);
                let m_handle = m.module_handle_at(s_handle.module);
                Type::Struct {
                    address: *m.address_identifier_at(m_handle.address),
                    module: m.identifier_at(m_handle.name).to_owned(),
                    name: m.identifier_at(s_handle.name).to_owned(),
                    type_arguments: type_actuals.iter().map(|t| Type::new(m, t)).collect(),
                }
            }
            Bool => Type::Bool,
            U8 => Type::U8,
            U16 => Type::U16,
            U32 => Type::U32,
            U64 => Type::U64,
            U128 => Type::U128,
            U256 => Type::U256,
            Address => Type::Address,
            Signer => Type::Signer,
            Vector(t) => Type::Vector(Box::new(Type::new(m, t))),
            TypeParameter(i) => Type::TypeParameter(*i),
            Reference(t) => Type::Reference(Box::new(Type::new(m, t))),
            MutableReference(t) => Type::MutableReference(Box::new(Type::new(m, t))),
        }
    }

    /// Return true if `self` is a closed type with no free type variables
    pub fn is_closed(&self) -> bool {
        use Type::*;
        match self {
            TypeParameter(_) => false,
            Bool => true,
            U8 => true,
            U16 => true,
            U32 => true,
            U64 => true,
            U128 => true,
            U256 => true,
            Address => true,
            Signer => true,
            Struct { type_arguments, .. } => type_arguments.iter().all(|t| t.is_closed()),
            Vector(t) | Reference(t) | MutableReference(t) => t.is_closed(),
        }
    }

    pub fn into_type_tag(self) -> Option<TypeTag> {
        use Type::*;
        Some(if self.is_closed() {
            match self {
                Reference(_) | MutableReference(_) => return None,
                Bool => TypeTag::Bool,
                U8 => TypeTag::U8,
                U16 => TypeTag::U16,
                U32 => TypeTag::U32,
                U64 => TypeTag::U64,
                U128 => TypeTag::U128,
                U256 => TypeTag::U256,
                Address => TypeTag::Address,
                Signer => TypeTag::Signer,
                Vector(t) => TypeTag::Vector(Box::new(
                    t.into_type_tag()
                        .expect("Invariant violation: vector type argument contains reference"),
                )),
                Struct {
                    address,
                    module,
                    name,
                    type_arguments,
                } => TypeTag::Struct(Box::new(StructTag {
                    address,
                    module,
                    name,
                    type_params: type_arguments
                        .into_iter()
                        .map(|t| {
                            t.into_type_tag().expect(
                                "Invariant violation: struct type argument contains reference",
                            )
                        })
                        .collect(),
                })),
                TypeParameter(_) => unreachable!(),
            }
        } else {
            return None;
        })
    }

    pub fn into_struct_tag(self) -> Option<StructTag> {
        match self.into_type_tag()? {
            TypeTag::Struct(s) => Some(*s),
            _ => None,
        }
    }

    pub fn subst(&self, type_args: &[Type]) -> Self {
        use Type::*;
        match self {
            Bool | U8 | U16 | U32 | U64 | U128 | U256 | Address | Signer => self.clone(),
            Reference(ty) => Reference(Box::new(ty.subst(type_args))),
            MutableReference(ty) => MutableReference(Box::new(ty.subst(type_args))),
            Vector(t) => Vector(Box::new(t.subst(type_args))),
            Struct {
                address,
                module,
                name,
                type_arguments,
            } => Struct {
                address: *address,
                module: module.clone(),
                name: name.clone(),
                type_arguments: type_arguments
                    .iter()
                    .map(|t| t.subst(type_args))
                    .collect::<Vec<_>>(),
            },
            TypeParameter(i) => type_args
                .get(*i as usize)
                .expect("Type parameter index out of bound")
                .clone(),
        }
    }
}

impl Field {
    /// Create a `Field` for `FieldDefinition` `f` in module `m`.
    pub fn new(m: &CompiledModule, f: &FieldDefinition) -> Self {
        Field {
            name: m.identifier_at(f.name).to_owned(),
            type_: Type::new(m, &f.signature.0),
        }
    }
}

impl Struct {
    /// Create a `Struct` for `StructDefinition` `def` in module `m`. Panics if `def` is a
    /// a native struct definition.
    pub fn new(m: &CompiledModule, def: &StructDefinition) -> (Identifier, Self) {
        let handle = m.datatype_handle_at(def.struct_handle);
        let fields = match &def.field_information {
            StructFieldInformation::Native => {
                // Pretend for compatibility checking no fields
                vec![]
            }
            StructFieldInformation::Declared(fields) => {
                fields.iter().map(|f| Field::new(m, f)).collect()
            }
        };
        let name = m.identifier_at(handle.name).to_owned();
        let s = Struct {
            abilities: handle.abilities,
            type_parameters: handle.type_parameters.clone(),
            fields,
        };
        (name, s)
    }

    pub fn from_idx(m: &CompiledModule, idx: &StructDefinitionIndex) -> (Identifier, Self) {
        Self::new(m, m.struct_def_at(*idx))
    }

    pub fn type_param_constraints(&self) -> impl ExactSizeIterator<Item = &AbilitySet> {
        self.type_parameters.iter().map(|param| &param.constraints)
    }
}

impl Function {
    /// Create a `FunctionSignature` for `FunctionHandle` `f` in module `m`.
    pub fn new(m: &CompiledModule, def: &FunctionDefinition) -> (Identifier, Self) {
        let fhandle = m.function_handle_at(def.function);
        let name = m.identifier_at(fhandle.name).to_owned();
        let code: Vec<_> = def
            .code
            .as_ref()
            .map(|code| {
                code.code
                    .iter()
                    .map(|bytecode| Bytecode::new(m, bytecode, &code.jump_tables))
                    .collect()
            })
            .unwrap_or_default();
        let f = Function {
            visibility: def.visibility,
            is_entry: def.is_entry,
            type_parameters: fhandle.type_parameters.clone(),
            parameters: m
                .signature_at(fhandle.parameters)
                .0
                .iter()
                .map(|s| Type::new(m, s))
                .collect(),
            return_: m
                .signature_at(fhandle.return_)
                .0
                .iter()
                .map(|s| Type::new(m, s))
                .collect(),
            code,
        };
        (name, f)
    }

    /// Create a `Function` for function named `func_name` in module `m`.
    pub fn new_from_name(m: &CompiledModule, func_name: &IdentStr) -> Option<Self> {
        for func_defs in &m.function_defs {
            if m.identifier_at(m.function_handle_at(func_defs.function).name) == func_name {
                return Some(Self::new(m, func_defs).1);
            }
        }
        None
    }
}

impl From<TypeTag> for Type {
    fn from(ty: TypeTag) -> Type {
        use Type::*;
        match ty {
            TypeTag::Bool => Bool,
            TypeTag::U8 => U8,
            TypeTag::U16 => U16,
            TypeTag::U32 => U32,
            TypeTag::U64 => U64,
            TypeTag::U128 => U128,
            TypeTag::U256 => U256,
            TypeTag::Address => Address,
            TypeTag::Signer => Signer,
            TypeTag::Vector(ty) => Vector(Box::new(Type::from(*ty))),
            TypeTag::Struct(s) => Struct {
                address: s.address,
                module: s.module,
                name: s.name,
                type_arguments: s.type_params.into_iter().map(|ty| ty.into()).collect(),
            },
        }
    }
}

impl FieldRef {
    pub fn new(m: &CompiledModule, field_handle: &FieldHandle) -> Self {
        Self {
            struct_name: m.struct_name(field_handle.owner).to_owned(),
            field_index: field_handle.field,
        }
    }

    pub fn from_idx(m: &CompiledModule, field_handle_idx: &FieldHandleIndex) -> Self {
        Self::new(m, m.field_handle_at(*field_handle_idx))
    }
}

impl FunctionRef {
    pub fn new(m: &CompiledModule, function_handle: &FunctionHandle) -> Self {
        Self {
            module_id: m.module_id_for_handle(m.module_handle_at(function_handle.module)),
            function_ident: m.identifier_at(function_handle.name).to_owned(),
        }
    }

    pub fn from_idx(m: &CompiledModule, function_handle_idx: &FunctionHandleIndex) -> Self {
        Self::new(m, m.function_handle_at(*function_handle_idx))
    }
}

impl Bytecode {
    pub fn new(
        m: &CompiledModule,
        bytecode: &FBytecode,
        jump_tables: &[FFVariantJumpTable],
    ) -> Self {
        use Bytecode as B;
        use FBytecode as FB;
        match bytecode {
            FB::Pop => B::Pop,
            FB::Ret => B::Ret,
            FB::CastU8 => B::CastU8,
            FB::CastU64 => B::CastU64,
            FB::CastU128 => B::CastU128,
            FB::LdTrue => B::LdTrue,
            FB::LdFalse => B::LdFalse,
            FB::ReadRef => B::ReadRef,
            FB::WriteRef => B::WriteRef,
            FB::FreezeRef => B::FreezeRef,
            FB::Add => B::Add,
            FB::Sub => B::Sub,
            FB::Mul => B::Mul,
            FB::Mod => B::Mod,
            FB::Div => B::Div,
            FB::BitOr => B::BitOr,
            FB::BitAnd => B::BitAnd,
            FB::Xor => B::Xor,
            FB::Or => B::Or,
            FB::And => B::And,
            FB::Not => B::Not,
            FB::Eq => B::Eq,
            FB::Neq => B::Neq,
            FB::Lt => B::Lt,
            FB::Gt => B::Gt,
            FB::Le => B::Le,
            FB::Ge => B::Ge,
            FB::Abort => B::Abort,
            FB::Nop => B::Nop,
            FB::Shl => B::Shl,
            FB::Shr => B::Shr,
            FB::CastU16 => B::CastU16,
            FB::CastU32 => B::CastU32,
            FB::CastU256 => B::CastU256,
            FB::BrTrue(x) => B::BrTrue(*x),
            FB::BrFalse(x) => B::BrFalse(*x),
            FB::Branch(x) => B::Branch(*x),
            FB::LdU8(x) => B::LdU8(*x),
            FB::LdU64(x) => B::LdU64(*x),
            FB::LdU128(x) => B::LdU128(*x),
            FB::CopyLoc(x) => B::CopyLoc(*x),
            FB::MoveLoc(x) => B::MoveLoc(*x),
            FB::StLoc(x) => B::StLoc(*x),
            FB::LdU16(x) => B::LdU16(*x),
            FB::LdU32(x) => B::LdU32(*x),
            FB::LdU256(x) => B::LdU256(*x),
            FB::LdConst(const_idx) => B::LdConst(Constant::new(m, m.constant_at(*const_idx))),
            FB::Call(fh_idx) => B::Call(FunctionRef::from_idx(m, fh_idx)),
            FB::CallGeneric(fhi_idx) => {
                let FunctionInstantiation {
                    handle,
                    type_parameters,
                } = m.function_instantiation_at(*fhi_idx);
                let type_params = m.signature_at(*type_parameters);
                B::CallGeneric((
                    FunctionRef::from_idx(m, handle),
                    type_params.0.iter().map(|tok| Type::new(m, tok)).collect(),
                ))
            }
            FB::Pack(s_idx) => B::Pack(m.struct_name(*s_idx).to_owned()),
            FB::PackGeneric(s_idx) => B::PackGeneric(struct_instantiation(m, s_idx)),
            FB::Unpack(s_idx) => B::Unpack(m.struct_name(*s_idx).to_owned()),
            FB::UnpackGeneric(si_idx) => B::UnpackGeneric(struct_instantiation(m, si_idx)),
            FB::MutBorrowLoc(x) => B::MutBorrowLoc(*x),
            FB::ImmBorrowLoc(x) => B::ImmBorrowLoc(*x),
            FB::MutBorrowField(fh_ixd) => B::MutBorrowField(FieldRef::from_idx(m, fh_ixd)),
            FB::MutBorrowFieldGeneric(fhi_idx) => {
                B::MutBorrowFieldGeneric(field_instantiation(m, fhi_idx))
            }
            FB::ImmBorrowField(fh_idx) => B::ImmBorrowField(FieldRef::from_idx(m, fh_idx)),
            FB::ImmBorrowFieldGeneric(fhi_idx) => {
                B::ImmBorrowFieldGeneric(field_instantiation(m, fhi_idx))
            }
            FB::MutBorrowGlobal(s_idx) => B::MutBorrowGlobal(m.struct_name(*s_idx).to_owned()),
            FB::MutBorrowGlobalGeneric(si_idx) => {
                B::MutBorrowGlobalGeneric(struct_instantiation(m, si_idx))
            }
            FB::ImmBorrowGlobal(s_idx) => B::ImmBorrowGlobal(m.struct_name(*s_idx).to_owned()),
            FB::ImmBorrowGlobalGeneric(si_idx) => {
                B::ImmBorrowGlobalGeneric(struct_instantiation(m, si_idx))
            }
            FB::Exists(s_idx) => B::Exists(m.struct_name(*s_idx).to_owned()),
            FB::ExistsGeneric(si_idx) => B::ExistsGeneric(struct_instantiation(m, si_idx)),
            FB::MoveFrom(s_idx) => B::MoveFrom(m.struct_name(*s_idx).to_owned()),
            FB::MoveFromGeneric(si_idx) => B::MoveFromGeneric(struct_instantiation(m, si_idx)),
            FB::MoveTo(s_idx) => B::MoveTo(m.struct_name(*s_idx).to_owned()),
            FB::MoveToGeneric(si_idx) => B::MoveToGeneric(struct_instantiation(m, si_idx)),
            FB::VecPack(sig_idx, len) => B::VecPack(signature_to_single_type(m, sig_idx), *len),
            FB::VecLen(sig_idx) => B::VecLen(signature_to_single_type(m, sig_idx)),
            FB::VecImmBorrow(sig_idx) => B::VecImmBorrow(signature_to_single_type(m, sig_idx)),
            FB::VecMutBorrow(sig_idx) => B::VecMutBorrow(signature_to_single_type(m, sig_idx)),
            FB::VecPushBack(sig_idx) => B::VecPushBack(signature_to_single_type(m, sig_idx)),
            FB::VecPopBack(sig_idx) => B::VecPopBack(signature_to_single_type(m, sig_idx)),
            FB::VecUnpack(sig_idx, len) => B::VecUnpack(signature_to_single_type(m, sig_idx), *len),
            FB::VecSwap(sig_idx) => B::VecSwap(signature_to_single_type(m, sig_idx)),
            FB::PackVariant(eidx, tag) => B::PackVariant(m.enum_name(*eidx).to_owned(), *tag),
            FB::PackVariantGeneric(edii, tag) => {
                B::PackVariantGeneric(enum_instantiation(m, edii), *tag)
            }
            FB::UnpackVariant(eidx, tag) => B::UnpackVariant(m.enum_name(*eidx).to_owned(), *tag),
            FB::UnpackVariantGeneric(edii, tag) => {
                B::UnpackVariantGeneric(enum_instantiation(m, edii), *tag)
            }
            FB::UnpackVariantImmRef(eidx, tag) => {
                B::UnpackVariantImmRef(m.enum_name(*eidx).to_owned(), *tag)
            }
            FB::UnpackVariantGenericImmRef(edii, tag) => {
                B::UnpackVariantGenericImmRef(enum_instantiation(m, edii), *tag)
            }
            FB::UnpackVariantMutRef(eidx, tag) => {
                B::UnpackVariantMutRef(m.enum_name(*eidx).to_owned(), *tag)
            }
            FB::UnpackVariantGenericMutRef(edii, tag) => {
                B::UnpackVariantGenericMutRef(enum_instantiation(m, edii), *tag)
            }
            FB::VariantSwitch(jti) => B::VariantSwitch(VariantJumpTable::new(
                m,
                jump_tables
                    .get(jti.0 as usize)
                    .expect("Invariant violation: invalid jump table index"),
            )),
        }
    }
}

impl VariantJumpTable {
    pub fn new(m: &CompiledModule, jt: &FFVariantJumpTable) -> Self {
        let e_def = m.enum_def_at(jt.head_enum);
        let e_handle = m.datatype_handle_at(e_def.enum_handle);
        let enum_name = m.identifier_at(e_handle.name).to_owned();
        Self {
            enum_name,
            jump_table: jt.jump_table.clone(),
        }
    }
}

impl Enum {
    pub fn new(m: &CompiledModule, def: &EnumDefinition) -> (Identifier, Self) {
        let handle = m.datatype_handle_at(def.enum_handle);
        let name = m.identifier_at(handle.name).to_owned();
        let variants = def
            .variants
            .iter()
            .map(|v| Variant::new(m, v))
            .collect::<Vec<_>>();
        let e = Enum {
            abilities: handle.abilities,
            type_parameters: handle.type_parameters.clone(),
            variants,
        };
        (name, e)
    }
}

impl Variant {
    pub fn new(m: &CompiledModule, v: &VariantDefinition) -> Self {
        Self {
            name: m.identifier_at(v.variant_name).to_owned(),
            fields: v
                .fields
                .iter()
                .map(|f| Field::new(m, f))
                .collect::<Vec<_>>(),
        }
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Type::Struct {
                address,
                module,
                name,
                type_arguments,
            } => {
                write!(
                    f,
                    "0x{}::{}::{}",
                    address.short_str_lossless(),
                    module,
                    name
                )?;
                if let Some(first_ty) = type_arguments.first() {
                    write!(f, "<")?;
                    write!(f, "{}", first_ty)?;
                    for ty in type_arguments.iter().skip(1) {
                        write!(f, ", {}", ty)?;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
            Type::Vector(ty) => write!(f, "vector<{}>", ty),
            Type::U8 => write!(f, "u8"),
            Type::U16 => write!(f, "u16"),
            Type::U32 => write!(f, "u32"),
            Type::U64 => write!(f, "u64"),
            Type::U128 => write!(f, "u128"),
            Type::U256 => write!(f, "u256"),
            Type::Address => write!(f, "address"),
            Type::Signer => write!(f, "signer"),
            Type::Bool => write!(f, "bool"),
            Type::Reference(r) => write!(f, "&{}", r),
            Type::MutableReference(r) => write!(f, "&mut {}", r),
            Type::TypeParameter(i) => write!(f, "T{:?}", i),
        }
    }
}

fn struct_instantiation(
    m: &CompiledModule,
    si_idx: &StructDefInstantiationIndex,
) -> (Identifier, Vec<Type>) {
    let StructDefInstantiation {
        def,
        type_parameters,
    } = m.struct_instantiation_at(*si_idx);
    let (name, _) = Struct::new(m, m.struct_def_at(*def));
    let types = m
        .signature_at(*type_parameters)
        .0
        .iter()
        .map(|tok| Type::new(m, tok))
        .collect();
    (name, types)
}

fn enum_instantiation(
    m: &CompiledModule,
    si_idx: &EnumDefInstantiationIndex,
) -> (Identifier, Vec<Type>) {
    let EnumDefInstantiation {
        def,
        type_parameters,
    } = m.enum_instantiation_at(*si_idx);
    let (name, _) = Enum::new(m, m.enum_def_at(*def));
    let types = m
        .signature_at(*type_parameters)
        .0
        .iter()
        .map(|tok| Type::new(m, tok))
        .collect();
    (name, types)
}

fn field_instantiation(m: &CompiledModule, idx: &FieldInstantiationIndex) -> (FieldRef, Vec<Type>) {
    let FieldInstantiation {
        handle,
        type_parameters,
    } = m.field_instantiation_at(*idx);
    let field_ref = FieldRef::new(m, m.field_handle_at(*handle));
    let types = m
        .signature_at(*type_parameters)
        .0
        .iter()
        .map(|tok| Type::new(m, tok))
        .collect();
    (field_ref, types)
}

fn signature_to_single_type(m: &CompiledModule, sig_idx: &SignatureIndex) -> Type {
    Type::new(m, &m.signature_at(*sig_idx).0[0])
}
