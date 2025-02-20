// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diagnostics::warning_filters::{WarningFilters, WarningFiltersTable},
    parser::ast::{
        self as P, Ability, Ability_, BinOp, BlockLabel, ConstantName, DatatypeName, DocComment,
        Field, FunctionName, ModuleName, QuantKind, UnaryOp, Var, VariantName, ENTRY_MODIFIER,
        MACRO_MODIFIER, NATIVE_MODIFIER,
    },
    shared::{
        ast_debug::*, known_attributes::KnownAttribute, unique_map::UniqueMap,
        unique_set::UniqueSet, *,
    },
};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::{collections::VecDeque, fmt, hash::Hash, sync::Arc};

//**************************************************************************************************
// Program
//**************************************************************************************************

#[derive(Debug, Clone)]
pub struct Program {
    /// Safety: This table should not be dropped as long as any `WarningFilters` are alive
    pub warning_filters_table: Arc<WarningFiltersTable>,
    // Map of declared named addresses, and their values if specified
    pub modules: UniqueMap<ModuleIdent, ModuleDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplicitUseFun {
    pub doc: DocComment,
    pub loc: Loc,
    pub attributes: Attributes,
    pub is_public: Option<Loc>,
    pub function: ModuleAccess,
    pub ty: ModuleAccess,
    pub method: Name,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImplicitUseFunKind {
    // From a function declaration in the module
    FunctionDeclaration,
    // From a normal, non 'use fun' use declaration,
    UseAlias { used: bool },
}

// These are only candidates as we have not yet checked if they have the proper signature for a
// use fun declaration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplicitUseFunCandidate {
    pub loc: Loc,
    pub attributes: Attributes,
    pub is_public: Option<Loc>,
    pub function: (ModuleIdent, Name),
    pub kind: ImplicitUseFunKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseFuns {
    pub explicit: Vec<ExplicitUseFun>,
    pub implicit: UniqueMap<Name, ImplicitUseFunCandidate>,
}

//**************************************************************************************************
// Attributes
//**************************************************************************************************

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeValue_ {
    Value(Value),
    Address(Address),
    Module(ModuleIdent),
    ModuleAccess(ModuleAccess),
}
pub type AttributeValue = Spanned<AttributeValue_>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attribute_ {
    Name(Name),
    Assigned(Name, Box<AttributeValue>),
    Parameterized(Name, InnerAttributes),
}
pub type Attribute = Spanned<Attribute_>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AttributeName_ {
    Unknown(Symbol),
    Known(KnownAttribute),
}

pub type AttributeName = Spanned<AttributeName_>;

pub type InnerAttributes = UniqueMap<AttributeName, Attribute>;
pub type Attributes = UniqueMap<Spanned<KnownAttribute>, Attribute>;

//**************************************************************************************************
// Modules
//**************************************************************************************************

#[derive(Clone, Copy)]
pub enum Address {
    Numerical {
        name: Option<Name>,
        value: Spanned<NumericalAddress>,
        // set to true when the same name is used across multiple packages
        name_conflict: bool,
    },
    NamedUnassigned(Name),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleIdent_ {
    pub address: Address,
    pub module: ModuleName,
}
pub type ModuleIdent = Spanned<ModuleIdent_>;

#[derive(Debug, Clone)]
pub struct ModuleDefinition {
    pub doc: DocComment,
    pub warning_filter: WarningFilters,
    // package name metadata from compiler arguments, not used for any language rules
    pub package_name: Option<Symbol>,
    pub attributes: Attributes,
    pub loc: Loc,
    pub target_kind: P::TargetKind,
    pub use_funs: UseFuns,
    pub friends: UniqueMap<ModuleIdent, Friend>,
    pub structs: UniqueMap<DatatypeName, StructDefinition>,
    pub enums: UniqueMap<DatatypeName, EnumDefinition>,
    pub functions: UniqueMap<FunctionName, Function>,
    pub constants: UniqueMap<ConstantName, Constant>,
}

//**************************************************************************************************
// Friend
//**************************************************************************************************

#[derive(Debug, Clone)]
pub struct Friend {
    pub attributes: Attributes,
    // We retain attr locations for Move 2024 migration: `flatten_attributes` in `translate.rs`
    // discards the overall attribute spans, but we need them to comment full attribute forms out.
    pub attr_locs: Vec<Loc>,
    pub loc: Loc,
}

//**************************************************************************************************
// Datatypes
//**************************************************************************************************

pub type Fields<T> = UniqueMap<Field, (usize, T)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatatypeTypeParameter {
    pub is_phantom: bool,
    pub name: Name,
    pub constraints: AbilitySet,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDefinition {
    pub doc: DocComment,
    pub warning_filter: WarningFilters,
    // index in the original order as defined in the source file
    pub index: usize,
    pub attributes: Attributes,
    pub loc: Loc,
    pub abilities: AbilitySet,
    pub type_parameters: Vec<DatatypeTypeParameter>,
    pub fields: StructFields,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StructFields {
    Positional(Vec<(DocComment, Type)>),
    Named(Fields<(DocComment, Type)>),
    Native(Loc),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDefinition {
    pub doc: DocComment,
    pub warning_filter: WarningFilters,
    // index in the original order as defined in the source file
    pub index: usize,
    pub attributes: Attributes,
    pub loc: Loc,
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
    Named(Fields<(DocComment, Type)>),
    Positional(Vec<(DocComment, Type)>),
    Empty,
}

//**************************************************************************************************
// Functions
//**************************************************************************************************

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum Visibility {
    Public(Loc),
    Friend(Loc),
    Package(Loc),
    Internal,
}

#[derive(PartialEq, Clone, Debug)]
pub struct FunctionSignature {
    pub type_parameters: Vec<(Name, AbilitySet)>,
    pub parameters: Vec<(Mutability, Var, Type)>,
    pub return_type: Type,
}

#[derive(PartialEq, Clone, Debug)]
pub enum FunctionBody_ {
    Defined(Sequence),
    Native,
}
pub type FunctionBody = Spanned<FunctionBody_>;

#[derive(PartialEq, Clone, Debug)]
pub struct Function {
    pub doc: DocComment,
    pub warning_filter: WarningFilters,
    // index in the original order as defined in the source file
    pub index: usize,
    pub attributes: Attributes,
    pub loc: Loc,
    pub visibility: Visibility,
    pub entry: Option<Loc>,
    pub macro_: Option<Loc>,
    pub signature: FunctionSignature,
    pub body: FunctionBody,
}

//**************************************************************************************************
// Constants
//**************************************************************************************************

#[derive(PartialEq, Clone, Debug)]
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AbilitySet(UniqueSet<Ability>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum ModuleAccess_ {
    Name(Name),
    ModuleAccess(ModuleIdent, Name),
    Variant(Spanned<(ModuleIdent, Name)>, Name),
}
pub type ModuleAccess = Spanned<ModuleAccess_>;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum Type_ {
    Unit,
    Multiple(Vec<Type>),
    Apply(ModuleAccess, Vec<Type>),
    Ref(bool, Box<Type>),
    Fun(Vec<Type>, Box<Type>),
    UnresolvedError,
}
pub type Type = Spanned<Type_>;

//**************************************************************************************************
// Expressions
//**************************************************************************************************

#[derive(Clone, Copy, Debug, Eq, PartialOrd, Ord)]
pub enum Mutability {
    Imm,
    Mut(Loc), // if the local had a `mut` prefix
    Either,   // for legacy and temps
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldBindings {
    Named(Fields<LValue>, Option<Loc>), /* Loc indicates ellipsis presence */
    Positional(Vec<Ellipsis<LValue>>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LValue_ {
    Var(Option<Mutability>, ModuleAccess, Option<Vec<Type>>),
    Unpack(ModuleAccess, Option<Vec<Type>>, FieldBindings),
}
pub type LValue = Spanned<LValue_>;
pub type LValueList_ = Vec<LValue>;
pub type LValueList = Spanned<LValueList_>;

pub type LValueWithRange_ = (LValue, Exp);
pub type LValueWithRange = Spanned<LValueWithRange_>;
pub type LValueWithRangeList_ = Vec<LValueWithRange>;
pub type LValueWithRangeList = Spanned<LValueWithRangeList_>;

pub type LambdaLValues_ = Vec<(LValueList, Option<Type>)>;
pub type LambdaLValues = Spanned<LambdaLValues_>;

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum ExpDotted_ {
    Exp(Box<Exp>),
    Dot(Box<ExpDotted>, /* dot location */ Loc, Name),
    Index(Box<ExpDotted>, Spanned<Vec<Exp>>),
    DotUnresolved(Loc, Box<ExpDotted>), // dot where Name could not be parsed
}
pub type ExpDotted = Spanned<ExpDotted_>;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum DottedUsage {
    Move(Loc),
    Copy(Loc),
    Use,
    Borrow(/* mut */ bool),
}

#[derive(Debug, Clone, PartialOrd, Ord, PartialEq, Eq)]
pub enum Value_ {
    // 0x<hex representation up to 64 digits with padding 0s>
    Address(Address),
    // <num>
    InferredNum(move_core_types::u256::U256),
    // <num>u8
    U8(u8),
    // <num>u16
    U16(u16),
    // <num>u32
    U32(u32),
    // <num>u64
    U64(u64),
    // <num>u128
    U128(u128),
    // <num>u256
    U256(move_core_types::u256::U256),
    // true
    // false
    Bool(bool),
    Bytearray(Vec<u8>),
}
pub type Value = Spanned<Value_>;

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Exp_ {
    Value(Value),

    Name(ModuleAccess, Option<Vec<Type>>),
    Call(
        ModuleAccess,
        /* is_macro */ Option<Loc>,
        Option<Vec<Type>>,
        Spanned<Vec<Exp>>,
    ),
    MethodCall(
        Box<ExpDotted>,
        Loc, // location of the dot
        Name,
        /* is_macro */ Option<Loc>,
        Option<Vec<Type>>,
        Spanned<Vec<Exp>>,
    ),
    Pack(ModuleAccess, Option<Vec<Type>>, Fields<Exp>),
    Vector(Loc, Option<Vec<Type>>, Spanned<Vec<Exp>>),

    IfElse(Box<Exp>, Box<Exp>, Option<Box<Exp>>),
    Match(Box<Exp>, Spanned<Vec<MatchArm>>),
    While(Option<BlockLabel>, Box<Exp>, Box<Exp>),
    Loop(Option<BlockLabel>, Box<Exp>),
    Block(Option<BlockLabel>, Sequence),
    Lambda(LambdaLValues, Option<Type>, Box<Exp>),
    Quant(
        QuantKind,
        LValueWithRangeList,
        Vec<Vec<Exp>>,
        Option<Box<Exp>>,
        Box<Exp>,
    ), // spec only

    Assign(LValueList, Box<Exp>),
    FieldMutate(Box<ExpDotted>, Box<Exp>),
    Mutate(Box<Exp>, Box<Exp>),
    Abort(Option<Box<Exp>>),
    Return(Option<BlockLabel>, Box<Exp>),
    Break(Option<BlockLabel>, Box<Exp>),
    Continue(Option<BlockLabel>),

    Dereference(Box<Exp>),
    UnaryExp(UnaryOp, Box<Exp>),
    BinopExp(Box<Exp>, BinOp, Box<Exp>),

    ExpList(Vec<Exp>),
    Unit {
        trailing: bool,
    },

    ExpDotted(DottedUsage, Box<ExpDotted>),
    Index(Box<Exp>, Box<Exp>), // spec only (no mutation needed right now)

    Cast(Box<Exp>, Type),
    Annotate(Box<Exp>, Type),

    UnresolvedError,
}
pub type Exp = Spanned<Exp_>;

pub type Sequence = (UseFuns, VecDeque<SequenceItem>);
#[derive(Debug, Clone, PartialEq)]
pub enum SequenceItem_ {
    Seq(Box<Exp>),
    Declare(LValueList, Option<Type>),
    Bind(LValueList, Box<Exp>),
}
pub type SequenceItem = Spanned<SequenceItem_>;

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm_ {
    pub pattern: MatchPattern,
    pub guard: Option<Box<Exp>>,
    pub rhs: Box<Exp>,
}

pub type MatchArm = Spanned<MatchArm_>;

#[derive(Debug, Clone, PartialEq)]
pub enum Ellipsis<T> {
    Binder(T),
    Ellipsis(Loc),
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatchPattern_ {
    PositionalConstructor(
        ModuleAccess,
        Option<Vec<Type>>,
        Spanned<Vec<Ellipsis<MatchPattern>>>,
    ),
    NamedConstructor(
        ModuleAccess,
        Option<Vec<Type>>,
        Fields<MatchPattern>,
        Option<Loc>,
    ),
    ModuleAccessName(ModuleAccess, Option<Vec<Type>>),
    Binder(Mutability, Var),
    Literal(Value),
    ErrorPat,
    Or(Box<MatchPattern>, Box<MatchPattern>),
    At(Var, Box<MatchPattern>),
}

pub type MatchPattern = Spanned<MatchPattern_>;

//**************************************************************************************************
// Traits
//**************************************************************************************************

impl TName for ModuleIdent {
    type Key = ModuleIdent_;
    type Loc = Loc;

    fn drop_loc(self) -> (Loc, ModuleIdent_) {
        (self.loc, self.value)
    }

    fn add_loc(loc: Loc, value: ModuleIdent_) -> ModuleIdent {
        sp(loc, value)
    }

    fn borrow(&self) -> (&Loc, &ModuleIdent_) {
        (&self.loc, &self.value)
    }
}

impl TName for AttributeName {
    type Key = AttributeName_;
    type Loc = Loc;

    fn drop_loc(self) -> (Self::Loc, Self::Key) {
        let sp!(loc, n_) = self;
        (loc, n_)
    }

    fn add_loc(loc: Self::Loc, name_: Self::Key) -> Self {
        sp(loc, name_)
    }

    fn borrow(&self) -> (&Self::Loc, &Self::Key) {
        let sp!(loc, n_) = self;
        (loc, n_)
    }
}

impl TName for Spanned<KnownAttribute> {
    type Key = KnownAttribute;
    type Loc = Loc;

    fn drop_loc(self) -> (Self::Loc, Self::Key) {
        let sp!(loc, n_) = self;
        (loc, n_)
    }

    fn add_loc(loc: Self::Loc, name_: Self::Key) -> Self {
        sp(loc, name_)
    }

    fn borrow(&self) -> (&Self::Loc, &Self::Key) {
        let sp!(loc, n_) = self;
        (loc, n_)
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl PartialEq for Address {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Numerical { value: l, .. }, Self::Numerical { value: r, .. }) => l == r,
            (Self::NamedUnassigned(l), Self::NamedUnassigned(r)) => l == r,
            _ => false,
        }
    }
}

impl Eq for Address {}

impl PartialOrd for Address {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Address {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        match (self, other) {
            (Self::Numerical { .. }, Self::NamedUnassigned(_)) => Ordering::Less,
            (Self::NamedUnassigned(_), Self::Numerical { .. }) => Ordering::Greater,

            (Self::Numerical { value: l, .. }, Self::Numerical { value: r, .. }) => l.cmp(r),
            (Self::NamedUnassigned(l), Self::NamedUnassigned(r)) => l.cmp(r),
        }
    }
}

impl Hash for Address {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Self::Numerical {
                value: sp!(_, bytes),
                ..
            } => bytes.hash(state),
            Self::NamedUnassigned(name) => name.hash(state),
        }
    }
}

//**************************************************************************************************
// impls
//**************************************************************************************************

impl Attribute_ {
    pub fn attribute_name(&self) -> &Name {
        match self {
            Attribute_::Name(nm)
            | Attribute_::Assigned(nm, _)
            | Attribute_::Parameterized(nm, _) => nm,
        }
    }
}

impl Attributes {
    pub fn is_test_or_test_only(&self) -> bool {
        self.contains_key_(&known_attributes::TestingAttribute::TestOnly.into())
            || self.contains_key_(&known_attributes::TestingAttribute::RandTest.into())
            || self.contains_key_(&known_attributes::TestingAttribute::Test.into())
    }
}

impl Default for UseFuns {
    fn default() -> Self {
        Self::new()
    }
}

impl UseFuns {
    pub fn new() -> Self {
        Self {
            explicit: vec![],
            implicit: UniqueMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        let Self { explicit, implicit } = self;
        explicit.is_empty() && implicit.is_empty()
    }
}

impl Address {
    pub const fn anonymous(loc: Loc, address: NumericalAddress) -> Self {
        Self::Numerical {
            name: None,
            value: sp(loc, address),
            name_conflict: false,
        }
    }

    pub fn into_addr_bytes(self) -> NumericalAddress {
        match self {
            Self::Numerical {
                value: sp!(_, bytes),
                ..
            } => bytes,
            Self::NamedUnassigned(_) => NumericalAddress::DEFAULT_ERROR_ADDRESS,
        }
    }

    pub fn is<Addr>(&self, address: &Addr) -> bool
    where
        NumericalAddress: PartialEq<Addr>,
    {
        self.numerical_value().is_some_and(|sp!(_, v)| v == address)
    }

    pub fn numerical_value(&self) -> Option<&Spanned<NumericalAddress>> {
        match self {
            Self::Numerical { value, .. } => Some(value),
            Self::NamedUnassigned(_) => None,
        }
    }
}

impl ModuleIdent_ {
    pub fn new(address: Address, module: ModuleName) -> Self {
        Self { address, module }
    }

    pub fn is<Addr>(&self, address: &Addr, module: impl AsRef<str>) -> bool
    where
        NumericalAddress: PartialEq<Addr>,
    {
        let Self {
            address: a,
            module: m,
        } = self;
        a.is(address) && m == module.as_ref()
    }
}

impl AbilitySet {
    /// All abilities
    pub const ALL: [Ability_; 4] = [
        Ability_::Copy,
        Ability_::Drop,
        Ability_::Store,
        Ability_::Key,
    ];
    /// Abilities for bool, u8, u16, u32, u64, u128, u256 and address
    pub const PRIMITIVES: [Ability_; 3] = [Ability_::Copy, Ability_::Drop, Ability_::Store];
    /// Abilities for &_ and &mut _
    pub const REFERENCES: [Ability_; 2] = [Ability_::Copy, Ability_::Drop];
    /// Abilities for signer
    pub const SIGNER: [Ability_; 1] = [Ability_::Drop];
    /// Abilities for vector<_>, note they are predicated on the type argument
    pub const COLLECTION: [Ability_; 3] = [Ability_::Copy, Ability_::Drop, Ability_::Store];
    /// Abilities for functions
    pub const FUNCTIONS: [Ability_; 0] = [];

    pub fn empty() -> Self {
        AbilitySet(UniqueSet::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn add(&mut self, a: Ability) -> Result<(), Loc> {
        self.0.add(a).map_err(|(_a, loc)| loc)
    }

    pub fn has_ability(&self, a: &Ability) -> bool {
        self.0.contains(a)
    }

    pub fn has_ability_(&self, a: Ability_) -> bool {
        self.0.contains_(&a)
    }

    pub fn ability_loc(&self, sp!(_, a_): &Ability) -> Option<Loc> {
        self.0.get_loc_(a_).copied()
    }

    pub fn ability_loc_(&self, a: Ability_) -> Option<Loc> {
        self.0.get_loc_(&a).copied()
    }

    // intersection of two sets. Keeps the loc of the first set
    pub fn intersect(&self, other: &Self) -> Self {
        Self(self.0.intersect(&other.0))
    }

    // union of two sets. Prefers the loc of the first set
    pub fn union(&self, other: &Self) -> Self {
        Self(self.0.union(&other.0))
    }

    pub fn is_subset(&self, other: &Self) -> bool {
        self.0.is_subset(&other.0)
    }

    pub fn iter(&self) -> AbilitySetIter {
        self.into_iter()
    }

    pub fn from_abilities(
        iter: impl IntoIterator<Item = Ability>,
    ) -> Result<Self, (Ability_, Loc, Loc)> {
        Ok(Self(UniqueSet::from_elements(iter)?))
    }

    pub fn from_abilities_(
        loc: Loc,
        iter: impl IntoIterator<Item = Ability_>,
    ) -> Result<Self, (Ability_, Loc, Loc)> {
        Ok(Self(UniqueSet::from_elements_(loc, iter)?))
    }

    pub fn all(loc: Loc) -> Self {
        Self::from_abilities_(loc, Self::ALL).unwrap()
    }

    pub fn primitives(loc: Loc) -> Self {
        Self::from_abilities_(loc, Self::PRIMITIVES).unwrap()
    }

    pub fn references(loc: Loc) -> Self {
        Self::from_abilities_(loc, Self::REFERENCES).unwrap()
    }

    pub fn signer(loc: Loc) -> Self {
        Self::from_abilities_(loc, Self::SIGNER).unwrap()
    }

    pub fn collection(loc: Loc) -> Self {
        Self::from_abilities_(loc, Self::COLLECTION).unwrap()
    }

    pub fn functions(loc: Loc) -> Self {
        Self::from_abilities_(loc, Self::COLLECTION).unwrap()
    }
}

impl Visibility {
    pub const FRIEND: &'static str = P::Visibility::FRIEND;
    pub const FRIEND_IDENT: &'static str = P::Visibility::FRIEND_IDENT;
    pub const INTERNAL: &'static str = P::Visibility::INTERNAL;
    pub const PACKAGE: &'static str = P::Visibility::PACKAGE;
    pub const PACKAGE_IDENT: &'static str = P::Visibility::PACKAGE_IDENT;
    pub const PUBLIC: &'static str = P::Visibility::PUBLIC;

    pub fn loc(&self) -> Option<Loc> {
        match self {
            Visibility::Friend(loc) | Visibility::Package(loc) | Visibility::Public(loc) => {
                Some(*loc)
            }
            Visibility::Internal => None,
        }
    }
}

//**************************************************************************************************
// Iter
//**************************************************************************************************

pub struct AbilitySetIter<'a>(unique_set::Iter<'a, Ability>);

impl<'a> Iterator for AbilitySetIter<'a> {
    type Item = Ability;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(loc, a_)| sp(loc, *a_))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<'a> IntoIterator for &'a AbilitySet {
    type Item = Ability;
    type IntoIter = AbilitySetIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        AbilitySetIter(self.0.iter())
    }
}

pub struct AbilitySetIntoIter(unique_set::IntoIter<Ability>);

impl Iterator for AbilitySetIntoIter {
    type Item = Ability;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl IntoIterator for AbilitySet {
    type Item = Ability;
    type IntoIter = AbilitySetIntoIter;

    fn into_iter(self) -> Self::IntoIter {
        AbilitySetIntoIter(self.0.into_iter())
    }
}

//**************************************************************************************************
// PartialEq
//**************************************************************************************************

impl PartialEq for Mutability {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Mutability::Imm, Mutability::Imm) => true,
            (Mutability::Either, Mutability::Either) => true,
            (Mutability::Mut(_), Mutability::Mut(_)) => true,
            (_, _) => false,
        }
    }
}

//**************************************************************************************************
// Display
//**************************************************************************************************

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Numerical {
                name: None,
                value: sp!(_, bytes),
                ..
            } => write!(f, "{}", bytes),
            Self::Numerical {
                name: Some(name),
                value: sp!(_, bytes),
                name_conflict: true,
            } => write!(f, "({}={})", name, bytes),
            Self::Numerical {
                name: Some(name),
                value: _,
                name_conflict: false,
            }
            | Self::NamedUnassigned(name) => write!(f, "{}", name),
        }
    }
}

impl fmt::Display for AttributeName_ {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match self {
            AttributeName_::Unknown(sym) => write!(f, "{}", sym),
            AttributeName_::Known(known) => write!(f, "{}", known.name()),
        }
    }
}

impl fmt::Display for ModuleIdent_ {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}::{}", self.address, &self.module)
    }
}

impl fmt::Display for ModuleAccess_ {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        use ModuleAccess_::*;
        match self {
            Name(n) => write!(f, "{}", n),
            ModuleAccess(m, n) => write!(f, "{}::{}", m, n),
            Variant(sp!(_, (m, n)), v) => write!(f, "{}::{}::{}", m, n, v),
        }
    }
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match &self {
                Visibility::Public(_) => Visibility::PUBLIC,
                Visibility::Friend(_) => Visibility::FRIEND,
                Visibility::Package(_) => Visibility::PACKAGE,
                Visibility::Internal => Visibility::INTERNAL,
            }
        )
    }
}

impl std::fmt::Display for Value_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Value_ as V;
        match self {
            V::Address(addr) => write!(f, "@{}", addr),
            V::InferredNum(u) => write!(f, "{}", u),
            V::U8(u) => write!(f, "{}", u),
            V::U16(u) => write!(f, "{}", u),
            V::U32(u) => write!(f, "{}", u),
            V::U64(u) => write!(f, "{}", u),
            V::U128(u) => write!(f, "{}", u),
            V::U256(u) => write!(f, "{}", u),
            V::Bool(b) => write!(f, "{}", b),
            // TODO preserve the user's original string
            V::Bytearray(v) => {
                write!(f, "vector[")?;
                for (idx, byte) in v.iter().enumerate() {
                    if idx != 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", byte)?;
                }
                write!(f, "]")
            }
        }
    }
}

//**************************************************************************************************
// Debug
//**************************************************************************************************

impl AstDebug for Program {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Program {
            warning_filters_table: _,
            modules,
        } = self;
        for (m, mdef) in modules.key_cloned_iter() {
            w.write(format!("module {}", m));
            w.block(|w| mdef.ast_debug(w));
            w.new_line();
        }
    }
}

impl AstDebug for ExplicitUseFun {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Self {
            doc,
            loc: _,
            attributes,
            is_public,
            function,
            ty,
            method,
        } = self;
        doc.ast_debug(w);
        attributes.ast_debug(w);
        w.new_line();
        if is_public.is_some() {
            w.write("public ");
        }
        w.write("use fun ");
        function.ast_debug(w);
        w.write(" as ");
        ty.ast_debug(w);
        w.writeln(format!(".{method};"));
    }
}

impl AstDebug for ImplicitUseFunCandidate {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Self {
            loc: _,
            attributes,
            is_public,
            function: (m, n),
            kind,
        } = self;
        attributes.ast_debug(w);
        w.new_line();
        if is_public.is_some() {
            w.write("public ");
        }
        let kind_str = match kind {
            ImplicitUseFunKind::UseAlias { used: true } => "#used",
            ImplicitUseFunKind::UseAlias { used: false } => "#unused",
            ImplicitUseFunKind::FunctionDeclaration => "#fundecl",
        };
        w.writeln(format!("implcit{kind_str}#use fun {m}::{n};"));
    }
}

impl AstDebug for UseFuns {
    fn ast_debug(&self, w: &mut AstWriter) {
        let UseFuns {
            explicit: explict,
            implicit,
        } = self;
        for use_fun in explict {
            use_fun.ast_debug(w);
        }
        for (_, _, use_fun) in implicit {
            use_fun.ast_debug(w);
        }
    }
}

impl AstDebug for AttributeValue_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            AttributeValue_::Value(v) => v.ast_debug(w),
            AttributeValue_::Module(m) => w.write(format!("{m}")),
            AttributeValue_::ModuleAccess(n) => n.ast_debug(w),
            AttributeValue_::Address(a) => w.write(format!("{a}")),
        }
    }
}

impl AstDebug for Attribute_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            Attribute_::Name(n) => w.write(format!("{}", n)),
            Attribute_::Assigned(n, v) => {
                w.write(format!("{}", n));
                w.write(" = ");
                v.ast_debug(w);
            }
            Attribute_::Parameterized(n, inners) => {
                w.write(format!("{}", n));
                w.write("(");
                w.list(inners, ", ", |w, (_, _, inner)| {
                    inner.ast_debug(w);
                    false
                });
                w.write(")");
            }
        }
    }
}

impl AstDebug for InnerAttributes {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write("#[");
        w.list(self, ", ", |w, (_, _, attr)| {
            attr.ast_debug(w);
            false
        });
        w.write("]");
    }
}

impl AstDebug for Attributes {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write("#[");
        w.list(self, ", ", |w, (_, _, attr)| {
            attr.ast_debug(w);
            false
        });
        w.write("]");
    }
}

impl AstDebug for ModuleDefinition {
    fn ast_debug(&self, w: &mut AstWriter) {
        let ModuleDefinition {
            doc,
            package_name,
            attributes,
            loc: _loc,
            target_kind,
            use_funs,
            friends,
            structs,
            enums,
            functions,
            constants,
            warning_filter,
        } = self;
        doc.ast_debug(w);
        warning_filter.ast_debug(w);
        if let Some(n) = package_name {
            w.writeln(format!("{}", n))
        }
        attributes.ast_debug(w);
        target_kind.ast_debug(w);
        use_funs.ast_debug(w);
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

pub fn ability_modifiers_ast_debug(w: &mut AstWriter, abilities: &AbilitySet) {
    if !abilities.is_empty() {
        w.write(" has ");
        w.list(abilities, " ", |w, ab| {
            ab.ast_debug(w);
            false
        });
    }
}

impl AstDebug for (DatatypeName, &StructDefinition) {
    fn ast_debug(&self, w: &mut AstWriter) {
        let (
            name,
            StructDefinition {
                doc,
                index,
                attributes,
                loc: _loc,
                abilities,
                type_parameters,
                fields,
                warning_filter,
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
        match fields {
            StructFields::Named(fields) => w.block(|w| {
                w.list(fields, ",", |w, (_, f, idx_st)| {
                    let (idx, (doc, st)) = idx_st;
                    doc.ast_debug(w);
                    w.write(format!("{}#{}: ", idx, f));
                    st.ast_debug(w);
                    true
                });
            }),
            StructFields::Positional(fields) => w.block(|w| {
                w.list(fields.iter().enumerate(), ",", |w, (idx, (doc, ty))| {
                    doc.ast_debug(w);
                    w.write(format!("{idx}#pos{idx}: "));
                    ty.ast_debug(w);
                    true
                });
            }),
            StructFields::Native(_) => (),
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
                attributes,
                loc: _loc,
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
                loc: _loc,
                fields,
            },
        ) = self;
        doc.ast_debug(w);
        w.write(format!("variant#{index} {name}"));
        match fields {
            VariantFields::Named(fields) => w.block(|w| {
                w.list(fields, ",", |w, (_, f, idx_st)| {
                    let (idx, (doc, st)) = idx_st;
                    doc.ast_debug(w);
                    w.write(format!("{}#{}: ", idx, f));
                    st.ast_debug(w);
                    true
                });
            }),
            VariantFields::Positional(fields) => w.block(|w| {
                w.list(fields.iter().enumerate(), ",", |w, (idx, (doc, ty))| {
                    doc.ast_debug(w);
                    w.write(format!("{idx}#pos{idx}: "));
                    ty.ast_debug(w);
                    true
                });
            }),
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
                index,
                attributes,
                loc: _loc,
                visibility,
                entry,
                macro_,
                signature,
                body,
                warning_filter,
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

impl AstDebug for Visibility {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write(format!("{} ", self))
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
        w.comma(parameters, |w, (mutability, v, st)| {
            mutability.ast_debug(w);
            w.write(format!("{}: ", v));
            st.ast_debug(w);
        });
        w.write("): ");
        return_type.ast_debug(w)
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
        w.write(format!("const#{index} {}:", name));
        signature.ast_debug(w);
        w.write(" = ");
        value.ast_debug(w);
        w.write(";");
    }
}

impl AstDebug for Type_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            Type_::Unit => w.write("()"),
            Type_::Multiple(ss) => {
                w.write("(");
                ss.ast_debug(w);
                w.write(")")
            }
            Type_::Apply(m, ss) => {
                m.ast_debug(w);
                if !ss.is_empty() {
                    w.write("<");
                    ss.ast_debug(w);
                    w.write(">");
                }
            }
            Type_::Ref(mut_, s) => {
                w.write("&");
                if *mut_ {
                    w.write("mut ");
                }
                s.ast_debug(w)
            }
            Type_::Fun(args, result) => {
                w.write("(");
                w.comma(args, |w, ty| ty.ast_debug(w));
                w.write("):");
                result.ast_debug(w);
            }
            Type_::UnresolvedError => w.write("_|_"),
        }
    }
}

impl AstDebug for Vec<(Name, AbilitySet)> {
    fn ast_debug(&self, w: &mut AstWriter) {
        if !self.is_empty() {
            w.write("<");
            w.comma(self, |w, tp| tp.ast_debug(w));
            w.write(">")
        }
    }
}

pub fn ability_constraints_ast_debug(w: &mut AstWriter, abilities: &AbilitySet) {
    if !abilities.is_empty() {
        w.write(": ");
        w.list(abilities, "+", |w, ab| {
            ab.ast_debug(w);
            false
        })
    }
}

impl AstDebug for (Name, AbilitySet) {
    fn ast_debug(&self, w: &mut AstWriter) {
        let (n, abilities) = self;
        w.write(n.value);
        ability_constraints_ast_debug(w, abilities)
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

impl AstDebug for DatatypeTypeParameter {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Self {
            is_phantom,
            name,
            constraints,
        } = self;
        if *is_phantom {
            w.write("phantom ");
        }
        w.write(name.value);
        ability_constraints_ast_debug(w, constraints)
    }
}

impl AstDebug for Vec<Type> {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.comma(self, |w, s| s.ast_debug(w))
    }
}

impl AstDebug for ModuleAccess_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write(format!("{}", self))
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

impl AstDebug for Value_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        use Value_ as V;
        match self {
            V::Address(addr) => w.write(format!("@{}", addr)),
            V::InferredNum(u) => w.write(format!("{}", u)),
            V::U8(u) => w.write(format!("{}u8", u)),
            V::U16(u) => w.write(format!("{}u16", u)),
            V::U32(u) => w.write(format!("{}u32", u)),
            V::U64(u) => w.write(format!("{}u64", u)),
            V::U128(u) => w.write(format!("{}u128", u)),
            V::U256(u) => w.write(format!("{}u256", u)),
            V::Bool(b) => w.write(format!("{}", b)),
            V::Bytearray(v) => w.write(format!("{:?}", v)),
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
            E::Name(ma, tys_opt) => {
                ma.ast_debug(w);
                if let Some(ss) = tys_opt {
                    w.write("<");
                    ss.ast_debug(w);
                    w.write(">");
                }
            }
            E::Call(ma, is_macro, tys_opt, sp!(_, rhs)) => {
                ma.ast_debug(w);
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
            E::Pack(ma, tys_opt, fields) => {
                ma.ast_debug(w);
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
            E::Vector(_loc, tys_opt, sp!(_, elems)) => {
                w.write("vector");
                if let Some(ss) = tys_opt {
                    w.write("<");
                    ss.ast_debug(w);
                    w.write(">");
                }
                w.write("[");
                w.comma(elems, |w, e| e.ast_debug(w));
                w.write("]");
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
                name.map(|name| w.write(format!("'{}: ", name)));
                w.write("while (");
                b.ast_debug(w);
                w.write(")");
                e.ast_debug(w);
            }
            E::Loop(name, e) => {
                name.map(|name| w.write(format!("'{}: ", name)));
                w.write("loop ");
                e.ast_debug(w);
            }
            E::Block(name, seq) => {
                name.map(|name| w.write(format!("'{}: ", name)));
                seq.ast_debug(w);
            }
            E::Lambda(sp!(_, bs), ty_opt, e) => {
                bs.ast_debug(w);
                if let Some(ty) = ty_opt {
                    w.write(" -> ");
                    ty.ast_debug(w);
                }
                e.ast_debug(w);
            }
            E::Quant(kind, sp!(_, rs), trs, c_opt, e) => {
                kind.ast_debug(w);
                w.write(" ");
                rs.ast_debug(w);
                trs.ast_debug(w);
                if let Some(c) = c_opt {
                    w.write(" where ");
                    c.ast_debug(w);
                }
                w.write(" : ");
                e.ast_debug(w);
            }
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

            E::Abort(e) => {
                w.write("abort");
                if let Some(e) = e {
                    w.write(" ");
                    e.ast_debug(w);
                }
            }
            E::Return(name, e) => {
                w.write("return ");
                name.map(|name| w.write(format!(" '{} ", name)));
                e.ast_debug(w);
            }
            E::Break(name, exp) => {
                w.write("break");
                name.map(|name| w.write(format!(" '{} ", name)));
                w.write(" ");
                exp.ast_debug(w);
            }
            E::Continue(name) => {
                w.write("continue");
                name.map(|name| w.write(format!(" '{}", name)));
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
            E::Index(oper, index) => {
                oper.ast_debug(w);
                w.write("[");
                index.ast_debug(w);
                w.write("]");
            }
            E::Annotate(e, ty) => {
                w.write("(");
                e.ast_debug(w);
                w.write(": ");
                ty.ast_debug(w);
                w.write(")");
            }
            E::UnresolvedError => w.write("_|_"),
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
            D::Index(e, rhs) => {
                e.ast_debug(w);
                w.write("[");
                w.comma(&rhs.value, |w, e| e.ast_debug(w));
                w.write("]");
            }
            D::DotUnresolved(_, e) => {
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
            guard,
            rhs,
        } = self;
        pattern.ast_debug(w);
        if let Some(exp) = guard.as_ref() {
            w.write(" if ");
            exp.ast_debug(w);
        }
        w.write(" => ");
        rhs.ast_debug(w);
    }
}

impl<T: AstDebug> AstDebug for Ellipsis<T> {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            Ellipsis::Binder(p) => p.ast_debug(w),
            Ellipsis::Ellipsis(_) => {
                w.write("..");
            }
        }
    }
}

impl AstDebug for MatchPattern_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        use MatchPattern_::*;
        match self {
            PositionalConstructor(name, tys_opt, fields) => {
                name.ast_debug(w);
                if let Some(ss) = tys_opt {
                    w.write("<");
                    ss.ast_debug(w);
                    w.write(">");
                }
                w.write("(");
                w.comma(fields.value.iter(), |w, pat| {
                    pat.ast_debug(w);
                });
                w.write(") ");
            }
            NamedConstructor(name, tys_opt, fields, ellipsis) => {
                name.ast_debug(w);
                if let Some(ss) = tys_opt {
                    w.write("<");
                    ss.ast_debug(w);
                    w.write(">");
                }
                w.write(" {");
                w.comma(fields.key_cloned_iter(), |w, (field, (idx, pat))| {
                    w.write(format!(" {}#{} : ", field, idx));
                    pat.ast_debug(w);
                });
                if ellipsis.is_some() {
                    w.write(" ..");
                }
                w.write("} ");
            }
            ModuleAccessName(name, tys_opt) => {
                name.ast_debug(w);
                if let Some(ss) = tys_opt {
                    w.write("<");
                    ss.ast_debug(w);
                    w.write(">");
                }
            }
            Binder(mut_, name) => {
                mut_.ast_debug(w);
                w.write(format!("{}", name))
            }
            Literal(v) => v.ast_debug(w),
            ErrorPat => w.write("_<err>_"),
            Or(lhs, rhs) => {
                lhs.ast_debug(w);
                w.write(" | ");
                rhs.ast_debug(w);
            }
            At(x, pat) => {
                w.write(format!("{} @ ", x));
                pat.ast_debug(w);
            }
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
            L::Var(mutability, v, tys_opt) => {
                if let Some(mutability) = mutability {
                    mutability.ast_debug(w);
                }
                w.write(format!("{}", v));
                if let Some(ss) = tys_opt {
                    w.write("<");
                    ss.ast_debug(w);
                    w.write(">");
                }
            }
            L::Unpack(ma, tys_opt, field_binds) => {
                ma.ast_debug(w);
                if let Some(ss) = tys_opt {
                    w.write("<");
                    ss.ast_debug(w);
                    w.write(">");
                }
                field_binds.ast_debug(w);
            }
        }
    }
}

impl AstDebug for Vec<LValueWithRange> {
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

impl AstDebug for (LValue, Exp) {
    fn ast_debug(&self, w: &mut AstWriter) {
        self.0.ast_debug(w);
        w.write(" in ");
        self.1.ast_debug(w);
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

impl AstDebug for Vec<Vec<Exp>> {
    fn ast_debug(&self, w: &mut AstWriter) {
        for trigger in self {
            w.write("{");
            w.comma(trigger, |w, b| b.ast_debug(w));
            w.write("}");
        }
    }
}

impl AstDebug for FieldBindings {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            FieldBindings::Named(fields, ellipsis) => {
                w.write("{");
                w.comma(fields, |w, (_, f, idx_b)| {
                    let (idx, b) = idx_b;
                    w.write(format!("{}#{}: ", idx, f));
                    b.ast_debug(w);
                });
                if ellipsis.is_some() {
                    w.write("..");
                }
                w.write("}");
            }
            FieldBindings::Positional(vals) => {
                w.write("(");
                w.comma(vals.iter().enumerate(), |w, (idx, lval)| {
                    w.write(format!("{idx}: "));
                    lval.ast_debug(w);
                });
                w.write(")");
            }
        }
    }
}

impl AstDebug for Mutability {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            Mutability::Mut(_) => w.write("mut "),
            Mutability::Either => w.write("mut? "),
            Mutability::Imm => (),
        }
    }
}
