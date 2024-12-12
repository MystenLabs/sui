// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diagnostics::warning_filters::{WarningFilters, WarningFiltersTable},
    expansion::ast::{
        ability_modifiers_ast_debug, AbilitySet, Attributes, Friend, ModuleIdent, Mutability,
    },
    naming::ast::{BuiltinTypeName, BuiltinTypeName_, DatatypeTypeParameter, TParam},
    parser::ast::{
        self as P, BinOp, ConstantName, DatatypeName, Field, FunctionName, TargetKind, UnaryOp,
        VariantName, ENTRY_MODIFIER,
    },
    shared::{
        ast_debug::*, program_info::TypingProgramInfo, unique_map::UniqueMap, Name,
        NumericalAddress, TName,
    },
};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

// High Level IR

//**************************************************************************************************
// Program
//**************************************************************************************************

#[derive(Debug, Clone)]
pub struct Program {
    pub info: Arc<TypingProgramInfo>,
    /// Safety: This table should not be dropped as long as any `WarningFilters` are alive
    pub warning_filters_table: Arc<WarningFiltersTable>,
    pub modules: UniqueMap<ModuleIdent, ModuleDefinition>,
}

//**************************************************************************************************
// Modules
//**************************************************************************************************

#[derive(Debug, Clone)]
pub struct ModuleDefinition {
    pub warning_filter: WarningFilters,
    // package name metadata from compiler arguments, not used for any language rules
    pub package_name: Option<Symbol>,
    pub attributes: Attributes,
    pub target_kind: TargetKind,
    /// `dependency_order` is the topological order/rank in the dependency graph.
    pub dependency_order: usize,
    pub friends: UniqueMap<ModuleIdent, Friend>,
    pub structs: UniqueMap<DatatypeName, StructDefinition>,
    pub enums: UniqueMap<DatatypeName, EnumDefinition>,
    pub constants: UniqueMap<ConstantName, Constant>,
    pub functions: UniqueMap<FunctionName, Function>,
}

//**************************************************************************************************
// Structs
//**************************************************************************************************

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct StructDefinition {
    pub warning_filter: WarningFilters,
    // index in the original order as defined in the source file
    pub index: usize,
    pub attributes: Attributes,
    pub abilities: AbilitySet,
    pub type_parameters: Vec<DatatypeTypeParameter>,
    pub fields: StructFields,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum StructFields {
    Defined(Vec<(Field, BaseType)>),
    Native(Loc),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDefinition {
    pub warning_filter: WarningFilters,
    // index in the original order as defined in the source file
    pub index: usize,
    pub attributes: Attributes,
    pub abilities: AbilitySet,
    pub type_parameters: Vec<DatatypeTypeParameter>,
    pub variants: UniqueMap<VariantName, VariantDefinition>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct VariantDefinition {
    // index in the original order as defined in the source file
    pub index: usize,
    pub loc: Loc,
    pub fields: Vec<(Field, BaseType)>,
}

//**************************************************************************************************
// Constants
//**************************************************************************************************

#[derive(PartialEq, Debug, Clone)]
pub struct Constant {
    pub warning_filter: WarningFilters,
    // index in the original order as defined in the source file
    pub index: usize,
    pub attributes: Attributes,
    pub loc: Loc,
    pub signature: BaseType,
    pub value: (UniqueMap<Var, (Mutability, SingleType)>, Block),
}

//**************************************************************************************************
// Functions
//**************************************************************************************************

// package visibility is removed after typing is done
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum Visibility {
    Public(Loc),
    Friend(Loc),
    Internal,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct FunctionSignature {
    pub type_parameters: Vec<TParam>,
    pub parameters: Vec<(Mutability, Var, SingleType)>,
    pub return_type: Type,
}

#[derive(PartialEq, Debug, Clone)]
pub enum FunctionBody_ {
    Native,
    Defined {
        locals: UniqueMap<Var, (Mutability, SingleType)>,
        body: Block,
    },
}
pub type FunctionBody = Spanned<FunctionBody_>;

#[derive(PartialEq, Debug, Clone)]
pub struct Function {
    pub warning_filter: WarningFilters,
    // index in the original order as defined in the source file
    pub index: usize,
    pub attributes: Attributes,
    pub loc: Loc,
    /// The original, declared visibility as defined in the source file
    pub visibility: Visibility,
    /// We sometimes change the visibility of functions, e.g. `entry` is marked as `public` in
    /// test_mode. This is the visibility we will actually emit in the compiled module
    pub compiled_visibility: Visibility,
    pub entry: Option<Loc>,
    pub signature: FunctionSignature,
    pub body: FunctionBody,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy, PartialOrd, Ord)]
pub struct Var(pub Name);

#[derive(PartialEq, Eq, Debug, Clone, Copy, PartialOrd, Ord)]
pub struct BlockLabel(pub Name);

//**************************************************************************************************
// Types
//**************************************************************************************************

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum TypeName_ {
    Builtin(BuiltinTypeName),
    ModuleType(ModuleIdent, DatatypeName),
}
pub type TypeName = Spanned<TypeName_>;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum BaseType_ {
    Param(TParam),
    Apply(AbilitySet, TypeName, Vec<BaseType>),
    Unreachable,
    UnresolvedError,
}
pub type BaseType = Spanned<BaseType_>;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SingleType_ {
    Base(BaseType),
    Ref(bool, BaseType),
}
pub type SingleType = Spanned<SingleType_>;

#[derive(Debug, PartialEq, Eq, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Type_ {
    Unit,
    Single(SingleType),
    Multiple(Vec<SingleType>),
}
pub type Type = Spanned<Type_>;

//**************************************************************************************************
// Statements
//**************************************************************************************************

#[derive(Debug, PartialEq, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Statement_ {
    Command(Command),
    IfElse {
        cond: Box<Exp>,
        if_block: Block,
        else_block: Block,
    },
    VariantMatch {
        subject: Box<Exp>,
        enum_name: DatatypeName,
        arms: Vec<(VariantName, Block)>,
    },
    While {
        name: BlockLabel,
        cond: (Block, Box<Exp>),
        block: Block,
    },
    Loop {
        name: BlockLabel,
        block: Block,
        has_break: bool,
    },
    NamedBlock {
        name: BlockLabel,
        block: Block,
    },
}
pub type Statement = Spanned<Statement_>;

pub type Block = VecDeque<Statement>;

pub type BasicBlocks = BTreeMap<Label, BasicBlock>;

pub type BasicBlock = VecDeque<Command>;

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, PartialOrd, Ord)]
pub struct Label(pub usize);

//**************************************************************************************************
// Commands
//**************************************************************************************************

#[derive(Debug, PartialEq, Eq, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Command_ {
    Assign(AssignCase, Vec<LValue>, Exp),
    Mutate(Box<Exp>, Box<Exp>),
    // Hold location of argument to abort before any inlining or value propagation
    Abort(Loc, Exp),
    Return {
        from_user: bool,
        exp: Exp,
    },
    Break(BlockLabel),
    Continue(BlockLabel),
    IgnoreAndPop {
        pop_num: usize,
        exp: Exp,
    },
    Jump {
        from_user: bool,
        target: Label,
    },
    JumpIf {
        cond: Exp,
        if_true: Label,
        if_false: Label,
    },
    VariantSwitch {
        subject: Exp,
        enum_name: DatatypeName,
        arms: Vec<(VariantName, Label)>,
    },
}
pub type Command = Spanned<Command_>;

// TODO: replace this with the `move_ir_types` one when possible.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum UnpackType {
    ByValue,
    ByImmRef,
    ByMutRef,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum LValue_ {
    Ignore,
    Var {
        var: Var,
        ty: Box<SingleType>,
        unused_assignment: bool,
    },
    Unpack(DatatypeName, Vec<BaseType>, Vec<(Field, LValue)>),
    UnpackVariant(
        DatatypeName,
        VariantName,
        UnpackType,
        Loc, /* rhs_loc */
        Vec<BaseType>,
        Vec<(Field, LValue)>,
    ),
}
pub type LValue = Spanned<LValue_>;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AssignCase {
    // from a let binding
    Let,
    // from an actual assignment
    Update,
}

//**************************************************************************************************
// Expressions
//**************************************************************************************************

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum UnitCase {
    Trailing,
    Implicit,
    FromUser,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ModuleCall {
    pub module: ModuleIdent,
    pub name: FunctionName,
    pub type_arguments: Vec<BaseType>,
    pub arguments: Vec<Exp>,
}

pub type FromUnpack = Option<Loc>;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Value_ {
    // @<address>
    Address(NumericalAddress),
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
    // vector<type> [ <value>,* ]
    Vector(Box<BaseType>, Vec<Value>),
}
pub type Value = Spanned<Value_>;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MoveOpAnnotation {
    // 'move' annotated by the user
    FromUser,
    // inferred based on liveness data
    InferredLastUsage,
    // inferred based on no 'copy' ability
    InferredNoCopy,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum UnannotatedExp_ {
    Unit {
        case: UnitCase,
    },
    Value(Value),
    Move {
        annotation: MoveOpAnnotation,
        var: Var,
    },
    Copy {
        from_user: bool,
        var: Var,
    },
    Constant(ConstantName),
    ErrorConstant {
        line_number_loc: Loc,
        error_constant: Option<ConstantName>,
    },

    ModuleCall(Box<ModuleCall>),
    Freeze(Box<Exp>),
    Vector(Loc, usize, Box<BaseType>, Vec<Exp>),

    Dereference(Box<Exp>),
    UnaryExp(UnaryOp, Box<Exp>),
    BinopExp(Box<Exp>, BinOp, Box<Exp>),

    Pack(DatatypeName, Vec<BaseType>, Vec<(Field, BaseType, Exp)>),
    PackVariant(
        DatatypeName,
        VariantName,
        Vec<BaseType>,
        Vec<(Field, BaseType, Exp)>,
    ),
    Multiple(Vec<Exp>),

    Borrow(bool, Box<Exp>, Field, FromUnpack),
    BorrowLocal(bool, Var),

    Cast(Box<Exp>, BuiltinTypeName),

    Unreachable,

    UnresolvedError,
}
pub type UnannotatedExp = Spanned<UnannotatedExp_>;
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Exp {
    pub ty: Type,
    pub exp: UnannotatedExp,
}
pub fn exp(ty: Type, exp: UnannotatedExp) -> Exp {
    Exp { ty, exp }
}

//**************************************************************************************************
// impls
//**************************************************************************************************

impl FunctionSignature {
    pub fn is_parameter(&self, v: &Var) -> bool {
        self.parameters
            .iter()
            .any(|(_, parameter_name, _)| parameter_name == v)
    }
}

impl Value_ {
    pub fn is_zero(&self) -> bool {
        match self {
            Self::U8(v) => *v == 0,
            Self::U16(v) => *v == 0,
            Self::U32(v) => *v == 0,
            Self::U64(v) => *v == 0,
            Self::U128(v) => *v == 0,
            Self::U256(v) => *v == move_core_types::u256::U256::zero(),
            Self::Address(_) | Self::Bool(_) | Self::Vector(_, _) => false,
        }
    }
}

impl Var {
    pub fn loc(&self) -> Loc {
        self.0.loc
    }

    pub fn value(&self) -> Symbol {
        self.0.value
    }

    pub fn is_underscore(&self) -> bool {
        self.0.value == symbol!("_")
    }

    pub fn starts_with_underscore(&self) -> bool {
        self.0.value.starts_with('_') || self.0.value.starts_with("$_")
    }
}

impl Visibility {
    pub const FRIEND: &'static str = P::Visibility::FRIEND;
    pub const INTERNAL: &'static str = P::Visibility::INTERNAL;
    pub const PUBLIC: &'static str = P::Visibility::PUBLIC;

    pub fn loc(&self) -> Option<Loc> {
        match self {
            Visibility::Friend(loc) | Visibility::Public(loc) => Some(*loc),
            Visibility::Internal => None,
        }
    }
}

impl Command_ {
    pub fn is_terminal(&self) -> bool {
        use Command_::*;
        match self {
            Break(_) | Continue(_) => panic!("ICE break/continue not translated to jumps"),
            Assign(_, _, _) | Mutate(_, _) | IgnoreAndPop { .. } => false,
            Abort(_, _) | Return { .. } | Jump { .. } | JumpIf { .. } | VariantSwitch { .. } => {
                true
            }
        }
    }

    pub fn is_exit(&self) -> bool {
        use Command_::*;
        match self {
            Break(_) | Continue(_) => panic!("ICE break/continue not translated to jumps"),
            Assign(_, _, _)
            | Mutate(_, _)
            | IgnoreAndPop { .. }
            | Jump { .. }
            | JumpIf { .. }
            | VariantSwitch { .. } => false,
            Abort(_, _) | Return { .. } => true,
        }
    }

    pub fn is_unit(&self) -> bool {
        use Command_::*;
        match self {
            Break(_) | Continue(_) => panic!("ICE break/continue not translated to jumps"),
            Assign(_, ls, e) => ls.is_empty() && e.is_unit(),
            IgnoreAndPop { exp: e, .. } => e.is_unit(),

            Mutate(_, _)
            | Return { .. }
            | Abort(_, _)
            | JumpIf { .. }
            | Jump { .. }
            | VariantSwitch { .. } => false,
        }
    }

    pub fn successors(&self) -> BTreeSet<Label> {
        use Command_::*;

        let mut successors = BTreeSet::new();
        match self {
            Break(_) | Continue(_) => panic!("ICE break/continue not translated to jumps"),
            Mutate(_, _) | Assign(_, _, _) | IgnoreAndPop { .. } => {
                panic!("ICE Should not be last command in block")
            }
            Abort(_, _) | Return { .. } => (),
            Jump { target, .. } => {
                successors.insert(*target);
            }
            JumpIf {
                if_true, if_false, ..
            } => {
                successors.insert(*if_true);
                successors.insert(*if_false);
            }
            VariantSwitch {
                subject: _,
                enum_name: _,
                arms,
            } => {
                for (_, target) in arms {
                    successors.insert(*target);
                }
            }
        }
        successors
    }

    pub fn is_hlir_terminal(&self) -> bool {
        use Command_::*;
        match self {
            Assign(_, _, _) | Mutate(_, _) | IgnoreAndPop { .. } => false,
            Break(_) | Continue(_) | Abort(_, _) | Return { .. } => true,
            Jump { .. } | JumpIf { .. } | VariantSwitch { .. } => {
                panic!("ICE found jump/jump-if/variant-switch in hlir")
            }
        }
    }
}

impl Exp {
    pub fn is_unit(&self) -> bool {
        self.exp.value.is_unit()
    }

    pub fn is_unreachable(&self) -> bool {
        self.exp.value.is_unreachable()
    }

    pub fn as_value(&self) -> Option<&Value> {
        self.exp.value.as_value()
    }
}

impl UnannotatedExp_ {
    pub fn is_unit(&self) -> bool {
        matches!(self, UnannotatedExp_::Unit { case: _case })
    }

    pub fn is_unreachable(&self) -> bool {
        matches!(self, UnannotatedExp_::Unreachable)
    }

    pub fn as_value(&self) -> Option<&Value> {
        match self {
            UnannotatedExp_::Value(v) => Some(v),
            _ => None,
        }
    }
}

impl TypeName_ {
    pub fn is<Addr>(&self, address: &Addr, module: impl AsRef<str>, name: impl AsRef<str>) -> bool
    where
        NumericalAddress: PartialEq<Addr>,
    {
        match self {
            TypeName_::Builtin(_) => false,
            TypeName_::ModuleType(mident, n) => {
                mident.value.is(address, module) && n == name.as_ref()
            }
        }
    }

    pub fn datatype_name(&self) -> Option<(ModuleIdent, DatatypeName)> {
        match self {
            TypeName_::Builtin(_) => None,
            TypeName_::ModuleType(mident, n) => Some((*mident, *n)),
        }
    }
}

impl BaseType_ {
    pub fn builtin(loc: Loc, b_: BuiltinTypeName_, ty_args: Vec<BaseType>) -> BaseType {
        use BuiltinTypeName_::*;

        let kind = match b_ {
            U8 | U16 | U32 | U64 | U128 | U256 | Bool | Address => AbilitySet::primitives(loc),
            Signer => AbilitySet::signer(loc),
            Vector => {
                let declared_abilities = AbilitySet::collection(loc);
                let ty_arg_abilities = {
                    assert!(ty_args.len() == 1);
                    ty_args[0].value.abilities(ty_args[0].loc)
                };
                AbilitySet::from_abilities(
                    declared_abilities
                        .into_iter()
                        .filter(|ab| ty_arg_abilities.has_ability_(ab.value.requires())),
                )
                .unwrap()
            }
        };
        let n = sp(loc, TypeName_::Builtin(sp(loc, b_)));
        sp(loc, BaseType_::Apply(kind, n, ty_args))
    }

    pub fn abilities(&self, loc: Loc) -> AbilitySet {
        match self {
            BaseType_::Apply(abilities, _, _) | BaseType_::Param(TParam { abilities, .. }) => {
                abilities.clone()
            }
            BaseType_::Unreachable | BaseType_::UnresolvedError => AbilitySet::all(loc),
        }
    }

    pub fn bool(loc: Loc) -> BaseType {
        Self::builtin(loc, BuiltinTypeName_::Bool, vec![])
    }

    pub fn address(loc: Loc) -> BaseType {
        Self::builtin(loc, BuiltinTypeName_::Address, vec![])
    }

    pub fn u8(loc: Loc) -> BaseType {
        Self::builtin(loc, BuiltinTypeName_::U8, vec![])
    }

    pub fn u16(loc: Loc) -> BaseType {
        Self::builtin(loc, BuiltinTypeName_::U16, vec![])
    }

    pub fn u32(loc: Loc) -> BaseType {
        Self::builtin(loc, BuiltinTypeName_::U32, vec![])
    }

    pub fn u64(loc: Loc) -> BaseType {
        Self::builtin(loc, BuiltinTypeName_::U64, vec![])
    }

    pub fn u128(loc: Loc) -> BaseType {
        Self::builtin(loc, BuiltinTypeName_::U128, vec![])
    }

    pub fn u256(loc: Loc) -> BaseType {
        Self::builtin(loc, BuiltinTypeName_::U256, vec![])
    }

    pub fn type_name(&self) -> Option<&TypeName> {
        match self {
            Self::Apply(_, tn, _) => Some(tn),
            _ => None,
        }
    }

    pub fn is_apply<Addr>(
        &self,
        address: &Addr,
        module: impl AsRef<str>,
        name: impl AsRef<str>,
    ) -> Option<(&AbilitySet, &TypeName, &[BaseType])>
    where
        NumericalAddress: PartialEq<Addr>,
    {
        match self {
            Self::Apply(abs, n, tys) if n.value.is(address, module, name) => Some((abs, n, tys)),
            _ => None,
        }
    }
}

impl SingleType_ {
    pub fn base(sp!(loc, b_): BaseType) -> SingleType {
        sp(loc, SingleType_::Base(sp(loc, b_)))
    }

    pub fn bool(loc: Loc) -> SingleType {
        Self::base(BaseType_::bool(loc))
    }

    pub fn address(loc: Loc) -> SingleType {
        Self::base(BaseType_::address(loc))
    }

    pub fn u8(loc: Loc) -> SingleType {
        Self::base(BaseType_::u8(loc))
    }

    pub fn u16(loc: Loc) -> SingleType {
        Self::base(BaseType_::u16(loc))
    }

    pub fn u32(loc: Loc) -> SingleType {
        Self::base(BaseType_::u32(loc))
    }

    pub fn u64(loc: Loc) -> SingleType {
        Self::base(BaseType_::u64(loc))
    }

    pub fn u128(loc: Loc) -> SingleType {
        Self::base(BaseType_::u128(loc))
    }

    pub fn u256(loc: Loc) -> SingleType {
        Self::base(BaseType_::u256(loc))
    }

    pub fn abilities(&self, loc: Loc) -> AbilitySet {
        match self {
            SingleType_::Ref(_, _) => AbilitySet::references(loc),
            SingleType_::Base(b) => b.value.abilities(loc),
        }
    }

    pub fn is_apply<Addr>(
        &self,
        address: &Addr,
        module: impl AsRef<str>,
        name: impl AsRef<str>,
    ) -> Option<(&AbilitySet, &TypeName, &[BaseType])>
    where
        NumericalAddress: PartialEq<Addr>,
    {
        match self {
            Self::Ref(_, b) | Self::Base(b) => b.value.is_apply(address, module, name),
        }
    }
}

impl Type_ {
    pub fn base(b: BaseType) -> Type {
        Self::single(SingleType_::base(b))
    }

    pub fn single(sp!(loc, s_): SingleType) -> Type {
        sp(loc, Type_::Single(sp(loc, s_)))
    }

    pub fn bool(loc: Loc) -> Type {
        Self::single(SingleType_::bool(loc))
    }

    pub fn address(loc: Loc) -> Type {
        Self::single(SingleType_::address(loc))
    }

    pub fn u8(loc: Loc) -> Type {
        Self::single(SingleType_::u8(loc))
    }

    pub fn u16(loc: Loc) -> Type {
        Self::single(SingleType_::u16(loc))
    }

    pub fn u32(loc: Loc) -> Type {
        Self::single(SingleType_::u32(loc))
    }

    pub fn u64(loc: Loc) -> Type {
        Self::single(SingleType_::u64(loc))
    }

    pub fn u128(loc: Loc) -> Type {
        Self::single(SingleType_::u128(loc))
    }

    pub fn u256(loc: Loc) -> Type {
        Self::single(SingleType_::u256(loc))
    }

    pub fn type_at_index(&self, idx: usize) -> &SingleType {
        match self {
            Type_::Unit => panic!("ICE type mismatch on index lookup"),
            Type_::Single(s) => {
                assert!(idx == 0);
                s
            }
            Type_::Multiple(ss) => {
                assert!(idx < ss.len());
                ss.get(idx).unwrap()
            }
        }
    }

    pub fn from_vec(loc: Loc, mut ss: Vec<SingleType>) -> Type {
        let t_ = match ss.len() {
            0 => Type_::Unit,
            1 => Type_::Single(ss.pop().unwrap()),
            _ => Type_::Multiple(ss),
        };
        sp(loc, t_)
    }

    pub fn is_apply<Addr>(
        &self,
        address: &Addr,
        module: impl AsRef<str>,
        name: impl AsRef<str>,
    ) -> Option<(&AbilitySet, &TypeName, &[BaseType])>
    where
        NumericalAddress: PartialEq<Addr>,
    {
        match self {
            Type_::Unit => None,
            Type_::Single(t) => t.value.is_apply(address, module, name),
            Type_::Multiple(_) => None,
        }
    }
}

impl TName for Var {
    type Key = Symbol;
    type Loc = Loc;

    fn drop_loc(self) -> (Loc, Symbol) {
        (self.0.loc, self.0.value)
    }

    fn add_loc(loc: Loc, value: Symbol) -> Var {
        Var(sp(loc, value))
    }

    fn borrow(&self) -> (&Loc, &Symbol) {
        (&self.0.loc, &self.0.value)
    }
}

impl TName for BlockLabel {
    type Key = Symbol;
    type Loc = Loc;

    fn drop_loc(self) -> (Loc, Symbol) {
        (self.0.loc, self.0.value)
    }

    fn add_loc(loc: Loc, value: Symbol) -> BlockLabel {
        BlockLabel(sp(loc, value))
    }

    fn borrow(&self) -> (&Loc, &Symbol) {
        (&self.0.loc, &self.0.value)
    }
}

impl ModuleCall {
    pub fn is<Addr>(
        &self,
        address: &Addr,
        module: impl AsRef<str>,
        function: impl AsRef<str>,
    ) -> bool
    where
        NumericalAddress: PartialEq<Addr>,
    {
        let Self {
            module: sp!(_, mident),
            name: f,
            ..
        } = self;
        mident.is(address, module) && f == function.as_ref()
    }
}

//**************************************************************************************************
// Display
//**************************************************************************************************

impl std::fmt::Display for TypeName_ {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use TypeName_::*;
        match self {
            Builtin(b) => write!(f, "{}", b),
            ModuleType(m, n) => write!(f, "{}::{}", m, n),
        }
    }
}

impl std::fmt::Display for Visibility {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match &self {
                Visibility::Public(_) => Visibility::PUBLIC,
                Visibility::Friend(_) => Visibility::FRIEND,
                Visibility::Internal => Visibility::INTERNAL,
            }
        )
    }
}

impl std::fmt::Display for Label {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

//**************************************************************************************************
// Debug
//**************************************************************************************************

impl AstDebug for Program {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Program {
            modules,
            info: _,
            warning_filters_table: _,
        } = self;

        for (m, mdef) in modules.key_cloned_iter() {
            w.write(format!("module {}", m));
            w.block(|w| mdef.ast_debug(w));
            w.new_line();
        }
    }
}

impl AstDebug for ModuleDefinition {
    fn ast_debug(&self, w: &mut AstWriter) {
        let ModuleDefinition {
            warning_filter,
            package_name,
            attributes,
            target_kind,
            dependency_order,
            friends,
            structs,
            enums,
            constants,
            functions,
        } = self;
        warning_filter.ast_debug(w);
        if let Some(n) = package_name {
            w.writeln(format!("{}", n))
        }
        attributes.ast_debug(w);
        target_kind.ast_debug(w);
        w.writeln(format!("dependency order #{}", dependency_order));
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
                warning_filter,
                index,
                attributes,
                abilities,
                type_parameters,
                fields,
            },
        ) = self;
        warning_filter.ast_debug(w);
        attributes.ast_debug(w);
        if let StructFields::Native(_) = fields {
            w.write("native ");
        }

        w.write(format!("struct#{index} {name}"));
        type_parameters.ast_debug(w);
        ability_modifiers_ast_debug(w, abilities);
        if let StructFields::Defined(fields) = fields {
            w.block(|w| {
                w.list(fields, ";", |w, (f, bt)| {
                    w.write(format!("{}: ", f));
                    bt.ast_debug(w);
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
                warning_filter,
                index,
                attributes,
                abilities,
                type_parameters,
                variants,
            },
        ) = self;
        warning_filter.ast_debug(w);
        attributes.ast_debug(w);

        w.write(format!("struct#{index} {name}"));
        type_parameters.ast_debug(w);
        ability_modifiers_ast_debug(w, abilities);
        w.block(|w| {
            w.list(variants, ";", |w, (_, v, vdef)| {
                w.write(format!("{} {{ ", v));
                vdef.ast_debug(w);
                w.write(" }");
                true
            })
        })
    }
}

impl AstDebug for VariantDefinition {
    fn ast_debug(&self, w: &mut AstWriter) {
        let VariantDefinition {
            index,
            fields,
            loc: _,
        } = self;
        w.write(format!("id:{}|", index));
        w.comma(fields, |w, (f, bt)| {
            w.write(format!("{}: ", f));
            bt.ast_debug(w);
        })
    }
}

impl AstDebug for (FunctionName, &Function) {
    fn ast_debug(&self, w: &mut AstWriter) {
        let (
            name,
            Function {
                warning_filter,
                index,
                attributes,
                loc: _,
                visibility,
                compiled_visibility,
                entry,
                signature,
                body,
            },
        ) = self;
        warning_filter.ast_debug(w);
        attributes.ast_debug(w);
        w.write("(");
        visibility.ast_debug(w);
        w.write(" as ");
        compiled_visibility.ast_debug(w);
        w.write(") ");
        if entry.is_some() {
            w.write(format!("{} ", ENTRY_MODIFIER));
        }
        if let FunctionBody_::Native = &body.value {
            w.write("native ");
        }
        w.write(format!("fun#{index} {name}"));
        signature.ast_debug(w);
        match &body.value {
            FunctionBody_::Defined { locals, body } => w.block(|w| (locals, body).ast_debug(w)),
            FunctionBody_::Native => w.writeln(";"),
        }
    }
}

impl AstDebug for (UniqueMap<Var, (Mutability, SingleType)>, Block) {
    fn ast_debug(&self, w: &mut AstWriter) {
        let (locals, body) = self;
        (locals, body).ast_debug(w)
    }
}

impl AstDebug for (&UniqueMap<Var, (Mutability, SingleType)>, &Block) {
    fn ast_debug(&self, w: &mut AstWriter) {
        let (locals, body) = self;
        w.write("locals:");
        w.indent(4, |w| {
            w.list(*locals, ",", |w, (_, v, (mut_, st))| {
                mut_.ast_debug(w);
                w.write(format!("{}: ", v));
                st.ast_debug(w);
                true
            })
        });
        w.new_line();
        body.ast_debug(w);
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

impl AstDebug for Visibility {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write(format!("{} ", self))
    }
}

impl AstDebug for Var {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write(format!("{}", self.0))
    }
}

impl AstDebug for BlockLabel {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write(format!("'{}", self.0))
    }
}

impl AstDebug for (ConstantName, &Constant) {
    fn ast_debug(&self, w: &mut AstWriter) {
        let (
            name,
            Constant {
                warning_filter,
                index,
                attributes,
                loc: _loc,
                signature,
                value,
            },
        ) = self;
        warning_filter.ast_debug(w);
        attributes.ast_debug(w);
        w.write(format!("const#{index} {name}:"));
        signature.ast_debug(w);
        w.write(" = ");
        w.block(|w| value.ast_debug(w));
        w.write(";");
    }
}

impl AstDebug for TypeName_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            TypeName_::Builtin(bt) => bt.ast_debug(w),
            TypeName_::ModuleType(m, s) => w.write(format!("{}::{}", m, s)),
        }
    }
}

impl AstDebug for BaseType_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            BaseType_::Param(tp) => tp.ast_debug(w),
            BaseType_::Apply(abilities, m, ss) => {
                w.annotate_gen(
                    |w| {
                        m.ast_debug(w);
                        if !ss.is_empty() {
                            w.write("<");
                            ss.ast_debug(w);
                            w.write(">");
                        }
                    },
                    abilities,
                    |w, abilities| {
                        w.list(abilities, "+", |w, ab| {
                            ab.ast_debug(w);
                            false
                        })
                    },
                );
            }
            BaseType_::Unreachable => w.write("_|_"),
            BaseType_::UnresolvedError => w.write("_"),
        }
    }
}

impl AstDebug for SingleType_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            SingleType_::Base(b) => b.ast_debug(w),
            SingleType_::Ref(mut_, s) => {
                w.write("&");
                if *mut_ {
                    w.write("mut ");
                }
                s.ast_debug(w)
            }
        }
    }
}

impl AstDebug for Type_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            Type_::Unit => w.write("()"),
            Type_::Single(s) => s.ast_debug(w),
            Type_::Multiple(ss) => {
                w.write("(");
                ss.ast_debug(w);
                w.write(")")
            }
        }
    }
}

impl AstDebug for Vec<SingleType> {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.comma(self, |w, s| s.ast_debug(w))
    }
}

impl AstDebug for Vec<BaseType> {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.comma(self, |w, s| s.ast_debug(w))
    }
}

impl AstDebug for VecDeque<Statement> {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.semicolon(self, |w, stmt| stmt.ast_debug(w))
    }
}

impl AstDebug for (Block, Box<Exp>) {
    fn ast_debug(&self, w: &mut AstWriter) {
        let (block, exp) = self;
        if block.is_empty() {
            exp.ast_debug(w);
        } else {
            w.block(|w| {
                block.ast_debug(w);
                w.writeln(";");
                exp.ast_debug(w);
            })
        }
    }
}

impl AstDebug for Statement_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        use Statement_ as S;
        match self {
            S::Command(cmd) => cmd.ast_debug(w),
            S::IfElse {
                cond,
                if_block,
                else_block,
            } => {
                w.write("if (");
                cond.ast_debug(w);
                w.write(") ");
                w.block(|w| if_block.ast_debug(w));
                w.write(" else ");
                w.block(|w| else_block.ast_debug(w));
            }
            S::VariantMatch {
                subject,
                enum_name,
                arms,
            } => {
                w.write("variant_match(");
                subject.ast_debug(w);
                w.write(format!(" : {})", enum_name));
                w.block(|w| {
                    w.comma(arms, |w, (variant, arm)| {
                        w.write(format!("{} =>", variant));
                        w.block(|w| arm.ast_debug(w));
                    })
                });
            }
            S::While { name, cond, block } => {
                w.write("while ");
                w.write(" (");
                cond.ast_debug(w);
                w.write(") ");
                name.ast_debug(w);
                w.write(":");
                w.block(|w| block.ast_debug(w))
            }
            S::Loop {
                name,
                block,
                has_break,
            } => {
                w.write("loop");
                if *has_break {
                    w.write("#has_break");
                }
                w.write(" ");
                name.ast_debug(w);
                w.write(": ");
                w.block(|w| block.ast_debug(w))
            }
            S::NamedBlock { name, block } => {
                w.write("named-block ");
                name.ast_debug(w);
                w.write(": ");
                w.block(|w| block.ast_debug(w))
            }
        }
    }
}

impl AstDebug for Command_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        use Command_ as C;
        match self {
            C::Assign(case, lvalues, rhs) => {
                match case {
                    AssignCase::Let => w.write("let "),
                    AssignCase::Update => w.write("update "),
                };
                lvalues.ast_debug(w);
                w.write(" = ");
                rhs.ast_debug(w);
            }
            C::Mutate(lhs, rhs) => {
                w.write("*");
                lhs.ast_debug(w);
                w.write(" = ");
                rhs.ast_debug(w);
            }
            C::Abort(_, e) => {
                w.write("abort ");
                e.ast_debug(w);
            }
            C::Return { exp: e, from_user } if *from_user => {
                w.write("return@");
                e.ast_debug(w);
            }
            C::Return { exp: e, .. } => {
                w.write("return ");
                e.ast_debug(w);
            }
            C::Break(name) => {
                w.write("break@");
                name.ast_debug(w);
            }
            C::Continue(name) => {
                w.write("continue");
                name.ast_debug(w);
            }
            C::IgnoreAndPop { pop_num, exp } => {
                w.write("pop ");
                w.comma(0..*pop_num, |w, _| w.write("_"));
                w.write(" = ");
                exp.ast_debug(w);
            }
            C::Jump { target, from_user } if *from_user => w.write(format!("jump@{}", target.0)),
            C::Jump { target, .. } => w.write(format!("jump {}", target.0)),
            C::JumpIf {
                cond,
                if_true,
                if_false,
            } => {
                w.write("jump_if(");
                cond.ast_debug(w);
                w.write(format!(") {} else {}", if_true.0, if_false.0));
            }
            C::VariantSwitch {
                subject,
                enum_name,
                arms,
            } => {
                w.write("variant_switch(");
                subject.ast_debug(w);
                w.write(format!(" : {})", enum_name));
                w.block(|w| {
                    w.comma(arms, |w, (variant, arm)| {
                        w.write(format!("{} => {}\n", variant, arm.0));
                    })
                });
            }
        }
    }
}

impl AstDebug for Value_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        use Value_ as V;
        match self {
            V::Address(addr) => w.write(format!("@{}", addr)),
            V::U8(u) => w.write(format!("{}u8", u)),
            V::U16(u) => w.write(format!("{}u16", u)),
            V::U32(u) => w.write(format!("{}u32", u)),
            V::U64(u) => w.write(format!("{}u64", u)),
            V::U128(u) => w.write(format!("{}u128", u)),
            V::U256(u) => w.write(format!("{}u256", u)),
            V::Bool(b) => w.write(format!("{}", b)),
            V::Vector(ty, elems) => {
                w.write("vector#value");
                w.write("<");
                ty.ast_debug(w);
                w.write(">");
                w.write("[");
                w.comma(elems, |w, e| e.ast_debug(w));
                w.write("]");
            }
        }
    }
}

impl AstDebug for Exp {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Exp { ty, exp } = self;
        w.annotate(|w| exp.ast_debug(w), ty)
    }
}

impl AstDebug for Vec<Exp> {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.comma(self, |w, e| e.ast_debug(w));
    }
}

impl AstDebug for UnannotatedExp_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        use UnannotatedExp_ as E;
        match self {
            E::Unit {
                case: UnitCase::FromUser,
            } => w.write("()"),
            E::Unit {
                case: UnitCase::Implicit,
            } => w.write("/*()*/"),
            E::Unit {
                case: UnitCase::Trailing,
            } => w.write("/*;()*/"),
            E::Value(v) => v.ast_debug(w),
            E::Move { annotation, var: v } => {
                let case = match annotation {
                    MoveOpAnnotation::FromUser => "@",
                    MoveOpAnnotation::InferredLastUsage => "#last ",
                    MoveOpAnnotation::InferredNoCopy => "#no-copy ",
                };
                w.write(format!("move{}", case));
                v.ast_debug(w)
            }
            E::Copy {
                from_user: false,
                var: v,
            } => {
                w.write("copy ");
                v.ast_debug(w)
            }
            E::Copy {
                from_user: true,
                var: v,
            } => {
                w.write("copy@");
                v.ast_debug(w)
            }
            E::Constant(c) => w.write(format!("{}", c)),
            E::ModuleCall(mcall) => {
                mcall.ast_debug(w);
            }
            E::Vector(_loc, n, ty, elems) => {
                w.write(format!("vector#{}", n));
                w.write("<");
                ty.ast_debug(w);
                w.write(">");
                w.write("[");
                elems.ast_debug(w);
                w.write("]");
            }
            E::Freeze(e) => {
                w.write("freeze(");
                e.ast_debug(w);
                w.write(")");
            }
            E::Pack(s, tys, fields) => {
                w.write(format!("{}", s));
                w.write("<");
                tys.ast_debug(w);
                w.write(">");
                w.write("{");
                w.comma(fields, |w, (f, bt, e)| {
                    w.annotate(|w| w.write(format!("{}", f)), bt);
                    w.write(": ");
                    e.ast_debug(w);
                });
                w.write("}");
            }
            E::PackVariant(e, v, tys, fields) => {
                w.write(format!("{}::{}", e, v));
                w.write("<");
                tys.ast_debug(w);
                w.write(">");
                w.write("{");
                w.comma(fields, |w, (f, bt, e)| {
                    w.annotate(|w| w.write(format!("{}", f)), bt);
                    w.write(": ");
                    e.ast_debug(w);
                });
                w.write("}");
            }

            E::Multiple(es) => {
                w.write("(");
                w.comma(es, |w, e| e.ast_debug(w));
                w.write(")");
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
            E::Borrow(mut_, e, f, from_unpack) => {
                w.write("&");
                if from_unpack.is_some() {
                    w.write("#from_unpack ");
                }
                if *mut_ {
                    w.write("mut ");
                }
                e.ast_debug(w);
                w.write(format!(".{}", f));
            }
            E::BorrowLocal(mut_, v) => {
                w.write("&");
                if *mut_ {
                    w.write("mut ");
                }
                v.ast_debug(w);
            }
            E::Cast(e, bt) => {
                w.write("(");
                e.ast_debug(w);
                w.write(" as ");
                bt.ast_debug(w);
                w.write(")");
            }
            E::UnresolvedError => w.write("_|_"),
            E::Unreachable => w.write("unreachable"),
            E::ErrorConstant {
                line_number_loc: _,
                error_constant,
            } => {
                w.write("ErrorConstant");
                if let Some(c) = error_constant {
                    w.write(format!("({})", c))
                }
            }
        }
    }
}

impl AstDebug for ModuleCall {
    fn ast_debug(&self, w: &mut AstWriter) {
        let ModuleCall {
            module,
            name,
            type_arguments,
            arguments,
        } = self;
        w.write(format!("{}::{}", module, name));
        w.write("<");
        type_arguments.ast_debug(w);
        w.write(">");
        w.write("(");
        arguments.ast_debug(w);
        w.write(")");
    }
}

impl AstDebug for Vec<LValue> {
    fn ast_debug(&self, w: &mut AstWriter) {
        let parens = self.len() != 1;
        if parens {
            w.write("(");
        }
        w.comma(self, |w, a| a.ast_debug(w));
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
            L::Var {
                var,
                ty,
                unused_assignment,
            } => {
                w.annotate(
                    |w| {
                        var.ast_debug(w);
                        if *unused_assignment {
                            w.write("#unused")
                        }
                    },
                    ty,
                );
            }
            L::Unpack(s, tys, fields) => {
                w.write(format!("{}", s));
                w.write("<");
                tys.ast_debug(w);
                w.write(">");
                w.write("{");
                w.comma(fields, |w, (f, l)| {
                    w.write(format!("{}: ", f));
                    l.ast_debug(w)
                });
                w.write("}");
            }
            L::UnpackVariant(e, v, unpack_type, _rhs_loc, tys, fields) => {
                w.write(format!("{}::{}", e, v));
                match unpack_type {
                    UnpackType::ByMutRef => w.write(" &mut "),
                    UnpackType::ByImmRef => w.write(" &"),
                    UnpackType::ByValue => (),
                }
                w.write("<");
                tys.ast_debug(w);
                w.write(">");
                w.write("{");
                w.comma(fields, |w, (f, l)| {
                    w.write(format!("{}: ", f));
                    l.ast_debug(w)
                });
                w.write("}");
            }
        }
    }
}
