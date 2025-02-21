// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diagnostics::{
        warning_filters::{WarningFilters, WarningFiltersTable},
        DiagnosticReporter,
    },
    expansion::ast::{
        Address, Attributes, Fields, Friend, ModuleIdent, Mutability, Value, Visibility,
    },
    ice,
    naming::ast::{
        BlockLabel, EnumDefinition, FunctionSignature, Neighbor, StructDefinition, SyntaxMethods,
        Type, Type_, UseFuns, Var,
    },
    parser::ast::{
        BinOp, ConstantName, DatatypeName, DocComment, Field, FunctionName, TargetKind, UnaryOp,
        VariantName, ENTRY_MODIFIER, MACRO_MODIFIER, NATIVE_MODIFIER,
    },
    shared::{ast_debug::*, program_info::TypingProgramInfo, unique_map::UniqueMap, Name},
};
use move_core_types::parsing::address::NumericalAddress;
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use move_symbol_pool::Symbol;
use std::{
    collections::{BTreeSet, VecDeque},
    fmt,
    sync::Arc,
};

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
    pub doc: DocComment,
    pub loc: Loc,
    pub warning_filter: WarningFilters,
    // package name metadata from compiler arguments, not used for any language rules
    pub package_name: Option<Symbol>,
    pub attributes: Attributes,
    pub target_kind: TargetKind,
    /// `dependency_order` is the topological order/rank in the dependency graph.
    /// `dependency_order` is initialized at `0` and set in the uses pass
    pub dependency_order: usize,
    pub immediate_neighbors: UniqueMap<ModuleIdent, Neighbor>,
    pub used_addresses: BTreeSet<Address>,
    pub use_funs: UseFuns,
    pub syntax_methods: SyntaxMethods,
    pub friends: UniqueMap<ModuleIdent, Friend>,
    pub structs: UniqueMap<DatatypeName, StructDefinition>,
    pub enums: UniqueMap<DatatypeName, EnumDefinition>,
    pub constants: UniqueMap<ConstantName, Constant>,
    pub functions: UniqueMap<FunctionName, Function>,
}

//**************************************************************************************************
// Functions
//**************************************************************************************************

#[derive(PartialEq, Debug, Clone)]
pub enum FunctionBody_ {
    Defined(Sequence),
    Native,
    Macro,
}
pub type FunctionBody = Spanned<FunctionBody_>;

#[derive(PartialEq, Debug, Clone)]
pub struct Function {
    pub doc: DocComment,
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
// Expressions
//**************************************************************************************************

#[derive(Debug, PartialEq, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum LValue_ {
    Ignore,
    Var {
        mut_: Option<Mutability>,
        var: Var,
        ty: Box<Type>,
        unused_binding: bool,
    },
    Unpack(ModuleIdent, DatatypeName, Vec<Type>, Fields<(Type, LValue)>),
    BorrowUnpack(
        bool,
        ModuleIdent,
        DatatypeName,
        Vec<Type>,
        Fields<(Type, LValue)>,
    ),
    UnpackVariant(
        ModuleIdent,
        DatatypeName,
        VariantName,
        Vec<Type>,
        Fields<(Type, LValue)>,
    ),
    BorrowUnpackVariant(
        bool,
        ModuleIdent,
        DatatypeName,
        VariantName,
        Vec<Type>,
        Fields<(Type, LValue)>,
    ),
}
pub type LValue = Spanned<LValue_>;
pub type LValueList_ = Vec<LValue>;
pub type LValueList = Spanned<LValueList_>;

#[derive(Debug, PartialEq, Clone)]
pub struct ModuleCall {
    pub module: ModuleIdent,
    pub name: FunctionName,
    pub type_arguments: Vec<Type>,
    pub arguments: Box<Exp>,
    pub parameter_types: Vec<Type>,
    pub method_name: Option<Name>, // if translated from method call
}

#[derive(Debug, PartialEq, Eq, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum BuiltinFunction_ {
    Freeze(Type),
    Assert(/* is_macro */ Option<Loc>),
}
pub type BuiltinFunction = Spanned<BuiltinFunction_>;

#[derive(Debug, PartialEq, Clone)]
pub enum UnannotatedExp_ {
    Unit {
        trailing: bool,
    },
    Value(Value),
    Move {
        from_user: bool,
        var: Var,
    },
    Copy {
        from_user: bool,
        var: Var,
    },
    Use(Var),
    Constant(ModuleIdent, ConstantName),

    ModuleCall(Box<ModuleCall>),
    Builtin(Box<BuiltinFunction>, Box<Exp>),
    Vector(Loc, usize, Box<Type>, Box<Exp>),

    IfElse(Box<Exp>, Box<Exp>, Option<Box<Exp>>),
    Match(Box<Exp>, Spanned<Vec<MatchArm>>),
    VariantMatch(
        Box<Exp>,
        (ModuleIdent, DatatypeName),
        Vec<(VariantName, Exp)>,
    ),
    While(BlockLabel, Box<Exp>, Box<Exp>),
    Loop {
        name: BlockLabel,
        has_break: bool,
        body: Box<Exp>,
    },
    NamedBlock(BlockLabel, Sequence),
    Block(Sequence),
    Assign(LValueList, Vec<Option<Type>>, Box<Exp>),
    Mutate(Box<Exp>, Box<Exp>),
    Return(Box<Exp>),
    Abort(Box<Exp>),
    Give(BlockLabel, Box<Exp>),
    Continue(BlockLabel),

    Dereference(Box<Exp>),
    UnaryExp(UnaryOp, Box<Exp>),
    BinopExp(Box<Exp>, BinOp, Box<Type>, Box<Exp>),

    Pack(ModuleIdent, DatatypeName, Vec<Type>, Fields<(Type, Exp)>),
    PackVariant(
        ModuleIdent,
        DatatypeName,
        VariantName,
        Vec<Type>,
        Fields<(Type, Exp)>,
    ),
    ExpList(Vec<ExpListItem>),

    Borrow(bool, Box<Exp>, Field),
    TempBorrow(bool, Box<Exp>),
    BorrowLocal(bool, Var),

    Cast(Box<Exp>, Box<Type>),
    Annotate(Box<Exp>, Box<Type>),
    ErrorConstant {
        line_number_loc: Loc,
        error_constant: Option<ConstantName>,
    },
    UnresolvedError,
}
pub type UnannotatedExp = Spanned<UnannotatedExp_>;
#[derive(Debug, PartialEq, Clone)]
pub struct Exp {
    pub ty: Type,
    pub exp: UnannotatedExp,
}

pub type Sequence = (UseFuns, VecDeque<SequenceItem>);
#[derive(Debug, PartialEq, Clone)]
pub enum SequenceItem_ {
    Seq(Box<Exp>),
    Declare(LValueList),
    Bind(LValueList, Vec<Option<Type>>, Box<Exp>),
}
pub type SequenceItem = Spanned<SequenceItem_>;

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm_ {
    pub pattern: MatchPattern,
    pub binders: Vec<(Var, Type)>,
    pub guard: Option<Box<Exp>>,
    pub guard_binders: UniqueMap<Var, Var>, // pattern binder name -> guard var name
    pub rhs_binders: BTreeSet<Var>,         // pattern binders used in the right-hand side
    pub rhs: Box<Exp>,
}

pub type MatchArm = Spanned<MatchArm_>;

#[derive(Debug, Clone, PartialEq)]
pub enum UnannotatedPat_ {
    Variant(
        ModuleIdent,
        DatatypeName,
        VariantName,
        Vec<Type>,
        Fields<(Type, MatchPattern)>,
    ),
    BorrowVariant(
        bool,
        ModuleIdent,
        DatatypeName,
        VariantName,
        Vec<Type>,
        Fields<(Type, MatchPattern)>,
    ),
    Struct(
        ModuleIdent,
        DatatypeName,
        Vec<Type>,
        Fields<(Type, MatchPattern)>,
    ),
    BorrowStruct(
        bool,
        ModuleIdent,
        DatatypeName,
        Vec<Type>,
        Fields<(Type, MatchPattern)>,
    ),
    Constant(ModuleIdent, ConstantName),
    Binder(Mutability, Var),
    Literal(Value),
    Wildcard,
    Or(Box<MatchPattern>, Box<MatchPattern>),
    At(Var, Box<MatchPattern>),
    ErrorPat,
}

pub type UnannotatedPat = Spanned<UnannotatedPat_>;

#[derive(Debug, Clone, PartialEq)]
pub struct MatchPattern {
    pub ty: Type,
    pub pat: Spanned<UnannotatedPat_>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum ExpListItem {
    Single(Exp, Box<Type>),
    Splat(Loc, Exp, Vec<Type>),
}

//**************************************************************************************************
// impls
//**************************************************************************************************

impl BuiltinFunction_ {
    pub fn display_name(&self) -> &'static str {
        use crate::naming::ast::BuiltinFunction_ as NB;
        use BuiltinFunction_ as B;
        match self {
            B::Freeze(_) => NB::FREEZE,
            B::Assert(_) => NB::ASSERT_MACRO,
        }
    }
}

impl Exp {
    pub fn is_unit(&self, diags: &DiagnosticReporter) -> bool {
        self.exp.value.is_unit(diags, self.exp.loc)
    }
}

impl UnannotatedExp_ {
    #[growing_stack]
    pub fn is_unit(&self, diags: &DiagnosticReporter, loc: Loc) -> bool {
        match &self {
            Self::Unit { .. } => true,
            Self::Annotate(inner, _) => inner.is_unit(diags),
            Self::Block((_, seq)) if seq.is_empty() => {
                diags.add_diag(ice!((loc, "Unexpected empty block without a value")));
                false
            }
            Self::Block((_, seq)) if seq.len() == 1 => seq[0].value.is_unit(diags),
            _ => false,
        }
    }
}

impl SequenceItem_ {
    pub fn is_unit(&self, diags: &DiagnosticReporter) -> bool {
        match &self {
            Self::Seq(e) => e.is_unit(diags),
            Self::Declare(_) | Self::Bind(_, _, _) => false,
        }
    }
}

pub fn explist(loc: Loc, mut es: Vec<Exp>) -> Exp {
    match es.len() {
        0 => {
            let e__ = UnannotatedExp_::Unit { trailing: false };
            let ty = sp(loc, Type_::Unit);
            exp(ty, sp(loc, e__))
        }
        1 => es.pop().unwrap(),
        _ => {
            let tys = es.iter().map(|e| e.ty.clone()).collect();
            let items = es.into_iter().map(single_item).collect();
            let ty = Type_::multiple(loc, tys);
            exp(ty, sp(loc, UnannotatedExp_::ExpList(items)))
        }
    }
}

pub fn exp(ty: Type, exp: UnannotatedExp) -> Exp {
    Exp { ty, exp }
}

pub fn single_item(e: Exp) -> ExpListItem {
    let ty = Box::new(e.ty.clone());
    ExpListItem::Single(e, ty)
}

pub fn pat(ty: Type, pat: UnannotatedPat) -> MatchPattern {
    MatchPattern { ty, pat }
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

impl fmt::Display for BuiltinFunction_ {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
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
            doc,
            loc: _,
            warning_filter,
            package_name,
            attributes,
            target_kind,
            dependency_order,
            immediate_neighbors,
            used_addresses,
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
        w.writeln(format!("dependency order #{}", dependency_order));
        for (mident, neighbor) in immediate_neighbors.key_cloned_iter() {
            w.write(format!("{mident} is"));
            neighbor.ast_debug(w);
            w.writeln(";");
        }
        for addr in used_addresses {
            w.write(format!("uses address {};", addr));
            w.new_line()
        }
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

impl AstDebug for (FunctionName, &Function) {
    fn ast_debug(&self, w: &mut AstWriter) {
        let (
            name,
            Function {
                doc,
                warning_filter,
                index,
                attributes,
                loc: _,
                visibility,
                compiled_visibility,
                entry,
                macro_,
                signature,
                body,
            },
        ) = self;
        doc.ast_debug(w);
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
        if macro_.is_some() {
            w.write(format!("{} ", MACRO_MODIFIER));
        }
        if let FunctionBody_::Native = &body.value {
            w.write(format!("{} ", NATIVE_MODIFIER));
        }
        w.write(format!("fun#{index} {name}"));
        signature.ast_debug(w);
        body.ast_debug(w);
    }
}

impl AstDebug for FunctionBody_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        use FunctionBody_ as F;
        match self {
            F::Defined(seq) => seq.ast_debug(w),
            F::Native | F::Macro => w.writeln(";"),
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
            I::Declare(sp!(_, bs)) => {
                w.write("let ");
                bs.ast_debug(w);
            }
            I::Bind(sp!(_, bs), expected_types, e) => {
                w.write("let ");
                bs.ast_debug(w);
                w.write(": (");
                expected_types.ast_debug(w);
                w.write(")");
                w.write(" = ");
                e.ast_debug(w);
            }
        }
    }
}

impl AstDebug for UnannotatedExp_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        use UnannotatedExp_ as E;
        match self {
            E::Unit { trailing } if !trailing => w.write("()"),
            E::Unit {
                trailing: _trailing,
            } => w.write("/*()*/"),
            E::Value(v) => v.ast_debug(w),
            E::Move {
                from_user: false,
                var: v,
            } => {
                w.write("move ");
                v.ast_debug(w)
            }
            E::Move {
                from_user: true,
                var: v,
            } => {
                w.write("move@");
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
            E::Use(v) => {
                w.write("use@");
                v.ast_debug(w)
            }
            E::Constant(m, c) => w.write(format!("{}::{}", m, c)),
            E::ModuleCall(mcall) => {
                mcall.ast_debug(w);
            }
            E::Builtin(bf, rhs) => {
                bf.ast_debug(w);
                w.write("(");
                rhs.ast_debug(w);
                w.write(")");
            }
            E::Vector(_loc, usize, ty, elems) => {
                w.write(format!("vector#{}", usize));
                w.write("<");
                ty.ast_debug(w);
                w.write(">");
                w.write("[");
                elems.ast_debug(w);
                w.write("]");
            }
            E::Pack(m, s, tys, fields) => {
                w.write(format!("{}::{}", m, s));
                w.write("<");
                tys.ast_debug(w);
                w.write(">");
                w.write("{");
                w.comma(fields, |w, (_, f, idx_bt_e)| {
                    let (idx, (bt, e)) = idx_bt_e;
                    w.write(format!("({}#{}:", idx, f));
                    bt.ast_debug(w);
                    w.write("): ");
                    e.ast_debug(w);
                });
                w.write("}");
            }
            E::PackVariant(m, e, v, tys, fields) => {
                w.write(format!("{}::{}::{}", m, e, v));
                w.write("<");
                tys.ast_debug(w);
                w.write(">");
                w.write("{");
                w.comma(fields, |w, (_, f, idx_bt_e)| {
                    let (idx, (bt, e)) = idx_bt_e;
                    w.write(format!("({}#{}:", idx, f));
                    bt.ast_debug(w);
                    w.write("): ");
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
            E::Match(esubject, arms) => {
                w.write("match (");
                esubject.ast_debug(w);
                w.write(") ");
                w.block(|w| {
                    w.comma(&arms.value, |w, sp!(_, arm)| {
                        arm.pattern.ast_debug(w);
                        if let Some(guard) = &arm.guard {
                            w.write(" if ");
                            guard.ast_debug(w);
                        }
                        w.write(" => ");
                        arm.rhs.ast_debug(w);
                    })
                });
            }
            E::VariantMatch(esubject, (m, enum_name), arms) => {
                w.write("variant_switch (");
                esubject.ast_debug(w);
                w.write(format!(" : {m}::{enum_name} ) "));
                w.block(|w| {
                    w.comma(arms.iter(), |w, (variant, rhs)| {
                        w.write(format!("{} =>", variant));
                        rhs.ast_debug(w);
                        println!();
                    })
                });
            }
            E::While(name, b, e) => {
                name.ast_debug(w);
                w.write(": while (");
                b.ast_debug(w);
                w.write(")");
                e.ast_debug(w);
            }
            E::Loop {
                name,
                has_break,
                body,
            } => {
                name.ast_debug(w);
                w.write(": loop");
                if *has_break {
                    w.write("#with_break");
                }
                w.write(" ");
                body.ast_debug(w);
            }
            E::NamedBlock(name, seq) => {
                name.ast_debug(w);
                w.write(": ");
                seq.ast_debug(w)
            }
            E::Block(seq) => seq.ast_debug(w),
            E::ExpList(es) => {
                w.write("(");
                w.comma(es, |w, e| e.ast_debug(w));
                w.write(")");
            }

            E::Assign(sp!(_, lvalues), expected_types, rhs) => {
                lvalues.ast_debug(w);
                w.write(": (");
                expected_types.ast_debug(w);
                w.write(") = ");
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
            E::Give(name, exp) => {
                w.write("give@");
                name.ast_debug(w);
                w.write(" ");
                exp.ast_debug(w);
            }
            E::Continue(name) => {
                w.write("continue@");
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
            E::BinopExp(l, op, ty, r) => {
                l.ast_debug(w);
                w.write(" ");
                op.ast_debug(w);
                w.write("@");
                ty.ast_debug(w);
                w.write(" ");
                r.ast_debug(w)
            }
            E::Borrow(mut_, e, f) => {
                w.write("&");
                if *mut_ {
                    w.write("mut ");
                }
                e.ast_debug(w);
                w.write(format!(".{}", f));
            }
            E::TempBorrow(mut_, e) => {
                w.write("&");
                if *mut_ {
                    w.write("mut ");
                }
                e.ast_debug(w);
            }
            E::BorrowLocal(mut_, v) => {
                w.write("&");
                if *mut_ {
                    w.write("mut ");
                }
                v.ast_debug(w);
            }
            E::Cast(e, ty) => {
                w.write("(");
                e.ast_debug(w);
                w.write(")");
                w.write(" as ");
                ty.ast_debug(w);
            }
            E::Annotate(e, ty) => {
                w.write("annot(");
                e.ast_debug(w);
                w.write(": ");
                ty.ast_debug(w);
                w.write(")");
            }
            E::UnresolvedError => w.write("_|_"),
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

impl AstDebug for Exp {
    fn ast_debug(&self, w: &mut AstWriter) {
        let Exp { ty, exp } = self;
        w.annotate(|w| exp.ast_debug(w), ty)
    }
}

impl AstDebug for ModuleCall {
    fn ast_debug(&self, w: &mut AstWriter) {
        let ModuleCall {
            module,
            name,
            type_arguments,
            parameter_types,
            arguments,
            method_name: _,
        } = self;
        w.write(format!("{}::{}", module, name));
        if !parameter_types.is_empty() {
            w.write("[");
            if !parameter_types.is_empty() {
                w.write("parameter_types: [");
                parameter_types.ast_debug(w);
                w.write("]");
            }
        }
        w.write("<");
        type_arguments.ast_debug(w);
        w.write(">");
        w.write("(");
        arguments.ast_debug(w);
        w.write(")");
    }
}

impl AstDebug for BuiltinFunction_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        use crate::naming::ast::BuiltinFunction_ as NF;
        use BuiltinFunction_ as F;
        let (n, bt_opt) = match self {
            F::Freeze(bt) => (NF::FREEZE, Some(bt)),
            F::Assert(_) => (NF::ASSERT_MACRO, None),
        };
        w.write(n);
        if let Some(bt) = bt_opt {
            w.write("<");
            bt.ast_debug(w);
            w.write(">");
        }
    }
}

impl AstDebug for ExpListItem {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            ExpListItem::Single(e, st) => w.annotate(|w| e.ast_debug(w), st),
            ExpListItem::Splat(_, e, ss) => {
                w.write("~");
                w.annotate(|w| e.ast_debug(w), ss)
            }
        }
    }
}

impl AstDebug for MatchArm_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        self.pattern.ast_debug(w);
        if let Some(guard) = &self.guard {
            w.write(" if ");
            guard.ast_debug(w);
        }
        w.write(" => ");
        self.rhs.ast_debug(w);
    }
}

impl AstDebug for MatchPattern {
    fn ast_debug(&self, w: &mut AstWriter) {
        let MatchPattern { ty, pat } = self;
        w.annotate(|w| pat.value.ast_debug(w), ty)
    }
}

impl AstDebug for UnannotatedPat_ {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            UnannotatedPat_::BorrowVariant(mut_, m, e, v, tys, fields) => {
                w.write("&");
                if *mut_ {
                    w.write("mut ");
                }
                w.write(format!("{}::{}::{}", m, e, v));
                w.write("<");
                tys.ast_debug(w);
                w.write(">");
                w.write("{");
                w.comma(fields, |w, (_, f, idx_bt_a)| {
                    let (idx, (bt, a)) = idx_bt_a;
                    w.annotate(|w| w.write(format!("{}#{}", idx, f)), bt);
                    w.write(": ");
                    a.ast_debug(w);
                });
                w.write("}");
            }
            UnannotatedPat_::Variant(m, e, v, tys, fields) => {
                w.write(format!("{}::{}::{}", m, e, v));
                w.write("<");
                tys.ast_debug(w);
                w.write(">");
                w.write("{");
                w.comma(fields, |w, (_, f, idx_bt_a)| {
                    let (idx, (bt, a)) = idx_bt_a;
                    w.annotate(|w| w.write(format!("{}#{}", idx, f)), bt);
                    w.write(": ");
                    a.ast_debug(w);
                });
                w.write("}");
            }
            UnannotatedPat_::BorrowStruct(mut_, m, s, tys, fields) => {
                w.write("&");
                if *mut_ {
                    w.write("mut ");
                }
                w.write(format!("{}::{}", m, s));
                w.write("<");
                tys.ast_debug(w);
                w.write(">");
                w.write("{");
                w.comma(fields, |w, (_, f, idx_bt_a)| {
                    let (idx, (bt, a)) = idx_bt_a;
                    w.annotate(|w| w.write(format!("{}#{}", idx, f)), bt);
                    w.write(": ");
                    a.ast_debug(w);
                });
                w.write("}");
            }
            UnannotatedPat_::Struct(m, e, tys, fields) => {
                w.write(format!("{}::{}", m, e));
                w.write("<");
                tys.ast_debug(w);
                w.write(">");
                w.write("{");
                w.comma(fields, |w, (_, f, idx_bt_a)| {
                    let (idx, (bt, a)) = idx_bt_a;
                    w.annotate(|w| w.write(format!("{}#{}", idx, f)), bt);
                    w.write(": ");
                    a.ast_debug(w);
                });
                w.write("}");
            }
            UnannotatedPat_::Constant(m, c) => {
                w.write(format!("{}::{}", m, c));
            }
            UnannotatedPat_::Or(lhs, rhs) => {
                w.write("(");
                lhs.ast_debug(w);
                w.write("|");
                rhs.ast_debug(w);
                w.write(")");
            }
            UnannotatedPat_::At(x, inner) => {
                x.ast_debug(w);
                w.write(" @ ");
                inner.ast_debug(w);
            }
            UnannotatedPat_::Binder(mut_, x) => {
                mut_.ast_debug(w);
                x.ast_debug(w)
            }
            UnannotatedPat_::Literal(v) => v.ast_debug(w),
            UnannotatedPat_::Wildcard => w.write("_"),
            UnannotatedPat_::ErrorPat => w.write("<err>"),
        }
    }
}

impl AstDebug for Vec<Option<Type>> {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.comma(self, |w, t_opt| match t_opt {
            Some(t) => t.ast_debug(w),
            None => w.write("%no_exp%"),
        })
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
                mut_,
                var: v,
                ty: st,
                unused_binding,
            } => w.annotate(
                |w| {
                    if let Some(mut_) = mut_ {
                        mut_.ast_debug(w);
                    }
                    v.ast_debug(w);
                    if *unused_binding {
                        w.write("#unused")
                    }
                },
                st,
            ),
            L::Unpack(m, s, tys, fields) => {
                w.write(format!("{}::{}", m, s));
                w.write("<");
                tys.ast_debug(w);
                w.write(">");
                w.write("{");
                w.comma(fields, |w, (_, f, idx_bt_a)| {
                    let (idx, (bt, a)) = idx_bt_a;
                    w.annotate(|w| w.write(format!("{}#{}", idx, f)), bt);
                    w.write(": ");
                    a.ast_debug(w);
                });
                w.write("}");
            }
            L::BorrowUnpack(mut_, m, s, tys, fields) => {
                w.write("&");
                if *mut_ {
                    w.write("mut ");
                }
                w.write(format!("{}::{}", m, s));
                w.write("<");
                tys.ast_debug(w);
                w.write(">");
                w.write("{");
                w.comma(fields, |w, (_, f, idx_bt_a)| {
                    let (idx, (bt, a)) = idx_bt_a;
                    w.annotate(|w| w.write(format!("{}#{}", idx, f)), bt);
                    w.write(": ");
                    a.ast_debug(w);
                });
                w.write("}");
            }
            L::UnpackVariant(m, e, v, tys, fields) => {
                w.write(format!("{}::{}::{}", m, e, v));
                w.write("<");
                tys.ast_debug(w);
                w.write(">");
                w.write("{");
                w.comma(fields, |w, (_, f, idx_bt_a)| {
                    let (idx, (bt, a)) = idx_bt_a;
                    w.annotate(|w| w.write(format!("{}#{}", idx, f)), bt);
                    w.write(": ");
                    a.ast_debug(w);
                });
                w.write("}");
            }
            L::BorrowUnpackVariant(mut_, m, e, v, tys, fields) => {
                w.write("&");
                if *mut_ {
                    w.write("mut ");
                }
                w.write(format!("{}::{}::{}", m, e, v));
                w.write("<");
                tys.ast_debug(w);
                w.write(">");
                w.write("{");
                w.comma(fields, |w, (_, f, idx_bt_a)| {
                    let (idx, (bt, a)) = idx_bt_a;
                    w.annotate(|w| w.write(format!("{}#{}", idx, f)), bt);
                    w.write(": ");
                    a.ast_debug(w);
                });
                w.write("}");
            }
        }
    }
}
