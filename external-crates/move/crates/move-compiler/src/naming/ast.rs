// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diagnostics::warning_filters::{WarningFilters, WarningFiltersTable},
    expansion::ast::{
        ability_constraints_ast_debug, ability_modifiers_ast_debug, AbilitySet, Attributes,
        DottedUsage, Fields, Friend, ImplicitUseFunCandidate, ModuleIdent, Mutability, Value,
        Value_, Visibility,
    },
    parser::ast::{
        self as P, Ability_, BinOp, ConstantName, DatatypeName, DocComment, Field, FunctionName,
        TargetKind, UnaryOp, VariantName, ENTRY_MODIFIER, MACRO_MODIFIER, NATIVE_MODIFIER,
    },
    shared::{
        ast_debug::*, known_attributes::SyntaxAttribute, program_info::NamingProgramInfo,
        unique_map::UniqueMap, *,
    },
};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use once_cell::sync::Lazy;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt,
    sync::Arc,
};

//**************************************************************************************************
// Program
//**************************************************************************************************

#[derive(Debug, Clone)]
pub struct Program {
    pub info: NamingProgramInfo,
    /// Safety: This table should not be dropped as long as any `WarningFilters` are alive
    pub warning_filters_table: Arc<WarningFiltersTable>,
    pub inner: Program_,
}

#[derive(Debug, Clone)]
pub struct Program_ {
    pub modules: UniqueMap<ModuleIdent, ModuleDefinition>,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum Neighbor_ {
    Dependency,
    Friend,
}
pub type Neighbor = Spanned<Neighbor_>;

//**************************************************************************************************
// Use Funs
//**************************************************************************************************

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseFunKind {
    Explicit,
    // From a function declaration in the module
    FunctionDeclaration,
    // From a normal, non 'use fun' use declaration,
    UseAlias,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UseFun {
    pub doc: DocComment,
    pub loc: Loc,
    pub attributes: Attributes,
    pub is_public: Option<Loc>,
    pub tname: TypeName,
    pub target_function: (ModuleIdent, FunctionName),
    // If None, disregard any use/unused information.
    // If Some, we track whether or not the associated function alias was used prior to receiver
    pub kind: UseFunKind,
    // Set to true on usage during typing on usage.
    // For UseAlias implicit use funs, this might already be set to true if it was used in a
    // non method syntax case
    pub used: bool,
}

// Mapping from type to their possible "methods"
pub type ResolvedUseFuns = BTreeMap<TypeName, UniqueMap<Name, UseFun>>;

// Color for scopes of use funs and variables
pub type Color = u16;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UseFuns {
    pub color: Color,
    pub resolved: ResolvedUseFuns,
    pub implicit_candidates: UniqueMap<Name, ImplicitUseFunCandidate>,
}

//**************************************************************************************************
// Syntax Methods
//**************************************************************************************************

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum SyntaxMethodKind_ {
    Index,
    IndexMut,
    // ForMut,
    // ForImm,
    // ForVal,
    // Assign,
}

pub type SyntaxMethodKind = Spanned<SyntaxMethodKind_>;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SyntaxMethod {
    pub loc: Loc,
    pub visibility: Visibility,
    pub tname: TypeName,
    pub target_function: (ModuleIdent, FunctionName),
    pub kind: SyntaxMethodKind,
    // We don't track usage because we require these to be public.
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexSyntaxMethods {
    pub index_mut: Option<Box<SyntaxMethod>>,
    pub index: Option<Box<SyntaxMethod>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct SyntaxMethodEntry {
    pub index: Option<Box<IndexSyntaxMethods>>,
}

// Mapping from type to their possible "syntax methods"
pub type SyntaxMethods = BTreeMap<TypeName, SyntaxMethodEntry>;

//**************************************************************************************************
// Modules
//**************************************************************************************************

#[derive(Debug, Clone)]
pub struct ModuleDefinition {
    pub doc: DocComment,
    pub loc: Loc,
    pub warning_filter: WarningFilters,
    // package name metadata from compiler arguments, not used for any language rules
    pub package_name: Option<Symbol>,
    pub attributes: Attributes,
    pub target_kind: TargetKind,
    pub use_funs: UseFuns,
    pub syntax_methods: SyntaxMethods,
    pub friends: UniqueMap<ModuleIdent, Friend>,
    pub structs: UniqueMap<DatatypeName, StructDefinition>,
    pub enums: UniqueMap<DatatypeName, EnumDefinition>,
    pub constants: UniqueMap<ConstantName, Constant>,
    pub functions: UniqueMap<FunctionName, Function>,
}

//**************************************************************************************************
// Data Types
//**************************************************************************************************

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct DatatypeTypeParameter {
    pub param: TParam,
    pub is_phantom: bool,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct StructDefinition {
    pub doc: DocComment,
    pub warning_filter: WarningFilters,
    // index in the original order as defined in the source file
    pub index: usize,
    pub loc: Loc,
    pub attributes: Attributes,
    pub abilities: AbilitySet,
    pub type_parameters: Vec<DatatypeTypeParameter>,
    pub fields: StructFields,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum StructFields {
    Defined(/* positional */ bool, Fields<(DocComment, Type)>),
    Native(Loc),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDefinition {
    pub doc: DocComment,
    pub warning_filter: WarningFilters,
    // index in the original order as defined in the source file
    pub index: usize,
    pub loc: Loc,
    pub attributes: Attributes,
    pub abilities: AbilitySet,
    pub type_parameters: Vec<DatatypeTypeParameter>,
    pub variants: UniqueMap<VariantName, VariantDefinition>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct VariantDefinition {
    pub doc: DocComment,
    // index in the original order as defined in the source file
    pub index: usize,
    pub loc: Loc,
    pub fields: VariantFields,
}

#[derive(Debug, PartialEq, Clone)]
pub enum VariantFields {
    Defined(/* positional */ bool, Fields<(DocComment, Type)>),
    Empty,
}

//**************************************************************************************************
// Functions
//**************************************************************************************************

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct FunctionSignature {
    pub type_parameters: Vec<TParam>,
    pub parameters: Vec<(Mutability, Var, Type)>,
    pub return_type: Type,
}

#[derive(PartialEq, Debug, Clone)]
pub enum FunctionBody_ {
    Defined(Sequence),
    Native,
}
pub type FunctionBody = Spanned<FunctionBody_>;

#[derive(PartialEq, Debug, Clone)]
pub struct Function {
    pub doc: DocComment,
    pub loc: Loc,
    pub warning_filter: WarningFilters,
    // index in the original order as defined in the source file
    pub index: usize,
    pub attributes: Attributes,
    pub visibility: Visibility,
    pub entry: Option<Loc>,
    pub macro_: Option<Loc>,
    pub signature: FunctionSignature,
    pub body: FunctionBody,
}

//**************************************************************************************************
// Constants
//**************************************************************************************************

#[derive(PartialEq, Debug, Clone)]
pub struct Constant {
    pub doc: DocComment,
    pub warning_filter: WarningFilters,
    // index in the original order as defined in the source file
    pub index: usize,
    pub attributes: Attributes,
    pub loc: Loc,
    pub signature: Type,
    pub value: Exp,
}

//**************************************************************************************************
// Types
//**************************************************************************************************

#[derive(Debug, PartialEq, Clone, Copy, PartialOrd, Eq, Ord)]
pub enum BuiltinTypeName_ {
    // address
    Address,
    // signer
    Signer,
    // u8
    U8,
    // u16
    U16,
    // u32
    U32,
    // u64
    U64,
    // u128
    U128,
    // u256
    U256,
    // Vector
    Vector,
    // bool
    Bool,
}
pub type BuiltinTypeName = Spanned<BuiltinTypeName_>;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum TypeName_ {
    // exp-list/tuple type
    Multiple(usize),
    Builtin(BuiltinTypeName),
    ModuleType(ModuleIdent, DatatypeName),
}
pub type TypeName = Spanned<TypeName_>;

#[derive(Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub struct TParamID(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TParam {
    pub id: TParamID,
    pub user_specified_name: Name,
    pub abilities: AbilitySet,
}

#[derive(Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub struct TVar(pub u64);

#[derive(Debug, Eq, PartialEq, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Type_ {
    Unit,
    Ref(bool, Box<Type>),
    Param(TParam),
    Apply(Option<AbilitySet>, TypeName, Vec<Type>),
    Fun(Vec<Type>, Box<Type>),
    Var(TVar),
    Anything,
    UnresolvedError,
}
pub type Type = Spanned<Type_>;

//**************************************************************************************************
// Expressions
//**************************************************************************************************

#[derive(Debug, Eq, PartialEq, Copy, Clone, PartialOrd, Ord)]
pub struct Var_ {
    pub name: Symbol,
    pub id: u16,
    pub color: Color,
}
pub type Var = Spanned<Var_>;

#[derive(Debug, PartialEq, Eq, Copy, Clone, PartialOrd, Ord)]
pub struct BlockLabel {
    pub label: Var,
    pub is_implicit: bool,
}

#[derive(Debug, PartialEq, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum LValue_ {
    Ignore,
    Var {
        mut_: Option<Mutability>,
        var: Var,
        unused_binding: bool,
    },
    Unpack(ModuleIdent, DatatypeName, Option<Vec<Type>>, Fields<LValue>),
    Error,
}
pub type LValue = Spanned<LValue_>;
pub type LValueList_ = Vec<LValue>;
pub type LValueList = Spanned<LValueList_>;

pub type LambdaLValues_ = Vec<(LValueList, Option<Type>)>;
pub type LambdaLValues = Spanned<LambdaLValues_>;

#[derive(Debug, PartialEq, Clone)]
pub enum ExpDotted_ {
    Exp(Box<Exp>),
    Dot(Box<ExpDotted>, /* dot loation */ Loc, Field),
    Index(Box<ExpDotted>, Spanned<Vec<Exp>>),
    DotAutocomplete(Loc, Box<ExpDotted>), // Dot (and its location) where Field could not be parsed
}
pub type ExpDotted = Spanned<ExpDotted_>;

#[derive(Debug, PartialEq, Eq, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum BuiltinFunction_ {
    Freeze(Option<Type>),
    Assert(/* is_macro */ Option<Loc>),
}
pub type BuiltinFunction = Spanned<BuiltinFunction_>;

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum NominalBlockUsage {
    Return,
    Break,
    Continue,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Lambda {
    pub parameters: LambdaLValues,
    pub return_type: Option<Type>,
    pub return_label: BlockLabel,
    pub use_fun_color: Color,
    pub body: Box<Exp>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Block {
    pub name: Option<BlockLabel>,
    pub from_macro_argument: Option<MacroArgument>,
    pub seq: Sequence,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MacroArgument {
    Lambda(Loc),
    Substituted(Loc),
}

#[derive(Debug, PartialEq, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Exp_ {
    Value(Value),
    Var(Var),
    Constant(ModuleIdent, ConstantName),

    ModuleCall(
        ModuleIdent,
        FunctionName,
        /* is_macro */ Option<Loc>,
        Option<Vec<Type>>,
        Spanned<Vec<Exp>>,
    ),
    MethodCall(
        ExpDotted,
        Loc, // location of the dot
        Name,
        /* is_macro */ Option<Loc>,
        Option<Vec<Type>>,
        Spanned<Vec<Exp>>,
    ),
    VarCall(Var, Spanned<Vec<Exp>>),
    Builtin(BuiltinFunction, Spanned<Vec<Exp>>),
    Vector(Loc, Option<Type>, Spanned<Vec<Exp>>),

    IfElse(Box<Exp>, Box<Exp>, Option<Box<Exp>>),
    Match(Box<Exp>, Spanned<Vec<MatchArm>>),
    While(BlockLabel, Box<Exp>, Box<Exp>),
    Loop(BlockLabel, Box<Exp>),
    Block(Block),
    Lambda(Lambda),

    Assign(LValueList, Box<Exp>),
    FieldMutate(ExpDotted, Box<Exp>),
    Mutate(Box<Exp>, Box<Exp>),

    Return(Box<Exp>),
    Abort(Box<Exp>),
    Give(NominalBlockUsage, BlockLabel, Box<Exp>),
    Continue(BlockLabel),

    Dereference(Box<Exp>),
    UnaryExp(UnaryOp, Box<Exp>),
    BinopExp(Box<Exp>, BinOp, Box<Exp>),

    Pack(ModuleIdent, DatatypeName, Option<Vec<Type>>, Fields<Exp>),
    PackVariant(
        ModuleIdent,
        DatatypeName,
        VariantName,
        Option<Vec<Type>>,
        Fields<Exp>,
    ),
    ExpList(Vec<Exp>),
    Unit {
        trailing: bool,
    },

    ExpDotted(DottedUsage, ExpDotted),

    Cast(Box<Exp>, Type),
    Annotate(Box<Exp>, Type),

    ErrorConstant {
        line_number_loc: Loc,
    },

    UnresolvedError,
}
pub type Exp = Spanned<Exp_>;

pub type Sequence = (UseFuns, VecDeque<SequenceItem>);
#[derive(Debug, PartialEq, Clone)]
pub enum SequenceItem_ {
    Seq(Box<Exp>),
    Declare(LValueList, Option<Type>),
    Bind(LValueList, Box<Exp>),
}
pub type SequenceItem = Spanned<SequenceItem_>;

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm_ {
    pub pattern: MatchPattern,
    pub binders: Vec<(Mutability, Var)>,
    pub guard: Option<Box<Exp>>,
    pub guard_binders: UniqueMap<Var, Var>, // pattern binder name -> guard var name
    pub rhs_binders: BTreeSet<Var>,         // pattern binders used in the right-hand side
    pub rhs: Box<Exp>,
}

pub type MatchArm = Spanned<MatchArm_>;

#[derive(Debug, Clone, PartialEq)]
pub enum MatchPattern_ {
    Variant(
        ModuleIdent,
        DatatypeName,
        VariantName,
        Option<Vec<Type>>,
        Fields<MatchPattern>,
    ),
    Struct(
        ModuleIdent,
        DatatypeName,
        Option<Vec<Type>>,
        Fields<MatchPattern>,
    ),
    Constant(ModuleIdent, ConstantName),
    Binder(Mutability, Var, /* unused binding */ bool),
    Literal(Value),
    Wildcard,
    Or(Box<MatchPattern>, Box<MatchPattern>),
    At(Var, /* unused binding */ bool, Box<MatchPattern>),
    ErrorPat,
}

pub type MatchPattern = Spanned<MatchPattern_>;

//**************************************************************************************************
// traits
//**************************************************************************************************

impl TName for Var {
    type Key = Var_;

    type Loc = Loc;

    fn drop_loc(self) -> (Self::Loc, Self::Key) {
        let sp!(loc, value) = self;
        (loc, value)
    }

    fn add_loc(loc: Self::Loc, key: Self::Key) -> Self {
        sp(loc, key)
    }

    fn borrow(&self) -> (&Self::Loc, &Self::Key) {
        let sp!(loc, value) = self;
        (loc, value)
    }
}

//**************************************************************************************************
// impls
//**************************************************************************************************

impl UseFuns {
    pub fn new(color: Color) -> Self {
        Self {
            color,
            resolved: BTreeMap::new(),
            implicit_candidates: UniqueMap::new(),
        }
    }
}

impl IndexSyntaxMethods {
    pub fn get_name_for_typing(&self) -> Option<(ModuleIdent, FunctionName)> {
        // We prefer `index` over `index_mut` because its type is subject and return type are higher
        // in the subtyping lattice.
        if let Some(index) = &self.index {
            Some(index.target_function)
        } else {
            self.index_mut
                .as_ref()
                .map(|index_mut| index_mut.target_function)
        }
    }
}

impl SyntaxMethodEntry {
    pub fn lookup_kind_entry<'entry>(
        &'entry mut self,
        sp!(_, kind): &SyntaxMethodKind,
    ) -> &'entry mut Option<Box<SyntaxMethod>> {
        match kind {
            SyntaxMethodKind_::Index => &mut self.index_entry().index,
            SyntaxMethodKind_::IndexMut => &mut self.index_entry().index_mut,
        }
    }

    fn index_entry(&mut self) -> &mut IndexSyntaxMethods {
        if self.index.is_none() {
            let new_index_syntax_method = IndexSyntaxMethods {
                index: None,
                index_mut: None,
            };
            self.index = Some(Box::new(new_index_syntax_method));
        }
        self.index.as_mut().unwrap()
    }
}

static BUILTIN_TYPE_ALL_NAMES: Lazy<BTreeSet<Symbol>> = Lazy::new(|| {
    [
        BuiltinTypeName_::ADDRESS,
        BuiltinTypeName_::SIGNER,
        BuiltinTypeName_::U_8,
        BuiltinTypeName_::U_16,
        BuiltinTypeName_::U_32,
        BuiltinTypeName_::U_64,
        BuiltinTypeName_::U_128,
        BuiltinTypeName_::U_256,
        BuiltinTypeName_::BOOL,
        BuiltinTypeName_::VECTOR,
    ]
    .into_iter()
    .map(Symbol::from)
    .collect()
});

static BUILTIN_TYPE_NUMERIC: Lazy<BTreeSet<BuiltinTypeName_>> = Lazy::new(|| {
    [
        BuiltinTypeName_::U8,
        BuiltinTypeName_::U16,
        BuiltinTypeName_::U32,
        BuiltinTypeName_::U64,
        BuiltinTypeName_::U128,
        BuiltinTypeName_::U256,
    ]
    .into_iter()
    .collect()
});

static BUILTIN_TYPE_BITS: Lazy<BTreeSet<BuiltinTypeName_>> =
    Lazy::new(|| BUILTIN_TYPE_NUMERIC.clone());

static BUILTIN_TYPE_ORDERED: Lazy<BTreeSet<BuiltinTypeName_>> =
    Lazy::new(|| BUILTIN_TYPE_BITS.clone());

impl BuiltinTypeName_ {
    pub const ADDRESS: &'static str = "address";
    pub const SIGNER: &'static str = "signer";
    pub const U_8: &'static str = "u8";
    pub const U_16: &'static str = "u16";
    pub const U_32: &'static str = "u32";
    pub const U_64: &'static str = "u64";
    pub const U_128: &'static str = "u128";
    pub const U_256: &'static str = "u256";
    pub const BOOL: &'static str = "bool";
    pub const VECTOR: &'static str = "vector";

    pub fn all_names() -> &'static BTreeSet<Symbol> {
        &BUILTIN_TYPE_ALL_NAMES
    }

    pub fn numeric() -> &'static BTreeSet<BuiltinTypeName_> {
        &BUILTIN_TYPE_NUMERIC
    }

    pub fn bits() -> &'static BTreeSet<BuiltinTypeName_> {
        &BUILTIN_TYPE_BITS
    }

    pub fn ordered() -> &'static BTreeSet<BuiltinTypeName_> {
        &BUILTIN_TYPE_ORDERED
    }

    pub fn is_numeric(&self) -> bool {
        Self::numeric().contains(self)
    }

    pub fn resolve(name_str: &str) -> Option<Self> {
        use BuiltinTypeName_ as BT;
        match name_str {
            BT::ADDRESS => Some(BT::Address),
            BT::SIGNER => Some(BT::Signer),
            BT::U_8 => Some(BT::U8),
            BT::U_16 => Some(BT::U16),
            BT::U_32 => Some(BT::U32),
            BT::U_64 => Some(BT::U64),
            BT::U_128 => Some(BT::U128),
            BT::U_256 => Some(BT::U256),
            BT::BOOL => Some(BT::Bool),
            BT::VECTOR => Some(BT::Vector),
            _ => None,
        }
    }

    pub fn declared_abilities(&self, loc: Loc) -> AbilitySet {
        use BuiltinTypeName_ as B;
        // Match here to make sure this function is fixed when collections are added
        match self {
            B::Address | B::U8 | B::U16 | B::U32 | B::U64 | B::U128 | B::U256 | B::Bool => {
                AbilitySet::primitives(loc)
            }
            B::Signer => AbilitySet::signer(loc),
            B::Vector => AbilitySet::collection(loc),
        }
    }

    pub fn tparam_constraints(&self, _loc: Loc) -> Vec<AbilitySet> {
        use BuiltinTypeName_ as B;
        // Match here to make sure this function is fixed when collections are added
        match self {
            B::Address
            | B::Signer
            | B::U8
            | B::U16
            | B::U32
            | B::U64
            | B::U128
            | B::U256
            | B::Bool => vec![],
            B::Vector => vec![AbilitySet::empty()],
        }
    }
}

impl TParamID {
    pub fn next() -> TParamID {
        TParamID(Counter::next())
    }
}

static BUILTIN_FUNCTION_ALL_NAMES: Lazy<BTreeSet<Symbol>> = Lazy::new(|| {
    [BuiltinFunction_::FREEZE, BuiltinFunction_::ASSERT_MACRO]
        .into_iter()
        .map(Symbol::from)
        .collect()
});

impl BuiltinFunction_ {
    pub const FREEZE: &'static str = "freeze";
    pub const ASSERT_MACRO: &'static str = "assert";

    pub fn all_names() -> &'static BTreeSet<Symbol> {
        &BUILTIN_FUNCTION_ALL_NAMES
    }

    pub fn resolve(name_str: &str, arg: Option<Type>) -> Option<Self> {
        use BuiltinFunction_ as BF;
        match name_str {
            BF::FREEZE => Some(BF::Freeze(arg)),
            _ => None,
        }
    }

    pub fn display_name(&self) -> &'static str {
        use BuiltinFunction_ as BF;
        match self {
            BF::Freeze(_) => BF::FREEZE,
            BF::Assert(_) => BF::ASSERT_MACRO,
        }
    }
}

impl TypeName_ {
    pub fn is<Addr>(&self, address: &Addr, module: impl AsRef<str>, name: impl AsRef<str>) -> bool
    where
        NumericalAddress: PartialEq<Addr>,
    {
        match self {
            TypeName_::Builtin(_) | TypeName_::Multiple(_) => false,
            TypeName_::ModuleType(mident, n) => {
                mident.value.is(address, module) && n == name.as_ref()
            }
        }
    }

    pub fn single_type(&self) -> Option<TypeName_> {
        match self {
            TypeName_::Multiple(_) => None,
            TypeName_::Builtin(_) | TypeName_::ModuleType(_, _) => Some(self.clone()),
        }
    }

    pub fn datatype_name(&self) -> Option<(ModuleIdent, DatatypeName)> {
        match self {
            TypeName_::Builtin(_) | TypeName_::Multiple(_) => None,
            TypeName_::ModuleType(mident, n) => Some((*mident, *n)),
        }
    }
}

impl Type_ {
    pub fn builtin_(b: BuiltinTypeName, ty_args: Vec<Type>) -> Type_ {
        use BuiltinTypeName_ as B;
        let abilities = match &b.value {
            B::Address | B::U8 | B::U16 | B::U32 | B::U64 | B::U128 | B::U256 | B::Bool => {
                Some(AbilitySet::primitives(b.loc))
            }
            B::Signer => Some(AbilitySet::signer(b.loc)),
            B::Vector => None,
        };
        let n = sp(b.loc, TypeName_::Builtin(b));
        Type_::Apply(abilities, n, ty_args)
    }

    pub fn builtin(loc: Loc, b: BuiltinTypeName, ty_args: Vec<Type>) -> Type {
        sp(loc, Self::builtin_(b, ty_args))
    }

    pub fn bool(loc: Loc) -> Type {
        Self::builtin(loc, sp(loc, BuiltinTypeName_::Bool), vec![])
    }

    pub fn address(loc: Loc) -> Type {
        Self::builtin(loc, sp(loc, BuiltinTypeName_::Address), vec![])
    }

    pub fn signer(loc: Loc) -> Type {
        Self::builtin(loc, sp(loc, BuiltinTypeName_::Signer), vec![])
    }

    pub fn u8(loc: Loc) -> Type {
        Self::builtin(loc, sp(loc, BuiltinTypeName_::U8), vec![])
    }

    pub fn u16(loc: Loc) -> Type {
        Self::builtin(loc, sp(loc, BuiltinTypeName_::U16), vec![])
    }

    pub fn u32(loc: Loc) -> Type {
        Self::builtin(loc, sp(loc, BuiltinTypeName_::U32), vec![])
    }

    pub fn u64(loc: Loc) -> Type {
        Self::builtin(loc, sp(loc, BuiltinTypeName_::U64), vec![])
    }

    pub fn u128(loc: Loc) -> Type {
        Self::builtin(loc, sp(loc, BuiltinTypeName_::U128), vec![])
    }

    pub fn u256(loc: Loc) -> Type {
        Self::builtin(loc, sp(loc, BuiltinTypeName_::U256), vec![])
    }

    pub fn vector(loc: Loc, elem: Type) -> Type {
        Self::builtin(loc, sp(loc, BuiltinTypeName_::Vector), vec![elem])
    }

    pub fn multiple(loc: Loc, tys: Vec<Type>) -> Type {
        sp(loc, Self::multiple_(loc, tys))
    }

    pub fn multiple_(loc: Loc, mut tys: Vec<Type>) -> Type_ {
        match tys.len() {
            0 => Type_::Unit,
            1 => tys.pop().unwrap().value,
            n => Type_::Apply(None, sp(loc, TypeName_::Multiple(n)), tys),
        }
    }

    pub fn builtin_name(&self) -> Option<&BuiltinTypeName> {
        match self {
            Type_::Apply(_, sp!(_, TypeName_::Builtin(b)), _) => Some(b),
            _ => None,
        }
    }

    pub fn type_name(&self) -> Option<&TypeName> {
        match self {
            Type_::Apply(_, tn, _) => Some(tn),
            _ => None,
        }
    }

    pub fn unfold_to_builtin_type_name(&self) -> Option<&BuiltinTypeName> {
        match self {
            Type_::Apply(_, sp!(_, TypeName_::Builtin(b)), _) => Some(b),
            Type_::Ref(_, inner) => inner.value.unfold_to_builtin_type_name(),
            _ => None,
        }
    }

    pub fn unfold_to_type_name(&self) -> Option<&TypeName> {
        match self {
            Type_::Apply(_, tn, _) => Some(tn),
            Type_::Ref(_, inner) => inner.value.unfold_to_type_name(),
            _ => None,
        }
    }

    pub fn type_arguments(&self) -> Option<&Vec<Type>> {
        match self {
            Type_::Apply(_, _, tyargs) => Some(tyargs),
            Type_::Ref(_, inner) => inner.value.type_arguments(),
            _ => None,
        }
    }

    pub fn is<Addr>(&self, address: &Addr, module: impl AsRef<str>, name: impl AsRef<str>) -> bool
    where
        NumericalAddress: PartialEq<Addr>,
    {
        self.type_name()
            .is_some_and(|tn| tn.value.is(address, module, name))
    }

    pub fn abilities(&self, loc: Loc) -> Option<AbilitySet> {
        match self {
            Type_::Apply(abilities, _, _) => abilities.clone(),
            Type_::Param(tp) => Some(tp.abilities.clone()),
            Type_::Unit => Some(AbilitySet::collection(loc)),
            Type_::Ref(_, _) => Some(AbilitySet::references(loc)),
            Type_::Anything | Type_::UnresolvedError => Some(AbilitySet::all(loc)),
            Type_::Fun(_, _) => Some(AbilitySet::functions(loc)),
            Type_::Var(_) => None,
        }
    }

    pub fn has_ability_(&self, ability: Ability_) -> Option<bool> {
        match self {
            Type_::Apply(abilities, _, _) => abilities.as_ref().map(|s| s.has_ability_(ability)),
            Type_::Param(tp) => Some(tp.abilities.has_ability_(ability)),
            Type_::Unit => Some(AbilitySet::COLLECTION.contains(&ability)),
            Type_::Ref(_, _) => Some(AbilitySet::REFERENCES.contains(&ability)),
            Type_::Anything | Type_::UnresolvedError => Some(true),
            Type_::Fun(_, _) => Some(AbilitySet::FUNCTIONS.contains(&ability)),
            Type_::Var(_) => None,
        }
    }

    // Returns an option holding the ref's mutability (or None, if it is not a reference type).
    // Also return None for `Anything`, `Var`, or other values that might be compatible wifh `Ref`
    // types.
    pub fn is_ref(&self) -> Option<bool> {
        match self {
            Type_::Ref(mut_, _) => Some(*mut_),
            Type_::Unit
            | Type_::Param(_)
            | Type_::Apply(_, _, _)
            | Type_::Fun(_, _)
            | Type_::Var(_)
            | Type_::Anything
            | Type_::UnresolvedError => None,
        }
    }

    // Unwraps refs
    pub fn base_type_(&self) -> Self {
        match self {
            Type_::Ref(_, inner) => inner.value.clone(),
            Type_::Unit
            | Type_::Param(_)
            | Type_::Apply(_, _, _)
            | Type_::Fun(_, _)
            | Type_::Var(_)
            | Type_::Anything
            | Type_::UnresolvedError => self.clone(),
        }
    }
}

impl Var_ {
    pub fn starts_with_underscore(&self) -> bool {
        P::Var::starts_with_underscore_name(self.name)
    }

    pub fn is_syntax_identifier(&self) -> bool {
        P::Var::is_syntax_identifier_name(self.name)
    }

    pub fn is_valid(&self) -> bool {
        P::Var::is_valid_name(self.name)
    }
}

impl BlockLabel {
    // base symbol to used when making names for unnamed loops or lambdas
    pub const IMPLICIT_LABEL_SYMBOL: Symbol = symbol!("%implicit");
    pub const MACRO_RETURN_NAME_SYMBOL: Symbol = symbol!("%macro");
}

impl Value_ {
    pub fn type_(&self, loc: Loc) -> Option<Type> {
        use Value_::*;
        Some(match self {
            Address(_) => Type_::address(loc),
            InferredNum(_) => return None,
            U8(_) => Type_::u8(loc),
            U16(_) => Type_::u16(loc),
            U32(_) => Type_::u32(loc),
            U64(_) => Type_::u64(loc),
            U128(_) => Type_::u128(loc),
            U256(_) => Type_::u256(loc),
            Bool(_) => Type_::bool(loc),
            Bytearray(_) => Type_::vector(loc, Type_::u8(loc)),
        })
    }
}

//**************************************************************************************************
// Display
//**************************************************************************************************

impl fmt::Display for BuiltinTypeName_ {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        use BuiltinTypeName_ as BT;
        write!(
            f,
            "{}",
            match self {
                BT::Address => BT::ADDRESS,
                BT::Signer => BT::SIGNER,
                BT::U8 => BT::U_8,
                BT::U16 => BT::U_16,
                BT::U32 => BT::U_32,
                BT::U64 => BT::U_64,
                BT::U128 => BT::U_128,
                BT::U256 => BT::U_256,
                BT::Bool => BT::BOOL,
                BT::Vector => BT::VECTOR,
            }
        )
    }
}

impl fmt::Display for TypeName_ {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        use TypeName_::*;
        match self {
            Multiple(_) => panic!("ICE cannot display expr-list type name"),
            Builtin(b) => write!(f, "{}", b),
            ModuleType(m, n) => write!(f, "{}::{}", m, n),
        }
    }
}

impl std::fmt::Display for NominalBlockUsage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                NominalBlockUsage::Return => "return",
                NominalBlockUsage::Break => "break",
                NominalBlockUsage::Continue => "continue",
            }
        )
    }
}

impl fmt::Display for SyntaxMethodKind_ {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        let msg = match self {
            SyntaxMethodKind_::IndexMut | SyntaxMethodKind_::Index => SyntaxAttribute::INDEX,
        };
        write!(f, "{}", msg)
    }
}

//**************************************************************************************************
// Debug
//**************************************************************************************************

impl AstDebug for Program {
    fn ast_debug(&self, w: &mut AstWriter) {
        self.inner.ast_debug(w)
    }
}

impl AstDebug for Program_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Self { modules } = self;
        for (m, mdef) in modules.key_cloned_iter() {
            w.write(format!("module {}", m));
            w.block(|w| mdef.ast_debug(w));
            w.new_line();
        }
    }
}

impl AstDebug for Neighbor_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            Neighbor_::Dependency => w.write("neighbor#dependency"),
            Neighbor_::Friend => w.write("neighbor#friend"),
        }
    }
}

impl AstDebug for UseFun {
    fn ast_debug(&self, w: &mut AstWriter) {
        let UseFun {
            doc,
            loc: _,
            attributes,
            is_public,
            tname: _,
            target_function: (target_m, target_f),
            kind,
            used,
        } = self;
        doc.ast_debug(w);
        attributes.ast_debug(w);
        w.new_line();
        if is_public.is_some() {
            w.write("public ")
        }
        let kind_str = match kind {
            UseFunKind::Explicit => "#explicit",
            UseFunKind::UseAlias => "#use-alias",
            UseFunKind::FunctionDeclaration => "#fundecl",
        };
        let usage = if *used { "#used" } else { "#unused" };
        w.write(format!("use{kind_str}{usage} {target_m}::{target_f}"));
    }
}

impl AstDebug for (&TypeName, &UniqueMap<Name, UseFun>) {
    fn ast_debug(&self, w: &mut AstWriter) {
        let (tn, methods) = *self;
        for (_, method_f, use_fun) in methods {
            use_fun.ast_debug(w);
            w.write(" as ");
            tn.ast_debug(w);
            w.writeln(format!(".{method_f};"));
        }
    }
}

impl AstDebug for ResolvedUseFuns {
    fn ast_debug(&self, w: &mut AstWriter) {
        for (tn, methods) in self {
            (tn, methods).ast_debug(w);
        }
    }
}

impl AstDebug for UseFuns {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Self {
            color,
            resolved,
            implicit_candidates,
        } = self;
        w.write(format!("use_funs#{} ", color));
        resolved.ast_debug(w);
        if !implicit_candidates.is_empty() {
            w.write("unresolved ");
            w.block(|w| {
                for (_, _, implicit) in implicit_candidates {
                    implicit.ast_debug(w)
                }
            });
        }
        w.new_line();
    }
}

impl AstDebug for SyntaxMethod {
    fn ast_debug(&self, w: &mut AstWriter) {
        let SyntaxMethod {
            loc: _,
            tname,
            target_function: (target_m, target_f),
            visibility: _,
            kind,
        } = self;
        let kind_str = format!("{:?}", kind.value);
        w.write(format!(
            "syntax({kind_str}) for {tname} -> {target_m}::{target_f}\n"
        ));
    }
}

impl AstDebug for (&TypeName, &SyntaxMethodEntry) {
    fn ast_debug(&self, w: &mut AstWriter) {
        let (_tn, methods) = *self;
        let SyntaxMethodEntry { index } = methods;
        if let Some(index) = &index {
            let IndexSyntaxMethods { index_mut, index } = &**index;
            if let Some(index) = index.as_ref() {
                index.ast_debug(w)
            }
            if let Some(index_mut) = index_mut.as_ref() {
                index_mut.ast_debug(w)
            }
        }
    }
}

impl AstDebug for SyntaxMethods {
    fn ast_debug(&self, w: &mut AstWriter) {
        for entry in self {
            entry.ast_debug(w);
        }
    }
}

impl AstDebug for ModuleDefinition {
    fn ast_debug(&self, w: &mut AstWriter) {
        let ModuleDefinition {
            doc,
            loc: _,
            warning_filter,
            package_name,
            attributes,
            target_kind,
            use_funs,
            syntax_methods,
            friends,
            structs,
            enums,
            constants,
            functions,
        } = self;
        doc.ast_debug(w);
        warning_filter.ast_debug(w);
        if let Some(n) = package_name {
            w.writeln(format!("{}", n))
        }
        attributes.ast_debug(w);
        target_kind.ast_debug(w);
        use_funs.ast_debug(w);
        syntax_methods.ast_debug(w);
        for (mident, _loc) in friends.key_cloned_iter() {
            w.write(format!("friend {};", mident));
            w.new_line();
        }
        for sdef in structs.key_cloned_iter() {
            sdef.ast_debug(w);
            w.new_line();
        }
        for edef in enums.key_cloned_iter() {
            edef.ast_debug(w);
            w.new_line();
        }
        for cdef in constants.key_cloned_iter() {
            cdef.ast_debug(w);
            w.new_line();
        }
        for fdef in functions.key_cloned_iter() {
            fdef.ast_debug(w);
            w.new_line();
        }
    }
}

impl AstDebug for (DatatypeName, &StructDefinition) {
    fn ast_debug(&self, w: &mut AstWriter) {
        let (
            name,
            StructDefinition {
                doc,
                warning_filter,
                index,
                loc: _,
                attributes,
                abilities,
                type_parameters,
                fields,
            },
        ) = self;
        doc.ast_debug(w);
        warning_filter.ast_debug(w);
        attributes.ast_debug(w);
        if let StructFields::Native(_) = fields {
            w.write("native ");
        }
        w.write(format!("struct#{index} {name}"));
        type_parameters.ast_debug(w);
        ability_modifiers_ast_debug(w, abilities);
        if let StructFields::Defined(is_positional, fields) = fields {
            if *is_positional {
                w.write("#positional");
            }
            w.block(|w| {
                w.list(fields, ",", |w, (_, f, idx_st)| {
                    let (idx, (doc, st)) = idx_st;
                    doc.ast_debug(w);
                    w.write(format!("{}#{}: ", idx, f));
                    st.ast_debug(w);
                    true
                })
            })
        }
    }
}

impl AstDebug for (DatatypeName, &EnumDefinition) {
    fn ast_debug(&self, w: &mut AstWriter) {
        let (
            name,
            EnumDefinition {
                doc,
                index,
                loc: _,
                attributes,
                abilities,
                type_parameters,
                variants,
                warning_filter,
            },
        ) = self;
        doc.ast_debug(w);
        warning_filter.ast_debug(w);
        attributes.ast_debug(w);

        w.write(format!("enum#{index} {name}"));
        type_parameters.ast_debug(w);
        ability_modifiers_ast_debug(w, abilities);
        w.block(|w| {
            for variant in variants.key_cloned_iter() {
                variant.ast_debug(w);
            }
        });
    }
}

impl AstDebug for (VariantName, &VariantDefinition) {
    fn ast_debug(&self, w: &mut AstWriter) {
        let (
            name,
            VariantDefinition {
                doc,
                index,
                fields,
                loc: _,
            },
        ) = self;

        doc.ast_debug(w);
        w.write(format!("variant#{index} {name}"));
        match fields {
            VariantFields::Defined(is_positional, fields) => {
                if *is_positional {
                    w.write("#positional");
                }
                w.block(|w| {
                    w.list(fields, ",", |w, (_, f, idx_st)| {
                        let (idx, (doc, st)) = idx_st;
                        doc.ast_debug(w);
                        w.write(format!("{}#{}: ", idx, f));
                        st.ast_debug(w);
                        true
                    });
                })
            }
            VariantFields::Empty => (),
        }
    }
}

impl AstDebug for (FunctionName, &Function) {
    fn ast_debug(&self, w: &mut AstWriter) {
        let (
            name,
            Function {
                doc,
                loc: _,
                warning_filter,
                index,
                attributes,
                visibility,
                macro_,
                entry,
                signature,
                body,
            },
        ) = self;
        doc.ast_debug(w);
        warning_filter.ast_debug(w);
        attributes.ast_debug(w);
        visibility.ast_debug(w);
        if entry.is_some() {
            w.write(format!("{} ", ENTRY_MODIFIER));
        }
        if macro_.is_some() {
            w.write(format!("{} ", MACRO_MODIFIER));
        }
        if let FunctionBody_::Native = &body.value {
            w.write(format!("{} ", NATIVE_MODIFIER));
        }
        w.write(format!("fun#{index} {name}"));
        signature.ast_debug(w);
        match &body.value {
            FunctionBody_::Defined(body) => body.ast_debug(w),
            FunctionBody_::Native => w.writeln(";"),
        }
    }
}

impl AstDebug for FunctionSignature {
    fn ast_debug(&self, w: &mut AstWriter) {
        let FunctionSignature {
            type_parameters,
            parameters,
            return_type,
        } = self;
        type_parameters.ast_debug(w);
        w.write("(");
        w.comma(parameters, |w, (mut_, v, st)| {
            mut_.ast_debug(w);
            v.ast_debug(w);
            w.write(": ");
            st.ast_debug(w);
        });
        w.write("): ");
        return_type.ast_debug(w)
    }
}

impl AstDebug for Var_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Self { name, id, color } = self;
        let id = *id;
        let color = *color;
        w.write(format!("{name}"));
        if id != 0 {
            w.write(format!("#{id}"));
        }
        if color != 0 {
            w.write(format!("#{color}"));
        }
    }
}

impl AstDebug for BlockLabel {
    fn ast_debug(&self, w: &mut AstWriter) {
        let BlockLabel {
            is_implicit: _,
            label: sp!(_, Var_ { name, id, color }),
        } = self;
        w.write(format!("'{name}"));
        if *id != 0 {
            w.write(format!("#{id}"));
        }
        if *color != 0 {
            w.write(format!("#{color}"));
        }
    }
}

impl AstDebug for Vec<TParam> {
    fn ast_debug(&self, w: &mut AstWriter) {
        if !self.is_empty() {
            w.write("<");
            w.comma(self, |w, tp| tp.ast_debug(w));
            w.write(">")
        }
    }
}

impl AstDebug for Vec<DatatypeTypeParameter> {
    fn ast_debug(&self, w: &mut AstWriter) {
        if !self.is_empty() {
            w.write("<");
            w.comma(self, |w, tp| tp.ast_debug(w));
            w.write(">")
        }
    }
}

impl AstDebug for (ConstantName, &Constant) {
    fn ast_debug(&self, w: &mut AstWriter) {
        let (
            name,
            Constant {
                doc,
                warning_filter,
                index,
                attributes,
                loc: _loc,
                signature,
                value,
            },
        ) = self;
        doc.ast_debug(w);
        warning_filter.ast_debug(w);
        attributes.ast_debug(w);
        w.write(format!("const#{index} {name}:"));
        signature.ast_debug(w);
        w.write(" = ");
        value.ast_debug(w);
        w.write(";");
    }
}

impl AstDebug for BuiltinTypeName_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write(format!("{}", self));
    }
}

impl AstDebug for TypeName_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            TypeName_::Multiple(len) => w.write(format!("Multiple({})", len)),
            TypeName_::Builtin(bt) => bt.ast_debug(w),
            TypeName_::ModuleType(m, s) => w.write(format!("{}::{}", m, s)),
        }
    }
}

impl AstDebug for TParam {
    fn ast_debug(&self, w: &mut AstWriter) {
        let TParam {
            id,
            user_specified_name,
            abilities,
        } = self;
        w.write(format!("{}#{}", user_specified_name, id.0));
        ability_constraints_ast_debug(w, abilities);
    }
}

impl AstDebug for DatatypeTypeParameter {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Self { is_phantom, param } = self;
        if *is_phantom {
            w.write("phantom ");
        }
        param.ast_debug(w);
    }
}

impl AstDebug for Type_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            Type_::Unit => w.write("()"),
            Type_::Ref(mut_, s) => {
                w.write("&");
                if *mut_ {
                    w.write("mut ");
                }
                s.ast_debug(w)
            }
            Type_::Param(tp) => tp.ast_debug(w),
            Type_::Apply(abilities_opt, sp!(_, TypeName_::Multiple(_)), ss) => {
                let w_ty = move |w: &mut AstWriter| {
                    w.write("(");
                    ss.ast_debug(w);
                    w.write(")");
                };
                match abilities_opt {
                    None => w_ty(w),
                    Some(abilities) => w.annotate_gen(w_ty, abilities, |w, annot| {
                        w.list(annot, "+", |w, a| {
                            a.ast_debug(w);
                            false
                        })
                    }),
                }
            }
            Type_::Apply(abilities_opt, m, ss) => {
                let w_ty = move |w: &mut AstWriter| {
                    m.ast_debug(w);
                    if !ss.is_empty() {
                        w.write("<");
                        ss.ast_debug(w);
                        w.write(">");
                    }
                };
                match abilities_opt {
                    None => w_ty(w),
                    Some(abilities) => w.annotate_gen(w_ty, abilities, |w, annot| {
                        w.list(annot, "+", |w, a| {
                            a.ast_debug(w);
                            false
                        })
                    }),
                }
            }
            Type_::Fun(args, result) => {
                w.write("|");
                w.comma(args, |w, ty| ty.ast_debug(w));
                w.write("|");
                result.ast_debug(w);
            }
            Type_::Var(tv) => w.write(format!("#{}", tv.0)),
            Type_::Anything => w.write("_"),
            Type_::UnresolvedError => w.write("_|_"),
        }
    }
}

impl AstDebug for Vec<Type> {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.comma(self, |w, s| s.ast_debug(w))
    }
}

impl AstDebug for Sequence {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.block(|w| {
            let (use_funs, items) = self;
            use_funs.ast_debug(w);
            w.semicolon(items, |w, item| item.ast_debug(w))
        })
    }
}

impl AstDebug for SequenceItem_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        use SequenceItem_ as I;
        match self {
            I::Seq(e) => e.ast_debug(w),
            I::Declare(sp!(_, bs), ty_opt) => {
                w.write("let ");
                bs.ast_debug(w);
                if let Some(ty) = ty_opt {
                    ty.ast_debug(w)
                }
            }
            I::Bind(sp!(_, bs), e) => {
                w.write("let ");
                bs.ast_debug(w);
                w.write(" = ");
                e.ast_debug(w);
            }
        }
    }
}

impl AstDebug for Exp_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        use Exp_ as E;
        match self {
            E::Unit { trailing } if !trailing => w.write("()"),
            E::Unit {
                trailing: _trailing,
            } => w.write("/*()*/"),
            E::Value(v) => v.ast_debug(w),
            E::Var(v) => v.ast_debug(w),
            E::Constant(m, c) => w.write(format!("{}::{}", m, c)),
            E::ModuleCall(m, f, is_macro, tys_opt, sp!(_, rhs)) => {
                w.write(format!("{}::{}", m, f));
                if is_macro.is_some() {
                    w.write("!");
                }
                if let Some(ss) = tys_opt {
                    w.write("<");
                    ss.ast_debug(w);
                    w.write(">");
                }
                w.write("(");
                w.comma(rhs, |w, e| e.ast_debug(w));
                w.write(")");
            }
            E::MethodCall(e, _, f, is_macro, tys_opt, sp!(_, rhs)) => {
                e.ast_debug(w);
                w.write(format!(".{}", f));
                if is_macro.is_some() {
                    w.write("!");
                }
                if let Some(ss) = tys_opt {
                    w.write("<");
                    ss.ast_debug(w);
                    w.write(">");
                }
                w.write("(");
                w.comma(rhs, |w, e| e.ast_debug(w));
                w.write(")");
            }
            E::VarCall(var, sp!(_, rhs)) => {
                var.ast_debug(w);
                w.write("(");
                w.comma(rhs, |w, e| e.ast_debug(w));
                w.write(")");
            }
            E::Builtin(bf, sp!(_, rhs)) => {
                bf.ast_debug(w);
                w.write("(");
                w.comma(rhs, |w, e| e.ast_debug(w));
                w.write(")");
            }
            E::Vector(_loc, ty_opt, sp!(_, elems)) => {
                w.write("vector");
                if let Some(ty) = ty_opt {
                    w.write("<");
                    ty.ast_debug(w);
                    w.write(">");
                }
                w.write("[");
                w.comma(elems, |w, e| e.ast_debug(w));
                w.write("]");
            }
            E::Pack(m, s, tys_opt, fields) => {
                w.write(format!("{}::{}", m, s));
                if let Some(ss) = tys_opt {
                    w.write("<");
                    ss.ast_debug(w);
                    w.write(">");
                }
                w.write("{");
                w.comma(fields, |w, (_, f, idx_e)| {
                    let (idx, e) = idx_e;
                    w.write(format!("{}#{}: ", idx, f));
                    e.ast_debug(w);
                });
                w.write("}");
            }
            E::PackVariant(m, e, v, tys_opt, fields) => {
                w.write(format!("{}::{}::{}", m, e, v));
                if let Some(ss) = tys_opt {
                    w.write("<");
                    ss.ast_debug(w);
                    w.write(">");
                }
                w.write("{");
                w.comma(fields, |w, (_, f, idx_e)| {
                    let (idx, e) = idx_e;
                    w.write(format!("{}#{}: ", idx, f));
                    e.ast_debug(w);
                });
                w.write("}");
            }
            E::IfElse(b, t, f_opt) => {
                w.write("if (");
                b.ast_debug(w);
                w.write(") ");
                t.ast_debug(w);
                if let Some(f) = f_opt {
                    w.write(" else ");
                    f.ast_debug(w);
                }
            }
            E::Match(subject, arms) => {
                w.write("match (");
                subject.ast_debug(w);
                w.write(") ");
                w.block(|w| {
                    w.list(&arms.value, ", ", |w, arm| {
                        arm.ast_debug(w);
                        true
                    })
                });
            }
            E::While(name, b, e) => {
                name.ast_debug(w);
                w.write(": ");
                w.write("while (");
                b.ast_debug(w);
                w.write(") ");
                e.ast_debug(w);
            }
            E::Loop(name, e) => {
                name.ast_debug(w);
                w.write(": ");
                w.write("loop ");
                e.ast_debug(w);
            }
            E::Block(seq) => seq.ast_debug(w),
            E::Lambda(l) => l.ast_debug(w),
            E::ExpList(es) => {
                w.write("(");
                w.comma(es, |w, e| e.ast_debug(w));
                w.write(")");
            }

            E::Assign(sp!(_, lvalues), rhs) => {
                lvalues.ast_debug(w);
                w.write(" = ");
                rhs.ast_debug(w);
            }
            E::FieldMutate(ed, rhs) => {
                ed.ast_debug(w);
                w.write(" = ");
                rhs.ast_debug(w);
            }
            E::Mutate(lhs, rhs) => {
                w.write("*");
                lhs.ast_debug(w);
                w.write(" = ");
                rhs.ast_debug(w);
            }

            E::Return(e) => {
                w.write("return ");
                e.ast_debug(w);
            }
            E::Abort(e) => {
                w.write("abort ");
                e.ast_debug(w);
            }
            E::Give(usage, name, e) => {
                w.write(format!("give#{usage} '"));
                name.ast_debug(w);
                w.write(" ");
                e.ast_debug(w);
            }
            E::Continue(name) => {
                w.write("continue @");
                name.ast_debug(w);
            }
            E::Dereference(e) => {
                w.write("*");
                e.ast_debug(w)
            }
            E::UnaryExp(op, e) => {
                op.ast_debug(w);
                w.write(" ");
                e.ast_debug(w);
            }
            E::BinopExp(l, op, r) => {
                l.ast_debug(w);
                w.write(" ");
                op.ast_debug(w);
                w.write(" ");
                r.ast_debug(w)
            }
            E::ExpDotted(usage, ed) => {
                let case = match usage {
                    DottedUsage::Move(_) => "move ",
                    DottedUsage::Copy(_) => "copy ",
                    DottedUsage::Use => "use ",
                    DottedUsage::Borrow(false) => "&",
                    DottedUsage::Borrow(true) => "&mut ",
                };
                w.write(case);
                ed.ast_debug(w)
            }
            E::Cast(e, ty) => {
                w.write("(");
                e.ast_debug(w);
                w.write(" as ");
                ty.ast_debug(w);
                w.write(")");
            }
            E::Annotate(e, ty) => {
                w.write("(");
                e.ast_debug(w);
                w.write(": ");
                ty.ast_debug(w);
                w.write(")");
            }
            E::UnresolvedError => w.write("_|_"),
            E::ErrorConstant { .. } => w.write("ErrorConstant"),
        }
    }
}

impl AstDebug for Lambda {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Lambda {
            parameters: sp!(_, bs),
            return_type,
            return_label,
            use_fun_color,
            body: e,
        } = self;
        return_label.ast_debug(w);
        w.write(": ");
        bs.ast_debug(w);
        if let Some(ty) = return_type {
            w.write(" -> ");
            ty.ast_debug(w);
        }
        w.write(format!("use_funs#{}", use_fun_color));
        e.ast_debug(w);
    }
}

impl AstDebug for Block {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Block {
            name,
            from_macro_argument,
            seq,
        } = self;
        if let Some(name) = name {
            name.ast_debug(w);
            w.write(": ");
        }
        if from_macro_argument.is_some() {
            w.write("substituted_macro_arg#");
        }
        seq.ast_debug(w);
    }
}

impl AstDebug for BuiltinFunction_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        use BuiltinFunction_ as F;
        let (n, bt) = match self {
            F::Freeze(bt) => (F::FREEZE, bt),
            F::Assert(_) => (F::ASSERT_MACRO, &None),
        };
        w.write(n);
        if let Some(bt) = bt {
            w.write("<");
            bt.ast_debug(w);
            w.write(">");
        }
    }
}

impl AstDebug for ExpDotted_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        use ExpDotted_ as D;
        match self {
            D::Exp(e) => e.ast_debug(w),
            D::Dot(e, _, n) => {
                e.ast_debug(w);
                w.write(format!(".{}", n))
            }
            D::Index(e, sp!(_, args)) => {
                e.ast_debug(w);
                w.write("(");
                w.comma(args, |w, e| e.ast_debug(w));
                w.write(")");
            }
            D::DotAutocomplete(_, e) => {
                e.ast_debug(w);
                w.write(".")
            }
        }
    }
}

impl AstDebug for MatchArm_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        let MatchArm_ {
            pattern,
            binders: _,
            guard,
            guard_binders: _,
            rhs_binders: _,
            rhs,
        } = self;
        pattern.ast_debug(w);
        if let Some(exp) = guard.as_ref() {
            w.write(" if (");
            exp.ast_debug(w);
        }
        w.write(") => ");
        rhs.ast_debug(w);
    }
}

impl AstDebug for MatchPattern_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        use MatchPattern_::*;
        match self {
            Variant(mident, enum_, variant, tys_opt, fields) => {
                w.write(format!("{}::{}::{}", mident, enum_, variant));
                if let Some(ss) = tys_opt {
                    w.write("<");
                    ss.ast_debug(w);
                    w.write(">");
                }
                w.comma(fields.key_cloned_iter(), |w, (field, (idx, pat))| {
                    w.write(format!(" {}#{} : ", field, idx));
                    pat.ast_debug(w);
                });
                w.write("} ");
            }
            Struct(mident, struct_, tys_opt, fields) => {
                w.write(format!("{}::{}", mident, struct_,));
                if let Some(ss) = tys_opt {
                    w.write("<");
                    ss.ast_debug(w);
                    w.write(">");
                }
                w.comma(fields.key_cloned_iter(), |w, (field, (idx, pat))| {
                    w.write(format!(" {}#{} : ", field, idx));
                    pat.ast_debug(w);
                });
                w.write("} ");
            }
            Constant(mident, const_) => {
                w.write(format!("const#{}::{}", mident, const_));
            }
            Binder(mut_, name, unused_binding) => {
                mut_.ast_debug(w);
                name.ast_debug(w);
                if *unused_binding {
                    w.write("#unused");
                }
            }
            Literal(v) => v.ast_debug(w),
            Wildcard => w.write("_"),
            Or(lhs, rhs) => {
                lhs.ast_debug(w);
                w.write(" | ");
                rhs.ast_debug(w);
            }
            At(x, unused_binding, pat) => {
                x.ast_debug(w);
                if *unused_binding {
                    w.write("#unused");
                }
                w.write(" @ ");
                pat.ast_debug(w);
            }
            ErrorPat => w.write("#err"),
        }
    }
}

impl AstDebug for Vec<LValue> {
    fn ast_debug(&self, w: &mut AstWriter) {
        let parens = self.len() != 1;
        if parens {
            w.write("(");
        }
        w.comma(self, |w, b| b.ast_debug(w));
        if parens {
            w.write(")");
        }
    }
}

impl AstDebug for LValue_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        use LValue_ as L;
        match self {
            L::Ignore => w.write("_"),
            L::Error => w.write("<_error>"),
            L::Var {
                mut_,
                var,
                unused_binding,
            } => {
                if let Some(mut_) = mut_ {
                    mut_.ast_debug(w);
                }
                var.ast_debug(w);
                if *unused_binding {
                    w.write("#unused");
                }
            }
            L::Unpack(m, s, tys_opt, fields) => {
                w.write(format!("{}::{}", m, s));
                if let Some(ss) = tys_opt {
                    w.write("<");
                    ss.ast_debug(w);
                    w.write(">");
                }
                w.write("{");
                w.comma(fields, |w, (_, f, idx_b)| {
                    let (idx, b) = idx_b;
                    w.write(format!("{}#{}: ", idx, f));
                    b.ast_debug(w);
                });
                w.write("}");
            }
        }
    }
}

impl AstDebug for LambdaLValues_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write("|");
        w.comma(self, |w, (lv, ty_opt)| {
            lv.ast_debug(w);
            if let Some(ty) = ty_opt {
                w.write(": ");
                ty.ast_debug(w);
            }
        });
        w.write("| ");
    }
}
