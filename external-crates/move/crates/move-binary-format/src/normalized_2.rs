// Copyright (c) The Move Contributors
// SPDX-License-S: Apache-2.0

use crate::file_format::{
    self, AbilitySet, Bytecode as FBytecode, CodeOffset, CompiledModule, DatatypeHandleIndex,
    DatatypeTyParameter, EnumDefinition, EnumDefinitionIndex, FieldDefinition, FieldHandleIndex,
    FieldInstantiationIndex, FunctionDefinition, FunctionHandleIndex, FunctionInstantiationIndex,
    JumpTableInner, LocalIndex, SignatureIndex, SignatureToken, StructDefInstantiationIndex,
    StructDefinition, StructDefinitionIndex, StructFieldInformation, TypeParameterIndex,
    VariantDefinition, VariantHandleIndex, VariantInstantiationHandleIndex, VariantTag, Visibility,
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage::{StructTag, TypeTag},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::rc::Rc;

pub trait StringPool {
    type String: Clone + Debug + Ord + PartialOrd + Eq + PartialEq;

    fn intern(&mut self, s: &IdentStr) -> Self::String;

    fn as_ident_str<'a>(&'a self, s: &'a Self::String) -> &'a IdentStr;
}

pub struct NoInterning;

impl StringPool for NoInterning {
    type String = Identifier;

    fn intern(&mut self, s: &IdentStr) -> Self::String {
        s.to_owned()
    }

    fn as_ident_str<'a>(&'a self, s: &'a Identifier) -> &'a IdentStr {
        s.as_ident_str()
    }
}

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct ModuleId<S> {
    pub address: AccountAddress,
    pub name: S,
}

// Defines normalized representations of Move types, fields, kinds, structs, functions, and
// modules. These representations are useful in situations that require require comparing
// functions, resources, and types across modules. This arises in linking, compatibility checks
// (e.g., "is it safe to deploy this new module without updating its dependents and/or restarting
// genesis?"), defining schemas for resources stored on-chain, and (possibly in the future)
// allowing module updates transactions.

/// A normalized version of `SignatureToken`, a type expression appearing in struct or function
/// declarations. Unlike `SignatureToken`s, `normalized::Type`s from different modules can safely be
/// compared.
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub enum Type<S> {
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
    Datatype(Box<Datatype<S>>),
    #[serde(rename = "vector")]
    Vector(Box<Type<S>>),
    TypeParameter(TypeParameterIndex),
    Reference(/* is_mut */ bool, Box<Type<S>>),
    // NOTE: Added in bytecode version v6, do not reorder!
    #[serde(rename = "u16")]
    U16,
    #[serde(rename = "u32")]
    U32,
    #[serde(rename = "u256")]
    U256,
}

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub struct Datatype<S> {
    address: AccountAddress,
    module: S,
    name: S,
    type_arguments: Vec<Type<S>>,
}

pub type Signature<S> = Rc<Vec<Rc<Type<S>>>>;

#[derive(Clone, Debug)]
struct Tables<S> {
    empty_signature: Signature<S>,
    signatures: Vec<Signature<S>>,
    constants: Vec<Rc<Constant<S>>>,
    struct_defs: Vec<Rc<Struct<S>>>,
    function_defs: Vec<Rc<Function<S>>>,
    enum_defs: Vec<Rc<Enum<S>>>,
}

/// Normalized version of a `CompiledModule`: its address, name, struct declarations, and public
/// function declarations.
#[derive(Clone, Debug)]
pub struct Module<S> {
    #[allow(unused)]
    tables: Tables<S>,
    pub id: ModuleId<S>,
    pub file_format_version: u32,
    pub address: AccountAddress,
    pub name: S,
    pub dependencies: Vec<ModuleId<S>>,
    pub friends: Vec<ModuleId<S>>,
    pub structs: BTreeMap<S, Rc<Struct<S>>>,
    pub enums: BTreeMap<S, Rc<Enum<S>>>,
    pub functions: BTreeMap<S, Rc<Function<S>>>,
    pub constants: Vec<Rc<Constant<S>>>,
}

/// Normalized version of a `Constant`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Constant<S> {
    pub type_: Type<S>,
    pub data: Vec<u8>,
}

/// Normalized version of a `StructDefinition`. Not safe to compare without an associated
/// `ModuleId` or `Module`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Struct<S> {
    pub name: S,
    pub abilities: AbilitySet,
    pub type_parameters: Vec<DatatypeTyParameter>,
    pub fields: Vec<Rc<Field<S>>>,
}

/// Normalized version of a `FieldDefinition`. The `name` is included even though it is
/// metadata that it is ignored by the VM. The reason: names are important to clients. We would
/// want a change from `Account { bal: u64, seq: u64 }` to `Account { seq: u64, bal: u64 }` to be
/// marked as incompatible. Not safe to compare without an enclosing `Struct`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Field<S> {
    pub name: S,
    pub type_: Type<S>,
}

/// Normalized version of a `FunctionDefinition`. Not safe to compare without an associated
/// `ModuleId` or `Module`.
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct Function<S> {
    pub name: S,
    pub visibility: Visibility,
    pub is_entry: bool,
    pub type_parameters: Vec<AbilitySet>,
    pub parameters: Signature<S>,
    pub return_: Signature<S>,
    pub jump_tables: Vec<Rc<VariantJumpTable<S>>>,
    pub code: Vec<Bytecode<S>>,
}

/// Normalized version of a `EnumDefinition`. Not safe to compare without an associated
/// `ModuleId` or `Module`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Enum<S> {
    pub name: S,
    pub abilities: AbilitySet,
    pub type_parameters: Vec<DatatypeTyParameter>,
    pub variants: Vec<Rc<Variant<S>>>,
}

/// Normalized version of a `VariantDefinition`. Not safe to compare without an associated
/// `ModuleId` or `Module`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Variant<S> {
    pub name: S,
    pub fields: Vec<Field<S>>,
}

/// Normalized version of a `VariantJumpTable`. Not safe to compare without an associated
/// `ModuleId` or `Module`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct VariantJumpTable<S> {
    pub enum_: Rc<Enum<S>>,
    pub jump_table: JumpTableInner,
}

pub type ConstantRef<S> = Rc<Constant<S>>;

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct StructRef<S> {
    pub struct_: Rc<Struct<S>>,
    pub type_arguments: Signature<S>,
}

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct FieldRef<S> {
    pub struct_: Rc<Struct<S>>,
    pub field: Rc<Field<S>>,
    /// Type arguments to the struct
    pub instantiation: Signature<S>,
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
pub struct FunctionRef<S> {
    pub module: ModuleId<S>,
    pub function: S,
    pub type_arguments: Signature<S>,
}

/// Normalized version of a `VariantRef` and `VariantInstantiationHandle`.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct VariantRef<S> {
    pub enum_: Rc<Enum<S>>,
    pub variant: Rc<Variant<S>>,
    /// The type arguments to the enum
    pub instantiation: Signature<S>,
}

pub type VariantJumpTableRef<S> = Rc<VariantJumpTable<S>>;

/// Normalized representation of bytecode.
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum Bytecode<S> {
    Pop,
    Ret,
    BrTrue(CodeOffset),
    BrFalse(CodeOffset),
    Branch(CodeOffset),
    LdU8(u8),
    LdU64(u64),
    LdU128(Box<u128>),
    CastU8,
    CastU64,
    CastU128,
    LdConst(ConstantRef<S>),
    LdTrue,
    LdFalse,
    CopyLoc(LocalIndex),
    MoveLoc(LocalIndex),
    StLoc(LocalIndex),
    Call(Box<FunctionRef<S>>),
    Pack(Box<StructRef<S>>),
    Unpack(Box<StructRef<S>>),
    ReadRef,
    WriteRef,
    FreezeRef,
    MutBorrowLoc(LocalIndex),
    ImmBorrowLoc(LocalIndex),
    MutBorrowField(Box<FieldRef<S>>),
    ImmBorrowField(Box<FieldRef<S>>),
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
    VecPack(Box<(Rc<Type<S>>, u64)>),
    VecLen(Rc<Type<S>>),
    VecImmBorrow(Rc<Type<S>>),
    VecMutBorrow(Rc<Type<S>>),
    VecPushBack(Rc<Type<S>>),
    VecPopBack(Rc<Type<S>>),
    VecUnpack(Box<(Rc<Type<S>>, u64)>),
    VecSwap(Rc<Type<S>>),
    LdU16(u16),
    LdU32(u32),
    LdU256(Box<move_core_types::u256::U256>),
    CastU16,
    CastU32,
    CastU256,
    PackVariant(Box<VariantRef<S>>),
    UnpackVariant(Box<VariantRef<S>>),
    UnpackVariantImmRef(Box<VariantRef<S>>),
    UnpackVariantMutRef(Box<VariantRef<S>>),
    VariantSwitch(VariantJumpTableRef<S>),
    // ******** DEPRECATED BYTECODES ********
    MutBorrowGlobalDeprecated(Box<StructRef<S>>),
    ImmBorrowGlobalDeprecated(Box<StructRef<S>>),
    ExistsDeprecated(Box<StructRef<S>>),
    MoveFromDeprecated(Box<StructRef<S>>),
    MoveToDeprecated(Box<StructRef<S>>),
}

impl<S> ModuleId<S> {
    pub fn new<Pool: StringPool<String = S>>(
        pool: &mut Pool,
        id: &move_core_types::language_storage::ModuleId,
    ) -> Self {
        let address = *id.address();
        let name = pool.intern(id.name());
        ModuleId { address, name }
    }

    pub fn into_core_module_id<Pool: StringPool<String = S>>(
        self,
        pool: &Pool,
    ) -> move_core_types::language_storage::ModuleId {
        move_core_types::language_storage::ModuleId::new(
            self.address,
            pool.as_ident_str(&self.name).to_owned(),
        )
    }
}

impl<S> Type<S> {
    /// Create a normalized `Type` for `SignatureToken` `s` in module `m`.
    pub fn new<Pool: StringPool<String = S>>(
        pool: &mut Pool,
        m: &CompiledModule,
        s: &SignatureToken,
    ) -> Self {
        use SignatureToken as S;
        match s {
            S::Datatype(idx) => {
                let dt = Datatype::new(pool, m, *idx, &[]);
                Type::Datatype(Box::new(dt))
            }
            S::DatatypeInstantiation(inst) => {
                let (idx, type_actuals) = &**inst;
                let dt = Datatype::new(pool, m, *idx, type_actuals);
                Type::Datatype(Box::new(dt))
            }
            S::Bool => Type::Bool,
            S::U8 => Type::U8,
            S::U16 => Type::U16,
            S::U32 => Type::U32,
            S::U64 => Type::U64,
            S::U128 => Type::U128,
            S::U256 => Type::U256,
            S::Address => Type::Address,
            S::Signer => Type::Signer,
            S::Vector(t) => Type::Vector(Box::new(Type::new(pool, m, t))),
            S::TypeParameter(i) => Type::TypeParameter(*i),
            S::Reference(t) => Type::Reference(false, Box::new(Type::new(pool, m, t))),
            S::MutableReference(t) => Type::Reference(true, Box::new(Type::new(pool, m, t))),
        }
    }

    pub fn signature<Pool: StringPool<String = S>>(
        pool: &mut Pool,
        m: &CompiledModule,
        signature: &file_format::Signature,
    ) -> Signature<S> {
        let tys = signature
            .0
            .iter()
            .map(|t| Rc::new(Type::new(pool, m, t)))
            .collect();
        Rc::new(tys)
    }

    pub fn into_type_tag<Pool: StringPool<String = S>>(self, pool: &Pool) -> Option<TypeTag> {
        use Type as T;
        if !self.is_closed() {
            return None;
        }
        Some(match self {
            T::Reference(_, _) => return None,
            T::Bool => TypeTag::Bool,
            T::U8 => TypeTag::U8,
            T::U16 => TypeTag::U16,
            T::U32 => TypeTag::U32,
            T::U64 => TypeTag::U64,
            T::U128 => TypeTag::U128,
            T::U256 => TypeTag::U256,
            T::Address => TypeTag::Address,
            T::Signer => TypeTag::Signer,
            T::Vector(t) => TypeTag::Vector(Box::new(
                t.into_type_tag(pool)
                    .expect("Invariant violation: vector type argument contains reference"),
            )),
            T::Datatype(dt) => TypeTag::Struct(Box::new(dt.into_struct_tag(pool))),
            T::TypeParameter(_) => unreachable!(),
        })
    }

    pub fn into_struct_tag<Pool: StringPool<String = S>>(self, pool: &Pool) -> Option<StructTag> {
        match self.into_type_tag(pool)? {
            TypeTag::Struct(s) => Some(*s),
            _ => None,
        }
    }

    pub fn from_type_tag<Pool: StringPool<String = S>>(pool: &mut Pool, ty: TypeTag) -> Self {
        use Type as T;
        match ty {
            TypeTag::Bool => T::Bool,
            TypeTag::U8 => T::U8,
            TypeTag::U16 => T::U16,
            TypeTag::U32 => T::U32,
            TypeTag::U64 => T::U64,
            TypeTag::U128 => T::U128,
            TypeTag::U256 => T::U256,
            TypeTag::Address => T::Address,
            TypeTag::Signer => T::Signer,
            TypeTag::Vector(ty) => T::Vector(Box::new(T::from_type_tag(pool, *ty))),
            TypeTag::Struct(s) => T::Datatype(Box::new(Datatype::from_struct_tag(pool, *s))),
        }
    }

    /// Return true if `self` is a closed type with no free type variables
    pub fn is_closed(&self) -> bool {
        use Type as T;
        match self {
            T::TypeParameter(_) => false,
            T::Bool => true,
            T::U8 => true,
            T::U16 => true,
            T::U32 => true,
            T::U64 => true,
            T::U128 => true,
            T::U256 => true,
            T::Address => true,
            T::Signer => true,
            T::Datatype(dt) => dt.is_closed(),
            T::Vector(t) | T::Reference(_, t) => t.is_closed(),
        }
    }

    pub fn subst(&self, type_args: &[Type<S>]) -> Self
    where
        S: Clone,
    {
        use Type as T;
        match self {
            T::Bool
            | T::U8
            | T::U16
            | T::U32
            | T::U64
            | T::U128
            | T::U256
            | T::Address
            | T::Signer => self.clone(),
            T::Reference(mut_, ty) => T::Reference(*mut_, Box::new(ty.subst(type_args))),
            T::Vector(t) => T::Vector(Box::new(t.subst(type_args))),
            T::Datatype(dt) => T::Datatype(Box::new(dt.subst(type_args))),
            T::TypeParameter(i) => type_args
                .get(*i as usize)
                .expect("Type parameter index out of bound")
                .clone(),
        }
    }
}

impl<S> Datatype<S> {
    /// Case for `Datatype` and `DatatypeInst` when normalizing `SignatureToken`
    pub fn new<Pool: StringPool<String = S>>(
        pool: &mut Pool,
        m: &CompiledModule,
        idx: DatatypeHandleIndex,
        type_arguments: &[SignatureToken],
    ) -> Self {
        let d_handle = m.datatype_handle_at(idx);
        let m_handle = m.module_handle_at(d_handle.module);
        let name = pool.intern(m.identifier_at(d_handle.name));
        let address = *m.address_identifier_at(m_handle.address);
        let type_arguments = type_arguments
            .iter()
            .map(|t| Type::new(pool, m, t))
            .collect();
        Datatype {
            address,
            module: pool.intern(m.name()),
            name,
            type_arguments,
        }
    }

    pub fn into_struct_tag<Pool: StringPool<String = S>>(self, pool: &Pool) -> StructTag {
        let Datatype {
            address,
            module,
            name,
            type_arguments,
        } = self;
        StructTag {
            address,
            module: pool.as_ident_str(&module).to_owned(),
            name: pool.as_ident_str(&name).to_owned(),
            type_params: type_arguments
                .into_iter()
                .map(|t| {
                    t.into_type_tag(pool)
                        .expect("Invariant violation: struct type argument contains reference")
                })
                .collect(),
        }
    }

    pub fn from_struct_tag<Pool: StringPool<String = S>>(pool: &mut Pool, tag: StructTag) -> Self {
        let StructTag {
            address,
            module,
            name,
            type_params,
        } = tag;
        Datatype {
            address,
            module: pool.intern(module.as_ident_str()),
            name: pool.intern(name.as_ident_str()),
            type_arguments: type_params
                .into_iter()
                .map(|t| Type::from_type_tag(pool, t))
                .collect(),
        }
    }

    pub fn is_closed(&self) -> bool {
        self.type_arguments.iter().all(|t| t.is_closed())
    }

    pub fn subst(&self, type_args: &[Type<S>]) -> Self
    where
        S: Clone,
    {
        let Self {
            address,
            module,
            name,
            type_arguments,
        } = self;
        let type_arguments = type_arguments.iter().map(|t| t.subst(type_args)).collect();
        Self {
            address: *address,
            module: module.clone(),
            name: name.clone(),
            type_arguments,
        }
    }
}

impl<S> Tables<S> {
    fn new<Pool: StringPool<String = S>>(pool: &mut Pool, m: &CompiledModule) -> Self {
        let mut tables = Tables {
            empty_signature: Rc::new(vec![]),
            signatures: Vec::new(),
            constants: Vec::new(),
            struct_defs: Vec::new(),
            function_defs: Vec::new(),
            enum_defs: Vec::new(),
        };
        tables.signatures = m
            .signatures
            .iter()
            .map(|s| Type::signature(pool, m, s))
            .collect();
        tables.constants = m
            .constant_pool
            .iter()
            .map(|c| Rc::new(Constant::new(pool, m, c)))
            .collect();
        tables.struct_defs = m
            .struct_defs
            .iter()
            .map(|s| Rc::new(Struct::new(pool, m, s)))
            .collect();
        tables.enum_defs = m
            .enum_defs
            .iter()
            .map(|e| Rc::new(Enum::new(pool, m, e)))
            .collect();
        tables.function_defs = m
            .function_defs
            .iter()
            .map(|f| Rc::new(Function::new(&tables, pool, m, f)))
            .collect();
        tables
    }
}

impl<S: Clone + Ord> Module<S> {
    /// Extract a normalized module from a `CompiledModule`. The module `m` should be verified.
    /// Nothing will break here if that is not the case, but there is little point in computing a
    /// normalized representation of a module that won't verify (since it can't be published).
    pub fn new<Pool: StringPool<String = S>>(pool: &mut Pool, m: &CompiledModule) -> Self {
        let tables = Tables::new(pool, m);
        let id = ModuleId::new(pool, &m.self_id());
        let friends = m
            .immediate_friends()
            .into_iter()
            .map(|f| ModuleId::new(pool, &f))
            .collect();
        let dependencies = m
            .immediate_dependencies()
            .into_iter()
            .map(|d| ModuleId::new(pool, &d))
            .collect();
        let constants = (0..m.constant_pool.len())
            .map(|idx| tables.constants[idx].clone())
            .collect();
        let structs = (0..m.struct_defs.len())
            .map(|idx| {
                let def = tables.struct_defs[idx].clone();
                (def.name.clone(), def)
            })
            .collect();
        let enums = (0..m.enum_defs.len())
            .map(|idx| {
                let def = tables.enum_defs[idx].clone();
                (def.name.clone(), def)
            })
            .collect();
        let functions = (0..m.function_defs.len())
            .map(|idx| {
                let def = tables.function_defs[idx].clone();
                (def.name.clone(), def)
            })
            .collect();
        Self {
            tables,
            id,
            file_format_version: m.version(),
            address: *m.address(),
            name: pool.intern(m.name()),
            friends,
            structs,
            enums,
            functions,
            dependencies,
            constants,
        }
    }
}

impl<S> Constant<S> {
    pub fn new<Pool: StringPool<String = S>>(
        pool: &mut Pool,
        m: &CompiledModule,
        constant: &file_format::Constant,
    ) -> Self {
        Self {
            type_: Type::new(pool, m, &constant.type_),
            data: constant.data.clone(),
        }
    }
}

impl<S> Struct<S> {
    /// Create a `Struct` for `StructDefinition` `def` in module `m`. Panics if `def` is a
    /// a native struct definition.
    fn new<Pool: StringPool<String = S>>(
        pool: &mut Pool,
        m: &CompiledModule,
        def: &StructDefinition,
    ) -> Self {
        let handle = m.datatype_handle_at(def.struct_handle);
        let fields = match &def.field_information {
            StructFieldInformation::Native => {
                // Pretend for compatibility checking no fields
                vec![]
            }
            StructFieldInformation::Declared(fields) => fields
                .iter()
                .map(|f| Rc::new(Field::new(pool, m, f)))
                .collect(),
        };
        let name = pool.intern(m.identifier_at(handle.name));
        Struct {
            name,
            abilities: handle.abilities,
            type_parameters: handle.type_parameters.clone(),
            fields,
        }
    }

    pub fn type_param_constraints(&self) -> impl ExactSizeIterator<Item = &AbilitySet> {
        self.type_parameters.iter().map(|param| &param.constraints)
    }
}

impl<S> Field<S> {
    /// Create a `Field` for `FieldDefinition` `f` in module `m`.
    pub fn new<Pool: StringPool<String = S>>(
        pool: &mut Pool,
        m: &CompiledModule,
        f: &FieldDefinition,
    ) -> Self {
        Field {
            name: pool.intern(m.identifier_at(f.name)),
            type_: Type::new(pool, m, &f.signature.0),
        }
    }
}

impl<S> Function<S> {
    /// Create a `FunctionSignature` for `FunctionHandle` `f` in module `m`.
    fn new<Pool: StringPool<String = S>>(
        tables: &Tables<S>,
        pool: &mut Pool,
        m: &CompiledModule,
        def: &FunctionDefinition,
    ) -> Self {
        let fhandle = m.function_handle_at(def.function);
        let name = pool.intern(m.identifier_at(fhandle.name));
        let jump_tables = def
            .code
            .iter()
            .flat_map(|code| code.jump_tables.iter())
            .map(|jt| Rc::new(VariantJumpTable::new(tables, jt)))
            .collect::<Vec<_>>();
        let code: Vec<_> = def
            .code
            .as_ref()
            .map(|code| {
                code.code
                    .iter()
                    .map(|bytecode| Bytecode::new(tables, pool, m, bytecode, &jump_tables))
                    .collect()
            })
            .unwrap_or_default();
        Function {
            name,
            visibility: def.visibility,
            is_entry: def.is_entry,
            type_parameters: fhandle.type_parameters.clone(),
            parameters: tables.signatures[fhandle.parameters.0 as usize].clone(),
            return_: tables.signatures[fhandle.return_.0 as usize].clone(),
            jump_tables,
            code,
        }
    }
}

impl<S> Enum<S> {
    pub fn new<Pool: StringPool<String = S>>(
        pool: &mut Pool,
        m: &CompiledModule,
        def: &EnumDefinition,
    ) -> Self {
        let handle = m.datatype_handle_at(def.enum_handle);
        let name = pool.intern(m.identifier_at(handle.name));
        let variants = def
            .variants
            .iter()
            .map(|v| Rc::new(Variant::new(pool, m, v)))
            .collect::<Vec<_>>();
        let e = Enum {
            name,
            abilities: handle.abilities,
            type_parameters: handle.type_parameters.clone(),
            variants,
        };
        e
    }
}

impl<S> Variant<S> {
    pub fn new<Pool: StringPool<String = S>>(
        pool: &mut Pool,
        m: &CompiledModule,
        v: &VariantDefinition,
    ) -> Self {
        Self {
            name: pool.intern(m.identifier_at(v.variant_name)),
            fields: v.fields.iter().map(|f| Field::new(pool, m, f)).collect(),
        }
    }
}

impl<S> VariantJumpTable<S> {
    fn new(tables: &Tables<S>, jt: &file_format::VariantJumpTable) -> Self {
        let enum_ = tables.enum_defs[jt.head_enum.0 as usize].clone();
        Self {
            enum_,
            jump_table: jt.jump_table.clone(),
        }
    }
}

impl<S> StructRef<S> {
    fn new(
        tables: &Tables<S>,
        struct_handle: StructDefinitionIndex,
        type_arguments: Option<SignatureIndex>,
    ) -> Self {
        let struct_ = tables.struct_defs[struct_handle.0 as usize].clone();
        let type_arguments = type_arguments
            .map(|idx| tables.signatures[idx.0 as usize].clone())
            .unwrap_or_else(|| tables.empty_signature.clone());
        Self {
            struct_,
            type_arguments,
        }
    }

    fn instantiated(
        tables: &Tables<S>,
        m: &CompiledModule,
        idx: StructDefInstantiationIndex,
    ) -> Self {
        let struct_instantiation = m.struct_instantiation_at(idx);
        Self::new(
            tables,
            struct_instantiation.def,
            Some(struct_instantiation.type_parameters),
        )
    }
}

impl<S> FieldRef<S> {
    fn new(
        tables: &Tables<S>,
        m: &CompiledModule,
        idx: FieldHandleIndex,
        instantiation: Option<SignatureIndex>,
    ) -> Self {
        let field_handle = m.field_handle_at(idx);
        let struct_ = tables.struct_defs[field_handle.owner.0 as usize].clone();
        let field = struct_.fields[field_handle.field as usize].clone();
        let instantiation = instantiation
            .map(|idx| tables.signatures[idx.0 as usize].clone())
            .unwrap_or_else(|| tables.empty_signature.clone());
        Self {
            struct_,
            field,
            instantiation,
        }
    }

    fn instantiated(tables: &Tables<S>, m: &CompiledModule, idx: FieldInstantiationIndex) -> Self {
        let field_instantiation = m.field_instantiation_at(idx);
        Self::new(
            tables,
            m,
            field_instantiation.handle,
            Some(field_instantiation.type_parameters),
        )
    }
}

impl<S> FunctionRef<S> {
    fn new<Pool: StringPool<String = S>>(
        tables: &Tables<S>,
        pool: &mut Pool,
        m: &CompiledModule,
        idx: FunctionHandleIndex,
        type_arguments: Option<SignatureIndex>,
    ) -> Self {
        let function_handle = m.function_handle_at(idx);
        let module_handle = m.module_handle_at(function_handle.module);
        let module = ModuleId {
            address: *m.address_identifier_at(module_handle.address),
            name: pool.intern(m.identifier_at(module_handle.name)),
        };
        let function = pool.intern(m.identifier_at(function_handle.name));
        let type_arguments = type_arguments
            .map(|idx| tables.signatures[idx.0 as usize].clone())
            .unwrap_or_else(|| tables.empty_signature.clone());
        Self {
            module,
            function,
            type_arguments,
        }
    }

    fn instantiated<Pool: StringPool<String = S>>(
        tables: &Tables<S>,
        pool: &mut Pool,
        m: &CompiledModule,
        idx: FunctionInstantiationIndex,
    ) -> Self {
        let function_instantiation = m.function_instantiation_at(idx);
        Self::new(
            tables,
            pool,
            m,
            function_instantiation.handle,
            Some(function_instantiation.type_parameters),
        )
    }
}

impl<S> VariantRef<S> {
    fn new(
        tables: &Tables<S>,
        enum_def: EnumDefinitionIndex,
        variant: VariantTag,
        instantiation: Option<SignatureIndex>,
    ) -> VariantRef<S> {
        let enum_ = tables.enum_defs[enum_def.0 as usize].clone();
        let variant = enum_.variants[variant as usize].clone();
        let instantiation = instantiation
            .map(|idx| tables.signatures[idx.0 as usize].clone())
            .unwrap_or_else(|| tables.empty_signature.clone());
        VariantRef {
            enum_,
            variant,
            instantiation,
        }
    }

    fn noninstantiated(
        tables: &Tables<S>,
        m: &CompiledModule,
        idx: VariantHandleIndex,
    ) -> VariantRef<S> {
        let variant_handle = m.variant_handle_at(idx);
        VariantRef::new(
            tables,
            variant_handle.enum_def,
            variant_handle.variant,
            None,
        )
    }

    fn instantiated(
        tables: &Tables<S>,
        m: &CompiledModule,
        idx: VariantInstantiationHandleIndex,
    ) -> VariantRef<S> {
        let variant_instantiation = m.variant_instantiation_handle_at(idx);
        let enum_instantiation = m.enum_instantiation_at(variant_instantiation.enum_def);
        VariantRef::new(
            tables,
            enum_instantiation.def,
            variant_instantiation.variant,
            Some(enum_instantiation.type_parameters),
        )
    }
}

impl<S> Bytecode<S> {
    fn new<Pool: StringPool<String = S>>(
        tables: &Tables<S>,
        pool: &mut Pool,
        m: &CompiledModule,
        bytecode: &FBytecode,
        jump_tables: &[Rc<VariantJumpTable<S>>],
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
            FB::LdU128(x) => B::LdU128(x.clone()),
            FB::CopyLoc(x) => B::CopyLoc(*x),
            FB::MoveLoc(x) => B::MoveLoc(*x),
            FB::StLoc(x) => B::StLoc(*x),
            FB::LdU16(x) => B::LdU16(*x),
            FB::LdU32(x) => B::LdU32(*x),
            FB::LdU256(x) => B::LdU256(x.clone()),
            FB::LdConst(const_idx) => B::LdConst(tables.constants[const_idx.0 as usize].clone()),
            FB::Call(fh_idx) => B::Call(Box::new(FunctionRef::new(tables, pool, m, *fh_idx, None))),
            FB::CallGeneric(fhi_idx) => B::Call(Box::new(FunctionRef::instantiated(
                tables, pool, m, *fhi_idx,
            ))),
            FB::Pack(idx) => B::Pack(Box::new(StructRef::new(tables, *idx, None))),
            FB::PackGeneric(idx) => B::Pack(Box::new(StructRef::instantiated(tables, m, *idx))),
            FB::Unpack(idx) => B::Unpack(Box::new(StructRef::new(tables, *idx, None))),
            FB::UnpackGeneric(idx) => B::Unpack(Box::new(StructRef::instantiated(tables, m, *idx))),
            FB::MutBorrowLoc(x) => B::MutBorrowLoc(*x),
            FB::ImmBorrowLoc(x) => B::ImmBorrowLoc(*x),
            FB::MutBorrowField(fh_ixd) => {
                B::MutBorrowField(Box::new(FieldRef::new(tables, m, *fh_ixd, None)))
            }
            FB::MutBorrowFieldGeneric(fhi_idx) => {
                B::MutBorrowField(Box::new(FieldRef::instantiated(tables, m, *fhi_idx)))
            }
            FB::ImmBorrowField(fh_idx) => {
                B::ImmBorrowField(Box::new(FieldRef::new(tables, m, *fh_idx, None)))
            }
            FB::ImmBorrowFieldGeneric(fhi_idx) => {
                B::ImmBorrowField(Box::new(FieldRef::instantiated(tables, m, *fhi_idx)))
            }
            FB::MutBorrowGlobalDeprecated(s_idx) => {
                B::MutBorrowGlobalDeprecated(Box::new(StructRef::new(tables, *s_idx, None)))
            }
            FB::MutBorrowGlobalGenericDeprecated(si_idx) => {
                B::MutBorrowGlobalDeprecated(Box::new(StructRef::instantiated(tables, m, *si_idx)))
            }
            FB::ImmBorrowGlobalDeprecated(s_idx) => {
                B::ImmBorrowGlobalDeprecated(Box::new(StructRef::new(tables, *s_idx, None)))
            }
            FB::ImmBorrowGlobalGenericDeprecated(si_idx) => {
                B::ImmBorrowGlobalDeprecated(Box::new(StructRef::instantiated(tables, m, *si_idx)))
            }
            FB::ExistsDeprecated(s_idx) => {
                B::ExistsDeprecated(Box::new(StructRef::new(tables, *s_idx, None)))
            }
            FB::ExistsGenericDeprecated(si_idx) => {
                B::ExistsDeprecated(Box::new(StructRef::instantiated(tables, m, *si_idx)))
            }
            FB::MoveFromDeprecated(s_idx) => {
                B::MoveFromDeprecated(Box::new(StructRef::new(tables, *s_idx, None)))
            }
            FB::MoveFromGenericDeprecated(si_idx) => {
                B::MoveFromDeprecated(Box::new(StructRef::instantiated(tables, m, *si_idx)))
            }
            FB::MoveToDeprecated(s_idx) => {
                B::MoveToDeprecated(Box::new(StructRef::new(tables, *s_idx, None)))
            }
            FB::MoveToGenericDeprecated(si_idx) => {
                B::MoveToDeprecated(Box::new(StructRef::instantiated(tables, m, *si_idx)))
            }
            FB::VecPack(sig_idx, len) => {
                B::VecPack(Box::new((signature_to_single_type(tables, *sig_idx), *len)))
            }
            FB::VecLen(sig_idx) => B::VecLen(signature_to_single_type(tables, *sig_idx)),
            FB::VecImmBorrow(sig_idx) => {
                B::VecImmBorrow(signature_to_single_type(tables, *sig_idx))
            }
            FB::VecMutBorrow(sig_idx) => {
                B::VecMutBorrow(signature_to_single_type(tables, *sig_idx))
            }
            FB::VecPushBack(sig_idx) => B::VecPushBack(signature_to_single_type(tables, *sig_idx)),
            FB::VecPopBack(sig_idx) => B::VecPopBack(signature_to_single_type(tables, *sig_idx)),
            FB::VecUnpack(sig_idx, len) => {
                B::VecUnpack(Box::new((signature_to_single_type(tables, *sig_idx), *len)))
            }
            FB::VecSwap(sig_idx) => B::VecSwap(signature_to_single_type(tables, *sig_idx)),
            FB::PackVariant(handle) => {
                B::PackVariant(Box::new(VariantRef::noninstantiated(tables, m, *handle)))
            }
            FB::PackVariantGeneric(handle) => {
                B::PackVariant(Box::new(VariantRef::instantiated(tables, m, *handle)))
            }
            FB::UnpackVariant(handle) => {
                B::UnpackVariant(Box::new(VariantRef::noninstantiated(tables, m, *handle)))
            }
            FB::UnpackVariantGeneric(handle) => {
                B::UnpackVariant(Box::new(VariantRef::instantiated(tables, m, *handle)))
            }
            FB::UnpackVariantImmRef(handle) => {
                B::UnpackVariantImmRef(Box::new(VariantRef::noninstantiated(tables, m, *handle)))
            }
            FB::UnpackVariantGenericImmRef(handle) => {
                B::UnpackVariantImmRef(Box::new(VariantRef::instantiated(tables, m, *handle)))
            }
            FB::UnpackVariantMutRef(handle) => {
                B::UnpackVariantMutRef(Box::new(VariantRef::noninstantiated(tables, m, *handle)))
            }
            FB::UnpackVariantGenericMutRef(handle) => {
                B::UnpackVariantMutRef(Box::new(VariantRef::instantiated(tables, m, *handle)))
            }
            FB::VariantSwitch(jti) => B::VariantSwitch(jump_tables[jti.0 as usize].clone()),
        }
    }
}

fn signature_to_single_type<S>(tables: &Tables<S>, sig_idx: SignatureIndex) -> Rc<Type<S>> {
    tables.signatures[sig_idx.0 as usize][0].clone()
}

impl<S: std::fmt::Display> std::fmt::Display for Type<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Type::Datatype(dt) => std::fmt::Display::fmt(dt, f),
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
            Type::Reference(false, r) => write!(f, "&{}", r),
            Type::Reference(true, r) => write!(f, "&mut {}", r),
            Type::TypeParameter(i) => write!(f, "T{:?}", i),
        }
    }
}

impl<S: std::fmt::Display> std::fmt::Display for Datatype<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let Datatype {
            address,
            module,
            name,
            type_arguments,
        } = self;
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
}

#[test]
fn sizes() {
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    struct Big([u8; 1024]);

    assert_eq!(std::mem::size_of::<Type<Big>>(), 16);
    assert_eq!(std::mem::size_of::<Bytecode<Big>>(), 16);
}
