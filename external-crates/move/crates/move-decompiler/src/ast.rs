// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    file_format::{AbilitySet, DatatypeTyParameter, Visibility},
    normalized::{self, Constant, ModuleId},
};

use indexmap::IndexMap;
use move_core_types::{account_address::AccountAddress, runtime_value::MoveValue as Value};
use move_model_2::{model::Model, source_kind::SourceKind};
use move_stackless_bytecode_2::ast::{DataOp, PrimitiveOp};
use move_symbol_pool::Symbol;

use std::collections::BTreeMap;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

pub struct Decompiled<S: SourceKind> {
    pub model: Model<S>,
    pub packages: Vec<Package>,
}

pub struct Package {
    pub name: Option<Symbol>,
    pub address: AccountAddress,
    pub modules: BTreeMap<Symbol, Module>,
}

#[derive(Debug, Clone)]
pub struct Module {
    pub name: Symbol,
    /// Structs in declaration order. We use `IndexMap` rather than `BTreeMap` because users
    /// expect source-position ordering for types, and bytecode preserves the original order in
    /// its index map. Functions still use `BTreeMap` (alphabetical) to match prior behavior.
    pub structs: IndexMap<Symbol, Struct>,
    pub enums: IndexMap<Symbol, Enum>,
    pub functions: BTreeMap<Symbol, Function>,
    /// `use 0xADDR::name;` declarations. Populated by the `collect_uses` refinement after a
    /// scan of every function body; arms of every `ModuleRef::Aliased(name)` in the bodies
    /// correspond to a key here. Initially empty; if `collect_uses` hasn't run (or no module
    /// is aliasable) it stays empty and bodies hold `ModuleRef::Qualified(...)` everywhere.
    pub uses: BTreeMap<ModuleId<Symbol>, Symbol>,
    /// `use 0xADDR::module::Type;` declarations. Populated by the `collect_uses` refinement
    /// alongside `uses`. Each entry maps a `(module, type_name)` pair to its alias; bodies
    /// hold `TypeRef::Aliased(alias)` wherever the alias applies.
    pub type_uses: BTreeMap<(ModuleId<Symbol>, Symbol), Symbol>,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: Symbol,
    pub visibility: Visibility,
    pub is_entry: bool,
    /// Per type-parameter abilities (in declaration order). Type parameters are referred to in
    /// types as `Type::TypeParameter(index)` and rendered as `T{index}`.
    pub type_parameters: Vec<AbilitySet>,
    /// Parameter types, in declaration order. Parameter names are not preserved in bytecode;
    /// the pretty printer generates `l0..l{N-1}` matching `term_reconstruction::local_name`.
    pub parameters: Vec<Type>,
    /// Return types. Empty for `(): ()`, single element for one return, multiple for a tuple.
    pub returns: Vec<Type>,
    pub code: Exp,
    /// Basic-block ids that the structurer received as input but never emitted into the
    /// structured output. Non-empty means the renderer prepends a
    /// `// Did not structure and emit blocks N, K, ...` notice so the reader knows part of
    /// the bytecode is missing from this function's source view.
    pub unstructured_blocks: Vec<u64>,
}

#[derive(Debug, Clone)]
pub struct Struct {
    pub name: Symbol,
    pub abilities: AbilitySet,
    pub type_parameters: Vec<DatatypeTyParameter>,
    /// Fields in declaration order. Positional vs. named is a source-level distinction not
    /// preserved in bytecode; we always render as `{ name: ty, ... }`.
    pub fields: Vec<(Symbol, Type)>,
}

#[derive(Debug, Clone)]
pub struct Enum {
    pub name: Symbol,
    pub abilities: AbilitySet,
    pub type_parameters: Vec<DatatypeTyParameter>,
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone)]
pub struct Variant {
    pub name: Symbol,
    pub fields: Vec<(Symbol, Type)>,
}

/// Type expressions appearing in parameter, return, and field positions. Mirrors
/// `move_binary_format::normalized::Type<Symbol>` (no source-only `Tuple`/`Fun`/`Any` variants,
/// because everything we model comes off compiled bytecode), with one key difference:
/// `Datatype` carries our `TypeRef` instead of a bare `ModuleId<Symbol>`. That makes types
/// participate in the same alias-rewriting pass (`collect_uses`) that handles call sites and
/// `Unpack`/`Switch`/`Match` heads — without it, a struct or function signature would render
/// fully qualified even when the body referenced the same type via an alias.
#[derive(Debug, Clone)]
pub enum Type {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
    Signer,
    Vector(Box<Type>),
    Reference(/* is_mut */ bool, Box<Type>),
    /// A reference to a declared type parameter by index. The containing `Struct`/`Enum`/
    /// `Function` owns the parameter list (with abilities/constraints); this is only the
    /// reference site, rendered as `T{index}`.
    TypeParameter(u16),
    Datatype(Box<Datatype>),
}

#[derive(Debug, Clone)]
pub struct Datatype {
    pub type_ref: TypeRef,
    pub type_arguments: Vec<Type>,
}

impl Type {
    /// Convert from the normalized bytecode type. The `Datatype` arm builds a
    /// `TypeRef::Qualified(ModuleRef::Qualified(mid), name)` — `collect_uses` later collapses
    /// these to aliased form where appropriate.
    pub fn from_normalized(t: &normalized::Type<Symbol>) -> Self {
        match t {
            normalized::Type::Bool => Type::Bool,
            normalized::Type::U8 => Type::U8,
            normalized::Type::U16 => Type::U16,
            normalized::Type::U32 => Type::U32,
            normalized::Type::U64 => Type::U64,
            normalized::Type::U128 => Type::U128,
            normalized::Type::U256 => Type::U256,
            normalized::Type::Address => Type::Address,
            normalized::Type::Signer => Type::Signer,
            normalized::Type::Vector(inner) => Type::Vector(Box::new(Type::from_normalized(inner))),
            normalized::Type::Reference(is_mut, inner) => {
                Type::Reference(*is_mut, Box::new(Type::from_normalized(inner)))
            }
            normalized::Type::TypeParameter(idx) => Type::TypeParameter(*idx),
            normalized::Type::Datatype(dt) => Type::Datatype(Box::new(Datatype {
                type_ref: TypeRef::Qualified(ModuleRef::Qualified(dt.module), dt.name),
                type_arguments: dt
                    .type_arguments
                    .iter()
                    .map(Type::from_normalized)
                    .collect(),
            })),
        }
    }
}

/// A reference to a module from inside an expression. Lowering emits `Qualified(mid)` everywhere;
/// the `collect_uses` refinement rewrites `Qualified(mid)` to `Aliased(name)` when `mid` is in the
/// containing `Module.uses` map.
#[derive(Debug, Clone, Copy)]
pub enum ModuleRef {
    Qualified(ModuleId<Symbol>),
    Aliased(Symbol),
    /// No module prefix — for macros and other unqualified builtins. The `Symbol` in the
    /// containing `Call` is rendered alone, e.g. `assert!`. `Display` emits an empty string;
    /// the Call printers detect this variant and skip the `::` separator.
    Builtin,
}

impl ModuleRef {
    pub fn is_builtin(&self) -> bool {
        matches!(self, ModuleRef::Builtin)
    }
}

impl std::fmt::Display for ModuleRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModuleRef::Qualified(mid) => write!(f, "{mid}"),
            ModuleRef::Aliased(name) => write!(f, "{name}"),
            ModuleRef::Builtin => Ok(()),
        }
    }
}

/// A reference to a struct or enum from inside an expression. Lowering emits
/// `Qualified(module, name)` everywhere; the `collect_uses` refinement rewrites it to
/// `Aliased(name)` when `(module_id, name)` is in the containing `Module.type_uses` map.
#[derive(Debug, Clone, Copy)]
pub enum TypeRef {
    Qualified(ModuleRef, Symbol),
    Aliased(Symbol),
}

impl std::fmt::Display for TypeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeRef::Qualified(mr, name) => write!(f, "{mr}::{name}"),
            TypeRef::Aliased(name) => write!(f, "{name}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnpackKind {
    Value,
    ImmRef,
    MutRef,
}

/// A loop label. `None` is the unlabeled form; `Some(L)` prints as `'loop_L:` / `break 'loop_L;`
/// / `continue 'loop_L;`. Structuring always emits `Some`; `strip_loop_labels` demotes labels
/// whose only uses sit directly in the labeled loop's body (no nested loops in between).
pub type Label = u64;

/// Dispatch-table tag width. Multi-succ-loop dispatch synthesizes a tag at each owned-succ
/// site (in `structure_loop`) and lowers it to `MoveValue::U32` at the runtime layer
/// (`translate.rs`). Keeping the width as a named alias means the structurer, the post-loop
/// `match`, and the runtime value agree by construction.
pub type DispatchTag = u32;

#[derive(Debug, Clone)]
pub enum Exp {
    Break(Option<Label>),
    Continue(Option<Label>),
    Loop(Option<Label>, Box<Exp>),
    Seq(Vec<Exp>),
    While(Option<Label>, Box<Exp>, Box<Exp>),
    IfElse(Box<Exp>, Box<Exp>, Box<Option<Exp>>),
    /// A tagged dispatch on an enum's variant — the shape structuring emits before pattern
    /// recovery runs. Each arm is `(variant, body)` with no pattern bindings. The
    /// `reconstruct_match` refinement promotes a `Switch` to `Match` when each arm's body
    /// starts with an `UnpackVariant` whose fields can be lifted into a pattern.
    Switch(
        Box<Exp>,
        /* enum */ TypeRef,
        /* variant x rhs */ Vec<(Symbol, Exp)>,
    ),
    /// A Move `match` expression with patterns. Created exclusively by `reconstruct_match`
    /// from a `Switch` whose arms had liftable leading `UnpackVariant`s; each arm carries the
    /// pattern's field bindings (possibly empty for fieldless variants in the same match).
    Match(
        Box<Exp>,
        /* enum */ TypeRef,
        /* variant x pattern-fields x rhs */ Vec<(Symbol, Vec<(Symbol, String)>, Exp)>,
    ),
    /// Integer-literal `match` on a synthetic dispatch local.
    MatchLit(Box<Exp>, Vec<(DispatchTag, Exp)>),
    Return(Vec<Exp>),
    // --------------------------------
    // non-control expressions
    Assign(Vec<String>, Box<Exp>),
    LetBind(Vec<String>, Box<Exp>),
    /// `let X;` — declaration with no initializer. Inserted by `hoist_declarations` when an
    /// arm-scope `let X = e` has to be lifted out to a common enclosing scope.
    Declare(Vec<String>),
    Call((ModuleRef, Symbol), Vec<Exp>),
    Abort(Box<Exp>),
    // Do we need drop?
    Primitive {
        op: PrimitiveOp,
        args: Vec<Exp>,
    },
    Data {
        op: DataOp,
        args: Vec<Exp>,
    },
    Unpack(TypeRef, Vec<(Symbol, String)>, Box<Exp>),
    UnpackVariant(
        UnpackKind,
        (TypeRef, /* variant */ Symbol),
        Vec<(Symbol, String)>,
        Box<Exp>,
    ),
    VecUnpack(Vec<String>, Box<Exp>),
    Borrow(/* mut*/ bool, Box<Exp>),
    Value(Value),
    Variable(String),
    Constant(std::rc::Rc<Constant<Symbol>>),
    // placeholder for structured control flow we cannot yet decompile.
    // ideally not needed, but useful to avoid failing on an entire module
    // just because we can't handle one thing
    Unstructured(Vec<UnstructuredNode>),
    /// A `D::Structured::Block(code)` after lowering. The `u64` matches the label carried by
    /// any `Unstructured(Goto)` that targets this block, so a surviving goto can be traced
    /// to its destination by scanning for the matching `Block` in the rendered output. The
    /// body is the lowered block contents (typically a `Seq`).
    Block(u64, Box<Exp>),
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl Exp {
    pub fn contains_break(&self) -> bool {
        match self {
            Exp::Continue(_) => false,
            Exp::Break(_) => true,
            Exp::Loop(_, _) | Exp::While(_, _, _) => false,
            Exp::Seq(seq) => seq.iter().any(|e| e.contains_break()),
            Exp::IfElse(_, conseq, alt) => {
                conseq.contains_break()
                    || if let Some(alt) = &**alt {
                        alt.contains_break()
                    } else {
                        false
                    }
            }
            Exp::Switch(_, _, cases) => cases.iter().any(|(_, e)| e.contains_break()),
            Exp::Match(_, _, cases) => cases.iter().any(|(_, _, e)| e.contains_break()),
            Exp::MatchLit(_, arms) => arms.iter().any(|(_, e)| e.contains_break()),
            Exp::Assign(_, exp) => exp.contains_break(),
            Exp::LetBind(_, exp) => exp.contains_break(),
            Exp::Declare(_) => false,
            Exp::Call(_, exps) => exps.iter().any(|e| e.contains_break()),
            Exp::Abort(exp) => exp.contains_break(),
            Exp::Primitive { op: _, args } => args.iter().any(|e| e.contains_break()),
            Exp::Data { op: _, args } => args.iter().any(|e| e.contains_break()),
            Exp::Borrow(_, exp) => exp.contains_break(),
            Exp::Return(_) | Exp::Value(_) | Exp::Variable(_) | Exp::Constant(_) => false,
            Exp::Unpack(_, _, exp) => exp.contains_break(),
            Exp::UnpackVariant(_, _, _, exp) => exp.contains_break(),
            Exp::VecUnpack(_, exp) => exp.contains_break(),
            Exp::Unstructured(nodes) => nodes.iter().any(|node| match node {
                UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) => {
                    body.contains_break()
                }
                UnstructuredNode::Goto(_) => false,
            }),
            Exp::Block(_, body) => body.contains_break(),
        }
    }

    pub fn contains_continue(&self) -> bool {
        match self {
            Exp::Continue(_) => true,
            Exp::Break(_) => false,
            // Ignore nested loops and whiles
            Exp::Loop(_, _) | Exp::While(_, _, _) => false,
            // Check sub-expressions
            Exp::Seq(seq) => seq.iter().any(|e| e.contains_continue()),
            Exp::IfElse(_, conseq, alt) => {
                conseq.contains_continue()
                    || if let Some(alt) = &**alt {
                        alt.contains_continue()
                    } else {
                        false
                    }
            }
            Exp::Switch(_, _, cases) => cases.iter().any(|(_, e)| e.contains_continue()),
            Exp::Match(_, _, cases) => cases.iter().any(|(_, _, e)| e.contains_continue()),
            Exp::MatchLit(_, arms) => arms.iter().any(|(_, e)| e.contains_continue()),
            Exp::Assign(_, exp) => exp.contains_continue(),
            Exp::LetBind(_, exp) => exp.contains_continue(),
            Exp::Declare(_) => false,
            Exp::Call(_, exps) => exps.iter().any(|e| e.contains_continue()),
            Exp::Abort(exp) => exp.contains_continue(),
            Exp::Primitive { op: _, args } => args.iter().any(|e| e.contains_continue()),
            Exp::Data { op: _, args } => args.iter().any(|e| e.contains_continue()),
            Exp::Borrow(_, exp) => exp.contains_continue(),
            Exp::Return(_) | Exp::Value(_) | Exp::Variable(_) | Exp::Constant(_) => false,
            Exp::Unpack(_, _, exp) => exp.contains_continue(),
            Exp::UnpackVariant(_, _, _, exp) => exp.contains_continue(),
            Exp::VecUnpack(_, exp) => exp.contains_continue(),
            Exp::Unstructured(nodes) => nodes.iter().any(|node| match node {
                UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) => {
                    body.contains_continue()
                }
                UnstructuredNode::Goto(_) => false,
            }),
            Exp::Block(_, body) => body.contains_continue(),
        }
    }

    pub fn map_mut<F>(&mut self, f: F)
    where
        F: FnOnce(Exp) -> Exp,
    {
        *self = f(std::mem::replace(self, Exp::Break(None)));
    }
}

// -------------------------------------------------------------------------------------------------
// Display
// -------------------------------------------------------------------------------------------------

// Display trait for module
impl std::fmt::Display for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "module {} {{", self.name)?;
        for (name, fun) in &self.functions {
            // TODO print function args
            writeln!(f, "    public fun {} () {{", name)?;
            write!(f, "{}", fun)?;
            writeln!(f, "    }}\n")?;
        }
        writeln!(f, "}}")
    }
}

// Display trait for function
impl std::fmt::Display for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)
    }
}

impl std::fmt::Display for Exp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn indent(f: &mut std::fmt::Formatter<'_>, level: usize) -> std::fmt::Result {
            for _ in 0..level {
                write!(f, "    ")?;
            }
            Ok(())
        }

        /// Print `exp` as a value on the right-hand side of an assignment/let-bind: no leading
        /// indent (the caller already wrote `lhs = `), and no trailing newline (the caller
        /// writes the closing `;`). For block-like expressions (IfElse, Switch) this keeps
        /// braces aligned with the assignment's indent level so the result reads like the
        /// idiomatic Move `let X = if (...) { ... } else { ... };` form.
        fn fmt_value(f: &mut std::fmt::Formatter<'_>, exp: &Exp, level: usize) -> std::fmt::Result {
            match exp {
                Exp::IfElse(cond, conseq, alt) => {
                    writeln!(f, "if ({}) {{", cond)?;
                    fmt_block_body(f, conseq, level + 1)?;
                    indent(f, level)?;
                    if let Some(alt) = &**alt {
                        writeln!(f, "}} else {{")?;
                        fmt_block_body(f, alt, level + 1)?;
                        indent(f, level)?;
                    }
                    write!(f, "}}")
                }
                Exp::Switch(term, enum_ty, cases) => {
                    writeln!(f, "match({}: {enum_ty}) {{", term)?;
                    for (variant, case) in cases {
                        indent(f, level + 1)?;
                        writeln!(f, "{enum_ty}::{variant} => {{")?;
                        fmt_block_body(f, case, level + 2)?;
                        indent(f, level + 1)?;
                        writeln!(f, "}},")?;
                    }
                    indent(f, level)?;
                    write!(f, "}}")
                }
                Exp::Match(term, enum_ty, cases) => {
                    writeln!(f, "match({}: {enum_ty}) {{", term)?;
                    for (variant, fields, case) in cases {
                        indent(f, level + 1)?;
                        write_match_pattern(f, enum_ty, variant, fields)?;
                        writeln!(f, " => {{")?;
                        fmt_block_body(f, case, level + 2)?;
                        indent(f, level + 1)?;
                        writeln!(f, "}},")?;
                    }
                    indent(f, level)?;
                    write!(f, "}}")
                }
                Exp::MatchLit(scrutinee, arms) => {
                    writeln!(f, "match ({}) {{", scrutinee)?;
                    for (lit, body) in arms {
                        indent(f, level + 1)?;
                        writeln!(f, "{lit} => {{")?;
                        fmt_block_body(f, body, level + 2)?;
                        indent(f, level + 1)?;
                        writeln!(f, "}},")?;
                    }
                    indent(f, level)?;
                    write!(f, "}}")
                }
                // Non-block expressions render inline via their normal Display.
                other => write!(f, "{}", other),
            }
        }

        /// Print `exp` as the body of a brace-block at `level`. Recurs into `Seq` so each
        /// item is positioned at the block's indent. Inline expressions (which `fmt_exp` would
        /// emit naked, since their normal use is as the RHS of an Assign where the caller
        /// already wrote the indent) get an explicit indent and trailing newline so they read
        /// like the trailing value of a block expression: `if (c) { ...; value }`.
        fn fmt_block_body(
            f: &mut std::fmt::Formatter<'_>,
            exp: &Exp,
            level: usize,
        ) -> std::fmt::Result {
            match exp {
                Exp::Seq(seq) => {
                    for item in seq {
                        fmt_block_body(f, item, level)?;
                    }
                    Ok(())
                }
                e if emits_own_line(e) => fmt_exp(f, e, level),
                e => {
                    indent(f, level)?;
                    writeln!(f, "{}", e)
                }
            }
        }

        /// `true` for `Exp` variants whose `fmt_exp` already starts with `indent(level)` and
        /// ends with `writeln!`. Everything else is an "inline" expression — `Value`,
        /// `Variable`, `Primitive`, `Borrow`, etc. — that needs `fmt_block_body` to provide
        /// indent/newline when it appears at statement position.
        fn emits_own_line(exp: &Exp) -> bool {
            matches!(
                exp,
                Exp::Break(_)
                    | Exp::Continue(_)
                    | Exp::Loop(_, _)
                    | Exp::While(_, _, _)
                    | Exp::IfElse(_, _, _)
                    | Exp::Switch(_, _, _)
                    | Exp::Match(_, _, _)
                    | Exp::MatchLit(_, _)
                    | Exp::Return(_)
                    | Exp::Assign(_, _)
                    | Exp::LetBind(_, _)
                    | Exp::Declare(_)
                    | Exp::Call(_, _)
                    | Exp::Abort(_)
                    | Exp::Data { .. }
                    | Exp::Unpack(_, _, _)
                    | Exp::UnpackVariant(_, _, _, _)
                    | Exp::VecUnpack(_, _)
                    | Exp::Unstructured(_)
                    | Exp::Block(_, _)
            )
        }

        fn fmt_exp(f: &mut std::fmt::Formatter<'_>, exp: &Exp, level: usize) -> std::fmt::Result {
            match exp {
                Exp::Break(label) => {
                    indent(f, level)?;
                    match label {
                        Some(l) => writeln!(f, "break 'loop_{};", l),
                        None => writeln!(f, "break;"),
                    }
                }
                Exp::Continue(label) => {
                    indent(f, level)?;
                    match label {
                        Some(l) => writeln!(f, "continue 'loop_{};", l),
                        None => writeln!(f, "continue;"),
                    }
                }
                Exp::Loop(label, body) => {
                    indent(f, level)?;
                    match label {
                        Some(l) => writeln!(f, "'loop_{}: loop {{", l)?,
                        None => writeln!(f, "loop {{")?,
                    }
                    fmt_exp(f, body, level + 1)?;
                    indent(f, level)?;
                    writeln!(f, "}}")
                }
                Exp::Seq(seq) => {
                    if seq.is_empty() {
                        return Ok(());
                    } else {
                        for exp in seq {
                            fmt_exp(f, exp, level)?;
                        }
                    }
                    Ok(())
                }
                Exp::While(label, cond, body) => {
                    indent(f, level)?;
                    match label {
                        Some(l) => writeln!(f, "'loop_{}: while({}) {{", l, cond)?,
                        None => writeln!(f, "while({}) {{", cond)?,
                    }
                    fmt_exp(f, body, level + 1)?;
                    indent(f, level)?;
                    writeln!(f, "}}")
                }
                Exp::IfElse(cond, conseq, alt) => {
                    indent(f, level)?;
                    writeln!(f, "if ({}) {{", cond)?;
                    fmt_exp(f, conseq, level + 1)?;
                    indent(f, level)?;
                    if let Some(alt) = &**alt {
                        writeln!(f, "}} else {{")?;
                        fmt_exp(f, alt, level + 1)?;
                        indent(f, level)?;
                    }
                    writeln!(f, "}}")
                }
                Exp::Switch(term, enum_ty, cases) => {
                    indent(f, level)?;
                    writeln!(f, "match({}: {enum_ty}) {{", term)?;
                    for (variant, case) in cases {
                        indent(f, level + 1)?;
                        writeln!(f, "{enum_ty}::{variant} => {{")?;
                        fmt_exp(f, case, level + 2)?;
                        indent(f, level + 1)?;
                        writeln!(f, "}},")?;
                    }
                    indent(f, level)?;
                    writeln!(f, "}}")
                }
                Exp::Match(term, enum_ty, cases) => {
                    indent(f, level)?;
                    writeln!(f, "match({}: {enum_ty}) {{", term)?;
                    for (variant, fields, case) in cases {
                        indent(f, level + 1)?;
                        write_match_pattern(f, enum_ty, variant, fields)?;
                        writeln!(f, " => {{")?;
                        fmt_exp(f, case, level + 2)?;
                        indent(f, level + 1)?;
                        writeln!(f, "}},")?;
                    }
                    indent(f, level)?;
                    writeln!(f, "}}")
                }
                Exp::MatchLit(scrutinee, arms) => {
                    indent(f, level)?;
                    writeln!(f, "match ({}) {{", scrutinee)?;
                    for (lit, body) in arms {
                        indent(f, level + 1)?;
                        writeln!(f, "{lit} => {{")?;
                        fmt_exp(f, body, level + 2)?;
                        indent(f, level + 1)?;
                        writeln!(f, "}},")?;
                    }
                    indent(f, level)?;
                    writeln!(f, "}}")
                }
                Exp::Data { op, args } => {
                    indent(f, level)?;
                    write_data_op(f, op, args)
                }
                Exp::Return(exps) => {
                    indent(f, level)?;
                    write!(f, "return ")?;
                    for exp in exps {
                        fmt_exp(f, exp, level)?;
                    }
                    writeln!(f)
                }
                Exp::Assign(items, exp) => {
                    indent(f, level)?;
                    write!(f, "{} = ", items.join(", "))?;
                    fmt_value(f, exp, level)?;
                    writeln!(f, ";")
                }
                Exp::LetBind(items, exp) => {
                    indent(f, level)?;
                    write!(f, "let {} = ", items.join(", "))?;
                    fmt_value(f, exp, level)?;
                    writeln!(f, ";")
                }
                Exp::Declare(items) => {
                    indent(f, level)?;
                    writeln!(f, "let {};", items.join(", "))
                }
                Exp::Call((module_name, fun_name), exps) => {
                    indent(f, level)?;
                    if module_name.is_builtin() {
                        write!(f, "{fun_name}(")?;
                    } else {
                        write!(f, "{module_name}::{fun_name}(")?;
                    }
                    for (i, exp) in exps.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        fmt_exp(f, exp, level)?;
                    }
                    writeln!(f, ")")
                }
                Exp::Abort(exp) => {
                    indent(f, level)?;
                    writeln!(f, "abort {};", exp)
                }
                Exp::Primitive { op, args } => write_primitive_op(f, op, args),
                Exp::Borrow(mut_, exp) => write!(f, "{}{}", if *mut_ { "&mut " } else { "&" }, exp),
                Exp::Value(value) => write!(f, "{}", value),
                Exp::Variable(name) => write!(f, "{}", name),
                Exp::Constant(constant) => write!(f, "{:?}", constant),
                Exp::Unpack(struct_ty, items, exp) => {
                    indent(f, level)?;
                    write!(f, "let {struct_ty} {{")?;
                    if !items.is_empty() {
                        write!(f, " ")?;
                    }
                    for (i, (sym, name)) in items.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{sym}: {name}")?;
                    }
                    if !items.is_empty() {
                        write!(f, " ")?;
                    }
                    writeln!(f, "}} = {};", exp)
                }
                Exp::UnpackVariant(unpack_kind, (enum_ty, variant), items, exp) => {
                    indent(f, level)?;
                    write!(f, "let {enum_ty}::{variant} {{")?;
                    if !items.is_empty() {
                        write!(f, " ")?;
                    }
                    for (i, (sym, name)) in items.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{sym}: {name}")?;
                    }
                    if !items.is_empty() {
                        write!(f, " ")?;
                    }
                    let unpack_str = match unpack_kind {
                        UnpackKind::Value => "",
                        UnpackKind::ImmRef => "&",
                        UnpackKind::MutRef => "&mut ",
                    };
                    writeln!(f, "}} = {unpack_str}{exp};",)
                }
                Exp::VecUnpack(items, exp) => {
                    indent(f, level)?;
                    write!(f, "let [")?;
                    for (i, name) in items.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", name)?;
                    }
                    writeln!(f, "] = {};", exp)
                }
                Exp::Unstructured(nodes) => {
                    indent(f, level)?;
                    writeln!(f, "unstructured {{")?;
                    for node in nodes {
                        match node {
                            UnstructuredNode::Labeled(label, body) => {
                                indent(f, level + 1)?;
                                writeln!(f, "'label_{}:", label)?;
                                fmt_exp(f, body, level + 1)?;
                            }
                            UnstructuredNode::Statement(exp) => {
                                fmt_exp(f, exp, level + 1)?;
                            }
                            UnstructuredNode::Goto(label) => {
                                indent(f, level + 1)?;
                                writeln!(f, "goto 'label_{};", label)?;
                            }
                        }
                    }
                    indent(f, level)?;
                    writeln!(f, "}}")
                }
                // Block is a marker; recur into the body. The pretty printer emits the
                // `/* block N */` comment; the Debug `Display` path is for tests and stays
                // transparent.
                Exp::Block(_, body) => fmt_exp(f, body, level),
            }
        }

        fmt_exp(f, self, 2)
    }
}

/// Print a match-arm pattern: `mid::enum::variant` for a tag-only arm, or
/// `mid::enum::variant { field: binder, ... }` when the refinement has hoisted an
/// `UnpackVariant` up into the pattern.
fn write_match_pattern(
    f: &mut std::fmt::Formatter<'_>,
    enum_ty: &TypeRef,
    variant: &Symbol,
    fields: &[(Symbol, String)],
) -> std::fmt::Result {
    write!(f, "{enum_ty}::{variant}")?;
    if fields.is_empty() {
        return Ok(());
    }
    write!(f, " {{ ")?;
    for (i, (sym, name)) in fields.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{sym}: {name}")?;
    }
    write!(f, " }}")
}

fn write_data_op(
    f: &mut std::fmt::Formatter<'_>,
    op: &DataOp,
    args: &[Exp],
) -> Result<(), std::fmt::Error> {
    match op {
        DataOp::Pack(_) => todo!(),
        DataOp::Unpack(_) => todo!(),
        DataOp::ReadRef => write!(f, "*{}", args[0]),
        DataOp::WriteRef => writeln!(f, "*{} = {}", args[0], args[1]),
        DataOp::FreezeRef => write!(f, "{}", args[0]),
        DataOp::MutBorrowField(field_ref) => {
            write!(f, "&mut ({}).{}", args[0], field_ref.field.name)
        }
        DataOp::ImmBorrowField(field_ref) => write!(f, "&{}.{}", args[0], field_ref.field.name),
        DataOp::VecPack(_) => write!(
            f,
            "vector[{}]",
            args.iter()
                .map(|e| format!("{}", e))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        DataOp::VecLen(_) => write!(f, "{}.length()", args[0]),
        DataOp::VecImmBorrow(_) => write!(f, "&{}[{}]", args[0], args[1]),
        DataOp::VecMutBorrow(_) => write!(f, "&mut {}[{}]", args[0], args[1]),
        DataOp::VecPushBack(_) => write!(f, "{}.push_back({})", args[0], args[1]),
        DataOp::VecPopBack(_) => write!(f, "{}.pop_back()", args[0]),
        DataOp::VecUnpack(_) => unreachable!(),
        DataOp::VecSwap(_) => write!(f, "{}.swap({}, {})", args[0], args[1], args[2]),
        DataOp::PackVariant(_) => write!(f, "E::V .. fields .. args"),
        DataOp::UnpackVariant(_) => unreachable!(),
        DataOp::UnpackVariantImmRef(_) => unreachable!(),
        DataOp::UnpackVariantMutRef(_) => unreachable!(),
    }
}

/// Render `exp` as an operand of `&&`/`||`. Naked `Seq` operands — which `recover_flag` emits
/// when each `||` branch needs its per-feed setup statements to stay lazy under short-circuit
/// — are wrapped in `{ … }` so the output parses as Move. All other shapes render via their
/// normal `Display`. This mirrors `pretty_printer::primitive_op_doc::bin`'s production
/// behavior so the debug `Display` path used by `run_move_test` produces valid Move.
fn fmt_short_circuit_operand(
    f: &mut std::fmt::Formatter<'_>,
    exp: &Exp,
) -> Result<(), std::fmt::Error> {
    match exp {
        Exp::Seq(items) => {
            write!(f, "{{ ")?;
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    write!(f, "; ")?;
                }
                write!(f, "{}", item)?;
            }
            write!(f, " }}")
        }
        _ => write!(f, "{}", exp),
    }
}

fn write_primitive_op(
    f: &mut std::fmt::Formatter<'_>,
    op: &PrimitiveOp,
    args: &[Exp],
) -> Result<(), std::fmt::Error> {
    match op {
        PrimitiveOp::CastU8 => todo!(),
        PrimitiveOp::CastU16 => todo!(),
        PrimitiveOp::CastU32 => todo!(),
        PrimitiveOp::CastU64 => todo!(),
        PrimitiveOp::CastU128 => todo!(),
        PrimitiveOp::CastU256 => todo!(),
        PrimitiveOp::Add => write!(f, "{} + {}", args[0], args[1]),
        PrimitiveOp::Subtract => write!(f, "{} - {}", args[0], args[1]),
        PrimitiveOp::Multiply => write!(f, "{} * {}", args[0], args[1]),
        PrimitiveOp::Modulo => write!(f, "{} % {}", args[0], args[1]),
        PrimitiveOp::Divide => write!(f, "{} / {}", args[0], args[1]),
        PrimitiveOp::BitOr => write!(f, "{} | {}", args[0], args[1]),
        PrimitiveOp::BitAnd => write!(f, "{} & {}", args[0], args[1]),
        PrimitiveOp::Xor => write!(f, "{} ^ {}", args[0], args[1]),
        PrimitiveOp::Or => {
            fmt_short_circuit_operand(f, &args[0])?;
            write!(f, " || ")?;
            fmt_short_circuit_operand(f, &args[1])
        }
        PrimitiveOp::And => {
            fmt_short_circuit_operand(f, &args[0])?;
            write!(f, " && ")?;
            fmt_short_circuit_operand(f, &args[1])
        }
        PrimitiveOp::Not => write!(f, "!({})", args[0]),
        PrimitiveOp::Equal => write!(f, "{} == {}", args[0], args[1]),
        PrimitiveOp::NotEqual => write!(f, "{} != {}", args[0], args[1]),
        PrimitiveOp::LessThan => write!(f, "{} < {}", args[0], args[1]),
        PrimitiveOp::GreaterThan => write!(f, "{} > {}", args[0], args[1]),
        PrimitiveOp::LessThanOrEqual => write!(f, "{} <= {}", args[0], args[1]),
        PrimitiveOp::GreaterThanOrEqual => write!(f, "{} >= {}", args[0], args[1]),
        PrimitiveOp::ShiftLeft => todo!(),
        PrimitiveOp::ShiftRight => todo!(),
    }
}

// -------------------------------------------------------------------------------------------------
// Unstructured Control Flow Types
// -------------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum UnstructuredNode {
    /// Labeled block: 'label_N: { ... }
    Labeled(Label, Box<Exp>),
    /// Unlabeled statement
    Statement(Box<Exp>),
    /// Goto statement: goto 'label_N;
    Goto(Label),
}
