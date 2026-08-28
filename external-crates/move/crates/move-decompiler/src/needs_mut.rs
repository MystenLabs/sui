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
//! The analysis is a backward walk, a la liveness: the fact at each point maps every name
//! to its future free uses (an assignment count saturated at two, plus a mutable-borrow
//! bit), and a binder reads its `mut` need off the fact flowing in from after it. Loop back
//! edges run to a fixpoint; `break` and `continue` take the fact at their target loop's
//! exit or head. Imprecision errs toward a spurious `mut` (a warning), never a missing one
//! (a compile error).
//!
//! Results key on node pointer identity (plus arm index for `Match`): valid only for the
//! exact tree analyzed, like `liveness::Liveness`.

use std::collections::{BTreeMap, BTreeSet};

use move_symbol_pool::Symbol;

use crate::ast::{Exp, Function, Label, MatchArm, UnstructuredNode};

// -------------------------------------------------------------------------------------------------
// Types

pub struct MutAnnotations {
    /// (binder node id, arm index) -> names bound at that binder that need `mut`. Arm index
    /// is 0 for every binder except `Match`, where each arm's pattern is its own binder.
    mut_binders: BTreeMap<(usize, usize), BTreeSet<String>>,
    /// Parameter names (`l{i}`) that need `mut` in the signature.
    params: BTreeSet<String>,
}

/// Saturating assignment count: we only ever need to distinguish 0, 1, and "2 or more".
const MANY: u8 = 2;

/// Future free uses of one name at one program point.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Use {
    /// Free assignments on some path from here, saturated at [`MANY`].
    assigns: u8,
    /// A free `&mut name` on some path from here.
    borrowed: bool,
}

/// The backward fact: every name's future free uses. Absent names have none. `poisoned`
/// models unstructured goto flow: every lookup answers [`Use::SATURATED`].
#[derive(Clone, Default, PartialEq, Eq)]
struct Env {
    uses: BTreeMap<String, Use>,
    poisoned: bool,
}

/// An enclosing loop during the walk: the facts a jump to it resumes at.
struct LoopFrame {
    label: Option<Label>,
    /// The fact after the loop: where `break` resumes.
    break_env: Env,
    /// The fact at the loop head: where `continue` and the back edge resume. The current
    /// fixpoint iterate while the loop's body is being walked.
    head_env: Env,
}

/// A name-binding statement; its `rhs` is evaluated before the bindings take effect. One of
/// `names`/`fields` is always empty.
struct Binder<'a> {
    names: &'a [String],
    fields: &'a [(Symbol, String)],
    initialized: bool,
    rhs: Option<&'a Exp>,
}

struct Context {
    mut_binders: BTreeMap<(usize, usize), BTreeSet<String>>,
    loops: Vec<LoopFrame>,
}

// -------------------------------------------------------------------------------------------------
// Impls

impl MutAnnotations {
    pub fn analyze(fun: &Function) -> Self {
        let mut context = Context {
            mut_binders: BTreeMap::new(),
            loops: Vec::new(),
        };
        let entry = scope_env(&mut context, &fun.code, Env::default());
        // Parameters share `term_reconstruction::local_name`'s `l{i}` scheme and are
        // initialized on entry.
        let params = (0..fun.parameters.len())
            .map(|i| format!("l{i}"))
            .filter(|name| entry.get(name).forces_mut(/* initialized */ true))
            .collect();
        MutAnnotations {
            mut_binders: context.mut_binders,
            params,
        }
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
}

impl Use {
    const SATURATED: Use = Use {
        assigns: MANY,
        borrowed: true,
    };

    /// Alternative paths: the worse of the two.
    fn join(self, other: Use) -> Use {
        Use {
            assigns: self.assigns.max(other.assigns),
            borrowed: self.borrowed || other.borrowed,
        }
    }

    /// Sequential paths: `self`'s uses, then `later`'s.
    fn then(self, later: Use) -> Use {
        Use {
            assigns: (self.assigns + later.assigns).min(MANY),
            borrowed: self.borrowed || later.borrowed,
        }
    }

    fn forces_mut(self, initialized: bool) -> bool {
        let threshold = if initialized { 1 } else { MANY };
        self.borrowed || self.assigns >= threshold
    }
}

impl Env {
    fn poisoned() -> Env {
        Env {
            uses: BTreeMap::new(),
            poisoned: true,
        }
    }

    fn get(&self, name: &str) -> Use {
        if self.poisoned {
            Use::SATURATED
        } else {
            self.uses.get(name).copied().unwrap_or_default()
        }
    }

    /// Set `name`'s raw entry, removing it when default: the loop fixpoint's equality
    /// check needs a canonical map.
    fn set(&mut self, name: &str, u: Use) {
        if u == Use::default() {
            self.uses.remove(name);
        } else {
            self.uses.insert(name.to_string(), u);
        }
    }

    /// Remove `name`'s raw entry.
    fn kill(&mut self, name: &str) {
        self.uses.remove(name);
    }

    /// Remove and return `name`'s raw entry.
    fn take(&mut self, name: &str) -> Use {
        self.uses.remove(name).unwrap_or_default()
    }

    fn add_assign(&mut self, name: &str) {
        let u = self.uses.entry(name.to_string()).or_default();
        u.assigns = (u.assigns + 1).min(MANY);
    }

    fn add_borrow(&mut self, name: &str) {
        self.uses.entry(name.to_string()).or_default().borrowed = true;
    }

    /// Alternative paths: pointwise [`Use::join`].
    fn join(mut self, other: Env) -> Env {
        for (name, u) in other.uses {
            let entry = self.uses.entry(name).or_default();
            *entry = entry.join(u);
        }
        self.poisoned = self.poisoned || other.poisoned;
        self
    }
}

impl<'a> Binder<'a> {
    fn names(&self) -> impl Iterator<Item = &'a String> {
        self.names.iter().chain(self.fields.iter().map(|(_, n)| n))
    }
}

impl Context {
    /// Record which of `names`, bound at `(node, arm_idx)`, the incoming fact forces `mut`.
    /// Under a loop this re-runs per fixpoint pass; earlier passes record a subset of the
    /// last.
    fn record<'a>(
        &mut self,
        node: &Exp,
        arm_idx: usize,
        names: impl Iterator<Item = &'a String>,
        initialized: bool,
        env: &Env,
    ) {
        for name in names {
            if env.get(name).forces_mut(initialized) {
                self.mut_binders
                    .entry((node_id(node), arm_idx))
                    .or_default()
                    .insert(name.clone());
            }
        }
    }

    fn push_loop(&mut self, label: Option<Label>, break_env: Env, head_env: Env) {
        self.loops.push(LoopFrame {
            label,
            break_env,
            head_env,
        });
    }

    fn pop_loop(&mut self) {
        self.loops.pop();
    }

    /// Resolve a `Break`/`Continue` label: `None` targets the innermost frame, `Some(L)`
    /// the nearest frame with that label.
    fn find_loop(&self, label: Option<Label>) -> Option<&LoopFrame> {
        match label {
            None => self.loops.last(),
            Some(l) => self.loops.iter().rev().find(|f| f.label == Some(l)),
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Backward walk

/// Backward walk of a scope's rendered statement list; `after` is the fact at the scope's
/// exit. A rebinding shadows only to the end of the list: on the way out, a rebound name's
/// uses resume with the outer binding's, sequentially.
fn scope_env(context: &mut Context, root: &Exp, after: Env) -> Env {
    let stmts = flatten_scope(root);
    let mut env = after.clone();
    for stmt in stmts.iter().rev() {
        env = stmt_env(context, stmt, env);
    }
    for stmt in &stmts {
        let Some(binder) = binder(stmt) else { continue };
        for name in binder.names() {
            let u = env.get(name).then(after.get(name));
            env.set(name, u);
        }
    }
    env
}

/// One statement backward: a binder reads its `mut` need off the incoming fact, then kills
/// its names so earlier statements see the shadow.
fn stmt_env(context: &mut Context, stmt: &Exp, mut env: Env) -> Env {
    let Some(binder) = binder(stmt) else {
        return exp_env(context, stmt, env);
    };
    context.record(stmt, 0, binder.names(), binder.initialized, &env);
    for name in binder.names() {
        env.kill(name);
    }
    match binder.rhs {
        Some(rhs) => exp_env(context, rhs, env),
        None => env,
    }
}

/// Backward transfer of one expression: `env` is the fact after it.
fn exp_env(context: &mut Context, exp: &Exp, mut env: Env) -> Env {
    use Exp as E;
    match exp {
        E::Variable(_) | E::Value(_) | E::Constant(_) | E::Declare(_) => env,
        // A jump discards the incoming fact for its target's.
        E::Break(label) => match context.find_loop(*label) {
            Some(frame) => frame.break_env.clone(),
            None => orphan_jump_env(),
        },
        E::Continue(label) => match context.find_loop(*label) {
            Some(frame) => frame.head_env.clone(),
            None => orphan_jump_env(),
        },
        E::Return(items) => {
            let mut env = Env::default();
            for item in items.iter().rev() {
                env = exp_env(context, item, env);
            }
            env
        }
        E::Abort(e) => exp_env(context, e, Env::default()),
        // Binding, not assignment: only the right-hand side can mutate an outer name;
        // statement-position binders go through `stmt_env`.
        E::LetBind(_, rhs)
        | E::VecUnpack(_, rhs)
        | E::Unpack(_, _, rhs)
        | E::UnpackVariant(_, _, _, rhs) => exp_env(context, rhs, env),
        E::Assign(targets, rhs) => {
            for target in targets {
                env.add_assign(target);
            }
            exp_env(context, rhs, env)
        }
        E::Borrow(is_mut, inner) => match inner.as_ref() {
            E::Variable(name) if *is_mut => {
                env.add_borrow(name);
                env
            }
            _ => exp_env(context, inner, env),
        },
        E::Seq(_) | E::Block(_, _) => scope_env(context, exp, env),
        E::IfElse(cond, conseq, alt) => {
            let taken = scope_env(context, conseq, env.clone());
            let joined = match alt.as_ref() {
                Some(alt) => taken.join(scope_env(context, alt, env)),
                None => taken.join(env),
            };
            exp_env(context, cond, joined)
        }
        E::Loop(_, _) | E::While(_, _, _) => loop_head_env(context, exp, env),
        E::Switch(subject, _, arms) => {
            let arms_env =
                join_scopes(context, arms.iter().map(|(_, body)| body), &env).unwrap_or(env);
            exp_env(context, subject, arms_env)
        }
        E::MatchLit(subject, arms) => {
            let arms_env =
                join_scopes(context, arms.iter().map(|(_, body)| body), &env).unwrap_or(env);
            exp_env(context, subject, arms_env)
        }
        E::Match(subject, _, arms) => {
            let mut joined: Option<Env> = None;
            for (arm_idx, arm) in arms.iter().enumerate() {
                let arm_env = match_arm_env(context, exp, arm_idx, arm, &env);
                joined = Some(match joined {
                    Some(j) => j.join(arm_env),
                    None => arm_env,
                });
            }
            exp_env(context, subject, joined.unwrap_or(env))
        }
        E::Call(_, items) | E::Primitive { args: items, .. } | E::Data { args: items, .. } => {
            let mut env = env;
            for item in items.iter().rev() {
                env = exp_env(context, item, env);
            }
            env
        }
        // Unmodeled goto flow: poison the fact; the bodies still walk so binders inside
        // them get records.
        E::Unstructured(nodes) => {
            let mut env = Env::poisoned();
            for node in nodes.iter().rev() {
                match node {
                    UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) => {
                        env = scope_env(context, body, env);
                    }
                    UnstructuredNode::Goto(_) => {}
                }
            }
            env
        }
    }
}

/// One `Match` arm backward. Pattern names shadow the guard and whole body; the guard sees
/// its bindings by immutable reference, so guard uses never force `mut`.
fn match_arm_env(
    context: &mut Context,
    match_exp: &Exp,
    arm_idx: usize,
    arm: &MatchArm,
    after: &Env,
) -> Env {
    let mut env = after.clone();
    let saved: Vec<Use> = arm.fields.iter().map(|(_, n)| env.take(n)).collect();
    let mut env = scope_env(context, &arm.rhs, env);
    context.record(
        match_exp,
        arm_idx,
        arm.fields.iter().map(|(_, n)| n),
        /* initialized */ true,
        &env,
    );
    for (_, name) in &arm.fields {
        env.kill(name);
    }
    if let Some(guard) = &arm.guard {
        env = exp_env(context, guard, env);
        for (_, name) in &arm.fields {
            env.kill(name);
        }
    }
    for ((_, name), u) in arm.fields.iter().zip(saved) {
        env.set(name, u);
    }
    env
}

/// The fact at a loop's head, as the fixpoint over its back edge. Transfers are per-name
/// independent and monotone on a chain of height four (assign count 0/1/2, then the borrow
/// bit), so iteration terminates.
fn loop_head_env(context: &mut Context, exp: &Exp, break_env: Env) -> Env {
    let (label, cond, body) = match exp {
        Exp::Loop(label, body) => (*label, None, body),
        Exp::While(label, cond, body) => (*label, Some(cond), body),
        _ => unreachable!("loop_head_env is only called on loops"),
    };
    let mut head = Env::default();
    loop {
        context.push_loop(label, break_env.clone(), head.clone());
        // The body falls through to the back edge; a `while` re-runs its condition every
        // iteration.
        let mut next = scope_env(context, body, head.clone());
        if let Some(cond) = cond {
            next = exp_env(context, cond, next.join(break_env.clone()));
        }
        context.pop_loop();
        if next == head {
            return head;
        }
        head = next;
    }
}

/// Join the walks of alternative scope bodies (`Switch`/`MatchLit` arms). `None` when there
/// are no bodies.
fn join_scopes<'a>(
    context: &mut Context,
    bodies: impl Iterator<Item = &'a Exp>,
    after: &Env,
) -> Option<Env> {
    let mut joined: Option<Env> = None;
    for body in bodies {
        let env = scope_env(context, body, after.clone());
        joined = Some(match joined {
            Some(j) => j.join(env),
            None => env,
        });
    }
    joined
}

/// A jump with no enclosing frame; structuring should never emit this.
fn orphan_jump_env() -> Env {
    debug_assert!(false, "jump outside its loop");
    Env::poisoned()
}

// -------------------------------------------------------------------------------------------------
// Utils

fn node_id(exp: &Exp) -> usize {
    exp as *const Exp as usize
}

/// The rendered statement list (mirrors the printer's `push_stmt`/`e_block` Block inlining):
/// `Block` wrappers peel, `Block(Seq)` items splice in as siblings. Bare `Seq`s stay single
/// statements; they render braced, opening a scope.
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

fn binder(stmt: &Exp) -> Option<Binder<'_>> {
    match stmt {
        Exp::LetBind(names, rhs) | Exp::VecUnpack(names, rhs) => Some(Binder {
            names,
            fields: &[],
            initialized: true,
            rhs: Some(rhs),
        }),
        Exp::Declare(names) => Some(Binder {
            names,
            fields: &[],
            initialized: false,
            rhs: None,
        }),
        Exp::Unpack(_, fields, rhs) | Exp::UnpackVariant(_, _, fields, rhs) => Some(Binder {
            names: &[],
            fields,
            initialized: true,
            rhs: Some(rhs),
        }),
        _ => None,
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
    fn labeled_break_exits_both_loops() {
        // let x; 'outer: loop { loop { x = 1; break 'outer } }: the assignment exits both
        // loops, so it never repeats.
        let inner = seq(vec![assign("x", num()), Exp::Break(Some(0))]);
        let outer = Exp::Loop(Some(0), Box::new(Exp::Loop(None, Box::new(inner))));
        assert!(!binder_mut(vec![declare("x"), outer], 0));
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
