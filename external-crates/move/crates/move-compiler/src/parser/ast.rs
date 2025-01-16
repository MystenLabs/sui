// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    diagnostics::Diagnostic,
    ice,
    shared::{
        ast_debug::*, format_comma, Identifier, Name, NamedAddressMap, NamedAddressMapIndex,
        NamedAddressMaps, NumericalAddress, TName,
    },
};
use move_command_line_common::files::FileHash;
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::{fmt, hash::Hash};

macro_rules! new_name {
    ($n:ident) => {
        #[derive(Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Clone, Copy)]
        pub struct $n(pub Name);

        impl TName for $n {
            type Key = Symbol;
            type Loc = Loc;

            fn drop_loc(self) -> (Loc, Symbol) {
                (self.0.loc, self.0.value)
            }

            fn add_loc(loc: Loc, key: Symbol) -> Self {
                $n(sp(loc, key))
            }

            fn borrow(&self) -> (&Loc, &Symbol) {
                (&self.0.loc, &self.0.value)
            }
        }

        impl PartialEq<str> for $n {
            fn eq(&self, s: &str) -> bool {
                self.0.value.as_str() == s
            }
        }

        impl Identifier for $n {
            fn value(&self) -> Symbol {
                self.0.value
            }
            fn loc(&self) -> Loc {
                self.0.loc
            }
        }

        impl fmt::Display for $n {
            fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", &self.0)
            }
        }
    };
}

//**************************************************************************************************
// Program
//**************************************************************************************************

#[derive(Debug, Clone)]
pub struct Program {
    pub named_address_maps: NamedAddressMaps,
    pub source_definitions: Vec<PackageDefinition>,
    pub lib_definitions: Vec<PackageDefinition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalTargetKind {
    Library,
    SkippedSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Specifies a source target or dependency
pub enum TargetKind {
    /// A source module. If is_root_package is false, some warnings might be suppressed.
    /// Bytecode/CompiledModules will be generated for any Source target
    Source { is_root_package: bool },
    /// A dependency only used for linking.
    /// No bytecode or CompiledModules are generated
    External(ExternalTargetKind),
}

#[derive(Debug, Clone)]
pub struct PackageDefinition {
    pub package: Option<Symbol>,
    pub named_address_map: NamedAddressMapIndex,
    pub def: Definition,
    pub target_kind: TargetKind,
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Definition {
    Module(ModuleDefinition),
    Address(AddressDefinition),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct DocComment(pub(crate) Option<Spanned<String>>);

#[derive(Debug, Clone)]
pub struct AddressDefinition {
    pub attributes: Vec<Attributes>,
    pub loc: Loc,
    pub addr: LeadingNameAccess,
    pub modules: Vec<ModuleDefinition>,
}

#[derive(Debug, PartialEq, Clone, Eq)]
pub enum Use {
    ModuleUse(ModuleIdent, ModuleUse),
    NestedModuleUses(LeadingNameAccess, Vec<(ModuleName, ModuleUse)>),
    Fun {
        visibility: Visibility,
        function: Box<NameAccessChain>,
        ty: Box<NameAccessChain>,
        method: Name,
    },
    // used for one of the three cases when `LeadingNameAccess` represents `some_pkg`
    // - `some_pkg`
    // - `some_pkg::`
    // - `some_pkg::{`
    // where first location represents `::` and the second one represents `{`
    Partial {
        package: LeadingNameAccess,
        colon_colon: Option<Loc>,
        opening_brace: Option<Loc>,
    },
}

#[derive(Debug, PartialEq, Clone, Eq)]
pub enum ModuleUse {
    Module(Option<ModuleName>),
    Members(Vec<(Name, Option<Name>)>),
    // used for one of the three cases when defining `ModuleUse` for `some_mod`:
    // - `... some_mod`
    // - `... some_mod::`
    // - `... some_mod::{`
    // where first location represents `::` and the second one represents `{`
    Partial {
        colon_colon: Option<Loc>,
        opening_brace: Option<Loc>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseDecl {
    pub doc: DocComment,
    pub loc: Loc,
    pub attributes: Vec<Attributes>,
    pub use_: Use,
}

//**************************************************************************************************
// Attributes
//**************************************************************************************************

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeValue_ {
    Value(Value),
    ModuleAccess(NameAccessChain),
}
pub type AttributeValue = Spanned<AttributeValue_>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attribute_ {
    Name(Name),
    Assigned(Name, Box<AttributeValue>),
    Parameterized(Name, Attributes),
}
pub type Attribute = Spanned<Attribute_>;

pub type Attributes = Spanned<Vec<Attribute>>;

//**************************************************************************************************
// Modules
//**************************************************************************************************

new_name!(ModuleName);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Specifies a name at the beginning of an access chain. Could be
/// - A module name
/// - A named address
/// - An address numerical value
pub enum LeadingNameAccess_ {
    AnonymousAddress(NumericalAddress),
    GlobalAddress(Name),
    Name(Name),
}
pub type LeadingNameAccess = Spanned<LeadingNameAccess_>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleIdent_ {
    pub address: LeadingNameAccess,
    pub module: ModuleName,
}
pub type ModuleIdent = Spanned<ModuleIdent_>;

#[derive(Debug, Clone)]
pub enum ModuleDefinitionMode {
    Braces,
    Semicolon,
}

#[derive(Debug, Clone)]
pub struct ModuleDefinition {
    pub doc: DocComment,
    pub attributes: Vec<Attributes>,
    pub loc: Loc,
    pub address: Option<LeadingNameAccess>,
    pub name: ModuleName,
    pub is_spec_module: bool,
    pub definition_mode: ModuleDefinitionMode,
    pub members: Vec<ModuleMember>,
}

#[derive(Debug, Clone)]
pub enum ModuleMember {
    Function(Function),
    Struct(StructDefinition),
    Enum(EnumDefinition),
    Use(UseDecl),
    Friend(FriendDecl),
    Constant(Constant),
    Spec(Spanned<String>),
}

//**************************************************************************************************
// Friends
//**************************************************************************************************

#[derive(Debug, Clone)]
pub struct FriendDecl {
    pub attributes: Vec<Attributes>,
    pub loc: Loc,
    pub friend: NameAccessChain,
}

//**************************************************************************************************
// Datatypes
//**************************************************************************************************

new_name!(Field);
new_name!(DatatypeName);

pub type ResourceLoc = Option<Loc>;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct DatatypeTypeParameter {
    pub is_phantom: bool,
    pub name: Name,
    pub constraints: Vec<Ability>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct StructDefinition {
    pub doc: DocComment,
    pub attributes: Vec<Attributes>,
    pub loc: Loc,
    pub abilities: Vec<Ability>,
    pub name: DatatypeName,
    pub type_parameters: Vec<DatatypeTypeParameter>,
    pub fields: StructFields,
}

#[derive(Debug, PartialEq, Clone)]
pub enum StructFields {
    Named(Vec<(DocComment, Field, Type)>),
    Native(Loc),
    Positional(Vec<(DocComment, Type)>),
}

new_name!(VariantName);

#[derive(Debug, PartialEq, Clone)]
pub struct EnumDefinition {
    pub doc: DocComment,
    pub attributes: Vec<Attributes>,
    pub loc: Loc,
    pub abilities: Vec<Ability>,
    pub name: DatatypeName,
    pub type_parameters: Vec<DatatypeTypeParameter>,
    pub variants: Vec<VariantDefinition>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct VariantDefinition {
    pub doc: DocComment,
    pub loc: Loc,
    pub name: VariantName,
    pub fields: VariantFields,
}

#[derive(Debug, PartialEq, Clone)]
pub enum VariantFields {
    Named(Vec<(DocComment, Field, Type)>),
    Positional(Vec<(DocComment, Type)>),
    Empty,
}

//**************************************************************************************************
// Functions
//**************************************************************************************************

new_name!(FunctionName);

pub const NATIVE_MODIFIER: &str = "native";
pub const ENTRY_MODIFIER: &str = "entry";
pub const MACRO_MODIFIER: &str = "macro";

#[derive(PartialEq, Clone, Debug)]
pub struct FunctionSignature {
    pub type_parameters: Vec<(Name, Vec<Ability>)>,
    pub parameters: Vec<(Mutability, Var, Type)>,
    pub return_type: Type,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum Visibility {
    Public(Loc),
    Friend(Loc),
    Package(Loc),
    Internal,
}

#[derive(PartialEq, Clone, Debug)]
pub enum FunctionBody_ {
    Defined(Sequence),
    Native,
}
pub type FunctionBody = Spanned<FunctionBody_>;

#[derive(PartialEq, Debug, Clone)]
// (public?) foo<T1(: copyable?), ..., TN(: copyable?)>(x1: t1, ..., xn: tn): t1 * ... * tn {
//    body
//  }
// (public?) native foo<T1(: copyable?), ..., TN(: copyable?)>(x1: t1, ..., xn: tn): t1 * ... * tn;
pub struct Function {
    pub doc: DocComment,
    pub attributes: Vec<Attributes>,
    pub loc: Loc,
    pub visibility: Visibility,
    pub entry: Option<Loc>,
    pub macro_: Option<Loc>,
    pub signature: FunctionSignature,
    pub name: FunctionName,
    pub body: FunctionBody,
}

//**************************************************************************************************
// Constants
//**************************************************************************************************

new_name!(ConstantName);

#[derive(PartialEq, Debug, Clone)]
pub struct Constant {
    pub doc: DocComment,
    pub attributes: Vec<Attributes>,
    pub loc: Loc,
    pub signature: Type,
    pub name: ConstantName,
    pub value: Exp,
}

//**************************************************************************************************
// Names
//**************************************************************************************************

// A single name with optional type arguments that may be a macro call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathEntry {
    pub name: Name,
    pub tyargs: Option<Spanned<Vec<Type>>>,
    pub is_macro: Option<Loc>,
}

// A path root.
// For now these should never have tyargs or macro call set (though the type arguments will be
// used for enums).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootPathEntry {
    pub name: LeadingNameAccess,
    pub tyargs: Option<Spanned<Vec<Type>>>,
    pub is_macro: Option<Loc>,
}

// INVARIANT: entries should be non-zero, or this should be converted to a `SingleName`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamePath {
    pub root: RootPathEntry,
    pub entries: Vec<PathEntry>,
    // if parsing of this name path was incomplete
    pub is_incomplete: bool,
}

// See the NameAccess trait below for usage.
// INVARIANT: never push onto a Single. A Single is a final form, demoted from a Path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameAccessChain_ {
    Single(PathEntry),
    Path(NamePath),
}
pub type NameAccessChain = Spanned<NameAccessChain_>;

//**************************************************************************************************
// Types
//**************************************************************************************************

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub enum Ability_ {
    Copy,
    Drop,
    Store,
    Key,
}
pub type Ability = Spanned<Ability_>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type_ {
    // N
    // N<t1, ... , tn>
    Apply(Box<NameAccessChain>),
    // &t
    // &mut t
    Ref(bool, Box<Type>),
    // (t1,...,tn):t
    Fun(Vec<Type>, Box<Type>),
    // ()
    Unit,
    // (t1, t2, ... , tn)
    // Used for return values and expression blocks
    Multiple(Vec<Type>),
    UnresolvedError,
}
pub type Type = Spanned<Type_>;

//**************************************************************************************************
// Expressions
//**************************************************************************************************

new_name!(Var);

// Some with loc if the local had a `mut` prefix
pub type Mutability = Option<Loc>;

#[derive(Debug, Clone, PartialEq)]
pub enum FieldBindings {
    Named(Vec<Ellipsis<(Field, Bind)>>),
    Positional(Vec<Ellipsis<Bind>>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Bind_ {
    // mut x
    // x
    Var(Mutability, Var),
    // T { f1: b1, ... fn: bn }
    // T<t1, ... , tn> { f1: b1, ... fn: bn }
    // T ( b1, ... bn )
    // T<t1, ... , tn> ( b1, ... bn )
    Unpack(Box<NameAccessChain>, FieldBindings),
}
pub type Bind = Spanned<Bind_>;
// b1, ..., bn
pub type BindList = Spanned<Vec<Bind>>;

pub type BindWithRange = Spanned<(Bind, Exp)>;
pub type BindWithRangeList = Spanned<Vec<BindWithRange>>;

pub type LambdaBindings_ = Vec<(BindList, Option<Type>)>;
pub type LambdaBindings = Spanned<LambdaBindings_>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value_ {
    // @<num>
    Address(LeadingNameAccess),
    // <num>(u8|u16|u32|u64|u128|u256)?
    Num(Symbol),
    // false
    Bool(bool),
    // x"[0..9A..F]+"
    HexString(Symbol),
    // b"(<ascii> | \n | \r | \t | \\ | \0 | \" | \x[0..9A..F][0..9A..F])+"
    ByteString(Symbol),
}
pub type Value = Spanned<Value_>;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum UnaryOp_ {
    // !
    Not,
}
pub type UnaryOp = Spanned<UnaryOp_>;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum BinOp_ {
    // Int ops
    // +
    Add,
    // -
    Sub,
    // *
    Mul,
    // %
    Mod,
    // /
    Div,
    // |
    BitOr,
    // &
    BitAnd,
    // ^
    Xor,
    // <<
    Shl,
    // >>
    Shr,
    // ..
    Range, // spec only

    // Bool ops
    // ==>
    Implies, // spec only
    // <==>
    Iff,
    // &&
    And,
    // ||
    Or,

    // Compare Ops
    // ==
    Eq,
    // !=
    Neq,
    // <
    Lt,
    // >
    Gt,
    // <=
    Le,
    // >=
    Ge,
}
pub type BinOp = Spanned<BinOp_>;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum QuantKind_ {
    Forall,
    Exists,
    Choose,
    ChooseMin,
}
pub type QuantKind = Spanned<QuantKind_>;

new_name!(BlockLabel);

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Exp_ {
    Value(Value),
    // move e
    Move(Loc, Box<Exp>),
    // copy e
    Copy(Loc, Box<Exp>),
    // [m::]n[<t1, .., tn>]
    Name(NameAccessChain),

    // f(earg,*)
    // f!(earg,*)
    Call(NameAccessChain, Spanned<Vec<Exp>>),

    // tn {f1: e1, ... , f_n: e_n }
    Pack(NameAccessChain, Vec<(Field, Exp)>),

    // vector [ e1, ..., e_n ]
    // vector<t> [e1, ..., en ]
    Vector(
        /* name loc */ Loc,
        Option<Vec<Type>>,
        Spanned<Vec<Exp>>,
    ),

    // if (eb) et else ef
    IfElse(Box<Exp>, Box<Exp>, Option<Box<Exp>>),
    // match subject arms
    Match(Box<Exp>, Spanned<Vec<MatchArm>>),
    // while (eb) eloop
    While(Box<Exp>, Box<Exp>),
    // loop eloop
    Loop(Box<Exp>),

    // 'label: e
    Labeled(BlockLabel, Box<Exp>),
    // { seq }
    Block(Sequence),
    // |lv1, ..., lvn| e
    // |lv1, ..., lvn| -> { e }
    Lambda(LambdaBindings, Option<Type>, Box<Exp>),
    // forall/exists x1 : e1, ..., xn [{ t1, .., tk } *] [where cond]: en.
    Quant(
        QuantKind,
        BindWithRangeList,
        Vec<Vec<Exp>>,
        Option<Box<Exp>>,
        Box<Exp>,
    ), // spec only
    // (e1, ..., en)
    ExpList(Vec<Exp>),
    // ()
    Unit,
    // (e)
    Parens(Box<Exp>),

    // a = e
    Assign(Box<Exp>, Box<Exp>),

    // abort e
    Abort(Option<Box<Exp>>),
    // return e
    Return(Option<BlockLabel>, Option<Box<Exp>>),
    // break
    Break(Option<BlockLabel>, Option<Box<Exp>>),
    // continue
    Continue(Option<BlockLabel>),

    // *e
    Dereference(Box<Exp>),
    // op e
    UnaryExp(UnaryOp, Box<Exp>),
    // e1 op e2
    BinopExp(Box<Exp>, BinOp, Box<Exp>),

    // &e
    // &mut e
    Borrow(bool, Box<Exp>),

    // e.f (along with the location of the dot)
    Dot(Box<Exp>, /* dot location */ Loc, Name),
    // e.f(earg,*)
    DotCall(
        Box<Exp>,
        Loc, // location of the dot
        Name,
        /* is_macro */ Option<Loc>,
        Option<Vec<Type>>,
        Spanned<Vec<Exp>>,
    ),
    // e[e']
    Index(Box<Exp>, Spanned<Vec<Exp>>), // spec only

    // (e as t)
    Cast(Box<Exp>, Type),
    // (e: t)
    Annotate(Box<Exp>, Type),

    // spec { ... }
    Spec(Spanned<String>),

    // Internal node marking an error was added to the error list
    // This is here so the pass can continue even when an error is hit
    UnresolvedError,
    // e.X, where X is not a valid tok after dot and cannot be parsed (includes location of the dot)
    DotUnresolved(Loc, Box<Exp>),
}
pub type Exp = Spanned<Exp_>;

// { e1; ... ; en }
// { e1; ... ; en; }
// The Loc field holds the source location of the final semicolon, if there is one.
pub type Sequence = (
    Vec<UseDecl>,
    Vec<SequenceItem>,
    Option<Loc>,
    Box<Option<Exp>>,
);
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum SequenceItem_ {
    // e;
    Seq(Box<Exp>),
    // let b : t = e;
    // let b = e;
    Declare(BindList, Option<Type>),
    // let b : t = e;
    // let b = e;
    Bind(BindList, Option<Type>, Box<Exp>),
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
    // T<t1, ..., tn>(pat1, ..., patn)
    PositionalConstructor(NameAccessChain, Spanned<Vec<Ellipsis<MatchPattern>>>),
    // T<t1, ..., tn> { x1: pat1, ..., xn: patn }
    FieldConstructor(
        NameAccessChain,
        Spanned<Vec<Ellipsis<(Field, MatchPattern)>>>,
    ),
    // T<t1, ..., tn>
    Name(Mutability, NameAccessChain),
    // 0 | true | ...
    Literal(Value),
    // pat1 | pat2
    Or(Box<MatchPattern>, Box<MatchPattern>),
    // x @ pat
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

impl TName for Ability {
    type Key = Ability_;
    type Loc = Loc;

    fn drop_loc(self) -> (Self::Loc, Self::Key) {
        let sp!(loc, ab_) = self;
        (loc, ab_)
    }

    fn add_loc(loc: Self::Loc, key: Self::Key) -> Self {
        sp(loc, key)
    }

    fn borrow(&self) -> (&Self::Loc, &Self::Key) {
        (&self.loc, &self.value)
    }
}

impl fmt::Debug for LeadingNameAccess_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

//**************************************************************************************************
// Impl
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

impl DocComment {
    pub fn empty() -> Self {
        Self(None)
    }

    pub fn loc(&self) -> Option<Loc> {
        self.0.as_ref().map(|sp!(loc, _)| *loc)
    }

    pub fn comment(&self) -> Option<Spanned<&str>> {
        self.0.as_ref().map(|sp!(loc, s)| sp(*loc, s.as_str()))
    }

    /// Returns the empty string if there is no doc comment.
    pub fn text(&self) -> &str {
        self.0.as_ref().map(|sp!(_, s)| s.as_str()).unwrap_or("")
    }
}

impl LeadingNameAccess_ {
    pub const fn anonymous(address: NumericalAddress) -> Self {
        Self::AnonymousAddress(address)
    }
}

impl NameAccessChain_ {
    pub fn single(name: Name) -> Self {
        NameAccessChain_::Single(PathEntry {
            name,
            tyargs: None,
            is_macro: None,
        })
    }

    pub fn path(root: RootPathEntry) -> NamePath {
        NamePath {
            root,
            entries: vec![],
            is_incomplete: false,
        }
    }
}

impl NamePath {
    /// Destructively take the type arguments, if any.
    pub(crate) fn take_tyargs(&mut self) -> Option<Spanned<Vec<Type>>> {
        if self.root.tyargs.is_some() {
            return std::mem::take(&mut self.root.tyargs);
        }
        for entry in self.entries.iter_mut() {
            if entry.tyargs.is_some() {
                return std::mem::take(&mut entry.tyargs);
            }
        }
        None
    }
}

// Possibly move this trait out of `ast.rs`?
#[allow(clippy::len_without_is_empty)]
pub trait NameAccess {
    fn is_macro(&self) -> Option<&Loc>;
    fn tyargs(&self) -> Option<&Spanned<Vec<Type>>>;

    fn push_path_entry(
        &mut self,
        name: Name,
        tyargs: Option<Spanned<Vec<Type>>>,
        is_macro: Option<Loc>,
    ) -> Vec<Diagnostic>;

    fn has_tyargs(&self) -> bool;
    fn tyargs_loc(&self) -> Option<Loc>;
    fn has_tyargs_last(&self) -> bool;
    fn len(&self) -> usize;
}

impl NameAccess for PathEntry {
    fn is_macro(&self) -> Option<&Loc> {
        self.is_macro.as_ref()
    }

    fn tyargs(&self) -> Option<&Spanned<Vec<Type>>> {
        self.tyargs.as_ref()
    }

    fn push_path_entry(
        &mut self,
        name: Name,
        _tyargs: Option<Spanned<Vec<Type>>>,
        _is_macro: Option<Loc>,
    ) -> Vec<Diagnostic> {
        let diag = ice!((name.loc, "Tried adding this name to a Single chain"));
        vec![diag]
    }

    fn has_tyargs(&self) -> bool {
        self.tyargs.is_some()
    }

    fn has_tyargs_last(&self) -> bool {
        true
    }

    fn tyargs_loc(&self) -> Option<Loc> {
        self.tyargs.as_ref().map(|sp!(loc, _)| *loc)
    }

    fn len(&self) -> usize {
        1
    }
}

impl NameAccess for NamePath {
    fn is_macro(&self) -> Option<&Loc> {
        self.root
            .is_macro
            .as_ref()
            .or_else(|| self.entries.iter().find_map(|e| e.is_macro.as_ref()))
    }

    fn tyargs(&self) -> Option<&Spanned<Vec<Type>>> {
        self.root
            .tyargs
            .as_ref()
            .or_else(|| self.entries.iter().find_map(|e| e.tyargs.as_ref()))
    }

    fn push_path_entry(
        &mut self,
        name: Name,
        tyargs: Option<Spanned<Vec<Type>>>,
        is_macro: Option<Loc>,
    ) -> Vec<Diagnostic> {
        let mut diags: Vec<Diagnostic> = vec![];

        let mut final_tyargs = tyargs;

        if let (Some(prev_loc), Some(sp!(new_loc, _))) = (self.tyargs_loc(), &final_tyargs) {
            let mut diag = diag!(
                Syntax::InvalidName,
                (
                    *new_loc,
                    "Paths cannot include type arguments more than once"
                ),
                (prev_loc, "Previous type arguments appeared here")
            );
            diag.add_note("Type arguments should only appear on module members");
            diags.push(diag);
            // If we already had tyargs, remove these.
            final_tyargs = None;
        }

        if let Some(prev_loc) = self.is_macro() {
            let diag = diag!(
                Syntax::InvalidName,
                (
                    name.loc,
                    "A macro call cannot have name access entries after it"
                ),
                (*prev_loc, "Macro invocation given here")
            );
            diags.push(diag);
            // If this is a macro, remove previous `!` usages.
            self.root.is_macro = None;
            for entry in self.entries.iter_mut() {
                entry.is_macro = None;
            }
        }

        if self.len() > 3 {
            let diag = diag!(
                Syntax::InvalidName,
                (name.loc, "Paths cannot have length greater than four")
            );
            diags.push(diag);
        } else {
            let path_entry = PathEntry {
                name,
                tyargs: final_tyargs,
                is_macro,
            };
            self.entries.push(path_entry);
        }
        diags
    }

    fn has_tyargs(&self) -> bool {
        self.root.tyargs.is_some() || self.entries.iter().any(|entry| entry.tyargs.is_some())
    }

    fn has_tyargs_last(&self) -> bool {
        if !self.has_tyargs() {
            // Tyargs are last vacuously
            true
        } else if let Some(last) = self.entries.last() {
            last.tyargs.is_some()
        } else {
            self.entries.is_empty() && self.root.tyargs.is_some()
        }
    }

    fn tyargs_loc(&self) -> Option<Loc> {
        self.tyargs().map(|tyarg_ref| tyarg_ref.loc)
    }

    fn len(&self) -> usize {
        1 + self.entries.len()
    }
}

macro_rules! forward_name_access {
    ($self:ident.$call:ident($($args:ident),*)) => {
        match $self {
            NameAccessChain_::Single(entry) => entry.$call($($args),*),
            NameAccessChain_::Path(entry) => entry.$call($($args),*),
        }
    }
}

impl NameAccess for NameAccessChain_ {
    fn is_macro(&self) -> Option<&Loc> {
        forward_name_access!(self.is_macro())
    }

    fn tyargs(&self) -> Option<&Spanned<Vec<Type>>> {
        forward_name_access!(self.tyargs())
    }

    fn push_path_entry(
        &mut self,
        name: Name,
        tyargs: Option<Spanned<Vec<Type>>>,
        is_macro: Option<Loc>,
    ) -> Vec<Diagnostic> {
        forward_name_access!(self.push_path_entry(name, tyargs, is_macro))
    }

    fn has_tyargs(&self) -> bool {
        forward_name_access!(self.has_tyargs())
    }

    fn has_tyargs_last(&self) -> bool {
        forward_name_access!(self.has_tyargs_last())
    }

    fn tyargs_loc(&self) -> Option<Loc> {
        forward_name_access!(self.tyargs_loc())
    }

    fn len(&self) -> usize {
        forward_name_access!(self.len())
    }
}

impl Definition {
    pub fn file_hash(&self) -> FileHash {
        match self {
            Definition::Module(m) => m.loc.file_hash(),
            Definition::Address(a) => a.loc.file_hash(),
        }
    }

    pub fn name_loc(&self) -> Loc {
        match self {
            Definition::Module(mdef) => mdef.name.loc(),
            Definition::Address(aref) => aref.addr.loc,
        }
    }
}

impl ModuleName {
    pub const SELF_NAME: &'static str = "Self";
}

impl Var {
    pub const SYNTAX_IDENT_START: char = '$';

    pub fn is_underscore(&self) -> bool {
        self.0.value == symbol!("_")
    }

    pub fn starts_with_underscore(&self) -> bool {
        Self::starts_with_underscore_name(self.0.value)
    }

    pub fn starts_with_underscore_name(n: Symbol) -> bool {
        n.starts_with('_') || n.starts_with("$_")
    }

    pub fn is_syntax_identifier(&self) -> bool {
        Self::is_syntax_identifier_name(self.0.value)
    }

    pub fn is_syntax_identifier_name(n: Symbol) -> bool {
        n.starts_with(Self::SYNTAX_IDENT_START)
    }

    pub fn is_valid(&self) -> bool {
        Self::is_valid_name(self.0.value)
    }

    pub fn is_valid_name(s: Symbol) -> bool {
        s.starts_with('_')
            || s.starts_with(|c: char| c.is_ascii_lowercase())
            || Self::is_syntax_identifier_name(s)
    }
}

impl Ability_ {
    pub const COPY: &'static str = "copy";
    pub const DROP: &'static str = "drop";
    pub const STORE: &'static str = "store";
    pub const KEY: &'static str = "key";

    /// For a struct with ability `a`, each field needs to have the ability `a.requires()`.
    /// Consider a generic type Foo<t1, ..., tn>, for Foo<t1, ..., tn> to have ability `a`, Foo must
    /// have been declared with `a` and each type argument ti must have the ability `a.requires()`
    pub fn requires(self) -> Ability_ {
        match self {
            Ability_::Copy => Ability_::Copy,
            Ability_::Drop => Ability_::Drop,
            Ability_::Store => Ability_::Store,
            Ability_::Key => Ability_::Store,
        }
    }

    /// An inverse of `requires`, where x is in a.required_by() iff x.requires() == a
    pub fn required_by(self) -> Vec<Ability_> {
        match self {
            Self::Copy => vec![Ability_::Copy],
            Self::Drop => vec![Ability_::Drop],
            Self::Store => vec![Ability_::Store, Ability_::Key],
            Self::Key => vec![],
        }
    }
}

impl Type_ {
    pub fn unit(loc: Loc) -> Type {
        sp(loc, Type_::Unit)
    }
}

impl UnaryOp_ {
    pub const NOT: &'static str = "!";

    pub fn symbol(&self) -> &'static str {
        use UnaryOp_ as U;
        match self {
            U::Not => U::NOT,
        }
    }

    pub fn is_pure(&self) -> bool {
        use UnaryOp_ as U;
        match self {
            U::Not => true,
        }
    }
}

impl BinOp_ {
    pub const ADD: &'static str = "+";
    pub const SUB: &'static str = "-";
    pub const MUL: &'static str = "*";
    pub const MOD: &'static str = "%";
    pub const DIV: &'static str = "/";
    pub const BIT_OR: &'static str = "|";
    pub const BIT_AND: &'static str = "&";
    pub const XOR: &'static str = "^";
    pub const SHL: &'static str = "<<";
    pub const SHR: &'static str = ">>";
    pub const AND: &'static str = "&&";
    pub const OR: &'static str = "||";
    pub const EQ: &'static str = "==";
    pub const NEQ: &'static str = "!=";
    pub const LT: &'static str = "<";
    pub const GT: &'static str = ">";
    pub const LE: &'static str = "<=";
    pub const GE: &'static str = ">=";
    pub const IMPLIES: &'static str = "==>";
    pub const IFF: &'static str = "<==>";
    pub const RANGE: &'static str = "..";

    pub fn symbol(&self) -> &'static str {
        use BinOp_ as B;
        match self {
            B::Add => B::ADD,
            B::Sub => B::SUB,
            B::Mul => B::MUL,
            B::Mod => B::MOD,
            B::Div => B::DIV,
            B::BitOr => B::BIT_OR,
            B::BitAnd => B::BIT_AND,
            B::Xor => B::XOR,
            B::Shl => B::SHL,
            B::Shr => B::SHR,
            B::And => B::AND,
            B::Or => B::OR,
            B::Eq => B::EQ,
            B::Neq => B::NEQ,
            B::Lt => B::LT,
            B::Gt => B::GT,
            B::Le => B::LE,
            B::Ge => B::GE,
            B::Implies => B::IMPLIES,
            B::Iff => B::IFF,
            B::Range => B::RANGE,
        }
    }

    pub fn is_pure(&self) -> bool {
        use BinOp_ as B;
        match self {
            B::Add | B::Sub | B::Mul | B::Mod | B::Div | B::Shl | B::Shr => false,
            B::BitOr
            | B::BitAnd
            | B::Xor
            | B::And
            | B::Or
            | B::Eq
            | B::Neq
            | B::Lt
            | B::Gt
            | B::Le
            | B::Ge
            | B::Range
            | B::Implies
            | B::Iff => true,
        }
    }

    pub fn is_spec_only(&self) -> bool {
        use BinOp_ as B;
        matches!(self, B::Range | B::Implies | B::Iff)
    }
}

impl Visibility {
    pub const FRIEND: &'static str = "public(friend)";
    pub const FRIEND_IDENT: &'static str = "friend";
    pub const INTERNAL: &'static str = "";
    pub const PACKAGE: &'static str = "public(package)";
    pub const PACKAGE_IDENT: &'static str = "package";
    pub const PUBLIC: &'static str = "public";

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
// Display
//**************************************************************************************************

impl fmt::Display for LeadingNameAccess_ {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::AnonymousAddress(bytes) => write!(f, "{}", bytes),
            Self::GlobalAddress(n) => write!(f, "::{}", n),
            Self::Name(n) => write!(f, "{}", n),
        }
    }
}

impl fmt::Display for ModuleIdent_ {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}::{}", self.address, &self.module)
    }
}

impl fmt::Display for RootPathEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl fmt::Display for PathEntry {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl fmt::Display for NamePath {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.root)?;
        for entry in self.entries.iter() {
            write!(f, "::{}", entry.name)?;
        }
        Ok(())
    }
}

impl fmt::Display for NameAccessChain_ {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match self {
            NameAccessChain_::Single(entry) => entry.fmt(f),
            NameAccessChain_::Path(entry) => entry.fmt(f),
        }
    }
}

impl fmt::Display for UnaryOp_ {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.symbol())
    }
}

impl fmt::Display for BinOp_ {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.symbol())
    }
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match &self {
                Visibility::Friend(_) => Visibility::FRIEND,
                Visibility::Internal => Visibility::INTERNAL,
                Visibility::Package(_) => Visibility::PACKAGE,
                Visibility::Public(_) => Visibility::PUBLIC,
            }
        )
    }
}

impl fmt::Display for Ability_ {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match &self {
                Ability_::Copy => Ability_::COPY,
                Ability_::Drop => Ability_::DROP,
                Ability_::Store => Ability_::STORE,
                Ability_::Key => Ability_::KEY,
            }
        )
    }
}

impl fmt::Display for Type_ {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        use Type_::*;
        match self {
            Apply(n) => write!(f, "{}", n),
            Ref(mut_, ty) => write!(f, "&{}{}", if *mut_ { "mut " } else { "" }, ty),
            Fun(args, result) => write!(f, "({}):{}", format_comma(args), result),
            Unit => write!(f, "()"),
            Multiple(tys) => {
                write!(f, "(")?;
                write!(f, "{}", format_comma(tys))?;
                write!(f, ")")
            }
            UnresolvedError => write!(f, "_|_"),
        }
    }
}

//**************************************************************************************************
// Debug
//**************************************************************************************************

impl AstDebug for Program {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Self {
            named_address_maps,
            source_definitions,
            lib_definitions,
        } = self;
        w.writeln("------ Lib Defs: ------");
        for def in lib_definitions {
            ast_debug_package_definition(w, named_address_maps, def)
        }
        w.new_line();
        w.writeln("------ Source Defs: ------");
        for def in source_definitions {
            ast_debug_package_definition(w, named_address_maps, def)
        }
    }
}

fn ast_debug_package_definition(
    w: &mut AstWriter,
    named_address_maps: &NamedAddressMaps,
    pkg: &PackageDefinition,
) {
    let PackageDefinition {
        package,
        named_address_map,
        def,
        target_kind: _,
    } = pkg;
    match package {
        Some(n) => w.writeln(format!("package: {}", n)),
        None => w.writeln("no package"),
    }
    named_address_maps.get(*named_address_map).ast_debug(w);
    def.ast_debug(w);
}

impl AstDebug for TargetKind {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.writeln(match self {
            TargetKind::Source {
                is_root_package: true,
            } => "root module".to_string(),
            TargetKind::Source {
                is_root_package: false,
            } => "dependency module".to_string(),
            TargetKind::External(k) => format!("external module {:?}", k),
        });
    }
}

impl AstDebug for DocComment {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Some(sp!(_, s)) = &self.0 else { return };
        if w.is_verbose() {
            w.writeln("/** ");
            w.writeln(s);
            w.writeln("**/");
        }
    }
}

impl AstDebug for NamedAddressMap {
    fn ast_debug(&self, w: &mut AstWriter) {
        for (sym, addr) in self {
            w.write(format!("{} => {}", sym, addr));
            w.new_line()
        }
    }
}

impl AstDebug for Definition {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            Definition::Address(a) => a.ast_debug(w),
            Definition::Module(m) => m.ast_debug(w),
        }
    }
}

impl AstDebug for AddressDefinition {
    fn ast_debug(&self, w: &mut AstWriter) {
        let AddressDefinition {
            attributes,
            loc: _loc,
            addr,
            modules,
        } = self;
        attributes.ast_debug(w);
        w.write(format!("address {}", addr));
        w.writeln(" {{");
        for m in modules {
            m.ast_debug(w)
        }
        w.writeln("}");
    }
}

impl AstDebug for AttributeValue_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            AttributeValue_::Value(v) => v.ast_debug(w),
            AttributeValue_::ModuleAccess(n) => n.ast_debug(w),
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
                w.list(&inners.value, ", ", |w, inner| {
                    inner.ast_debug(w);
                    false
                });
                w.write(")");
            }
        }
    }
}

impl AstDebug for Vec<Attribute> {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write("#[");
        w.list(self, ", ", |w, attr| {
            attr.ast_debug(w);
            false
        });
        w.write("]");
    }
}

impl AstDebug for Vec<Attributes> {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.list(self, "", |w, attrs| {
            attrs.ast_debug(w);
            true
        });
    }
}

impl AstDebug for ModuleDefinition {
    fn ast_debug(&self, w: &mut AstWriter) {
        let ModuleDefinition {
            doc,
            attributes,
            loc: _loc,
            address,
            name,
            is_spec_module,
            members,
            definition_mode: _,
        } = self;
        doc.ast_debug(w);
        attributes.ast_debug(w);
        match address {
            None => w.write(format!(
                "module {}{}",
                if *is_spec_module { "spec " } else { "" },
                name
            )),
            Some(addr) => w.write(format!("module {}::{}", addr, name)),
        };
        w.block(|w| {
            for mem in members {
                mem.ast_debug(w)
            }
        });
    }
}

impl AstDebug for ModuleMember {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            ModuleMember::Function(f) => f.ast_debug(w),
            ModuleMember::Struct(s) => s.ast_debug(w),
            ModuleMember::Enum(e) => e.ast_debug(w),
            ModuleMember::Use(u) => u.ast_debug(w),
            ModuleMember::Friend(f) => f.ast_debug(w),
            ModuleMember::Constant(c) => c.ast_debug(w),
            ModuleMember::Spec(s) => w.write(&s.value),
        }
    }
}

impl AstDebug for UseDecl {
    fn ast_debug(&self, w: &mut AstWriter) {
        let UseDecl {
            doc,
            attributes,
            loc: _,
            use_,
        } = self;
        doc.ast_debug(w);
        attributes.ast_debug(w);
        use_.ast_debug(w);
    }
}

impl AstDebug for ModuleUse {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            ModuleUse::Module(alias) => {
                alias.map(|alias| w.write(format!("as {}", alias)));
            }
            ModuleUse::Members(members) => w.block(|w| {
                w.comma(members, |w, (name, alias)| {
                    w.write(format!("{}", name));
                    alias.map(|alias| w.write(format!("as {}", alias.value)));
                })
            }),
            ModuleUse::Partial {
                colon_colon,
                opening_brace,
            } => {
                colon_colon.map(|_| w.write("::"));
                opening_brace.map(|_| w.write("{"));
            }
        }
    }
}

impl AstDebug for Use {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write("use ");
        match self {
            Use::ModuleUse(mident, use_) => {
                w.write(format!("{}", mident));
                use_.ast_debug(w);
            }
            Use::NestedModuleUses(addr, entries) => {
                w.write(format!("{}::", addr));
                w.block(|w| {
                    w.comma(entries, |w, (name, use_)| {
                        w.write(format!("{}::", name));
                        use_.ast_debug(w);
                    })
                })
            }
            Use::Fun {
                visibility,
                function,
                ty,
                method,
            } => {
                visibility.ast_debug(w);
                w.write(" use fun ");
                function.ast_debug(w);
                w.write(" as ");
                ty.ast_debug(w);
                w.write(format!(".{method}"));
            }
            Use::Partial {
                package,
                colon_colon,
                opening_brace,
            } => {
                w.write(package.to_string());
                colon_colon.map(|_| w.write("::"));
                opening_brace.map(|_| w.write("{"));
            }
        }
        w.write(";")
    }
}

impl AstDebug for FriendDecl {
    fn ast_debug(&self, w: &mut AstWriter) {
        let FriendDecl {
            attributes,
            loc: _,
            friend,
        } = self;
        attributes.ast_debug(w);
        w.write(format!("friend {}", friend));
    }
}

impl AstDebug for EnumDefinition {
    fn ast_debug(&self, w: &mut AstWriter) {
        let EnumDefinition {
            doc,
            attributes,
            loc: _loc,
            abilities,
            name,
            type_parameters,
            variants,
        } = self;
        doc.ast_debug(w);
        attributes.ast_debug(w);

        if !abilities.is_empty() {
            w.write("[");
            w.list(abilities, " ", |w, ab_mod| {
                ab_mod.ast_debug(w);
                false
            });
            w.write("]");
        }

        w.write(format!(" enum {}", name));
        type_parameters.ast_debug(w);
        w.block(|w| {
            w.list(variants, ",", |w, variant| {
                variant.ast_debug(w);
                true
            })
        });
    }
}

impl AstDebug for VariantDefinition {
    fn ast_debug(&self, w: &mut AstWriter) {
        let VariantDefinition {
            doc,
            loc: _,
            name,
            fields,
        } = self;
        doc.ast_debug(w);
        w.write(format!("{}", name));
        match fields {
            VariantFields::Named(fields) => w.block(|w| {
                w.semicolon(fields, |w, (doc, f, st)| {
                    doc.ast_debug(w);
                    w.write(format!("{}: ", f));
                    st.ast_debug(w);
                });
            }),
            VariantFields::Positional(types) => w.block(|w| {
                w.semicolon(types.iter().enumerate(), |w, (i, (doc, st))| {
                    doc.ast_debug(w);
                    w.write(format!("pos{}: ", i));
                    st.ast_debug(w);
                });
            }),
            VariantFields::Empty => (),
        }
    }
}

impl AstDebug for StructDefinition {
    fn ast_debug(&self, w: &mut AstWriter) {
        let StructDefinition {
            doc,
            attributes,
            loc: _loc,
            abilities,
            name,
            type_parameters,
            fields,
        } = self;
        doc.ast_debug(w);
        attributes.ast_debug(w);

        w.list(abilities, " ", |w, ab_mod| {
            ab_mod.ast_debug(w);
            false
        });

        if let StructFields::Native(_) = fields {
            w.write("native ");
        }

        w.write(format!("struct {}", name));
        type_parameters.ast_debug(w);
        match fields {
            StructFields::Named(fields) => w.block(|w| {
                w.semicolon(fields, |w, (doc, f, st)| {
                    doc.ast_debug(w);
                    w.write(format!("{}: ", f));
                    st.ast_debug(w);
                });
            }),
            StructFields::Positional(types) => w.block(|w| {
                w.semicolon(types.iter().enumerate(), |w, (i, (doc, st))| {
                    doc.ast_debug(w);
                    w.write(format!("pos{}: ", i));
                    st.ast_debug(w);
                });
            }),
            StructFields::Native(_) => (),
        }
    }
}

impl AstDebug for Function {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Function {
            doc,
            attributes,
            loc: _loc,
            visibility,
            entry,
            macro_,
            signature,
            name,
            body,
        } = self;
        doc.ast_debug(w);
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
        w.write(format!("fun {}", name));
        signature.ast_debug(w);
        match &body.value {
            FunctionBody_::Defined(body) => w.block(|w| body.ast_debug(w)),
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
        w.comma(parameters, |w, (mut_, v, st)| {
            if mut_.is_some() {
                w.write("mut ");
            }
            w.write(format!("{}: ", v));
            st.ast_debug(w);
        });
        w.write(")");
        w.write(": ");
        return_type.ast_debug(w)
    }
}

impl AstDebug for Constant {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Constant {
            doc,
            attributes,
            loc: _loc,
            name,
            signature,
            value,
        } = self;
        doc.ast_debug(w);
        attributes.ast_debug(w);
        w.write(format!("const {}:", name));
        signature.ast_debug(w);
        w.write(" = ");
        value.ast_debug(w);
        w.write(";");
    }
}

impl AstDebug for Vec<(Name, Vec<Ability>)> {
    fn ast_debug(&self, w: &mut AstWriter) {
        if !self.is_empty() {
            w.write("<");
            w.comma(self, |w, tp| tp.ast_debug(w));
            w.write(">")
        }
    }
}

impl AstDebug for (Name, Vec<Ability>) {
    fn ast_debug(&self, w: &mut AstWriter) {
        let (n, abilities) = self;
        w.write(n.value);
        ability_constraints_ast_debug(w, abilities);
    }
}

impl AstDebug for Vec<DatatypeTypeParameter> {
    fn ast_debug(&self, w: &mut AstWriter) {
        if !self.is_empty() {
            w.write("<");
            w.comma(self, |w, tp| tp.ast_debug(w));
            w.write(">");
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
        ability_constraints_ast_debug(w, constraints);
    }
}

fn ability_constraints_ast_debug(w: &mut AstWriter, abilities: &[Ability]) {
    if !abilities.is_empty() {
        w.write(": ");
        w.list(abilities, "+", |w, ab| {
            ab.ast_debug(w);
            false
        })
    }
}

impl AstDebug for Ability_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write(format!("{}", self))
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
            Type_::Apply(m) => {
                m.ast_debug(w);
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

impl AstDebug for Vec<Type> {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.comma(self, |w, s| s.ast_debug(w))
    }
}

impl AstDebug for RootPathEntry {
    fn ast_debug(&self, w: &mut AstWriter) {
        let RootPathEntry {
            name,
            tyargs,
            is_macro,
        } = self;
        w.write(format!("{}", name));
        if is_macro.is_some() {
            w.write("!");
        }
        if let Some(ts) = tyargs {
            w.write("<");
            ts.ast_debug(w);
            w.write(">");
        }
    }
}

impl AstDebug for PathEntry {
    fn ast_debug(&self, w: &mut AstWriter) {
        let PathEntry {
            name,
            tyargs,
            is_macro,
        } = self;
        w.write(format!("{}", name));
        if is_macro.is_some() {
            w.write("!");
        }
        if let Some(ts) = tyargs {
            w.write("<");
            ts.ast_debug(w);
            w.write(">");
        }
    }
}

impl AstDebug for NamePath {
    fn ast_debug(&self, w: &mut AstWriter) {
        let NamePath {
            root,
            entries,
            is_incomplete,
        } = self;
        w.write(format!("{}::", root));
        w.list(entries, "::", |w, e| {
            e.ast_debug(w);
            false
        });
        if *is_incomplete {
            if !entries.is_empty() {
                w.write("::");
            }
            w.write("_#incomplete#_");
        }
    }
}

impl AstDebug for NameAccessChain_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            NameAccessChain_::Single(entry) => entry.ast_debug(w),
            NameAccessChain_::Path(entry) => entry.ast_debug(w),
        }
    }
}

impl AstDebug
    for (
        Vec<UseDecl>,
        Vec<SequenceItem>,
        Option<Loc>,
        Box<Option<Exp>>,
    )
{
    fn ast_debug(&self, w: &mut AstWriter) {
        let (uses, seq, _, last_e) = self;
        for u in uses {
            u.ast_debug(w);
            w.new_line();
        }
        w.semicolon(seq, |w, item| item.ast_debug(w));
        if !seq.is_empty() {
            w.writeln(";")
        }
        if let Some(e) = &**last_e {
            e.ast_debug(w)
        }
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
            I::Bind(sp!(_, bs), ty_opt, e) => {
                w.write("let ");
                bs.ast_debug(w);
                if let Some(ty) = ty_opt {
                    ty.ast_debug(w)
                }
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
            E::Unit => w.write("()"),
            E::Parens(e) => {
                w.write("(");
                e.ast_debug(w);
                w.write(")");
            }
            E::Value(v) => v.ast_debug(w),
            E::Move(_, e) => {
                w.write("move ");
                e.ast_debug(w);
            }
            E::Copy(_, e) => {
                w.write("copy ");
                e.ast_debug(w);
            }
            E::Name(ma) => {
                ma.ast_debug(w);
            }
            E::Call(ma, sp!(_, rhs)) => {
                ma.ast_debug(w);
                w.write("(");
                w.comma(rhs, |w, e| e.ast_debug(w));
                w.write(")");
            }
            E::Pack(ma, fields) => {
                ma.ast_debug(w);
                w.write("{");
                w.comma(fields, |w, (f, e)| {
                    w.write(format!("{}: ", f));
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
            E::While(b, e) => {
                w.write("while (");
                b.ast_debug(w);
                w.write(")");
                e.ast_debug(w);
            }
            E::Loop(e) => {
                w.write("loop ");
                e.ast_debug(w);
            }
            E::Labeled(name, e) => {
                w.write(format!("'{name}: "));
                e.ast_debug(w)
            }
            E::Block(seq) => w.block(|w| seq.ast_debug(w)),
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
            E::Assign(lvalue, rhs) => {
                lvalue.ast_debug(w);
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
                w.write("return");
                name.map(|name| w.write(format!(" '{} ", name)));
                if let Some(v) = e {
                    w.write(" ");
                    v.ast_debug(w);
                }
            }
            E::Break(name, e) => {
                w.write("break");
                name.map(|name| w.write(format!(" '{} ", name)));
                if let Some(v) = e {
                    w.write(" ");
                    v.ast_debug(w);
                }
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
            E::Borrow(mut_, e) => {
                w.write("&");
                if *mut_ {
                    w.write("mut ");
                }
                e.ast_debug(w);
            }
            E::Dot(e, _, n) => {
                e.ast_debug(w);
                w.write(format!(".{}", n));
            }
            E::DotCall(e, _, n, is_macro, tyargs, sp!(_, rhs)) => {
                e.ast_debug(w);
                w.write(format!(".{}", n));
                if is_macro.is_some() {
                    w.write("!");
                }
                if let Some(ts) = tyargs {
                    w.write("<");
                    ts.ast_debug(w);
                    w.write("<");
                }
                w.write("(");
                w.comma(rhs, |w, e| e.ast_debug(w));
                w.write(")");
            }
            E::Cast(e, ty) => {
                w.write("(");
                e.ast_debug(w);
                w.write(" as ");
                ty.ast_debug(w);
                w.write(")");
            }
            E::Index(e, rhs) => {
                e.ast_debug(w);
                w.write("[");
                w.comma(&rhs.value, |w, e| e.ast_debug(w));
                w.write("]");
            }
            E::Annotate(e, ty) => {
                w.write("(");
                e.ast_debug(w);
                w.write(": ");
                ty.ast_debug(w);
                w.write(")");
            }
            E::Spec(s) => {
                w.write(&s.value);
            }
            E::UnresolvedError => w.write("_|_"),
            E::DotUnresolved(_, e) => {
                e.ast_debug(w);
                w.write(".");
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
            Ellipsis::Ellipsis(_) => {
                w.write("..");
            }
            Ellipsis::Binder(p) => p.ast_debug(w),
        }
    }
}

impl AstDebug for Ellipsis<(Field, MatchPattern)> {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            Ellipsis::Ellipsis(_) => {
                w.write("..");
            }
            Ellipsis::Binder((n, p)) => {
                w.write(format!("{}: ", n));
                p.ast_debug(w);
            }
        }
    }
}

impl AstDebug for MatchPattern_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        use MatchPattern_::*;
        match self {
            PositionalConstructor(name, fields) => {
                name.ast_debug(w);
                w.write("(");
                w.comma(fields.value.iter(), |w, pat| {
                    pat.ast_debug(w);
                });
                w.write(") ");
            }
            FieldConstructor(name, fields) => {
                name.ast_debug(w);
                w.write(" {");
                w.comma(fields.value.iter(), |w, field_pat| field_pat.ast_debug(w));
                w.write("} ");
            }
            Name(mut_, name) => {
                if mut_.is_some() {
                    w.write("mut ");
                }
                name.ast_debug(w)
            }
            Literal(v) => v.ast_debug(w),
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

impl AstDebug for BinOp_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write(format!("{}", self));
    }
}

impl AstDebug for UnaryOp_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write(format!("{}", self));
    }
}

impl AstDebug for QuantKind_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            QuantKind_::Forall => w.write("forall"),
            QuantKind_::Exists => w.write("exists"),
            QuantKind_::Choose => w.write("choose"),
            QuantKind_::ChooseMin => w.write("min"),
        }
    }
}

impl AstDebug for Vec<BindWithRange> {
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

impl AstDebug for (Bind, Exp) {
    fn ast_debug(&self, w: &mut AstWriter) {
        self.0.ast_debug(w);
        w.write(" in ");
        self.1.ast_debug(w);
    }
}

impl AstDebug for Value_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        use Value_ as V;
        w.write(&match self {
            V::Address(addr) => format!("@{}", addr),
            V::Num(u) => u.to_string(),
            V::Bool(b) => format!("{}", b),
            V::HexString(s) => format!("x\"{}\"", s),
            V::ByteString(s) => format!("b\"{}\"", s),
        })
    }
}

impl AstDebug for Vec<Bind> {
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

impl AstDebug for Vec<Vec<Exp>> {
    fn ast_debug(&self, w: &mut AstWriter) {
        for trigger in self {
            w.write("{");
            w.comma(trigger, |w, b| b.ast_debug(w));
            w.write("}");
        }
    }
}

impl AstDebug for Bind_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        use Bind_ as B;
        match self {
            B::Var(mut_, v) => {
                if mut_.is_some() {
                    w.write("mut ");
                }
                w.write(format!("{}", v))
            }
            B::Unpack(ma, fields) => {
                ma.ast_debug(w);
                fields.ast_debug(w);
            }
        }
    }
}

impl AstDebug for LambdaBindings_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write("|");
        w.comma(self, |w, (b, ty_opt)| {
            b.ast_debug(w);
            if let Some(ty) = ty_opt {
                w.write(": ");
                ty.ast_debug(w);
            }
        });
        w.write("| ");
    }
}

impl AstDebug for Ellipsis<(Field, Bind)> {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            Ellipsis::Ellipsis(_) => {
                w.write("..");
            }
            Ellipsis::Binder((n, b)) => {
                w.write(format!("{}: ", n));
                b.ast_debug(w);
            }
        }
    }
}

impl AstDebug for FieldBindings {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            FieldBindings::Named(bs) => {
                w.write("{");
                w.comma(bs, |w, e| {
                    e.ast_debug(w);
                });
                w.write("}");
            }
            FieldBindings::Positional(bs) => {
                w.write("(");
                w.comma(bs, |w, b| {
                    b.ast_debug(w);
                });
                w.write(")");
            }
        }
    }
}
