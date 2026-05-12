// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// TODO: Should this live with the refinements instead?

// -------------------------------------------------------------------------------------------------
// Compositional `let X;` hoisting
// -------------------------------------------------------------------------------------------------
//
// Per-block term reconstruction emits `LetBind` for each local's first StoreLoc, but blocks have
// no view of the surrounding scope. An arm-scope `let X` doesn't outlive the arm, so when those
// per-block LetBinds end up inside an IfElse/Switch arm or a Loop/While body, anything reading
// `X` from outside the arm either fails to resolve or shadows a still-live outer binding.
//
// This pass walks the Exp bottom-up. At each Seq it goes item-by-item with an in-scope set
// (inherited from the enclosing scope, plus earlier items in this Seq). Items that are
// IfElse/Switch/Loop/While get their arm/body top-level intros classified: if the name is
// already in scope, appears in a sibling arm, or is referenced later in this Seq, it's demoted
// to `Assign` and (unless already in scope) a fresh `Declare([X])` is prepended before the item.
// Hoists surface as new top-level intros of their containing Seq, so an inner hoist becomes
// visible to the next outer Seq and can bubble further up the same way.

use crate::ast::{Exp, UnstructuredNode};

use std::collections::HashSet;

/// Lift `let X` introductions out of inner arms/bodies to their proper enclosing scope.
/// See module-level comment for the contract this pass enforces.
pub fn hoist_declarations(exp: &mut Exp) {
    let mut scope: HashSet<String> = HashSet::new();
    hoist(exp, &mut scope);
}

fn hoist(exp: &mut Exp, scope: &mut HashSet<String>) {
    recur_children(exp, scope);
    if let Exp::Seq(items) = exp {
        stitch_seq(items, scope);
    }
}

/// Recur into `exp`'s children. Control-flow boundaries (Loop, While, IfElse, Switch arms)
/// each get a cloned scope: outer bindings are visible inside, but inner intros don't escape
/// except via the explicit Declares that `stitch_seq` may insert.
fn recur_children(exp: &mut Exp, scope: &mut HashSet<String>) {
    match exp {
        Exp::Seq(items) => {
            for item in items.iter_mut() {
                hoist(item, scope);
                for n in top_level_arm_intros(item) {
                    scope.insert(n);
                }
            }
        }
        Exp::Loop(_, body) => {
            let mut inner = scope.clone();
            hoist(body, &mut inner);
        }
        Exp::While(_, cond, body) => {
            hoist(cond, scope);
            let mut inner = scope.clone();
            hoist(body, &mut inner);
        }
        Exp::IfElse(cond, conseq, alt) => {
            hoist(cond, scope);
            let mut inner = scope.clone();
            hoist(conseq, &mut inner);
            if let Some(a) = alt.as_mut() {
                let mut inner = scope.clone();
                hoist(a, &mut inner);
            }
        }
        Exp::Switch(cond, _, cases) => {
            hoist(cond, scope);
            for (_, arm) in cases.iter_mut() {
                let mut inner = scope.clone();
                hoist(arm, &mut inner);
            }
        }
        Exp::Assign(_, value)
        | Exp::LetBind(_, value)
        | Exp::Abort(value)
        | Exp::Borrow(_, value)
        | Exp::Unpack(_, _, value)
        | Exp::UnpackVariant(_, _, _, value)
        | Exp::VecUnpack(_, value) => {
            hoist(value, scope);
        }
        Exp::Return(items) | Exp::Call(_, items) => {
            for item in items.iter_mut() {
                hoist(item, scope);
            }
        }
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            for arg in args.iter_mut() {
                hoist(arg, scope);
            }
        }
        Exp::Break(_)
        | Exp::Continue(_)
        | Exp::Value(_)
        | Exp::Variable(_)
        | Exp::Constant(_)
        | Exp::Declare(_)
        | Exp::Unstructured(_) => {}
    }
}

/// One forward pass over a Seq. For each item, ask `hoist_arm_intros` which names need to lift
/// out to this Seq's scope; prepend the new Declares, then advance `scope` with whatever the
/// (possibly demoted) item now introduces at top level.
fn stitch_seq(items: &mut Vec<Exp>, scope: &mut HashSet<String>) {
    if items.is_empty() {
        return;
    }

    // `item_refs[i]` is every name referenced anywhere inside `items[i]`; suffix-union gives
    // "referenced anywhere after position i" in O(1) per lookup once it's built.
    let item_refs: Vec<HashSet<String>> = items.iter().map(collect_references).collect();

    let mut out: Vec<Exp> = Vec::with_capacity(items.len());
    for (i, item) in std::mem::take(items).into_iter().enumerate() {
        let later_refs: HashSet<String> = item_refs[i + 1..]
            .iter()
            .flat_map(|s| s.iter().cloned())
            .collect();

        let (needs_declare, rebuilt) = hoist_arm_intros(item, scope, &later_refs);
        if !needs_declare.is_empty() {
            let mut names: Vec<String> = needs_declare.into_iter().collect();
            names.sort();
            for n in &names {
                scope.insert(n.clone());
            }
            out.push(Exp::Declare(names));
        }
        for n in top_level_arm_intros(&rebuilt) {
            scope.insert(n);
        }
        out.push(rebuilt);
    }
    *items = out;
}

/// Inspect one Seq item. If it's a multi-arm construct (IfElse/Switch) or a single-body one
/// (Loop/While), return the names that need a fresh outer `Declare` and the item with its
/// arm-level LetBinds demoted to Assigns. Other items pass through unchanged.
fn hoist_arm_intros(
    item: Exp,
    earlier_scope: &HashSet<String>,
    later_refs: &HashSet<String>,
) -> (HashSet<String>, Exp) {
    match item {
        Exp::IfElse(cond, conseq, alt) => {
            let alt_opt: Option<Exp> = *alt;
            let arm_refs: Vec<HashSet<String>> = std::iter::once(&*conseq)
                .chain(alt_opt.iter())
                .map(collect_references)
                .collect();
            let arm_intros: Vec<HashSet<String>> = std::iter::once(&*conseq)
                .chain(alt_opt.iter())
                .map(top_level_arm_intros)
                .collect();

            let to_demote = decide_demotions(&arm_intros, &arm_refs, earlier_scope, later_refs);
            let needs_declare: HashSet<String> =
                to_demote.difference(earlier_scope).cloned().collect();

            let conseq = Box::new(demote_top_level_intros(*conseq, &to_demote));
            let alt = Box::new(alt_opt.map(|a| demote_top_level_intros(a, &to_demote)));
            (needs_declare, Exp::IfElse(cond, conseq, alt))
        }
        Exp::Switch(cond, enum_, arms) => {
            let arm_exps: Vec<&Exp> = arms.iter().map(|(_, e)| e).collect();
            let arm_refs: Vec<HashSet<String>> =
                arm_exps.iter().map(|e| collect_references(e)).collect();
            let arm_intros: Vec<HashSet<String>> =
                arm_exps.iter().map(|e| top_level_arm_intros(e)).collect();

            let to_demote = decide_demotions(&arm_intros, &arm_refs, earlier_scope, later_refs);
            let needs_declare: HashSet<String> =
                to_demote.difference(earlier_scope).cloned().collect();

            let arms = arms
                .into_iter()
                .map(|(v, e)| (v, demote_top_level_intros(e, &to_demote)))
                .collect();
            (needs_declare, Exp::Switch(cond, enum_, arms))
        }
        Exp::Loop(label, body) => {
            let (to_demote, needs_declare) =
                single_body_demotions(&body, earlier_scope, later_refs);
            let body = Box::new(demote_top_level_intros(*body, &to_demote));
            (needs_declare, Exp::Loop(label, body))
        }
        Exp::While(label, cond, body) => {
            let (to_demote, needs_declare) =
                single_body_demotions(&body, earlier_scope, later_refs);
            let body = Box::new(demote_top_level_intros(*body, &to_demote));
            (needs_declare, Exp::While(label, cond, body))
        }
        other => (HashSet::new(), other),
    }
}

/// Loop/While reduction of `decide_demotions`: no sibling arms, so the only triggers are
/// already-in-scope (shadow) and referenced-after (forward use).
fn single_body_demotions(
    body: &Exp,
    earlier_scope: &HashSet<String>,
    later_refs: &HashSet<String>,
) -> (HashSet<String>, HashSet<String>) {
    let intros = top_level_arm_intros(body);
    let to_demote: HashSet<String> = intros
        .into_iter()
        .filter(|n| earlier_scope.contains(n) || later_refs.contains(n))
        .collect();
    let needs_declare = to_demote.difference(earlier_scope).cloned().collect();
    (to_demote, needs_declare)
}

/// A name introduced at the top of an arm needs to lift out iff at least one of:
///   - it's already in scope above this Seq item (the arm-level `let` would shadow);
///   - another arm of the same item also introduces or references it;
///   - some later sibling in this Seq references it.
fn decide_demotions(
    arm_intros: &[HashSet<String>],
    arm_refs: &[HashSet<String>],
    earlier_scope: &HashSet<String>,
    later_refs: &HashSet<String>,
) -> HashSet<String> {
    let mut out = HashSet::new();
    for (i, intros) in arm_intros.iter().enumerate() {
        for name in intros {
            if out.contains(name) {
                continue;
            }
            let already_in_scope = earlier_scope.contains(name);
            let used_later = later_refs.contains(name);
            let other_arm_touches = arm_intros
                .iter()
                .enumerate()
                .any(|(j, other)| j != i && other.contains(name))
                || arm_refs
                    .iter()
                    .enumerate()
                    .any(|(j, other)| j != i && other.contains(name));
            if already_in_scope || used_later || other_arm_touches {
                out.insert(name.clone());
            }
        }
    }
    out
}

/// Names introduced at the top of `exp`, descending only through `Seq`. Nested
/// IfElse/Switch/Loop/While arms have their own scopes — anything introduced inside them was
/// already given a hoist opportunity by their own Seqs.
fn top_level_arm_intros(exp: &Exp) -> HashSet<String> {
    let mut out = HashSet::new();
    top_level_arm_intros_into(exp, &mut out);
    out
}

fn top_level_arm_intros_into(exp: &Exp, out: &mut HashSet<String>) {
    match exp {
        Exp::LetBind(names, _) | Exp::Declare(names) => {
            for n in names {
                out.insert(n.clone());
            }
        }
        Exp::Seq(items) => {
            for item in items {
                top_level_arm_intros_into(item, out);
            }
        }
        _ => {}
    }
}

/// Top-level rewrite for the names in `targets`: `LetBind([X], e)` → `Assign([X], e)`, and X
/// drops out of any top-level `Declare`. Descends only through `Seq` — see `top_level_arm_intros`.
fn demote_top_level_intros(exp: Exp, targets: &HashSet<String>) -> Exp {
    if targets.is_empty() {
        return exp;
    }
    match exp {
        Exp::LetBind(names, value) => {
            if names.len() == 1 && targets.contains(&names[0]) {
                Exp::Assign(names, value)
            } else {
                Exp::LetBind(names, value)
            }
        }
        Exp::Declare(names) => {
            let kept: Vec<String> = names.into_iter().filter(|n| !targets.contains(n)).collect();
            if kept.is_empty() {
                // Empty Seq is dropped by flatten_seq later.
                Exp::Seq(vec![])
            } else {
                Exp::Declare(kept)
            }
        }
        Exp::Seq(items) => {
            let items = items
                .into_iter()
                .map(|item| demote_top_level_intros(item, targets))
                .collect();
            Exp::Seq(items)
        }
        other => other,
    }
}

/// Every name read, assigned, or otherwise mentioned anywhere in `exp`. Used as the
/// "referenced" set for hoist decisions; over-approximating just causes spurious Declares,
/// never missing ones.
fn collect_references(exp: &Exp) -> HashSet<String> {
    let mut out = HashSet::new();
    collect_references_into(exp, &mut out);
    out
}

fn collect_references_into(exp: &Exp, out: &mut HashSet<String>) {
    match exp {
        Exp::Variable(name) => {
            out.insert(name.clone());
        }
        Exp::Assign(names, value) => {
            for n in names {
                out.insert(n.clone());
            }
            collect_references_into(value, out);
        }
        Exp::LetBind(_, value) => {
            collect_references_into(value, out);
        }
        Exp::Declare(_) => {}
        Exp::Seq(items) => {
            for item in items {
                collect_references_into(item, out);
            }
        }
        Exp::IfElse(cond, conseq, alt) => {
            collect_references_into(cond, out);
            collect_references_into(conseq, out);
            if let Some(alt) = alt.as_ref() {
                collect_references_into(alt, out);
            }
        }
        Exp::Switch(cond, _, cases) => {
            collect_references_into(cond, out);
            for (_, body) in cases {
                collect_references_into(body, out);
            }
        }
        Exp::Loop(_, body) => collect_references_into(body, out),
        Exp::While(_, cond, body) => {
            collect_references_into(cond, out);
            collect_references_into(body, out);
        }
        Exp::Return(items) => {
            for item in items {
                collect_references_into(item, out);
            }
        }
        Exp::Call(_, args) => {
            for a in args {
                collect_references_into(a, out);
            }
        }
        Exp::Abort(e) => collect_references_into(e, out),
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            for a in args {
                collect_references_into(a, out);
            }
        }
        Exp::Borrow(_, e) => collect_references_into(e, out),
        Exp::Unpack(_, _, e) => collect_references_into(e, out),
        Exp::UnpackVariant(_, _, _, e) => collect_references_into(e, out),
        Exp::VecUnpack(_, e) => collect_references_into(e, out),
        Exp::Break(_) | Exp::Continue(_) | Exp::Value(_) | Exp::Constant(_) => {}
        Exp::Unstructured(nodes) => {
            for node in nodes {
                match node {
                    UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) => {
                        collect_references_into(body, out);
                    }
                    UnstructuredNode::Goto(_) => {}
                }
            }
        }
    }
}
