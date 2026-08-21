// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Per-binder `mut` inference for the pretty printer. Bytecode carries no `mut`, so it is
//! recomputed on the final AST, immediately before printing:
//!
//! ```text
//! let mut x = 1; x = 2;                     // reassigned
//! let mut y = 1; loop { y = y + 1 };        // back edge repeats the assignment
//! let mut z = 1; f(&mut z);                 // mutably borrowed
//! let w; if (c) { w = 1 } else { w = 2 };   // no mut: one assignment per path
//! ```
//!
//! Initialized binders (`let x = e`, unpack and pattern bindings, parameters) need `mut` at
//! one later free assignment or `&mut`; a `Declare` (`let x;`) at two on some path, its
//! first being the initializer. "Free" means not shadowed; scope boundaries mirror the
//! printer's (`Block`s splice, bare `Seq`s brace). Only `Borrow(true, Variable(x))` is a
//! mutable borrow: writes through a reference held in a local (`WriteRef`) do not mutate
//! the local.
//!
//! Results key on node pointer identity (plus arm index for `Match`): valid only for the
//! exact tree analyzed, like `liveness::Liveness`.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{Exp, Function, UnstructuredNode};

pub struct MutAnnotations {
    /// (binder node id, arm index) -> names bound at that binder that need `mut`. Arm index
    /// is 0 for every binder except `Match`, where each arm's pattern is its own binder.
    mut_binders: BTreeMap<(usize, usize), BTreeSet<String>>,
    /// Parameter names (`l{i}`) that need `mut` in the signature.
    params: BTreeSet<String>,
}

/// Saturating path-assignment count: we only ever need to distinguish 0, 1, and "2 or more".
const MANY: u8 = 2;

impl MutAnnotations {
    pub fn analyze(fun: &Function) -> Self {
        let mut annots = MutAnnotations {
            mut_binders: BTreeMap::new(),
            params: BTreeSet::new(),
        };
        let top = flatten_scope(&fun.code);
        // Parameters share `term_reconstruction::local_name`'s `l{i}` scheme and are
        // initialized on entry; a shadowing `let l{i}` stops the scan.
        for i in 0..fun.parameters.len() {
            let name = format!("l{i}");
            let (assigns, mut_borrowed) = scan_stmts(&top, &name);
            if assigns >= 1 || mut_borrowed {
                annots.params.insert(name);
            }
        }
        annots.visit_scope(&fun.code);
        annots
    }

    pub fn param_needs_mut(&self, name: &str) -> bool {
        self.params.contains(name)
    }

    /// Does `name`, bound at `binder` (a `LetBind`, `Declare`, `Unpack`, `UnpackVariant`, or
    /// `VecUnpack` node), need `mut`?
    pub fn needs_mut(&self, binder: &Exp, name: &str) -> bool {
        self.arm_needs_mut(binder, 0, name)
    }

    /// Does `name`, bound by the pattern of arm `arm_idx` of `match_exp`, need `mut`?
    pub fn arm_needs_mut(&self, match_exp: &Exp, arm_idx: usize, name: &str) -> bool {
        self.mut_binders
            .get(&(node_id(match_exp), arm_idx))
            .is_some_and(|s| s.contains(name))
    }

    /// Flatten `root` into its rendered statement list, record each binder's `mut` needs
    /// against its tail, then descend into sub-scopes.
    fn visit_scope(&mut self, root: &Exp) {
        let stmts = flatten_scope(root);
        for (i, stmt) in stmts.iter().enumerate() {
            self.record_binder(stmt, &stmts[i + 1..]);
            self.visit_children(stmt);
        }
    }

    fn record_binder(&mut self, stmt: &Exp, tail: &[&Exp]) {
        // (initialized-at-binder?, bound names)
        let (initialized, names): (bool, Vec<&String>) = match stmt {
            Exp::LetBind(names, _) | Exp::VecUnpack(names, _) => (true, names.iter().collect()),
            Exp::Declare(names) => (false, names.iter().collect()),
            Exp::Unpack(_, fields, _) | Exp::UnpackVariant(_, _, fields, _) => {
                (true, fields.iter().map(|(_, n)| n).collect())
            }
            Exp::Break(_)
            | Exp::Continue(_)
            | Exp::Loop(_, _)
            | Exp::Seq(_)
            | Exp::While(_, _, _)
            | Exp::IfElse(_, _, _)
            | Exp::Switch(_, _, _)
            | Exp::Match(_, _, _)
            | Exp::MatchLit(_, _)
            | Exp::Return(_)
            | Exp::Assign(_, _)
            | Exp::Call(_, _)
            | Exp::Abort(_)
            | Exp::Primitive { .. }
            | Exp::Data { .. }
            | Exp::Borrow(_, _)
            | Exp::Value(_)
            | Exp::Variable(_)
            | Exp::Constant(_)
            | Exp::Unstructured(_)
            | Exp::Block(_, _) => return,
        };
        let threshold = if initialized { 1 } else { MANY };
        for name in names {
            let (assigns, mut_borrowed) = scan_stmts(tail, name);
            if assigns >= threshold || mut_borrowed {
                self.mut_binders
                    .entry((node_id(stmt), 0))
                    .or_default()
                    .insert(name.clone());
            }
        }
    }

    /// Descend into sub-expressions as new scopes. `Block`s never reach here: flattening
    /// unwraps them.
    fn visit_children(&mut self, stmt: &Exp) {
        match stmt {
            Exp::Break(_)
            | Exp::Continue(_)
            | Exp::Declare(_)
            | Exp::Value(_)
            | Exp::Variable(_)
            | Exp::Constant(_) => {}
            Exp::Loop(_, body) => self.visit_scope(body),
            Exp::While(_, cond, body) => {
                self.visit_scope(cond);
                self.visit_scope(body);
            }
            Exp::IfElse(cond, conseq, alt) => {
                self.visit_scope(cond);
                self.visit_scope(conseq);
                if let Some(alt) = alt.as_ref() {
                    self.visit_scope(alt);
                }
            }
            Exp::Switch(subject, _, arms) => {
                self.visit_scope(subject);
                for (_, body) in arms {
                    self.visit_scope(body);
                }
            }
            Exp::Match(subject, _, arms) => {
                self.visit_scope(subject);
                for (arm_idx, arm) in arms.iter().enumerate() {
                    arm.guard.iter().for_each(|g| self.visit_scope(g));
                    // Pattern bindings scope over the arm body only; the guard sees
                    // immutable references, so it never forces `mut`.
                    let body_stmts = flatten_scope(&arm.rhs);
                    for (_, name) in &arm.fields {
                        let (assigns, mut_borrowed) = scan_stmts(&body_stmts, name);
                        if assigns >= 1 || mut_borrowed {
                            self.mut_binders
                                .entry((node_id(stmt), arm_idx))
                                .or_default()
                                .insert(name.clone());
                        }
                    }
                    self.visit_scope(&arm.rhs);
                }
            }
            Exp::MatchLit(subject, arms) => {
                self.visit_scope(subject);
                for (_, body) in arms {
                    self.visit_scope(body);
                }
            }
            Exp::Seq(items) | Exp::Return(items) | Exp::Call(_, items) => {
                for item in items {
                    self.visit_scope(item);
                }
            }
            Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
                for arg in args {
                    self.visit_scope(arg);
                }
            }
            Exp::Assign(_, rhs) | Exp::LetBind(_, rhs) => self.visit_scope(rhs),
            Exp::VecUnpack(_, e)
            | Exp::Unpack(_, _, e)
            | Exp::UnpackVariant(_, _, _, e)
            | Exp::Abort(e)
            | Exp::Borrow(_, e)
            | Exp::Block(_, e) => self.visit_scope(e),
            Exp::Unstructured(nodes) => {
                for node in nodes {
                    match node {
                        UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) => {
                            self.visit_scope(body)
                        }
                        UnstructuredNode::Goto(_) => {}
                    }
                }
            }
        }
    }
}

fn node_id(exp: &Exp) -> usize {
    exp as *const Exp as usize
}

// -------------------------------------------------------------------------------------------------
// Scope flattening (mirrors the printer's `push_stmt`/`e_block` Block inlining)

/// The rendered statement list: `Block` wrappers peel, `Block(Seq)` items splice in as
/// siblings. Bare `Seq`s stay single statements; they render braced, opening a scope.
fn flatten_scope(root: &Exp) -> Vec<&Exp> {
    let mut inner = root;
    while let Exp::Block(_, body) = inner {
        inner = body;
    }
    let mut out = Vec::new();
    match inner {
        Exp::Seq(items) => {
            for item in items {
                push_flat_stmt(item, &mut out);
            }
        }
        other => out.push(other),
    }
    out
}

fn push_flat_stmt<'a>(exp: &'a Exp, out: &mut Vec<&'a Exp>) {
    match exp {
        Exp::Block(_, body) => match body.as_ref() {
            Exp::Seq(items) => {
                for item in items {
                    push_flat_stmt(item, out);
                }
            }
            inner => push_flat_stmt(inner, out),
        },
        other => out.push(other),
    }
}

// -------------------------------------------------------------------------------------------------
// Mutation scan

/// Does `stmt` rebind `name`, shadowing it for the rest of the statement list?
fn shadows(stmt: &Exp, name: &str) -> bool {
    match stmt {
        Exp::LetBind(names, _) | Exp::Declare(names) | Exp::VecUnpack(names, _) => {
            names.iter().any(|n| n == name)
        }
        Exp::Unpack(_, fields, _) | Exp::UnpackVariant(_, _, fields, _) => {
            fields.iter().any(|(_, n)| n == name)
        }
        _ => false,
    }
}

/// Mutation summary for one name over one construct: per-continuation maximum free-assignment
/// counts, saturated at [`MANY`] (`None` = no path exits that way). Flow-sensitive like the
/// compiler's check: an assignment repeats only if a back edge is reachable after it, so
/// `l = e; break` counts once but `l = e; continue` counts many.
///
/// `brk`/`cont` are label-blind: a labeled form may target an outer loop, so `Loop` both
/// consumes and re-propagates them. All imprecision errs toward a spurious `mut` (a warning),
/// never a missing one (a compile error).
#[derive(Clone, Copy)]
struct Paths {
    /// Paths that fall through to the next statement.
    fall: Option<u8>,
    /// Paths that reach a `break` (any label).
    brk: Option<u8>,
    /// Paths that reach a `continue` (any label).
    cont: Option<u8>,
    /// Paths that leave the function (`return`/`abort`).
    ret: Option<u8>,
    /// Maximum count at any point inside: an upper bound on the fields above, and the only
    /// witness on diverging paths (a `loop` with no `break`).
    seen: u8,
    /// Any free `&mut name` anywhere in the construct, on any path.
    borrow: bool,
}

impl Paths {
    /// Straight-line code with `n` assignments and no borrow.
    fn pure(n: u8) -> Self {
        Paths {
            fall: Some(n),
            brk: None,
            cont: None,
            ret: None,
            seen: n,
            borrow: false,
        }
    }

    /// Sequence: `next` runs only on `self`'s fall-through paths, their assignments already
    /// counted.
    fn then(self, next: Paths) -> Paths {
        let Some(c) = self.fall else {
            return self;
        };
        Paths {
            fall: add_assigns(next.fall, c),
            brk: max_assigns(self.brk, add_assigns(next.brk, c)),
            cont: max_assigns(self.cont, add_assigns(next.cont, c)),
            ret: max_assigns(self.ret, add_assigns(next.ret, c)),
            seen: self.seen.max((c + next.seen).min(MANY)),
            borrow: self.borrow || next.borrow,
        }
    }

    /// Merge sibling branches: one of `self` or `other` runs.
    fn or(self, other: Paths) -> Paths {
        Paths {
            fall: max_assigns(self.fall, other.fall),
            brk: max_assigns(self.brk, other.brk),
            cont: max_assigns(self.cont, other.cont),
            ret: max_assigns(self.ret, other.ret),
            seen: self.seen.max(other.seen),
            borrow: self.borrow || other.borrow,
        }
    }

    /// A loop back edge with at least one assignment makes every count unbounded.
    fn saturate(self) -> Paths {
        let many = |o: Option<u8>| o.map(|_| MANY);
        Paths {
            fall: many(self.fall),
            brk: many(self.brk),
            cont: many(self.cont),
            ret: many(self.ret),
            seen: MANY,
            borrow: self.borrow,
        }
    }
}

fn add_assigns(o: Option<u8>, c: u8) -> Option<u8> {
    o.map(|v| (v + c).min(MANY))
}

fn max_assigns(a: Option<u8>, b: Option<u8>) -> Option<u8> {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(a), Some(b)) => Some(a.max(b)),
    }
}

/// Maximum free assignments of `name` along any path in a statement tail, and whether any
/// free `&mut name` occurs. Stops after a shadowing rebinding (whose right-hand side still
/// scans).
fn scan_stmts(stmts: &[&Exp], name: &str) -> (u8, bool) {
    let paths = stmt_list_paths(stmts, name);
    (paths.seen, paths.borrow)
}

fn stmt_list_paths(stmts: &[&Exp], name: &str) -> Paths {
    let mut acc = Paths::pure(0);
    for stmt in stmts {
        if acc.fall.is_none() {
            break;
        }
        acc = acc.then(exp_paths(stmt, name));
        if shadows(stmt, name) {
            break;
        }
    }
    acc
}

/// `stmt_list_paths` over the rendered statement list of a body/branch position.
fn scope_paths(root: &Exp, name: &str) -> Paths {
    stmt_list_paths(&flatten_scope(root), name)
}

/// Paths of expressions evaluated in sequence (call arguments and the like).
fn seq_paths(items: &[Exp], name: &str) -> Paths {
    items
        .iter()
        .fold(Paths::pure(0), |acc, item| acc.then(exp_paths(item, name)))
}

/// Fall-through and `continue` feed the back edge; `break` exits. Any back-edge assignment
/// repeats, saturating counts; `brk`/`cont` re-propagate for labeled forms.
fn loop_paths(body: Paths) -> Paths {
    let back_edge = max_assigns(body.fall, body.cont);
    let out = Paths {
        fall: body.brk,
        brk: body.brk,
        cont: body.cont,
        ret: body.ret,
        seen: body.seen,
        borrow: body.borrow,
    };
    if back_edge.is_some_and(|c| c > 0) {
        out.saturate()
    } else {
        out
    }
}

/// Free mutations of `name` in one expression; nested `Seq`/`Block` reopen scope handling.
fn exp_paths(exp: &Exp, name: &str) -> Paths {
    match exp {
        Exp::Variable(_) | Exp::Value(_) | Exp::Constant(_) | Exp::Declare(_) => Paths::pure(0),
        Exp::Break(_) => Paths {
            fall: None,
            brk: Some(0),
            cont: None,
            ret: None,
            seen: 0,
            borrow: false,
        },
        Exp::Continue(_) => Paths {
            fall: None,
            brk: None,
            cont: Some(0),
            ret: None,
            seen: 0,
            borrow: false,
        },
        Exp::Return(items) => {
            let p = seq_paths(items, name);
            Paths {
                fall: None,
                ret: max_assigns(p.ret, p.fall),
                ..p
            }
        }
        Exp::Abort(e) => {
            let p = exp_paths(e, name);
            Paths {
                fall: None,
                ret: max_assigns(p.ret, p.fall),
                ..p
            }
        }
        // Binding, not assignment: only the right-hand side can mutate the outer `name`.
        Exp::LetBind(_, rhs) => exp_paths(rhs, name),
        Exp::VecUnpack(_, e) | Exp::Unpack(_, _, e) | Exp::UnpackVariant(_, _, _, e) => {
            exp_paths(e, name)
        }
        Exp::Assign(targets, rhs) => {
            let hit = targets.iter().any(|t| t == name) as u8;
            exp_paths(rhs, name).then(Paths::pure(hit))
        }
        Exp::Borrow(is_mut, inner) => {
            if *is_mut && matches!(inner.as_ref(), Exp::Variable(n) if n == name) {
                Paths {
                    borrow: true,
                    ..Paths::pure(0)
                }
            } else {
                exp_paths(inner, name)
            }
        }
        Exp::Seq(_) | Exp::Block(_, _) => scope_paths(exp, name),
        Exp::IfElse(cond, conseq, alt) => {
            let arms = match alt.as_ref() {
                Some(alt) => scope_paths(conseq, name).or(scope_paths(alt, name)),
                None => scope_paths(conseq, name).or(Paths::pure(0)),
            };
            exp_paths(cond, name).then(arms)
        }
        Exp::Loop(_, body) => loop_paths(scope_paths(body, name)),
        Exp::While(_, cond, body) => {
            // `loop { if (cond) { body } else { break } }`: the condition runs every
            // iteration.
            let cond_paths = exp_paths(cond, name);
            let exit = Paths {
                fall: None,
                brk: Some(0),
                cont: None,
                ret: None,
                seen: 0,
                borrow: false,
            };
            let iteration = cond_paths.then(scope_paths(body, name).or(exit));
            loop_paths(iteration)
        }
        Exp::Switch(subject, _, arms) => {
            let arm_paths = arms
                .iter()
                .map(|(_, body)| scope_paths(body, name))
                .reduce(Paths::or)
                .unwrap_or(Paths::pure(0));
            exp_paths(subject, name).then(arm_paths)
        }
        Exp::Match(subject, _, arms) => {
            let arm_paths = arms
                .iter()
                .map(|arm| {
                    if arm.fields.iter().any(|(_, n)| n == name) {
                        // The arm pattern shadows `name` for its guard and whole body.
                        Paths::pure(0)
                    } else {
                        let body = scope_paths(&arm.rhs, name);
                        match &arm.guard {
                            Some(g) => exp_paths(g, name).then(body),
                            None => body,
                        }
                    }
                })
                .reduce(Paths::or)
                .unwrap_or(Paths::pure(0));
            exp_paths(subject, name).then(arm_paths)
        }
        Exp::MatchLit(subject, arms) => {
            let arm_paths = arms
                .iter()
                .map(|(_, body)| scope_paths(body, name))
                .reduce(Paths::or)
                .unwrap_or(Paths::pure(0));
            exp_paths(subject, name).then(arm_paths)
        }
        Exp::Call(_, items) => seq_paths(items, name),
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => seq_paths(args, name),
        // Unmodeled goto control flow: assume the worst, since spurious `mut` is only a
        // warning.
        Exp::Unstructured(_) => Paths {
            fall: Some(MANY),
            brk: Some(MANY),
            cont: Some(MANY),
            ret: Some(MANY),
            seen: MANY,
            borrow: true,
        },
    }
}

// -------------------------------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;

    fn var(n: &str) -> Exp {
        Exp::Variable(n.to_string())
    }
    fn letb(name: &str, rhs: Exp) -> Exp {
        Exp::LetBind(vec![name.to_string()], Box::new(rhs))
    }
    fn declare(name: &str) -> Exp {
        Exp::Declare(vec![name.to_string()])
    }
    fn assign(name: &str, rhs: Exp) -> Exp {
        Exp::Assign(vec![name.to_string()], Box::new(rhs))
    }
    fn num() -> Exp {
        Exp::Value(move_core_types::runtime_value::MoveValue::U64(0))
    }
    fn seq(items: Vec<Exp>) -> Exp {
        Exp::Seq(items)
    }
    fn ifelse(c: Exp, t: Exp, e: Option<Exp>) -> Exp {
        Exp::IfElse(Box::new(c), Box::new(t), Box::new(e))
    }
    fn mut_borrow(name: &str) -> Exp {
        Exp::Borrow(true, Box::new(var(name)))
    }

    fn fun_with(code: Exp) -> Function {
        Function {
            name: move_symbol_pool::Symbol::from("f"),
            visibility: move_binary_format::file_format::Visibility::Private,
            is_entry: false,
            type_parameters: vec![],
            parameters: vec![],
            returns: vec![],
            code,
            unstructured_blocks: vec![],
            residual_jumps: vec![],
        }
    }

    /// Whether the (single) `let x` binder in `stmts[binder_idx]` gets `mut`.
    fn binder_mut(stmts: Vec<Exp>, binder_idx: usize) -> bool {
        let fun = fun_with(seq(stmts));
        let annots = MutAnnotations::analyze(&fun);
        let Exp::Seq(items) = &fun.code else {
            unreachable!()
        };
        let (Exp::LetBind(names, _) | Exp::Declare(names)) = &items[binder_idx] else {
            panic!("binder_idx must point at a LetBind or Declare");
        };
        annots.needs_mut(&items[binder_idx], &names[0])
    }

    #[test]
    fn reassigned_let_needs_mut() {
        assert!(binder_mut(vec![letb("x", num()), assign("x", num())], 0));
    }

    #[test]
    fn unassigned_let_stays_immutable() {
        assert!(!binder_mut(vec![letb("x", num()), var("x")], 0));
    }

    #[test]
    fn mut_borrowed_let_needs_mut() {
        assert!(binder_mut(vec![letb("x", num()), mut_borrow("x")], 0));
    }

    #[test]
    fn declare_single_assign_per_path_stays_immutable() {
        // let x; if (c) { x = 1 } else { x = 2 }
        assert!(!binder_mut(
            vec![
                declare("x"),
                ifelse(num(), assign("x", num()), Some(assign("x", num()))),
            ],
            0,
        ));
    }

    #[test]
    fn declare_double_assign_needs_mut() {
        assert!(binder_mut(
            vec![declare("x"), assign("x", num()), assign("x", num())],
            0,
        ));
    }

    #[test]
    fn declare_assign_then_break_in_loop_stays_immutable() {
        // let x; loop { if (c) { x = 1; break }; ... }: the assign never reaches the back
        // edge, so it runs at most once.
        let body = seq(vec![
            ifelse(num(), seq(vec![assign("x", num()), Exp::Break(None)]), None),
            num(),
        ]);
        assert!(!binder_mut(
            vec![declare("x"), Exp::Loop(None, Box::new(body))],
            0,
        ));
    }

    #[test]
    fn assign_reaching_back_edge_needs_mut() {
        // let x = 0; loop { x = x + 1 }: the back edge repeats the assignment, for both
        // the LetBind and Declare forms.
        let body = seq(vec![assign("x", num())]);
        assert!(binder_mut(
            vec![declare("x"), Exp::Loop(None, Box::new(body.clone()))],
            0,
        ));
        assert!(binder_mut(
            vec![letb("x", num()), Exp::Loop(None, Box::new(body))],
            0,
        ));
    }

    #[test]
    fn shadowing_let_stops_the_scan() {
        // let x = 0; let x = 1; x = 2: the assignment mutates the second binding only.
        assert!(!binder_mut(
            vec![letb("x", num()), letb("x", num()), assign("x", num())],
            0,
        ));
        assert!(binder_mut(
            vec![letb("x", num()), letb("x", num()), assign("x", num())],
            1,
        ));
    }

    #[test]
    fn match_arm_shadow_protects_outer_binding() {
        // let x = 0; match (s) { V { f: x } => { x = 1 } }: the arm rebinds x, so the outer
        // x is untouched and the arm binding needs mut.
        let arms = vec![crate::ast::MatchArm {
            variant: move_symbol_pool::Symbol::from("V"),
            fields: vec![(move_symbol_pool::Symbol::from("f"), "x".to_string())],
            guard: None,
            rhs: seq(vec![assign("x", num())]),
        }];
        let match_exp = Exp::Match(
            Box::new(num()),
            crate::ast::TypeRef::Aliased(move_symbol_pool::Symbol::from("E")),
            arms,
        );
        let fun = fun_with(seq(vec![letb("x", num()), match_exp]));
        let annots = MutAnnotations::analyze(&fun);
        let Exp::Seq(items) = &fun.code else {
            unreachable!()
        };
        assert!(!annots.needs_mut(&items[0], "x"));
        assert!(annots.arm_needs_mut(&items[1], 0, "x"));
    }

    #[test]
    fn param_mut_borrow_is_detected() {
        let mut fun = fun_with(seq(vec![Exp::Call(
            (
                crate::ast::ModuleRef::Builtin,
                move_symbol_pool::Symbol::from("g"),
            ),
            vec![mut_borrow("l0")],
        )]));
        fun.parameters = vec![crate::ast::Type::U64];
        let annots = MutAnnotations::analyze(&fun);
        assert!(annots.param_needs_mut("l0"));
        assert!(!annots.param_needs_mut("l1"));
    }
}
