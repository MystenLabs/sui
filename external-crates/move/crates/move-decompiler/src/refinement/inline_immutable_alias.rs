// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Inline let-bindings whose RHS is an immutable variable.
//!
//! For a binding `let X = Y;` we fire when:
//!   - `Y` is never written in the function body - its value is stable for the entire
//!     function, regardless of how many places read it;
//!   - `X` has exactly one defining `LetBind` (this one), is never re-assigned, and is not
//!     declared or pattern-bound elsewhere;
//!   - if `X` has more than one syntactic read, no path reads it twice (so the substitution
//!     doesn't change the *number* of reads on any path - relevant for non-`copy` types
//!     where re-reading would be a use-after-move). When `X` has a single syntactic read,
//!     the per-path check is moot.
//!
//! In that case the binding is dropped and uses of `X` are rewritten to `Y`. Multiple
//! non-conflicting bindings are processed in one pass; chains like
//! `let l20 = l3; let l3 = l5;` are resolved via transitive closure before substitution.
//!
//! This is the generalization of "single-use" inlining to "single-source" (immutable RHS):
//! the cetus argument-staging idiom
//!
//! ```text
//! let l40 = l13; let l39 = l12; let l38 = l11;
//! is_stable(freeze(l0), l38, l39, l40)
//! ```
//!
//! collapses to `is_stable(freeze(l0), l11, l12, l13)` because each staging local's RHS is
//! a never-written parameter alias, even though `l11`/`l12`/`l13` are themselves used in
//! other call sites.

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
    // Local check first, then recur.
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
        E::MatchLit(subject, arms) => {
            collect_candidates(subject, root, liveness, out);
            for (_, b) in arms {
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

    // Soundness rests on a slot-invariance argument.
    //
    // For `let X = Y;`, the substitution `X -> Y` (drop the binding, rewrite each
    // `Variable(X)` use site to `Variable(Y)`) preserves semantics iff *slot* `Y`'s
    // value is invariant from the LetBind to every X-use. Reason:
    //
    //   - Slot `X` is set once by the LetBind to `V0 := value of slot Y at binding time`.
    //     The X-side guards below ensure slot `X` is never reassigned, so reads of
    //     `Variable(X)` always return `V0`.
    //   - Reads of `Variable(Y)` return slot `Y`'s *current* value.
    //   - Substituting `X -> Y` preserves the read result iff slot `Y`'s current value
    //     equals `V0` at every X-use site.
    //
    // The two ways a slot can be mutated:
    //   1. `Assign([Y], _)`            - direct reassignment.
    //   2. `WriteRef` through a `&mut`-handle to the slot, which requires
    //      `Borrow(true, Variable(Y))` somewhere upstream (even via a chain of aliased
    //      handles, the chain must start with that `Borrow`).
    //
    // `assigns(Y) == 0 && mut_borrows(Y) == 0` rules both out globally. The check is
    // type-agnostic: for `&mut T`-typed `Y`, pointee mutations (`*Y = e`, `f(Y)` where
    // `f` writes through) operate on the heap rather than slot `Y`, so they're invisible
    // to the alias relationship `X <-> Y` we're preserving - both sides observe the same
    // pointee at the same time. The dangerous shapes - `&mut Y` upstream of an X-use -
    // are exactly what `mut_borrows` counts.
    if counts.assigns(y) != 0 || counts.mut_borrows(y) != 0 {
        return false;
    }

    // Destination `x`: declared once via this binding, never written, never re-introduced,
    // never mut-borrowed. The mut-borrow guard on `x` is the symmetric requirement: if
    // some site takes `&mut x`, our substitution would need the writes-through-the-borrow
    // to land on `y` too, which the rewrite doesn't replicate.
    if counts.assigns(x) != 0
        || counts.letbinds(x) != 1
        || counts.declares(x) != 0
        || counts.unpacks(x) != 0
        || counts.mut_borrows(x) != 0
    {
        return false;
    }

    // `x` must have at least one read (otherwise it's dead code, not our concern). When there
    // are multiple syntactic reads we additionally require that at most one runs on any given
    // path - substituting wouldn't change `y`'s observed value, but it would change the
    // *number* of reads on a path, which matters when the type lacks `copy`. A single
    // syntactic read inside a loop is still fine: only one Variable node, so the per-path
    // count is trivially at most one.
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
        E::MatchLit(subject, arms) => {
            apply(subject, drop_set, sub_map);
            for (_, b) in arms {
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
