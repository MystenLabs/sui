// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Shared local-name analyses used by refinements.
//!
//! Three layers, in order of decreasing detail:
//!
//!   * [`NameCounts`]: function-wide occurrence tallies — for each local, how many times it is
//!     read, assigned, let-bound, declared, or pattern-bound. Cheap; single linear walk.
//!
//!   * [`Liveness`]: per-point live-out sets computed by classical backward dataflow over the
//!     structured AST, with a fixpoint at every loop. Bundles the [`NameCounts`] for free, and
//!     provides the higher-level [`Liveness::singly_used`] query consumers actually need.
//!     Keys are pointer-identity: a `Liveness` is valid for the exact subtree it was
//!     constructed from, and only until that subtree is mutated. Loops use unidirectional
//!     growth (live sets only expand iteration over iteration), so iteration always
//!     terminates.
//!
//!   * Standalone helpers — [`referenced_names`] (every local touched in a subtree, reads and
//!     writes unified) and [`collect_local_names`] (the flat set of locals across an entire
//!     module, returned as `Symbol`s for the alias picker).

use std::collections::{BTreeMap, BTreeSet};

use move_symbol_pool::Symbol;

use crate::ast::{Exp, Label, Module, UnstructuredNode};

// -------------------------------------------------------------------------------------------------
// NameCounts

#[derive(Default, Debug, Clone)]
pub struct NameCounts {
    reads: BTreeMap<String, usize>,
    assigns: BTreeMap<String, usize>,
    letbinds: BTreeMap<String, usize>,
    declares: BTreeMap<String, usize>,
    unpacks: BTreeMap<String, usize>,
}

impl NameCounts {
    pub fn analyze(exp: &Exp) -> Self {
        let mut nc = NameCounts::default();
        nc.visit(exp);
        nc
    }

    pub fn reads(&self, n: &str) -> usize {
        *self.reads.get(n).unwrap_or(&0)
    }
    pub fn assigns(&self, n: &str) -> usize {
        *self.assigns.get(n).unwrap_or(&0)
    }
    pub fn letbinds(&self, n: &str) -> usize {
        *self.letbinds.get(n).unwrap_or(&0)
    }
    pub fn declares(&self, n: &str) -> usize {
        *self.declares.get(n).unwrap_or(&0)
    }
    pub fn unpacks(&self, n: &str) -> usize {
        *self.unpacks.get(n).unwrap_or(&0)
    }

    fn bump(map: &mut BTreeMap<String, usize>, n: &str) {
        *map.entry(n.to_string()).or_insert(0) += 1;
    }

    fn visit(&mut self, exp: &Exp) {
        use Exp as E;
        match exp {
            E::Variable(n) => Self::bump(&mut self.reads, n),
            E::Assign(targets, rhs) => {
                for t in targets {
                    Self::bump(&mut self.assigns, t);
                }
                self.visit(rhs);
            }
            E::LetBind(targets, rhs) => {
                for t in targets {
                    Self::bump(&mut self.letbinds, t);
                }
                self.visit(rhs);
            }
            E::Declare(names) => {
                for n in names {
                    Self::bump(&mut self.declares, n);
                }
            }
            E::VecUnpack(names, e) => {
                for n in names {
                    Self::bump(&mut self.unpacks, n);
                }
                self.visit(e);
            }
            E::Unpack(_, fields, e) | E::UnpackVariant(_, _, fields, e) => {
                for (_, n) in fields {
                    Self::bump(&mut self.unpacks, n);
                }
                self.visit(e);
            }
            E::Match(subject, _, arms) => {
                self.visit(subject);
                for (_, fields, body) in arms {
                    for (_, n) in fields {
                        Self::bump(&mut self.unpacks, n);
                    }
                    self.visit(body);
                }
            }
            E::Switch(subject, _, arms) => {
                self.visit(subject);
                for (_, body) in arms {
                    self.visit(body);
                }
            }
            E::IfElse(c, t, alt) => {
                self.visit(c);
                self.visit(t);
                if let Some(a) = alt.as_ref().as_ref() {
                    self.visit(a);
                }
            }
            E::Seq(items) | E::Return(items) | E::Call(_, items) => {
                for i in items {
                    self.visit(i);
                }
            }
            E::Loop(_, b) => self.visit(b),
            E::While(_, c, b) => {
                self.visit(c);
                self.visit(b);
            }
            E::Abort(e) | E::Borrow(_, e) | E::Block(_, e) => self.visit(e),
            E::Primitive { args, .. } | E::Data { args, .. } => {
                for a in args {
                    self.visit(a);
                }
            }
            E::Break(_) | E::Continue(_) | E::Value(_) | E::Constant(_) => {}
            E::Unstructured(nodes) => {
                for n in nodes {
                    match n {
                        UnstructuredNode::Labeled(_, b) | UnstructuredNode::Statement(b) => {
                            self.visit(b);
                        }
                        UnstructuredNode::Goto(_) => {}
                    }
                }
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Liveness

/// Per-point live-out sets for every `Exp` node in a single subtree, plus the [`NameCounts`]
/// for that subtree. Computed by classical backward dataflow with a fixpoint at each loop.
/// Live-in is derivable from `live_out` plus the node-local gen/kill at any node that needs
/// it — only live-out is stored here because the refinement consumer doesn't need live-in.
///
/// Keys are pointer-identity into the AST passed to [`Liveness::analyze`]; entries remain
/// valid as long as that AST is not mutated.
pub struct Liveness {
    counts: NameCounts,
    /// Pointer-identity (`&Exp as *const Exp as usize`) -> live-out set (live just after
    /// `exp` evaluates, before any subsequent statement runs).
    live_out: BTreeMap<usize, BTreeSet<String>>,
    /// Set to `true` whenever the walker encountered an `Unstructured` node. Live sets that
    /// flow through such regions are conservative approximations; callers that need a sound
    /// answer should bail when this is true.
    has_unstructured: bool,
}

impl Liveness {
    pub fn analyze(root: &Exp) -> Self {
        let counts = NameCounts::analyze(root);
        let mut builder = Builder {
            live_out: BTreeMap::new(),
            loops: Vec::new(),
            has_unstructured: false,
        };
        builder.compute(root, &BTreeSet::new());
        Self {
            counts,
            live_out: builder.live_out,
            has_unstructured: builder.has_unstructured,
        }
    }

    pub fn counts(&self) -> &NameCounts {
        &self.counts
    }

    pub fn has_unstructured(&self) -> bool {
        self.has_unstructured
    }

    /// Live-out at `exp`, i.e. the set of locals that may be read along some path that
    /// resumes immediately after `exp` finishes evaluating. Returns `None` if `exp` is not
    /// a node from the analyzed subtree.
    pub fn live_out(&self, exp: &Exp) -> Option<&BTreeSet<String>> {
        self.live_out.get(&node_id(exp))
    }

    /// `name` is dead immediately after `exp` evaluates. Returns `false` for nodes outside
    /// the analyzed subtree (i.e., absent information is treated as live, matching the
    /// conservative direction).
    pub fn is_dead_after(&self, exp: &Exp, name: &str) -> bool {
        self.live_out(exp).is_some_and(|s| !s.contains(name))
    }

    /// True when every syntactic read of `name` in `root` is the *only* read on every
    /// execution path it participates in — equivalently, when `name` is dead immediately
    /// after every one of its uses. Walks `root` looking for `Variable(name)`; returns
    /// `false` if `root` contains an `Unstructured` region (live sets are unsound there).
    pub fn singly_used(&self, root: &Exp, name: &str) -> bool {
        if self.has_unstructured() {
            return false;
        }
        !any_use_kept_live(root, name, self)
    }
}

fn any_use_kept_live(exp: &Exp, name: &str, l: &Liveness) -> bool {
    use Exp as E;
    match exp {
        E::Variable(n) if n == name => !l.is_dead_after(exp, name),
        E::Variable(_)
        | E::Value(_)
        | E::Constant(_)
        | E::Break(_)
        | E::Continue(_)
        | E::Declare(_) => false,
        E::LetBind(_, e)
        | E::Assign(_, e)
        | E::Abort(e)
        | E::Borrow(_, e)
        | E::VecUnpack(_, e)
        | E::Unpack(_, _, e)
        | E::UnpackVariant(_, _, _, e)
        | E::Block(_, e) => any_use_kept_live(e, name, l),
        E::Loop(_, b) => any_use_kept_live(b, name, l),
        E::While(_, c, b) => any_use_kept_live(c, name, l) || any_use_kept_live(b, name, l),
        E::IfElse(c, t, alt) => {
            any_use_kept_live(c, name, l)
                || any_use_kept_live(t, name, l)
                || alt
                    .as_ref()
                    .as_ref()
                    .is_some_and(|a| any_use_kept_live(a, name, l))
        }
        E::Seq(items) | E::Return(items) | E::Call(_, items) => {
            items.iter().any(|i| any_use_kept_live(i, name, l))
        }
        E::Switch(s, _, arms) => {
            any_use_kept_live(s, name, l) || arms.iter().any(|(_, b)| any_use_kept_live(b, name, l))
        }
        E::Match(s, _, arms) => {
            any_use_kept_live(s, name, l)
                || arms.iter().any(|(_, _, b)| any_use_kept_live(b, name, l))
        }
        E::Primitive { args, .. } | E::Data { args, .. } => {
            args.iter().any(|a| any_use_kept_live(a, name, l))
        }
        E::Unstructured(_) => true,
    }
}

fn node_id(exp: &Exp) -> usize {
    exp as *const Exp as usize
}

struct Builder {
    live_out: BTreeMap<usize, BTreeSet<String>>,
    loops: Vec<LoopFrame>,
    has_unstructured: bool,
}

/// One entry per active enclosing loop. `break_live` is fixed for the life of the frame (it's
/// the live-out of the loop expression itself); `continue_live` grows monotonically as the
/// fixpoint iterates and is what `Continue` resolves to.
struct LoopFrame {
    label: Option<Label>,
    break_live: BTreeSet<String>,
    continue_live: BTreeSet<String>,
}

impl Builder {
    fn compute(&mut self, exp: &Exp, live_out: &BTreeSet<String>) -> BTreeSet<String> {
        use Exp as E;
        let live_in = match exp {
            E::Variable(n) => {
                let mut s = live_out.clone();
                s.insert(n.clone());
                s
            }

            // Declare introduces a new lexical slot; the declared name is dead just before this
            // point. Removing it from `live_out` matches that intuition (and matters when the
            // same name appears in sibling arms).
            E::Declare(names) => subtract_strs(live_out, names),

            E::Value(_) | E::Constant(_) => live_out.clone(),

            E::Break(label) => self
                .find_loop(*label)
                .map(|i| self.loops[i].break_live.clone())
                .unwrap_or_default(),

            E::Continue(label) => self
                .find_loop(*label)
                .map(|i| self.loops[i].continue_live.clone())
                .unwrap_or_default(),

            E::Return(items) => self.seq_backward(items, &BTreeSet::new()),

            E::Abort(e) => self.compute(e, &BTreeSet::new()),

            E::Assign(targets, rhs) | E::LetBind(targets, rhs) => {
                let after_kill = subtract_strs(live_out, targets);
                self.compute(rhs, &after_kill)
            }
            E::VecUnpack(names, e) => {
                let after_kill = subtract_strs(live_out, names);
                self.compute(e, &after_kill)
            }
            E::Unpack(_, fields, e) | E::UnpackVariant(_, _, fields, e) => {
                let after_kill = subtract_fields(live_out, fields);
                self.compute(e, &after_kill)
            }

            E::Seq(items) | E::Call(_, items) => self.seq_backward(items, live_out),
            E::Primitive { args, .. } | E::Data { args, .. } => self.seq_backward(args, live_out),

            E::Borrow(_, e) | E::Block(_, e) => self.compute(e, live_out),

            E::IfElse(c, t, alt) => {
                let lt = self.compute(t, live_out);
                let la = match alt.as_ref().as_ref() {
                    Some(a) => self.compute(a, live_out),
                    None => live_out.clone(),
                };
                let merged: BTreeSet<String> = lt.union(&la).cloned().collect();
                self.compute(c, &merged)
            }
            E::Switch(subject, _, arms) => {
                let mut merged = BTreeSet::new();
                for (_, body) in arms {
                    let lb = self.compute(body, live_out);
                    merged.extend(lb);
                }
                self.compute(subject, &merged)
            }
            E::Match(subject, _, arms) => {
                let mut merged = BTreeSet::new();
                for (_, fields, body) in arms {
                    let lb = self.compute(body, live_out);
                    let after = subtract_fields(&lb, fields);
                    merged.extend(after);
                }
                self.compute(subject, &merged)
            }

            E::Loop(label, body) => self.fixpoint_loop(*label, live_out, None, body),
            E::While(label, cond, body) => self.fixpoint_loop(*label, live_out, Some(cond), body),

            // Unstructured: control flow is goto-driven and we don't model it. Record this so
            // callers can choose to bail; pass live_out through (it carries no real meaning
            // here, but keeps the recursion shape uniform).
            E::Unstructured(_) => {
                self.has_unstructured = true;
                live_out.clone()
            }
        };
        self.live_out.insert(node_id(exp), live_out.clone());
        live_in
    }

    /// Walk `items` right-to-left from `live_out` (each item's live-out is the next item's
    /// live-in). Models statement-sequence evaluation order for `Seq`, function-argument
    /// evaluation order for `Call`/`Primitive`/`Data`, and tuple-element evaluation order for
    /// `Return`.
    fn seq_backward(&mut self, items: &[Exp], live_out: &BTreeSet<String>) -> BTreeSet<String> {
        let mut current = live_out.clone();
        for it in items.iter().rev() {
            current = self.compute(it, &current);
        }
        current
    }

    /// Iterate the loop body (and condition, for `While`) until the loop's live-in set
    /// stabilizes. Monotone — live sets only grow — so termination is guaranteed. On the final
    /// iteration the converged frame's `continue_live` is the back-edge live-out used to
    /// compute body's table entries, so per-node entries are sound at fixpoint.
    fn fixpoint_loop(
        &mut self,
        label: Option<Label>,
        live_out_outside: &BTreeSet<String>,
        cond: Option<&Exp>,
        body: &Exp,
    ) -> BTreeSet<String> {
        self.loops.push(LoopFrame {
            label,
            break_live: live_out_outside.clone(),
            continue_live: BTreeSet::new(),
        });

        let mut live_in_loop = BTreeSet::new();
        loop {
            let prev = live_in_loop.clone();
            // Body's live-out = current continue target (set at end of previous iteration,
            // empty on the first iteration).
            let body_live_out = self.loops.last().unwrap().continue_live.clone();
            let body_live_in = self.compute(body, &body_live_out);

            let new_live_in_loop = match cond {
                Some(c) => {
                    // The condition's "true" branch falls through to body; its "false" branch
                    // exits to `live_out_outside`. Backward: live_out(cond) = body_live_in ∪
                    // live_out_outside.
                    let after: BTreeSet<String> =
                        body_live_in.union(live_out_outside).cloned().collect();
                    self.compute(c, &after)
                }
                None => body_live_in.clone(),
            };

            // `continue` jumps to the start of the next iteration: cond for While, body for
            // Loop. Either way, the target's live-in == the loop's own live-in.
            self.loops.last_mut().unwrap().continue_live = new_live_in_loop.clone();

            live_in_loop = new_live_in_loop;
            if live_in_loop == prev {
                break;
            }
        }

        self.loops.pop();
        live_in_loop
    }

    /// Resolve a `Break`/`Continue` label to a frame index in `self.loops`. `None` targets
    /// the innermost frame; `Some(L)` targets the nearest frame with that label.
    fn find_loop(&self, target: Option<Label>) -> Option<usize> {
        match target {
            None => self.loops.len().checked_sub(1),
            Some(l) => self.loops.iter().rposition(|f| f.label == Some(l)),
        }
    }
}

fn subtract_strs(set: &BTreeSet<String>, names: &[String]) -> BTreeSet<String> {
    let drop: BTreeSet<&str> = names.iter().map(String::as_str).collect();
    set.iter()
        .filter(|n| !drop.contains(n.as_str()))
        .cloned()
        .collect()
}

fn subtract_fields(set: &BTreeSet<String>, fields: &[(Symbol, String)]) -> BTreeSet<String> {
    let drop: BTreeSet<&str> = fields.iter().map(|(_, n)| n.as_str()).collect();
    set.iter()
        .filter(|n| !drop.contains(n.as_str()))
        .cloned()
        .collect()
}

// -------------------------------------------------------------------------------------------------
// referenced_names — subtree query

/// Every local name mentioned anywhere in `exp` — reads (`Variable`), writes
/// (`Assign`/`LetBind`/`VecUnpack`/`Unpack` targets), and declarations (`Declare`,
/// `LetBind`) — unified into one set.
///
/// Intentionally unifies reads and writes so callers that want "did this subtree touch X in
/// any way" can ask once. Use this when you need an over-approximation of the locals an
/// expression can read or modify, e.g. to decide whether moving a declaration across it would
/// change behavior. Sub-expression values of `Unpack` etc. are recursed into, but the
/// unpacked struct/enum/variant identifiers are not included (those are types, not locals).
pub fn referenced_names(exp: &Exp) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    collect_referenced_names(exp, &mut out);
    out
}

fn collect_referenced_names(exp: &Exp, out: &mut BTreeSet<String>) {
    match exp {
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
            collect_referenced_names(value, out);
        }
        Exp::VecUnpack(names, value) => {
            for n in names {
                out.insert(n.clone());
            }
            collect_referenced_names(value, out);
        }
        Exp::Unpack(_, fields, value) | Exp::UnpackVariant(_, _, fields, value) => {
            for (_, name) in fields {
                out.insert(name.clone());
            }
            collect_referenced_names(value, out);
        }
        Exp::Seq(items) | Exp::Return(items) | Exp::Call(_, items) => {
            for it in items {
                collect_referenced_names(it, out);
            }
        }
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            for a in args {
                collect_referenced_names(a, out);
            }
        }
        Exp::IfElse(cond, conseq, alt) => {
            collect_referenced_names(cond, out);
            collect_referenced_names(conseq, out);
            if let Some(a) = alt.as_ref() {
                collect_referenced_names(a, out);
            }
        }
        Exp::Switch(cond, _, cases) => {
            collect_referenced_names(cond, out);
            for (_, body) in cases {
                collect_referenced_names(body, out);
            }
        }
        Exp::Match(cond, _, cases) => {
            collect_referenced_names(cond, out);
            for (_, _, body) in cases {
                collect_referenced_names(body, out);
            }
        }
        Exp::Loop(_, body) => collect_referenced_names(body, out),
        Exp::While(_, cond, body) => {
            collect_referenced_names(cond, out);
            collect_referenced_names(body, out);
        }
        Exp::Abort(value) | Exp::Borrow(_, value) | Exp::Block(_, value) => {
            collect_referenced_names(value, out)
        }
        Exp::Unstructured(nodes) => {
            for node in nodes {
                match node {
                    UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) => {
                        collect_referenced_names(body, out);
                    }
                    UnstructuredNode::Goto(_) => {}
                }
            }
        }
        Exp::Value(_) | Exp::Constant(_) | Exp::Break(_) | Exp::Continue(_) => {}
    }
}

// -------------------------------------------------------------------------------------------------
// collect_local_names — module-wide flat set

/// Every local name appearing anywhere in `module`'s function bodies, as a flat set of
/// `Symbol`s. Used as the "taboo" input to module-alias selection (`collect_uses`): an alias
/// that collides with a local would shadow it, so the picker treats every local as already
/// taken.
///
/// Differs from [`referenced_names`] in two ways: it walks an entire `Module` (not a single
/// `Exp`), and it returns `Symbol` (not `String`) because the consumer is `move-symbol-pool`-
/// based. It also picks up pattern-field bindings introduced by `Match` arms, which
/// `referenced_names` does not — that asymmetry is preserved to avoid churning the alias
/// picker's output.
pub fn collect_local_names(module: &Module) -> BTreeSet<Symbol> {
    let mut out = BTreeSet::new();
    for fun in module.functions.values() {
        collect_local_names_exp(&fun.code, &mut out);
    }
    out
}

fn collect_local_names_exp(exp: &Exp, out: &mut BTreeSet<Symbol>) {
    match exp {
        Exp::LetBind(names, e) | Exp::Assign(names, e) => {
            for n in names {
                out.insert(Symbol::from(n.as_str()));
            }
            collect_local_names_exp(e, out);
        }
        Exp::Declare(names) => {
            for n in names {
                out.insert(Symbol::from(n.as_str()));
            }
        }
        Exp::Variable(n) => {
            out.insert(Symbol::from(n.as_str()));
        }
        Exp::Unpack(_, fields, e) | Exp::UnpackVariant(_, _, fields, e) => {
            for (_, n) in fields {
                out.insert(Symbol::from(n.as_str()));
            }
            collect_local_names_exp(e, out);
        }
        Exp::VecUnpack(names, e) => {
            for n in names {
                out.insert(Symbol::from(n.as_str()));
            }
            collect_local_names_exp(e, out);
        }
        Exp::Seq(items) | Exp::Return(items) | Exp::Call(_, items) => {
            for i in items {
                collect_local_names_exp(i, out);
            }
        }
        Exp::IfElse(c, t, alt) => {
            collect_local_names_exp(c, out);
            collect_local_names_exp(t, out);
            if let Some(a) = alt.as_ref().as_ref() {
                collect_local_names_exp(a, out);
            }
        }
        Exp::Switch(c, _, arms) => {
            collect_local_names_exp(c, out);
            for (_, body) in arms {
                collect_local_names_exp(body, out);
            }
        }
        Exp::Match(c, _, arms) => {
            collect_local_names_exp(c, out);
            for (_, fields, body) in arms {
                for (_, n) in fields {
                    out.insert(Symbol::from(n.as_str()));
                }
                collect_local_names_exp(body, out);
            }
        }
        Exp::Loop(_, b) => collect_local_names_exp(b, out),
        Exp::While(_, c, b) => {
            collect_local_names_exp(c, out);
            collect_local_names_exp(b, out);
        }
        Exp::Abort(e) | Exp::Borrow(_, e) | Exp::Block(_, e) => collect_local_names_exp(e, out),
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            for a in args {
                collect_local_names_exp(a, out);
            }
        }
        Exp::Break(_) | Exp::Continue(_) | Exp::Value(_) | Exp::Constant(_) => {}
        Exp::Unstructured(nodes) => {
            for node in nodes {
                match node {
                    UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) => {
                        collect_local_names_exp(body, out);
                    }
                    UnstructuredNode::Goto(_) => {}
                }
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use move_binary_format::normalized::Constant;

    fn var(n: &str) -> Exp {
        Exp::Variable(n.to_string())
    }
    fn letb(name: &str, rhs: Exp) -> Exp {
        Exp::LetBind(vec![name.to_string()], Box::new(rhs))
    }
    fn assign(name: &str, rhs: Exp) -> Exp {
        Exp::Assign(vec![name.to_string()], Box::new(rhs))
    }
    fn unit() -> Exp {
        // A leaf with no behavior we care about for liveness.
        Exp::Constant(std::rc::Rc::new(Constant {
            type_: move_binary_format::normalized::Type::U64,
            data: vec![],
        }))
    }
    fn seq(items: Vec<Exp>) -> Exp {
        Exp::Seq(items)
    }
    fn ifelse(c: Exp, t: Exp, e: Option<Exp>) -> Exp {
        Exp::IfElse(Box::new(c), Box::new(t), Box::new(e))
    }

    #[test]
    fn live_out_through_let() {
        // let x = <unit>; use(x);
        let exp = seq(vec![letb("x", unit()), var("x")]);
        let l = Liveness::analyze(&exp);

        // counts
        assert_eq!(l.counts().letbinds("x"), 1);
        assert_eq!(l.counts().reads("x"), 1);

        // live_out at the leaf use of x — empty, since nothing follows.
        let use_node = match &exp {
            Exp::Seq(items) => &items[1],
            _ => unreachable!(),
        };
        assert!(l.is_dead_after(use_node, "x"));
        assert!(l.live_out(use_node).unwrap().is_empty());

        assert!(!l.has_unstructured());
        assert!(l.singly_used(&exp, "x"));
    }

    #[test]
    fn two_uses_on_one_path_keep_first_live() {
        // use(x); use(x); — second read keeps x live after first.
        let exp = seq(vec![var("x"), var("x")]);
        let l = Liveness::analyze(&exp);

        let first = match &exp {
            Exp::Seq(items) => &items[0],
            _ => unreachable!(),
        };
        assert!(!l.is_dead_after(first, "x"));
        assert!(!l.singly_used(&exp, "x"));
    }

    #[test]
    fn branched_uses_are_singly_used() {
        // if (c) { use(x) } else { use(x) }  — only one runs per path.
        let exp = ifelse(unit(), var("x"), Some(var("x")));
        let l = Liveness::analyze(&exp);
        assert!(l.singly_used(&exp, "x"));
    }

    #[test]
    fn loop_body_reuse_is_not_singly_used() {
        // loop { use(x) }  — back edge keeps x live across iterations.
        let exp = Exp::Loop(None, Box::new(var("x")));
        let l = Liveness::analyze(&exp);
        // x is still live just after the body (the back edge keeps it live).
        let body = match &exp {
            Exp::Loop(_, b) => b.as_ref(),
            _ => unreachable!(),
        };
        assert!(!l.is_dead_after(body, "x"));
        assert!(!l.singly_used(&exp, "x"));
    }

    #[test]
    fn assignment_leaves_x_live_outward_to_next_read() {
        // x = <unit>; use(x);  — after the assign, x is live (the use will read it).
        let exp = seq(vec![assign("x", unit()), var("x")]);
        let l = Liveness::analyze(&exp);
        let assign_node = match &exp {
            Exp::Seq(items) => &items[0],
            _ => unreachable!(),
        };
        assert!(!l.is_dead_after(assign_node, "x"));
    }

    #[test]
    fn unstructured_flag_disables_singly_used() {
        let exp = seq(vec![Exp::Unstructured(vec![]), var("x")]);
        let l = Liveness::analyze(&exp);
        assert!(l.has_unstructured());
        // Even with one read, the unstructured region poisons the result.
        assert!(!l.singly_used(&exp, "x"));
    }

    #[test]
    fn referenced_names_unifies_reads_and_writes() {
        let exp = seq(vec![assign("x", var("y")), var("z")]);
        let names = referenced_names(&exp);
        assert!(names.contains("x"));
        assert!(names.contains("y"));
        assert!(names.contains("z"));
    }
}
