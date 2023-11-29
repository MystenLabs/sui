// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    location::*,
    spec_language_ast::{Condition, Invariant, SyntheticDefinition},
};
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, language_storage::ModuleId,
    runtime_value::MoveValue,
};
use move_symbol_pool::Symbol;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashSet, VecDeque},
    fmt,
};

//**************************************************************************************************
// Program
//**************************************************************************************************

#[derive(Debug, Clone)]
/// A set of Move modules and a Move transaction script
pub struct Program {
    /// The modules to publish
    pub modules: Vec<ModuleDefinition>,
}

//**************************************************************************************************
// Modules
//**************************************************************************************************

/// Newtype for a name of a module
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct ModuleName(pub Symbol);

/// Newtype of the address + the module name
/// `addr.m`
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct ModuleIdent {
    /// Name for the module. Will be unique among modules published under the same address
    pub name: ModuleName,
    /// Address that this module is published under
    pub address: AccountAddress,
}

/// A Move module
#[derive(Clone, Debug, PartialEq)]
pub struct ModuleDefinition {
    /// The location of this module
    pub loc: Loc,
    /// name and address of the module
    pub identifier: ModuleIdent,
    /// the module's friends
    pub friends: Vec<ModuleIdent>,
    /// the module's dependencies
    pub imports: Vec<ImportDefinition>,
    /// Explicit declaration of dependencies. If not provided, will be inferred based on given
    /// dependencies to the IR compiler
    pub explicit_dependency_declarations: Vec<ModuleDependency>,
    /// the structs (including resources) that the module defines
    pub structs: Vec<StructDefinition>,
    /// The enums that the module defines
    pub enums: Vec<EnumDefinition>,
    /// the constants that the script defines. Only a utility, the identifiers are not carried into
    /// the Move bytecode
    pub constants: Vec<Constant>,
    /// the procedure that the module defines
    pub functions: Vec<(FunctionName, Function)>,
    /// the synthetic, specification variables the module defines.
    pub synthetics: Vec<SyntheticDefinition>,
}

/// Explicitly given dependency
#[derive(Clone, Debug, PartialEq)]
pub struct ModuleDependency {
    /// Qualified identifer of the dependency
    pub name: ModuleName,
    /// The data types (including resources) that the dependency defines
    pub datatypes: Vec<DatatypeDependency>,
    /// The signatures of functions that the dependency defines
    pub functions: Vec<FunctionDependency>,
}

//**************************************************************************************************
// Imports
//**************************************************************************************************

/// A dependency/import declaration
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportDefinition {
    /// the dependency
    /// `addr.m`
    pub ident: ModuleIdent,
    /// the alias for that dependency
    /// `m`
    pub alias: ModuleName,
}

//**************************************************************************************************
// Vars
//**************************************************************************************************

/// Newtype for a variable/local
#[derive(Debug, PartialEq, Hash, Eq, Clone, Ord, PartialOrd)]
pub struct Var_(pub Symbol);

/// The type of a variable with a location
pub type Var = Spanned<Var_>;

/// New type that represents a type variable. Used to declare type formals & reference them.
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct TypeVar_(pub Symbol);

/// The type of a type variable with a location.
pub type TypeVar = Spanned<TypeVar_>;

//**************************************************************************************************
// Abilities
//**************************************************************************************************

/// The abilities of a type. Analogous to `move_binary_format::file_format::Ability`.
#[derive(Debug, Clone, Eq, Copy, Hash, Ord, PartialEq, PartialOrd)]
pub enum Ability {
    /// Allows values of types with this ability to be copied
    Copy,
    /// Allows values of types with this ability to be dropped or if left in a local at return
    Drop,
    /// Allows values of types with this ability to exist inside a struct in global storage
    Store,
    /// Allows the type to serve as a key for global storage operations
    Key,
}
//**************************************************************************************************
// Types
//**************************************************************************************************

/// The type of a single value
#[derive(Debug, PartialEq, Clone)]
pub enum Type {
    /// `address`
    Address,
    /// `signer`
    Signer,
    /// `u8`
    U8,
    /// `u16`
    U16,
    /// `u32`
    U32,
    /// `u64`
    U64,
    /// `u128`
    U128,
    /// `u256`
    U256,
    /// `bool`
    Bool,
    /// `vector`
    Vector(Box<Type>),
    /// A module defined struct
    Datatype(QualifiedDatatypeIdent, Vec<Type>),
    /// A reference type, the bool flag indicates whether the reference is mutable
    Reference(bool, Box<Type>),
    /// A type parameter
    TypeParameter(TypeVar_),
}

//**************************************************************************************************
// Data Types
//**************************************************************************************************

/// Identifier for a struct definition. Tells us where to look in the storage layer to find the
/// code associated with the interface
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct QualifiedDatatypeIdent {
    /// Module name and address in which the struct is contained
    pub module: ModuleName,
    /// Name for the struct class. Should be unique among structs published under the same
    /// module+address
    pub name: DatatypeName,
}

/// The field newtype
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct Field_(pub Symbol);

/// A field coupled with source location information
pub type Field = Spanned<Field_>;

/// A fully-qualified field identifier.
///
/// Rather than simply referring to a field 'f' with a single identifier and
/// relying on type inference to determine the type of the struct being
/// accessed, this type refers to the field 'f' on the explicit struct type
/// 'S<T>' -- that is, 'S<T>::f'.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldIdent_ {
    /// The name of the struct type on which the field is declared.
    pub struct_name: DatatypeName,
    /// For generic struct types, the type parameters used to instantiate the
    /// struct type (this is an empty vector for non-generic struct types).
    pub type_actuals: Vec<Type>,
    /// The name of the field.
    pub field: Field,
}

pub type FieldIdent = Spanned<FieldIdent_>;

/// A field map
pub type Fields<T> = Vec<(Field, T)>;

/// Newtype for the name of a data type
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct DatatypeName(pub Symbol);

/// Newtype for the name of a variant
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct VariantName(pub Symbol);

/// A struct type parameter with its constraints and whether it's declared as phantom.
pub type DatatypeTypeParameter = (bool, TypeVar, BTreeSet<Ability>);

/// A Move struct
#[derive(Clone, Debug, PartialEq)]
pub struct StructDefinition_ {
    /// The declared abilities for the struct
    pub abilities: BTreeSet<Ability>,
    /// Human-readable name for the struct that also serves as a nominal type
    pub name: DatatypeName,
    /// The list of formal type arguments
    pub type_formals: Vec<DatatypeTypeParameter>,
    /// the fields each instance has
    pub fields: StructDefinitionFields,
    /// the invariants for this struct
    pub invariants: Vec<Invariant>,
}
/// The type of a StructDefinition along with its source location information
pub type StructDefinition = Spanned<StructDefinition_>;

/// An explicit struct dependency
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatatypeDependency {
    /// The declared abilities for the struct
    pub abilities: BTreeSet<Ability>,
    /// Human-readable name for the struct that also serves as a nominal type
    pub name: DatatypeName,
    /// The list of formal type arguments
    pub type_formals: Vec<DatatypeTypeParameter>,
}

/// The fields of a Move struct definition
#[derive(Clone, Debug, PartialEq)]
pub enum StructDefinitionFields {
    /// The fields are declared
    Move { fields: Fields<Type> },
    /// The struct is a type provided by the VM
    Native,
}

/// A Move enum
#[derive(Clone, Debug, PartialEq)]
pub struct EnumDefinition_ {
    /// The declared abilities for the struct
    pub abilities: BTreeSet<Ability>,
    /// Human-readable name for the struct that also serves as a nominal type
    pub name: DatatypeName,
    /// The list of formal type arguments
    pub type_formals: Vec<DatatypeTypeParameter>,
    /// the fields each instance has
    pub variants: VariantDefinitions,
    /// the invariants for this enum
    pub invariants: Vec<Invariant>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariantDefinition_ {
    pub name: VariantName,
    pub fields: Fields<Type>,
}

pub type VariantDefinition = Spanned<VariantDefinition_>;

pub type VariantDefinitions = Vec<VariantDefinition>;

/// The type of a EnumDefinition along with its source location information
pub type EnumDefinition = Spanned<EnumDefinition_>;

//**************************************************************************************************
// Constants
//**************************************************************************************************

/// Newtype for the name of a constant
#[derive(Debug, Serialize, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Clone)]
pub struct ConstantName(pub Symbol);

/// A constant declaration in a module or script
#[derive(Clone, Debug, PartialEq)]
pub struct Constant {
    /// The constant's name. Not carried through to the Move bytecode
    pub name: ConstantName,
    /// The type of the constant's value
    pub signature: Type,
    /// The constant's value
    pub value: MoveValue,
}

//**************************************************************************************************
// Functions
//**************************************************************************************************

/// Newtype for the name of a function
#[derive(Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Clone)]
pub struct FunctionName(pub Symbol);

/// The signature of a function
#[derive(PartialEq, Debug, Clone)]
pub struct FunctionSignature {
    /// Possibly-empty list of (formal name, formal type) pairs. Names are unique.
    pub formals: Vec<(Var, Type)>,
    /// Optional return types
    pub return_type: Vec<Type>,
    /// Possibly-empty list of type parameters and their constraints
    pub type_formals: Vec<(TypeVar, BTreeSet<Ability>)>,
}

/// An explicit function dependency
#[derive(PartialEq, Debug, Clone)]
pub struct FunctionDependency {
    /// Name of the function dependency
    pub name: FunctionName,
    /// Signature of the function dependency
    pub signature: FunctionSignature,
}

/// Public or internal modifier for a procedure
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum FunctionVisibility {
    /// The procedure can be invoked anywhere
    /// `public`
    Public,
    /// The procedure can be invoked internally as well as by modules in the friend list
    /// `public(friend)`
    Friend,
    /// The procedure can be invoked only internally
    /// `<no modifier>`
    Internal,
}

/// The body of a Move function
#[derive(PartialEq, Debug, Clone)]
pub enum FunctionBody {
    /// The body is declared
    /// `locals` are all of the declared locals
    /// `code` is the code that defines the procedure
    Move {
        locals: Vec<(Var, Type)>,
        code: Vec<Block>,
    },
    Bytecode {
        locals: Vec<(Var, Type)>,
        code: BytecodeBlocks,
    },
    /// The body is provided by the runtime
    Native,
}

/// A Move function/procedure
#[derive(PartialEq, Debug, Clone)]
pub struct Function_ {
    /// The visibility
    pub visibility: FunctionVisibility,
    /// Is entry function
    pub is_entry: bool,
    /// The type signature
    pub signature: FunctionSignature,
    /// List of specifications for the Move prover (experimental)
    pub specifications: Vec<Condition>,
    /// The code for the procedure
    pub body: FunctionBody,
}

/// The type of a Function coupled with its source location information.
pub type Function = Spanned<Function_>;

//**************************************************************************************************
// Statements
//**************************************************************************************************

/// Builtin "function"-like operators that often have a signature not expressable in the
/// type system and/or have access to some runtime/storage context
#[derive(Debug, PartialEq, Clone)]
pub enum Builtin {
    /// Pack a vector fix a fixed number of elements. Zero elements means an empty vector.
    VecPack(Vec<Type>, u64),
    /// Get the length of a vector
    VecLen(Vec<Type>),
    /// Acquire an immutable reference to the element at a given index of the vector
    VecImmBorrow(Vec<Type>),
    /// Acquire a mutable reference to the element at a given index of the vector
    VecMutBorrow(Vec<Type>),
    /// Push an element to the end of the vector
    VecPushBack(Vec<Type>),
    /// Pop and return an element from the end of the vector
    VecPopBack(Vec<Type>),
    /// Destroy a vector of a fixed length. Zero length means destroying an empty vector.
    VecUnpack(Vec<Type>, u64),
    /// Swap the elements at twi indices in the vector
    VecSwap(Vec<Type>),

    /// Convert a mutable reference into an immutable one
    Freeze,

    /// Cast an integer into u8.
    ToU8,
    /// Cast an integer into u16.
    ToU16,
    /// Cast an integer into u32.
    ToU32,
    /// Cast an integer into u64.
    ToU64,
    /// Cast an integer into u128.
    ToU128,
    /// Cast an integer into u256.
    ToU256,
}

/// Enum for different function calls
#[derive(Debug, PartialEq, Clone)]
pub enum FunctionCall_ {
    /// functions defined in the host environment
    Builtin(Builtin),
    /// The call of a module defined procedure
    ModuleFunctionCall {
        module: ModuleName,
        name: FunctionName,
        type_actuals: Vec<Type>,
    },
}
/// The type for a function call and its location
pub type FunctionCall = Spanned<FunctionCall_>;

/// Enum for Move lvalues
#[derive(Debug, Clone, PartialEq)]
pub enum LValue_ {
    /// `x`
    Var(Var),
    /// `*e`
    Mutate(Exp),
    /// `_`
    Pop,
}
pub type LValue = Spanned<LValue_>;

#[derive(Debug, Clone, PartialEq)]
pub enum UnpackType {
    ByValue,
    ByImmRef,
    ByMutRef,
}

/// A [`Block_`] is composed of zero or more "statements," which can be translated into one or more
/// bytecode instructions.
#[derive(Debug, Clone, PartialEq)]
pub enum Statement_ {
    /// `abort e`.
    Abort(Option<Box<Exp>>),
    /// `assert(e_1, e_2)`
    Assert(Box<Exp>, Box<Exp>),
    /// `return e_1, ... , e_j`
    Return(Box<Exp>),
    /// `l_1, ..., l_n = e`
    Assign(Vec<LValue>, Exp),
    /// A statement representing an expression `e`.
    Exp(Box<Exp>),
    /// `jump lbl`
    Jump(BlockLabel),
    /// `jump_if (e) lbl`
    JumpIf(Box<Exp>, BlockLabel),
    /// `jump_if_false (e) lbl`
    JumpIfFalse(Box<Exp>, BlockLabel),
    /// `n { f_1: x_1, ... , f_j: x_j  } = e`
    Unpack(DatatypeName, Vec<Type>, Fields<Var>, Box<Exp>),
    /// `e::v { f_1: x_1, ... , f_j: x_j  } = e`
    UnpackVariant(
        DatatypeName,
        VariantName,
        Vec<Type>,
        Fields<Var>,
        Box<Exp>,
        UnpackType,
    ),
    /// `variant_switch (e) [(v1, lbl_1), ..., (v_n, lbl_n)]`
    VariantSwitch(DatatypeName, Vec<(VariantName, BlockLabel)>, Box<Exp>),
}
/// A [`Statement_`] with a location.
pub type Statement = Spanned<Statement_>;

/// A block is composed of a [`BlockLabel`], followed by 0 or more [`Statement`],
/// e.g.: `label b: s_1; ... s_n;`.
#[derive(Debug, PartialEq, Clone)]
pub struct Block_ {
    /// The label that can be used to jump to this block.
    pub label: BlockLabel,
    /// The statements that make up the block.
    pub statements: VecDeque<Statement>,
}
/// A [`Block_`] with a location.
pub type Block = Spanned<Block_>;

//**************************************************************************************************
// Expressions
//**************************************************************************************************

/// Bottom of the value hierarchy. These values can be trivially copyable and stored in statedb as a
/// single entry.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum CopyableVal_ {
    /// An address in the global storage
    Address(AccountAddress),
    /// An unsigned 8-bit integer
    U8(u8),
    /// An unsigned 16-bit integer
    U16(u16),
    /// An unsigned 32-bit integer
    U32(u32),
    /// An unsigned 64-bit integer
    U64(u64),
    /// An unsigned 128-bit integer
    U128(u128),
    /// An unsigned 256-bit integer
    U256(move_core_types::u256::U256),
    /// true or false
    Bool(bool),
    /// `b"<bytes>"`
    ByteArray(Vec<u8>),
}

/// The type of a value and its location
pub type CopyableVal = Spanned<CopyableVal_>;

/// The type for fields and their bound expressions
pub type ExpFields = Fields<Exp>;

/// Enum for unary operators
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnaryOp {
    /// Boolean negation
    Not,
}

/// Enum for binary operators
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinOp {
    // u64 ops
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `%`
    Mod,
    /// `/`
    Div,
    /// `|`
    BitOr,
    /// `&`
    BitAnd,
    /// `^`
    Xor,
    /// `<<`
    Shl,
    /// `>>`
    Shr,

    // Bool ops
    /// `&&`
    And,
    /// `||`
    Or,

    // Compare Ops
    /// `==`
    Eq,
    /// `!=`
    Neq,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `>=`
    Ge,
    /// '..'  only used in specs
    Subrange,
}

/// Enum for all expressions
#[derive(Debug, Clone, PartialEq)]
pub enum Exp_ {
    /// `*e`
    Dereference(Box<Exp>),
    /// `op e`
    UnaryExp(UnaryOp, Box<Exp>),
    /// `e_1 op e_2`
    BinopExp(Box<Exp>, BinOp, Box<Exp>),
    /// Wrapper to lift `CopyableVal` into `Exp`
    /// `v`
    Value(CopyableVal),
    /// Takes the given field values and instantiates the struct
    /// Returns a fresh `StructInstance` whose type and kind (resource or otherwise)
    /// as the current struct class (i.e., the class of the method we're currently executing).
    /// `n { f_1: e_1, ... , f_j: e_j }`
    Pack(DatatypeName, Vec<Type>, ExpFields),
    /// `&e.f`, `&mut e.f`
    Borrow {
        /// mutable or not
        is_mutable: bool,
        /// the expression containing the reference
        exp: Box<Exp>,
        /// the field being borrowed
        field: FieldIdent,
    },
    /// `move(x)`
    Move(Var),
    /// `copy(x)`
    Copy(Var),
    /// `&x` or `&mut x`
    BorrowLocal(bool, Var),
    /// `f(e)` or `f(e_1, e_2, ..., e_j)`
    FunctionCall(FunctionCall, Box<Exp>),
    /// (e_1, e_2, e_3, ..., e_j)
    ExprList(Vec<Exp>),
    /// Takes the given field values and instantiates the variant of the enum.
    /// `e::v { f_1: e_1, ... , f_j: e_j }`
    PackVariant(DatatypeName, VariantName, Vec<Type>, ExpFields),
}

/// The type for a `Exp_` and its location
pub type Exp = Spanned<Exp_>;

//**************************************************************************************************
// Bytecode
//**************************************************************************************************

pub type BytecodeBlocks = Vec<(BlockLabel_, BytecodeBlock)>;
pub type BytecodeBlock = Vec<Bytecode>;

#[derive(Debug, Clone, Hash, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlockLabel_(pub Symbol);
pub type BlockLabel = Spanned<BlockLabel_>;

#[derive(Debug, Clone, Hash, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NopLabel(pub Symbol);

#[derive(Debug, Clone, PartialEq)]
pub enum Bytecode_ {
    Pop,
    Ret,
    Nop(Option<NopLabel>),
    BrTrue(BlockLabel_),
    BrFalse(BlockLabel_),
    Branch(BlockLabel_),
    LdU8(u8),
    LdU16(u16),
    LdU32(u32),
    LdU64(u64),
    LdU128(u128),
    LdU256(move_core_types::u256::U256),
    CastU8,
    CastU16,
    CastU32,
    CastU64,
    CastU128,
    CastU256,
    LdTrue,
    LdFalse,
    LdConst(Type, MoveValue),
    LdNamedConst(ConstantName),
    CopyLoc(Var),
    MoveLoc(Var),
    StLoc(Var),
    Call(ModuleName, FunctionName, Vec<Type>),
    Pack(DatatypeName, Vec<Type>),
    Unpack(DatatypeName, Vec<Type>),
    ReadRef,
    WriteRef,
    FreezeRef,
    MutBorrowLoc(Var),
    ImmBorrowLoc(Var),
    MutBorrowField(DatatypeName, Vec<Type>, Field),
    ImmBorrowField(DatatypeName, Vec<Type>, Field),
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
    PackVariant(DatatypeName, VariantName, Vec<Type>),
    UnpackVariant(DatatypeName, VariantName, Vec<Type>, UnpackType),
    VariantSwitch(DatatypeName, Vec<(VariantName, BlockLabel)>),
}
pub type Bytecode = Spanned<Bytecode_>;

//**************************************************************************************************
// impls
//**************************************************************************************************

fn get_external_deps(imports: &[ImportDefinition]) -> Vec<ModuleId> {
    let mut deps = HashSet::new();
    for dep in imports.iter() {
        let identifier = Identifier::new(dep.ident.name.0.as_str().to_owned()).unwrap();
        deps.insert(ModuleId::new(dep.ident.address, identifier));
    }
    deps.into_iter().collect()
}

impl Program {
    /// Create a new `Program` from modules and transaction script
    pub fn new(modules: Vec<ModuleDefinition>) -> Self {
        Program { modules }
    }
}

static SELF_MODULE_NAME: Lazy<Symbol> = Lazy::new(|| Symbol::from("Self"));

impl ModuleName {
    /// Name for the current module handle
    pub fn self_name() -> &'static str {
        SELF_MODULE_NAME.as_str()
    }

    /// Create a new `ModuleName` from `self_name`.
    pub fn module_self() -> Self {
        ModuleName(*SELF_MODULE_NAME)
    }
}

impl ModuleIdent {
    /// Creates a new fully qualified module identifier from the module name and the address at
    /// which it is published
    pub fn new(name: ModuleName, address: AccountAddress) -> Self {
        ModuleIdent { name, address }
    }

    /// Accessor for the name of the fully qualified module identifier
    pub fn name(&self) -> &ModuleName {
        &self.name
    }

    /// Accessor for the address at which the module is published
    pub fn address(&self) -> &AccountAddress {
        &self.address
    }
}

impl ModuleDefinition {
    /// Creates a new `ModuleDefinition` from its string name, dependencies, structs+resources,
    /// and procedures
    /// Does not verify the correctness of any internal properties of its elements
    pub fn new(
        loc: Loc,
        identifier: ModuleIdent,
        friends: Vec<ModuleIdent>,
        imports: Vec<ImportDefinition>,
        explicit_dependency_declarations: Vec<ModuleDependency>,
        structs: Vec<StructDefinition>,
        enums: Vec<EnumDefinition>,
        constants: Vec<Constant>,
        functions: Vec<(FunctionName, Function)>,
        synthetics: Vec<SyntheticDefinition>,
    ) -> Self {
        ModuleDefinition {
            loc,
            identifier,
            friends,
            imports,
            explicit_dependency_declarations,
            structs,
            enums,
            constants,
            functions,
            synthetics,
        }
    }

    /// Return a vector of `ModuleId` for the external dependencies.
    pub fn get_external_deps(&self) -> Vec<ModuleId> {
        get_external_deps(self.imports.as_slice())
    }
}

impl Ability {
    pub const COPY: &'static str = "copy";
    pub const DROP: &'static str = "drop";
    pub const STORE: &'static str = "store";
    pub const KEY: &'static str = "key";
}

impl Type {
    /// Creates a new struct type
    pub fn r#struct(ident: QualifiedDatatypeIdent, type_actuals: Vec<Type>) -> Type {
        Type::Datatype(ident, type_actuals)
    }

    /// Creates a new reference type from its mutability and underlying type
    pub fn reference(is_mutable: bool, t: Type) -> Type {
        Type::Reference(is_mutable, Box::new(t))
    }

    /// Creates a new address type
    pub fn address() -> Type {
        Type::Address
    }

    /// Creates a new u64 type
    pub fn u64() -> Type {
        Type::U64
    }

    /// Creates a new bool type
    pub fn bool() -> Type {
        Type::Bool
    }
}

impl QualifiedDatatypeIdent {
    /// Creates a new StructType handle from the name of the module alias and the name of the struct
    pub fn new(module: ModuleName, name: DatatypeName) -> Self {
        QualifiedDatatypeIdent { module, name }
    }

    /// Accessor for the module alias
    pub fn module(&self) -> &ModuleName {
        &self.module
    }

    /// Accessor for the struct name
    pub fn name(&self) -> &DatatypeName {
        &self.name
    }
}

impl ImportDefinition {
    /// Creates a new import definition from a module identifier and an optional alias
    /// If the alias is `None`, the alias will be a cloned copy of the identifiers module name
    pub fn new(ident: ModuleIdent, alias_opt: Option<ModuleName>) -> Self {
        let alias = match alias_opt {
            Some(alias) => alias,
            None => *ident.name(),
        };
        ImportDefinition { ident, alias }
    }
}

impl StructDefinition_ {
    /// Creates a new StructDefinition from the abilities, the string representation of the name,
    /// and the user specified fields, a map from their names to their types
    /// Does not verify the correctness of any internal properties, e.g. doesn't check that the
    /// fields do not have reference types
    pub fn move_declared(
        abilities: BTreeSet<Ability>,
        name: Symbol,
        type_formals: Vec<DatatypeTypeParameter>,
        fields: Fields<Type>,
        invariants: Vec<Invariant>,
    ) -> Self {
        StructDefinition_ {
            abilities,
            name: DatatypeName(name),
            type_formals,
            fields: StructDefinitionFields::Move { fields },
            invariants,
        }
    }

    /// Creates a new StructDefinition from the abilities, the string representation of the name,
    /// and the user specified fields, a map from their names to their types
    pub fn native(
        abilities: BTreeSet<Ability>,
        name: Symbol,
        type_formals: Vec<DatatypeTypeParameter>,
    ) -> Self {
        StructDefinition_ {
            abilities,
            name: DatatypeName(name),
            type_formals,
            fields: StructDefinitionFields::Native,
            invariants: vec![],
        }
    }
}

impl EnumDefinition_ {
    pub fn new(
        abilities: BTreeSet<Ability>,
        name: Symbol,
        type_formals: Vec<DatatypeTypeParameter>,
        variants: VariantDefinitions,
        invariants: Vec<Invariant>,
    ) -> Self {
        Self {
            abilities,
            name: DatatypeName(name),
            type_formals,
            variants,
            invariants,
        }
    }
}

impl VariantDefinition_ {
    pub fn new(name: Symbol, fields: Fields<Type>) -> Self {
        Self {
            name: VariantName(name),
            fields,
        }
    }
}

impl FunctionSignature {
    /// Creates a new function signature from the parameters and the return types
    pub fn new(
        formals: Vec<(Var, Type)>,
        return_type: Vec<Type>,
        type_parameters: Vec<(TypeVar, BTreeSet<Ability>)>,
    ) -> Self {
        FunctionSignature {
            formals,
            return_type,
            type_formals: type_parameters,
        }
    }
}

impl Function_ {
    /// Creates a new function declaration from the components of the function
    /// See the declaration of the struct `Function` for more details
    pub fn new(
        visibility: FunctionVisibility,
        is_entry: bool,
        formals: Vec<(Var, Type)>,
        return_type: Vec<Type>,
        type_parameters: Vec<(TypeVar, BTreeSet<Ability>)>,
        specifications: Vec<Condition>,
        body: FunctionBody,
    ) -> Self {
        let signature = FunctionSignature::new(formals, return_type, type_parameters);
        Function_ {
            visibility,
            is_entry,
            signature,
            specifications,
            body,
        }
    }
}

impl FunctionCall_ {
    /// Creates a `FunctionCall::ModuleFunctionCall` variant
    pub fn module_call(module: ModuleName, name: FunctionName, type_actuals: Vec<Type>) -> Self {
        FunctionCall_::ModuleFunctionCall {
            module,
            name,
            type_actuals,
        }
    }

    /// Creates a `FunctionCall::Builtin` variant with no location information
    pub fn builtin(bif: Builtin) -> FunctionCall {
        Spanned::unsafe_no_loc(FunctionCall_::Builtin(bif))
    }
}

impl Statement_ {
    /// Creates a statement that returns no values.
    pub fn return_empty() -> Self {
        Statement_::Return(Box::new(Spanned::unsafe_no_loc(Exp_::ExprList(vec![]))))
    }

    /// Creates a statement that returns a single value.
    pub fn return_(op: Exp) -> Self {
        Statement_::Return(Box::new(op))
    }
}

impl Block_ {
    /// Creates a new block from a label and a vector of statements.
    pub fn new(label: BlockLabel, statements: Vec<Statement>) -> Self {
        Self {
            label,
            statements: VecDeque::from(statements),
        }
    }
}

impl Exp_ {
    /// Creates a new address `Exp` with no location information
    pub fn address(addr: AccountAddress) -> Exp {
        Spanned::unsafe_no_loc(Exp_::Value(Spanned::unsafe_no_loc(CopyableVal_::Address(
            addr,
        ))))
    }

    /// Creates a new value `Exp` with no location information
    pub fn value(b: CopyableVal_) -> Exp {
        Spanned::unsafe_no_loc(Exp_::Value(Spanned::unsafe_no_loc(b)))
    }

    /// Creates a new u64 `Exp` with no location information
    pub fn u64(i: u64) -> Exp {
        Exp_::value(CopyableVal_::U64(i))
    }

    /// Creates a new bool `Exp` with no location information
    pub fn bool(b: bool) -> Exp {
        Exp_::value(CopyableVal_::Bool(b))
    }

    /// Creates a new bytearray `Exp` with no location information
    pub fn byte_array(buf: Vec<u8>) -> Exp {
        Exp_::value(CopyableVal_::ByteArray(buf))
    }

    /// Creates a new pack/struct-instantiation `Exp` with no location information
    pub fn instantiate(n: DatatypeName, tys: Vec<Type>, s: ExpFields) -> Exp {
        Spanned::unsafe_no_loc(Exp_::Pack(n, tys, s))
    }

    /// Creates a new binary operator `Exp` with no location information
    pub fn binop(lhs: Exp, op: BinOp, rhs: Exp) -> Exp {
        Spanned::unsafe_no_loc(Exp_::BinopExp(Box::new(lhs), op, Box::new(rhs)))
    }

    /// Creates a new `e+e` `Exp` with no location information
    pub fn add(lhs: Exp, rhs: Exp) -> Exp {
        Exp_::binop(lhs, BinOp::Add, rhs)
    }

    /// Creates a new `e-e` `Exp` with no location information
    pub fn sub(lhs: Exp, rhs: Exp) -> Exp {
        Exp_::binop(lhs, BinOp::Sub, rhs)
    }

    /// Creates a new `*e` `Exp` with no location information
    pub fn dereference(e: Exp) -> Exp {
        Spanned::unsafe_no_loc(Exp_::Dereference(Box::new(e)))
    }

    /// Creates a new borrow field `Exp` with no location information
    pub fn borrow(is_mutable: bool, exp: Box<Exp>, field: FieldIdent) -> Exp {
        Spanned::unsafe_no_loc(Exp_::Borrow {
            is_mutable,
            exp,
            field,
        })
    }

    /// Creates a new copy-local `Exp` with no location information
    pub fn copy(v: Var) -> Exp {
        Spanned::unsafe_no_loc(Exp_::Copy(v))
    }

    /// Creates a new move-local `Exp` with no location information
    pub fn move_(v: Var) -> Exp {
        Spanned::unsafe_no_loc(Exp_::Move(v))
    }

    /// Creates a new function call `Exp` with no location information
    pub fn function_call(f: FunctionCall, e: Exp) -> Exp {
        Spanned::unsafe_no_loc(Exp_::FunctionCall(f, Box::new(e)))
    }

    pub fn expr_list(exps: Vec<Exp>) -> Exp {
        Spanned::unsafe_no_loc(Exp_::ExprList(exps))
    }
}

//**************************************************************************************************
// Trait impls
//**************************************************************************************************

impl Iterator for Block_ {
    type Item = Statement;

    fn next(&mut self) -> Option<Statement> {
        self.statements.pop_front()
    }
}

//**************************************************************************************************
// Display
//**************************************************************************************************

impl fmt::Display for TypeVar_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for Ability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Ability::Copy => Ability::COPY,
                Ability::Drop => Ability::DROP,
                Ability::Store => Ability::STORE,
                Ability::Key => Ability::KEY,
            }
        )
    }
}

fn format_constraints(set: &BTreeSet<Ability>) -> String {
    set.iter()
        .map(|a| format!("{}", a))
        .collect::<Vec<_>>()
        .join(" + ")
}

impl fmt::Display for ModuleName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for ModuleIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}.{}", self.address, self.name)
    }
}

impl fmt::Display for ModuleDefinition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Module({}, ", self.identifier)?;

        writeln!(f, "Imports(")?;
        for import in &self.imports {
            writeln!(f, "{};", import)?;
        }
        writeln!(f, ")")?;

        writeln!(f, "Dependency(")?;
        for dependency in &self.explicit_dependency_declarations {
            writeln!(f, "{},", dependency)?;
        }
        writeln!(f, ")")?;

        writeln!(f, "Structs(")?;
        for struct_def in &self.structs {
            writeln!(f, "{}, ", struct_def)?;
        }
        writeln!(f, ")")?;

        writeln!(f, "Enums(")?;
        for enum_def in &self.enums {
            writeln!(f, "{}, ", enum_def)?;
        }
        writeln!(f, ")")?;

        writeln!(f, "Constants(")?;
        for constant in &self.constants {
            writeln!(f, "{};", constant)?;
        }
        writeln!(f, ")")?;

        writeln!(f, "Functions(")?;
        for (fun_name, fun) in &self.functions {
            writeln!(f, "({}, {}), ", fun_name, fun)?;
        }
        writeln!(f, ")")?;

        writeln!(f, ")")
    }
}

impl fmt::Display for ImportDefinition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "import {} as {}", &self.ident, &self.alias)
    }
}

impl fmt::Display for ModuleDependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Dependency({}, ", &self.name)?;
        for sdep in &self.datatypes {
            writeln!(f, "{}, ", sdep)?
        }
        for fdep in &self.functions {
            writeln!(f, "{}, ", fdep)?
        }
        writeln!(f, ")")
    }
}

impl fmt::Display for DatatypeDependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "StructDep({} {}{}",
            self.abilities
                .iter()
                .map(|a| format!("{}", a))
                .collect::<Vec<_>>()
                .join(" "),
            &self.name,
            format_struct_type_formals(&self.type_formals)
        )
    }
}

impl fmt::Display for FunctionDependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FunctionDep({}{}", &self.name, &self.signature)
    }
}

impl fmt::Display for StructDefinition_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Struct({}{}, ",
            self.name,
            format_struct_type_formals(&self.type_formals)
        )?;
        match &self.fields {
            StructDefinitionFields::Move { fields } => writeln!(f, "{}", format_fields(fields))?,
            StructDefinitionFields::Native => writeln!(f, "{{native}}")?,
        }
        write!(f, ")")
    }
}

impl fmt::Display for EnumDefinition_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Enum({}{}, ",
            self.name,
            format_struct_type_formals(&self.type_formals)
        )?;
        for variant in &self.variants {
            writeln!(f, "{}", variant)?;
        }
        write!(f, ")")
    }
}

impl fmt::Display for VariantDefinition_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Variant({}", self.name)?;
        if !self.fields.is_empty() {
            write!(f, "{}", format_fields(&self.fields))?;
        }
        writeln!(f, ")")
    }
}

impl fmt::Display for Constant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "const {}: {} = {}",
            &self.name.0,
            self.signature,
            format_move_value(&self.value)
        )
    }
}

impl fmt::Display for Function_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.signature, self.body)
    }
}

impl fmt::Display for Field_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for FieldIdent_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.struct_name, self.field)
    }
}

impl fmt::Display for DatatypeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for VariantName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for FunctionName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for BlockLabel_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for ConstantName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for FunctionBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FunctionBody::Move {
                ref locals,
                ref code,
            } => {
                for (local, ty) in locals {
                    write!(f, "let {}: {};", local, ty)?;
                }
                for block in code {
                    writeln!(f, "{}", block.value)?;
                }
                Ok(())
            }
            FunctionBody::Bytecode { locals, code } => {
                write!(f, "locals: [")?;
                for (local, ty) in locals {
                    write!(f, "{}: {},", local, ty)?;
                }
                writeln!(f, "]")?;
                for (label, block) in code {
                    writeln!(f, "{}:", &label)?;
                    for instr in block {
                        writeln!(f, "  {}", instr)?;
                    }
                }
                Ok(())
            }
            FunctionBody::Native => write!(f, "native"),
        }
    }
}

// TODO: This function should take an iterator instead.
fn intersperse<T: fmt::Display>(items: &[T], join: &str) -> String {
    // TODO: Any performance issues here? Could be O(n^2) if not optimized.
    items.iter().fold(String::new(), |acc, v| {
        format!("{acc}{join}{v}", acc = acc, join = join, v = v)
    })
}

fn format_fields<T: fmt::Display>(fields: &[(Field, T)]) -> String {
    fields.iter().fold(String::new(), |acc, (field, val)| {
        format!("{} {}: {},", acc, field.value, val)
    })
}

impl fmt::Display for FunctionSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", format_fun_type_formals(&self.type_formals))?;
        write!(f, "(")?;
        for (v, ty) in self.formals.iter() {
            write!(f, "{}: {}, ", v, ty)?;
        }
        write!(f, ")")?;
        Ok(())
    }
}

impl fmt::Display for QualifiedDatatypeIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.module, self.name)
    }
}

fn format_type_actuals(tys: &[Type]) -> String {
    if tys.is_empty() {
        "".to_string()
    } else {
        format!("<{}>", intersperse(tys, ", "))
    }
}

fn format_fun_type_formals(formals: &[(TypeVar, BTreeSet<Ability>)]) -> String {
    if formals.is_empty() {
        "".to_string()
    } else {
        let formatted = formals
            .iter()
            .map(|(tv, abilities)| format!("{}: {}", tv.value, format_constraints(abilities)))
            .collect::<Vec<_>>();
        format!("<{}>", intersperse(&formatted, ", "))
    }
}

fn format_struct_type_formals(formals: &[DatatypeTypeParameter]) -> String {
    if formals.is_empty() {
        "".to_string()
    } else {
        let formatted = formals
            .iter()
            .map(|(is_phantom, tv, abilities)| {
                format!(
                    "{}{}: {}",
                    if *is_phantom { "phantom " } else { "" },
                    tv.value,
                    format_constraints(abilities)
                )
            })
            .collect::<Vec<_>>();
        format!("<{}>", intersperse(&formatted, ", "))
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::U8 => write!(f, "u8"),
            Type::U16 => write!(f, "u16"),
            Type::U32 => write!(f, "u32"),
            Type::U64 => write!(f, "u64"),
            Type::U128 => write!(f, "u128"),
            Type::U256 => write!(f, "u256"),
            Type::Bool => write!(f, "bool"),
            Type::Address => write!(f, "address"),
            Type::Signer => write!(f, "signer"),
            Type::Vector(ty) => write!(f, "vector<{}>", ty),
            Type::Datatype(ident, tys) => write!(f, "{}{}", ident, format_type_actuals(tys)),
            Type::Reference(is_mutable, t) => {
                write!(f, "&{}{}", if *is_mutable { "mut " } else { "" }, t)
            }
            Type::TypeParameter(s) => write!(f, "{}", s),
        }
    }
}

impl fmt::Display for Var_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for Builtin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Builtin::VecPack(tys, num) => write!(f, "vec_pack_{}{}", num, format_type_actuals(tys)),
            Builtin::VecLen(tys) => write!(f, "vec_len{}", format_type_actuals(tys)),
            Builtin::VecImmBorrow(tys) => write!(f, "vec_imm_borrow{}", format_type_actuals(tys)),
            Builtin::VecMutBorrow(tys) => write!(f, "vec_mut_borrow{}", format_type_actuals(tys)),
            Builtin::VecPushBack(tys) => write!(f, "vec_push_back{}", format_type_actuals(tys)),
            Builtin::VecPopBack(tys) => write!(f, "vec_pop_back{}", format_type_actuals(tys)),
            Builtin::VecUnpack(tys, num) => {
                write!(f, "vec_unpack_{}{}", num, format_type_actuals(tys))
            }
            Builtin::VecSwap(tys) => write!(f, "vec_swap{}", format_type_actuals(tys)),
            Builtin::Freeze => write!(f, "freeze"),
            Builtin::ToU8 => write!(f, "to_u8"),
            Builtin::ToU16 => write!(f, "to_u16"),
            Builtin::ToU32 => write!(f, "to_u32"),
            Builtin::ToU64 => write!(f, "to_u64"),
            Builtin::ToU128 => write!(f, "to_u128"),
            Builtin::ToU256 => write!(f, "to_u256"),
        }
    }
}

impl fmt::Display for FunctionCall_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FunctionCall_::Builtin(fun) => write!(f, "{}", fun),
            FunctionCall_::ModuleFunctionCall {
                module,
                name,
                type_actuals,
            } => write!(
                f,
                "{}.{}{}",
                module,
                name,
                format_type_actuals(type_actuals)
            ),
        }
    }
}

impl fmt::Display for LValue_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LValue_::Var(x) => write!(f, "{}", x),
            LValue_::Mutate(e) => write!(f, "*{}", e),
            LValue_::Pop => write!(f, "_"),
        }
    }
}

impl fmt::Display for UnpackType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            UnpackType::ByValue => "",
            UnpackType::ByImmRef => "&",
            UnpackType::ByMutRef => "&mut",
        };
        write!(f, "{}", s)
    }
}

impl fmt::Display for Statement_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Statement_::Abort(None) => write!(f, "abort;"),
            Statement_::Abort(Some(err)) => write!(f, "abort {};", err),
            Statement_::Assert(cond, err) => write!(f, "assert({}, {});", cond, err),
            Statement_::Assign(var_list, e) => {
                if var_list.is_empty() {
                    write!(f, "{};", e)
                } else {
                    write!(f, "{} = ({});", intersperse(var_list, ", "), e)
                }
            }
            Statement_::Exp(e) => write!(f, "({});", e),
            Statement_::Jump(label) => write!(f, "jump {}", label),
            Statement_::JumpIf(e, label) => write!(f, "jump_if ({}) {}", e, label),
            Statement_::JumpIfFalse(e, label) => write!(f, "jump_if_false ({}) {}", e, label),
            Statement_::Return(exps) => write!(f, "return {};", exps),
            Statement_::Unpack(n, tys, bindings, e) => write!(
                f,
                "{}{} {{ {} }} = {}",
                n,
                format_type_actuals(tys),
                bindings
                    .iter()
                    .fold(String::new(), |acc, (field, var)| format!(
                        "{} {} : {},",
                        acc, field, var
                    )),
                e
            ),
            Statement_::UnpackVariant(name, variant_name, tys, bindings, e, unpack_type) => {
                write!(
                    f,
                    "{}{}::{}{} {{ {} }} = {}",
                    unpack_type,
                    name,
                    variant_name,
                    format_type_actuals(tys),
                    bindings
                        .iter()
                        .fold(String::new(), |acc, (field, var)| format!(
                            "{} {} : {},",
                            acc, field, var
                        )),
                    e
                )
            }
            Statement_::VariantSwitch(name, lbls, e) => {
                write!(
                    f,
                    "variant_switch ({}: {}) {{ {} }}",
                    e,
                    name,
                    lbls.iter()
                        .enumerate()
                        .fold(String::new(), |acc, (tag, (name, lbl))| format!(
                            "{} {}:{} => {},",
                            acc, name, tag, lbl
                        ))
                )
            }
        }
    }
}

impl fmt::Display for Block_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "label {}:", self.label)?;
        for statement in self.statements.iter() {
            writeln!(f, "    {}", statement)?;
        }
        Ok(())
    }
}

impl fmt::Display for CopyableVal_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CopyableVal_::U8(v) => write!(f, "{}u8", v),
            CopyableVal_::U16(v) => write!(f, "{}u16", v),
            CopyableVal_::U32(v) => write!(f, "{}u32", v),
            CopyableVal_::U64(v) => write!(f, "{}", v),
            CopyableVal_::U128(v) => write!(f, "{}u128", v),
            CopyableVal_::U256(v) => write!(f, "{}u256", v),
            CopyableVal_::Bool(v) => write!(f, "{}", v),
            CopyableVal_::ByteArray(v) => write!(f, "0b{}", hex::encode(v)),
            CopyableVal_::Address(v) => write!(f, "0x{}", hex::encode(v)),
        }
    }
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                UnaryOp::Not => "!",
            }
        )
    }
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Mod => "%",
                BinOp::Div => "/",
                BinOp::BitOr => "|",
                BinOp::BitAnd => "&",
                BinOp::Xor => "^",
                BinOp::Shl => "<<",
                BinOp::Shr => ">>",

                // Bool ops
                BinOp::Or => "||",
                BinOp::And => "&&",

                // Compare Ops
                BinOp::Eq => "==",
                BinOp::Neq => "!=",
                BinOp::Lt => "<",
                BinOp::Gt => ">",
                BinOp::Le => "<=",
                BinOp::Ge => ">=",
                BinOp::Subrange => "..",
            }
        )
    }
}

impl fmt::Display for Exp_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Exp_::Dereference(e) => write!(f, "*({})", e),
            Exp_::UnaryExp(o, e) => write!(f, "({}{})", o, e),
            Exp_::BinopExp(e1, o, e2) => write!(f, "({} {} {})", o, e1, e2),
            Exp_::Value(v) => write!(f, "{}", v),
            Exp_::Pack(n, tys, s) => write!(
                f,
                "{}{}{{{}}}",
                n,
                format_type_actuals(tys),
                s.iter().fold(String::new(), |acc, (field, op)| format!(
                    "{} {} : {},",
                    acc, field, op,
                ))
            ),
            Exp_::Borrow {
                is_mutable,
                exp,
                field,
            } => write!(
                f,
                "&{}{}.{}",
                if *is_mutable { "mut " } else { "" },
                exp,
                field
            ),
            Exp_::Move(v) => write!(f, "move({})", v),
            Exp_::Copy(v) => write!(f, "copy({})", v),
            Exp_::BorrowLocal(is_mutable, v) => {
                write!(f, "&{}{}", if *is_mutable { "mut " } else { "" }, v)
            }
            Exp_::FunctionCall(func, e) => write!(f, "{}({})", func, e),
            Exp_::ExprList(exps) => {
                if exps.is_empty() {
                    write!(f, "()")
                } else {
                    write!(f, "({})", intersperse(exps, ", "))
                }
            }
            Exp_::PackVariant(name, variant_name, tys, exps) => {
                write!(
                    f,
                    "{}::{}{}{{{}}}",
                    name,
                    variant_name,
                    format_type_actuals(tys),
                    exps.iter().fold(String::new(), |acc, (field, op)| format!(
                        "{} {} : {},",
                        acc, field, op,
                    ))
                )
            }
        }
    }
}

impl fmt::Display for Bytecode_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Bytecode_::Pop => write!(f, "Pop"),
            Bytecode_::Ret => write!(f, "Ret"),
            Bytecode_::Nop(None) => write!(f, "Nop"),
            Bytecode_::Nop(Some(s)) => write!(f, "Nop {}", &s.0),
            Bytecode_::BrTrue(lbl) => write!(f, "BrTrue {}", &lbl.0),
            Bytecode_::BrFalse(lbl) => write!(f, "BrFalse {}", &lbl.0),
            Bytecode_::Branch(lbl) => write!(f, "Branch {}", &lbl.0),
            Bytecode_::LdU8(u) => write!(f, "LdU8 {}", u),
            Bytecode_::LdU16(u) => write!(f, "LdU16 {}", u),
            Bytecode_::LdU32(u) => write!(f, "LdU32 {}", u),
            Bytecode_::LdU64(u) => write!(f, "LdU64 {}", u),
            Bytecode_::LdU128(u) => write!(f, "LdU128 {}", u),
            Bytecode_::LdU256(u) => write!(f, "LdU256 {}", u),
            Bytecode_::CastU8 => write!(f, "CastU8"),
            Bytecode_::CastU16 => write!(f, "CastU16"),
            Bytecode_::CastU32 => write!(f, "CastU32"),
            Bytecode_::CastU64 => write!(f, "CastU64"),
            Bytecode_::CastU128 => write!(f, "CastU128"),
            Bytecode_::CastU256 => write!(f, "CastU256"),
            Bytecode_::LdTrue => write!(f, "LdTrue"),
            Bytecode_::LdFalse => write!(f, "LdFalse"),
            Bytecode_::LdConst(ty, v) => write!(f, "LdConst<{}> {}", ty, format_move_value(v)),
            Bytecode_::LdNamedConst(n) => write!(f, "LdNamedConst {}", &n.0),
            Bytecode_::CopyLoc(v) => write!(f, "CopyLoc {}", v),
            Bytecode_::MoveLoc(v) => write!(f, "MoveLoc {}", v),
            Bytecode_::StLoc(v) => write!(f, "StLoc {}", v),
            Bytecode_::Call(m, n, tys) => write!(f, "Call {}.{}{}", m, n, format_type_actuals(tys)),
            Bytecode_::Pack(n, tys) => write!(f, "Pack {}{}", n, format_type_actuals(tys)),
            Bytecode_::Unpack(n, tys) => write!(f, "Unpack {}{}", n, format_type_actuals(tys)),
            Bytecode_::ReadRef => write!(f, "ReadRef"),
            Bytecode_::WriteRef => write!(f, "WriteRef"),
            Bytecode_::FreezeRef => write!(f, "FreezeRef"),
            Bytecode_::MutBorrowLoc(v) => write!(f, "MutBorrowLoc {}", v),
            Bytecode_::ImmBorrowLoc(v) => write!(f, "ImmBorrowLoc {}", v),
            Bytecode_::MutBorrowField(n, tys, field) => write!(
                f,
                "MutBorrowField {}{}.{}",
                n,
                format_type_actuals(tys),
                field
            ),
            Bytecode_::ImmBorrowField(n, tys, field) => write!(
                f,
                "ImmBorrowField {}{}.{}",
                n,
                format_type_actuals(tys),
                field
            ),
            Bytecode_::Add => write!(f, "Add"),
            Bytecode_::Sub => write!(f, "Sub"),
            Bytecode_::Mul => write!(f, "Mul"),
            Bytecode_::Mod => write!(f, "Mod"),
            Bytecode_::Div => write!(f, "Div"),
            Bytecode_::BitOr => write!(f, "BitOr"),
            Bytecode_::BitAnd => write!(f, "BitAnd"),
            Bytecode_::Xor => write!(f, "Xor"),
            Bytecode_::Or => write!(f, "Or"),
            Bytecode_::And => write!(f, "And"),
            Bytecode_::Not => write!(f, "Not"),
            Bytecode_::Eq => write!(f, "Eq"),
            Bytecode_::Neq => write!(f, "Neq"),
            Bytecode_::Lt => write!(f, "Lt"),
            Bytecode_::Gt => write!(f, "Gt"),
            Bytecode_::Le => write!(f, "Le"),
            Bytecode_::Ge => write!(f, "Ge"),
            Bytecode_::Abort => write!(f, "Abort"),
            Bytecode_::Shl => write!(f, "Shl"),
            Bytecode_::Shr => write!(f, "Shr"),
            Bytecode_::VecPack(ty, n) => write!(f, "VecPack {} {}", ty, n),
            Bytecode_::VecLen(ty) => write!(f, "VecLen {}", ty),
            Bytecode_::VecImmBorrow(ty) => write!(f, "VecImmBorrow {}", ty),
            Bytecode_::VecMutBorrow(ty) => write!(f, "VecMutBorrow {}", ty),
            Bytecode_::VecPushBack(ty) => write!(f, "VecPushBack {}", ty),
            Bytecode_::VecPopBack(ty) => write!(f, "VecPopBack {}", ty),
            Bytecode_::VecUnpack(ty, n) => write!(f, "VecUnpack {} {}", ty, n),
            Bytecode_::VecSwap(ty) => write!(f, "VecSwap {}", ty),
            Bytecode_::PackVariant(name, variant_name, tys) => {
                write!(
                    f,
                    "PackVariant {}::{}{}",
                    name,
                    variant_name,
                    format_type_actuals(tys)
                )
            }
            Bytecode_::UnpackVariant(name, variant_name, tys, unpack_type) => {
                write!(
                    f,
                    "UnpackVariant {}{}::{}{}",
                    unpack_type,
                    name,
                    variant_name,
                    format_type_actuals(tys)
                )
            }
            Bytecode_::VariantSwitch(name, lbls) => {
                write!(
                    f,
                    "VariantSwitch {}{}",
                    name,
                    lbls.iter()
                        .enumerate()
                        .fold(String::new(), |acc, (tag, (name, lbl))| format!(
                            "{} {}:{} => {},",
                            acc, name, tag, lbl
                        ))
                )
            }
        }
    }
}

fn format_move_value(v: &MoveValue) -> String {
    match v {
        MoveValue::U8(u) => format!("{}u8", u),
        MoveValue::U64(u) => format!("{}u64", u),
        MoveValue::U128(u) => format!("{}u128", u),
        MoveValue::Bool(true) => "true".to_owned(),
        MoveValue::Bool(false) => "false".to_owned(),
        MoveValue::Address(a) => format!("0x{}", a.short_str_lossless()),
        MoveValue::Vector(v) => {
            let items = v
                .iter()
                .map(format_move_value)
                .collect::<Vec<_>>()
                .join(", ");
            format!("vector[{}]", items)
        }
        MoveValue::Struct(_) | MoveValue::Signer(_) | MoveValue::Variant(_) => {
            panic!("Should be inexpressible as a constant")
        }
        MoveValue::U16(u) => format!("{}u16", u),
        MoveValue::U32(u) => format!("{}u32", u),
        MoveValue::U256(u) => format!("{}u256", u),
    }
}
