// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::normalized::{Constant, ModuleId};

use move_core_types::{account_address::AccountAddress, runtime_value::MoveValue as Value};
use move_model_2::{model::Model, source_kind::SourceKind};
use move_stackless_bytecode_2::ast::{DataOp, PrimitiveOp};
use move_symbol_pool::Symbol;

use std::collections::{BTreeMap, BTreeSet};

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
    pub code: Exp,
    // TODO add function args?
}

/// A reference to a module from inside an expression. Lowering emits `Qualified(mid)` everywhere;
/// the `collect_uses` refinement rewrites `Qualified(mid)` to `Aliased(name)` when `mid` is in the
/// containing `Module.uses` map.
#[derive(Debug, Clone, Copy)]
pub enum ModuleRef {
    Qualified(ModuleId<Symbol>),
    Aliased(Symbol),
}

impl std::fmt::Display for ModuleRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModuleRef::Qualified(mid) => write!(f, "{mid}"),
            ModuleRef::Aliased(name) => write!(f, "{name}"),
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

#[derive(Debug, Clone)]
pub enum UnpackKind {
    Value,
    ImmRef,
    MutRef,
}

/// A loop label. `None` is the unlabeled form; `Some(L)` prints as `'loop_L:` / `break 'loop_L;`
/// / `continue 'loop_L;`. Structuring always emits `Some`; `strip_loop_labels` demotes labels
/// whose only uses sit directly in the labeled loop's body (no nested loops in between).
pub type Label = u64;

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
        }
    }

    pub fn map_mut<F>(&mut self, f: F)
    where
        F: FnOnce(Exp) -> Exp,
    {
        *self = f(std::mem::replace(self, Exp::Break(None)));
    }

    /// Every local name mentioned anywhere in `self` — reads (`Variable`), writes
    /// (`Assign`/`LetBind`/`VecUnpack`/`Unpack` targets), and declarations (`Declare`,
    /// `LetBind`) — collected recursively through children. Sub-expression values of `Unpack`
    /// etc. are recursed into, but the unpacked struct/enum/variant identifiers are not
    /// included (those are types, not locals).
    ///
    /// This intentionally unifies reads and writes: callers that want "did this subtree
    /// touch X in any way" can ask once. Use this when you need an over-approximation of
    /// the locals an expression can read or modify, e.g. to decide whether moving a
    /// declaration across it would change behavior.
    pub fn referenced_names(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        self.collect_referenced_names(&mut out);
        out
    }

    fn collect_referenced_names(&self, out: &mut BTreeSet<String>) {
        match self {
            Exp::Variable(n) => {
                out.insert(n.clone());
            }
            Exp::Declare(names) => {
                for n in names {
                    out.insert(n.clone());
                }
            }
            Exp::LetBind(names, value) | Exp::Assign(names, value) => {
                for n in names {
                    out.insert(n.clone());
                }
                value.collect_referenced_names(out);
            }
            Exp::VecUnpack(names, value) => {
                for n in names {
                    out.insert(n.clone());
                }
                value.collect_referenced_names(out);
            }
            Exp::Unpack(_, fields, value) | Exp::UnpackVariant(_, _, fields, value) => {
                for (_, name) in fields {
                    out.insert(name.clone());
                }
                value.collect_referenced_names(out);
            }
            Exp::Seq(items) | Exp::Return(items) | Exp::Call(_, items) => {
                for it in items {
                    it.collect_referenced_names(out);
                }
            }
            Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
                for a in args {
                    a.collect_referenced_names(out);
                }
            }
            Exp::IfElse(cond, conseq, alt) => {
                cond.collect_referenced_names(out);
                conseq.collect_referenced_names(out);
                if let Some(a) = alt.as_ref() {
                    a.collect_referenced_names(out);
                }
            }
            Exp::Switch(cond, _, cases) => {
                cond.collect_referenced_names(out);
                for (_, body) in cases {
                    body.collect_referenced_names(out);
                }
            }
            Exp::Match(cond, _, cases) => {
                cond.collect_referenced_names(out);
                for (_, _, body) in cases {
                    body.collect_referenced_names(out);
                }
            }
            Exp::Loop(_, body) => body.collect_referenced_names(out),
            Exp::While(_, cond, body) => {
                cond.collect_referenced_names(out);
                body.collect_referenced_names(out);
            }
            Exp::Abort(value) | Exp::Borrow(_, value) => value.collect_referenced_names(out),
            Exp::Unstructured(nodes) => {
                for node in nodes {
                    match node {
                        UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) => {
                            body.collect_referenced_names(out);
                        }
                        UnstructuredNode::Goto(_) => {}
                    }
                }
            }
            Exp::Value(_) | Exp::Constant(_) | Exp::Break(_) | Exp::Continue(_) => {}
        }
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
                    write!(f, "{module_name}::{fun_name}(")?;
                    for exp in exps {
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
        PrimitiveOp::Or => write!(f, "{} || {}", args[0], args[1]),
        PrimitiveOp::And => write!(f, "{} && {}", args[0], args[1]),
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
