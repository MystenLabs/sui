// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Inline let-bindings whose RHS is a single variable, when both names are used at most once.
//!
//! For a binding `let X = Y;` we fire when:
//!   - `Y` is read exactly once in the function body (the binding's own RHS) and never written;
//!   - `X` is read at most once on any execution path, never re-assigned, and not declared
//!     elsewhere.
//!
//! In that case the binding is dropped and the single use of `X` is rewritten to `Y` directly.
//! Multiple non-conflicting bindings can be processed in one pass; chains like
//! `let l20 = l3; let l3 = l5;` are resolved via transitive closure before substitution.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::{Exp, UnstructuredNode},
    refinement::liveness::Liveness,
};

pub fn refine(exp: &mut Exp) -> bool {
    let liveness = Liveness::analyze(exp);

    let mut candidates: Vec<(String, String)> = Vec::new();
    collect_candidates(exp, exp, &liveness, &mut candidates);
    if candidates.is_empty() {
        return false;
    }

    let drop_set: BTreeSet<(String, String)> = candidates.iter().cloned().collect();
    let mut sub_map: BTreeMap<String, String> = candidates.into_iter().collect();
    transitive_closure(&mut sub_map);

    apply(exp, &drop_set, &sub_map);
    true
}

// -------------------------------------------------------------------------------------------------
// Candidate collection

fn collect_candidates(exp: &Exp, root: &Exp, liveness: &Liveness, out: &mut Vec<(String, String)>) {
    use Exp as E;
    // Local check first, then recurse.
    if let E::LetBind(targets, rhs) = exp
        && targets.len() == 1
        && let E::Variable(y) = rhs.as_ref()
        && is_eligible(&targets[0], y, root, liveness)
    {
        out.push((targets[0].clone(), y.clone()));
    }
    match exp {
        E::Variable(_)
        | E::Declare(_)
        | E::Value(_)
        | E::Constant(_)
        | E::Break(_)
        | E::Continue(_) => {}
        E::LetBind(_, e)
        | E::Assign(_, e)
        | E::Abort(e)
        | E::Borrow(_, e)
        | E::VecUnpack(_, e)
        | E::Unpack(_, _, e)
        | E::UnpackVariant(_, _, _, e)
        | E::Block(_, e) => collect_candidates(e, root, liveness, out),
        E::Loop(_, b) => collect_candidates(b, root, liveness, out),
        E::While(_, c, b) => {
            collect_candidates(c, root, liveness, out);
            collect_candidates(b, root, liveness, out);
        }
        E::IfElse(c, t, alt) => {
            collect_candidates(c, root, liveness, out);
            collect_candidates(t, root, liveness, out);
            if let Some(a) = alt.as_ref().as_ref() {
                collect_candidates(a, root, liveness, out);
            }
        }
        E::Seq(items) | E::Return(items) | E::Call(_, items) => {
            for i in items {
                collect_candidates(i, root, liveness, out);
            }
        }
        E::Switch(subject, _, arms) => {
            collect_candidates(subject, root, liveness, out);
            for (_, b) in arms {
                collect_candidates(b, root, liveness, out);
            }
        }
        E::Match(subject, _, arms) => {
            collect_candidates(subject, root, liveness, out);
            for (_, _, b) in arms {
                collect_candidates(b, root, liveness, out);
            }
        }
        E::Primitive { args, .. } | E::Data { args, .. } => {
            for a in args {
                collect_candidates(a, root, liveness, out);
            }
        }
        E::Unstructured(nodes) => {
            for n in nodes {
                match n {
                    UnstructuredNode::Labeled(_, b) | UnstructuredNode::Statement(b) => {
                        collect_candidates(b, root, liveness, out);
                    }
                    UnstructuredNode::Goto(_) => {}
                }
            }
        }
    }
}

fn is_eligible(x: &str, y: &str, root: &Exp, liveness: &Liveness) -> bool {
    if x == y {
        return false;
    }
    let counts = liveness.counts();

    // Source `y`: exactly one read (this binding's RHS), never written.
    if counts.reads(y) != 1 || counts.assigns(y) != 0 {
        return false;
    }

    // Destination `x`: declared once via this binding, never written or re-introduced.
    if counts.assigns(x) != 0
        || counts.letbinds(x) != 1
        || counts.declares(x) != 0
        || counts.unpacks(x) != 0
    {
        return false;
    }

    // `x` must have at least one read (otherwise it's dead code, not our concern). When there
    // are multiple syntactic reads we additionally require that at most one runs on any given
    // path — equivalently, `x` is dead immediately after each read. A single syntactic read
    // inside a loop is still fine: the substituted RHS is a variable that we already know is
    // never re-assigned, so re-reading it across loop iterations is value-preserving.
    if counts.reads(x) == 0 {
        return false;
    }
    if counts.reads(x) > 1 && !liveness.singly_used(root, x) {
        return false;
    }
    true
}

// -------------------------------------------------------------------------------------------------
// Apply

/// Resolve each value to its furthest target so chains `(X -> Y), (Y -> Z)` collapse to
/// `(X -> Z), (Y -> Z)` before we substitute Variable nodes.
fn transitive_closure(subs: &mut BTreeMap<String, String>) {
    let keys: Vec<String> = subs.keys().cloned().collect();
    for k in keys {
        let mut cur = subs[&k].clone();
        while let Some(next) = subs.get(&cur) {
            if *next == cur {
                break;
            }
            cur = next.clone();
        }
        subs.insert(k, cur);
    }
}

fn apply(exp: &mut Exp, drop_set: &BTreeSet<(String, String)>, sub_map: &BTreeMap<String, String>) {
    use Exp as E;
    if let E::Seq(items) = exp {
        items.retain(|item| !is_dropped(item, drop_set));
    }
    if let E::Variable(n) = exp {
        if let Some(y) = sub_map.get(n.as_str()) {
            *n = y.clone();
        }
        return;
    }
    match exp {
        E::Variable(_)
        | E::Declare(_)
        | E::Value(_)
        | E::Constant(_)
        | E::Break(_)
        | E::Continue(_) => {}
        E::LetBind(_, e)
        | E::Assign(_, e)
        | E::Abort(e)
        | E::Borrow(_, e)
        | E::VecUnpack(_, e)
        | E::Unpack(_, _, e)
        | E::UnpackVariant(_, _, _, e)
        | E::Block(_, e) => apply(e, drop_set, sub_map),
        E::Loop(_, b) => apply(b, drop_set, sub_map),
        E::While(_, c, b) => {
            apply(c, drop_set, sub_map);
            apply(b, drop_set, sub_map);
        }
        E::IfElse(c, t, alt) => {
            apply(c, drop_set, sub_map);
            apply(t, drop_set, sub_map);
            if let Some(a) = alt.as_mut().as_mut() {
                apply(a, drop_set, sub_map);
            }
        }
        E::Seq(items) | E::Return(items) | E::Call(_, items) => {
            for i in items {
                apply(i, drop_set, sub_map);
            }
        }
        E::Switch(subject, _, arms) => {
            apply(subject, drop_set, sub_map);
            for (_, b) in arms {
                apply(b, drop_set, sub_map);
            }
        }
        E::Match(subject, _, arms) => {
            apply(subject, drop_set, sub_map);
            for (_, _, b) in arms {
                apply(b, drop_set, sub_map);
            }
        }
        E::Primitive { args, .. } | E::Data { args, .. } => {
            for a in args {
                apply(a, drop_set, sub_map);
            }
        }
        E::Unstructured(nodes) => {
            for n in nodes {
                match n {
                    UnstructuredNode::Labeled(_, b) | UnstructuredNode::Statement(b) => {
                        apply(b, drop_set, sub_map);
                    }
                    UnstructuredNode::Goto(_) => {}
                }
            }
        }
    }
}

fn is_dropped(item: &Exp, drop_set: &BTreeSet<(String, String)>) -> bool {
    if let Exp::LetBind(targets, rhs) = item
        && targets.len() == 1
        && let Exp::Variable(y) = rhs.as_ref()
    {
        drop_set.contains(&(targets[0].clone(), y.clone()))
    } else {
        false
    }
}
