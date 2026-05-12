// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// `let X;` hoisting — two-pass bottom-up / top-down design
// -------------------------------------------------------------------------------------------------
//
// Per-block term reconstruction emits `LetBind` for each local's first StoreLoc, with no view of
// the surrounding scope. An arm-scope `let X` doesn't outlive its arm, so when those per-block
// LetBinds end up inside an IfElse/Switch/Loop/While arm — or are duplicated across sibling
// items of a Seq — anything that reads `X` from a different scope either fails to resolve or
// shadows a still-live outer binding.
//
// This pass runs in two phases.
//
// **Pass 1: `summarize`** (bottom-up, read-only). Walks the input and builds a parallel
// `Summary { entries, subs }` tree. `entries` is the set of names this subtree *touches* —
// the union of names introduced (LetBind/Declare targets) and names referenced (Variable,
// Assign targets). The two are unified deliberately: a name that's introduced in one position
// and referenced in another is exactly the cross-position case we want to detect, and one set
// catches both.
//
// **Pass 2: `hoist`** (top-down, rewriting). Walks the AST and the summary tree in lockstep,
// carrying an immutable `already_bound: &HashSet<String>` of names declared by some ancestor.
// At every "splitting" node (IfElse / Switch / While / Seq) the rule is the same:
//
//     1. Apply the per-construct rule to the children's entries to identify "names this scope
//        is responsible for." For all splitting nodes the rule is simply: a name belongs here
//        if ≥2 children touch it.
//     2. `decl_here = those_names \ already_bound`. Anything already declared by an ancestor
//        is the ancestor's responsibility, not ours.
//     3. Hand `new_bound = already_bound ∪ decl_here` to each child, by immutable reference.
//        No mutation flows back up; each recursive call constructs its own view.
//     4. Emit a `Declare(decl_here)` and wrap the rewritten node with it.
//
// For Seq, step 4 is refined for readability: each name in `decl_here` is emitted in a
// `Declare` placed immediately before the first item that touches it, not at the top of the
// Seq. That makes the declaration adjacent to the first assignment (after `seq_append`
// flattening), so the later `fuse_let` refinement can collapse `Declare(X); Assign(X, e);`
// back into a single `let X = e;`.
//
// `LetBind(X, e)` checks `already_bound` at its own position: if X is in there, an ancestor
// took responsibility for declaring X, so this LetBind demotes to `Assign(X, e)`. `Declare(X)`
// likewise drops names that are already bound (an ancestor's Declare already covers them).
//
// One safety invariant worth stating: if `X ∈ decl_here` in a Seq, the *first* sub-summary to
// touch X must touch it as an intro. Proof: X ∈ decl_here ⇒ X ∉ already_bound ⇒ no ancestor
// scope declared X. For a Seq item to *reference* X without an ancestor declaration, the
// bytecode would be loading an unstored register — invalid. So the earliest touch must be the
// LetBind that introduces X, and placing the Declare just before it is sound.

use crate::ast::{Exp, UnstructuredNode};

use std::collections::{HashMap, HashSet};

/// Lift `let X;` introductions out of inner arms/bodies to their proper enclosing scope.
///
/// `params` is the list of names already in scope at the function's outermost Seq — the
/// function's parameter names. Seeding `already_bound` with them keeps the pass from
/// emitting a spurious `let X;` for a parameter that happens to be touched by ≥2 children
/// (e.g. a parameter referenced inside both arms of an `if`).
pub fn hoist_declarations(e: &mut Exp, params: Vec<String>) {
    let summary = summarize_exp(e);
    let owned = std::mem::replace(e, Exp::Seq(vec![]));
    *e = exp(owned, summary, &params.into_iter().collect());
}

// -------------------------------------------------------------------------------------------------
// Pass 1: summarize
// -------------------------------------------------------------------------------------------------

/// Bottom-up summary mirroring the AST
struct Summary {
    // Names this subtree touches (intros + refs).
    entries: HashSet<String>,
    /// Per-child summaries in the same canonical order pass 2 will recur in.
    subs: Vec<Summary>,
}

fn summarize_exp(e: &Exp) -> Summary {
    match e {
        Exp::Seq(items) => {
            let subs: Vec<Summary> = items.iter().map(summarize_exp).collect();
            let entries = union_entries(&subs);
            Summary { entries, subs }
        }
        Exp::IfElse(cond, conseq, alt) => {
            let mut subs = vec![summarize_exp(cond), summarize_exp(conseq)];
            if let Some(a) = alt.as_ref() {
                subs.push(summarize_exp(a));
            }
            let entries = union_entries(&subs);
            Summary { entries, subs }
        }
        Exp::Switch(cond, _, cases) => {
            let mut subs = Vec::with_capacity(1 + cases.len());
            subs.push(summarize_exp(cond));
            for (_, body) in cases {
                subs.push(summarize_exp(body));
            }
            let entries = union_entries(&subs);
            Summary { entries, subs }
        }
        Exp::Loop(_, body) => {
            let body_sum = summarize_exp(body);
            let entries = body_sum.entries.clone();
            Summary {
                entries,
                subs: vec![body_sum],
            }
        }
        Exp::While(_, cond, body) => {
            let subs = vec![summarize_exp(cond), summarize_exp(body)];
            let entries = union_entries(&subs);
            Summary { entries, subs }
        }
        Exp::Match(..) => unreachable!("`reconstruct_match` runs after `hoist_declarations`"),
        Exp::Return(items) | Exp::Call(_, items) => {
            let subs: Vec<Summary> = items.iter().map(summarize_exp).collect();
            let entries = union_entries(&subs);
            Summary { entries, subs }
        }
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            let subs: Vec<Summary> = args.iter().map(summarize_exp).collect();
            let entries = union_entries(&subs);
            Summary { entries, subs }
        }
        Exp::LetBind(names, value) | Exp::Assign(names, value) => {
            let v = summarize_exp(value);
            let mut entries = v.entries.clone();
            for n in names {
                entries.insert(n.clone());
            }
            Summary {
                entries,
                subs: vec![v],
            }
        }
        Exp::Declare(names) => Summary {
            entries: names.iter().cloned().collect(),
            subs: vec![],
        },
        Exp::Variable(name) => Summary {
            entries: std::iter::once(name.clone()).collect(),
            subs: vec![],
        },
        Exp::Abort(value) | Exp::Borrow(_, value) => {
            let v = summarize_exp(value);
            let entries = v.entries.clone();
            Summary {
                entries,
                subs: vec![v],
            }
        }
        // Unpack-style bindings introduce field-destructured names, but those names live with the
        // Unpack node itself — they can't be hoisted out as `let X;` declarations the way a plain
        // `LetBind(X, e)` can. Treat the destructured names as opaque so the algorithm doesn't try
        // to lift them; just propagate the value's entries.
        Exp::Unpack(_, _, value)
        | Exp::UnpackVariant(_, _, _, value)
        | Exp::VecUnpack(_, value) => {
            let v = summarize_exp(value);
            let entries = v.entries.clone();
            Summary {
                entries,
                subs: vec![v],
            }
        }
        Exp::Value(_) | Exp::Constant(_) | Exp::Break(_) | Exp::Continue(_) => Summary {
            entries: HashSet::new(),
            subs: vec![],
        },
        // Unstructured holds raw goto-style nodes we don't rewrite. Summarize the inner Exps just
        // to surface their entries to ancestors; drop the `subs` since pass 2 won't recur into
        // them.
        Exp::Unstructured(nodes) => {
            let mut entries = HashSet::new();
            for node in nodes {
                match node {
                    UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) => {
                        let s = summarize_exp(body);
                        entries.extend(s.entries);
                    }
                    UnstructuredNode::Goto(_) => {}
                }
            }
            Summary {
                entries,
                subs: vec![],
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Pass 2: hoist (rewrite)
// -------------------------------------------------------------------------------------------------

fn exp(e: Exp, summary: Summary, already_bound: &HashSet<String>) -> Exp {
    let Summary { entries: _, subs } = summary;
    match e {
        Exp::Seq(items) => {
            assert_eq!(items.len(), subs.len(), "Seq sub-count mismatch");

            let child_entries: Vec<&HashSet<String>> = subs.iter().map(|s| &s.entries).collect();
            let decl = decl_here(&child_entries, already_bound);
            let new_bound = extend_bound(already_bound, &decl);

            // Late-binding: pin each declared name to the first item that touches it. Grouping by first-
            // touch index lets multiple names that "wake up" at the same item share a single `Declare`.
            let mut by_first_touch: HashMap<usize, Vec<String>> = HashMap::new();
            for name in &decl {
                let idx = subs
                    .iter()
                    .position(|s| s.entries.contains(name))
                    .expect("decl_here name must be touched by some sub");
                by_first_touch.entry(idx).or_default().push(name.clone());
            }

            let mut out: Vec<Exp> = Vec::with_capacity(items.len() + decl.len());
            for (i, (item, sub)) in items.into_iter().zip(subs.into_iter()).enumerate() {
                if let Some(mut names) = by_first_touch.remove(&i) {
                    names.sort();
                    out.push(Exp::Declare(names));
                }
                let rewritten = exp(item, sub, &new_bound);
                seq_append(&mut out, rewritten);
            }
            Exp::Seq(out)
        }
        Exp::IfElse(cond, conseq, alt) => {
            let cond = cond;
            let conseq = conseq;
            let mut iter = subs.into_iter();
            let cond_sum = iter.next().expect("IfElse must summarize a cond");
            let conseq_sum = iter.next().expect("IfElse must summarize a conseq");
            let alt_sum = iter.next();

            let child_entries: Vec<&HashSet<String>> = std::iter::once(&cond_sum.entries)
                .chain(std::iter::once(&conseq_sum.entries))
                .chain(alt_sum.iter().map(|s| &s.entries))
                .collect();
            let decl = decl_here(&child_entries, already_bound);
            let new_bound = extend_bound(already_bound, &decl);

            let cond = Box::new(exp(*cond, cond_sum, &new_bound));
            let conseq = Box::new(exp(*conseq, conseq_sum, &new_bound));
            let alt_inner = match (*alt, alt_sum) {
                (Some(a), Some(s)) => Some(exp(a, s, &new_bound)),
                (None, None) => None,
                _ => unreachable!("alt presence and alt summary must agree"),
            };
            make_decls(decl, Exp::IfElse(cond, conseq, Box::new(alt_inner)))
        }
        Exp::Switch(cond, enum_, cases) => {
            let cond = cond;
            let mut iter = subs.into_iter();
            let cond_sum = iter.next().expect("Switch must summarize a cond");
            let arm_sums: Vec<Summary> = iter.collect();
            assert_eq!(arm_sums.len(), cases.len(), "Switch arm-count mismatch");

            let child_entries: Vec<&HashSet<String>> = std::iter::once(&cond_sum.entries)
                .chain(arm_sums.iter().map(|s| &s.entries))
                .collect();
            let decl = decl_here(&child_entries, already_bound);
            let new_bound = extend_bound(already_bound, &decl);

            let cond = Box::new(exp(*cond, cond_sum, &new_bound));
            let new_cases: Vec<(move_symbol_pool::Symbol, Exp)> = cases
                .into_iter()
                .zip(arm_sums)
                .map(|((v, body), s)| (v, exp(body, s, &new_bound)))
                .collect();
            make_decls(decl, Exp::Switch(cond, enum_, new_cases))
        }
        Exp::Match(..) => unreachable!("`reconstruct_match` runs after `hoist_declarations`"),
        Exp::Loop(label, body) => {
            // Loop has one sub; "≥2 children" can never fire — no decl_here at this level.
            let body_sum = expect_one(subs);
            Exp::Loop(label, Box::new(exp(*body, body_sum, already_bound)))
        }
        Exp::While(label, cond, body) => {
            let cond = cond;
            let body = body;
            let mut iter = subs.into_iter();
            let cond_sum = iter.next().expect("While must summarize a cond");
            let body_sum = iter.next().expect("While must summarize a body");

            let child_entries = [&cond_sum.entries, &body_sum.entries];
            let decl = decl_here(&child_entries, already_bound);
            let new_bound = extend_bound(already_bound, &decl);

            let cond = Box::new(exp(*cond, cond_sum, &new_bound));
            let body = Box::new(exp(*body, body_sum, &new_bound));
            make_decls(decl, Exp::While(label, cond, body))
        }
        Exp::LetBind(names, value) => {
            let value_sum = expect_one(subs);
            let value = Box::new(exp(*value, value_sum, already_bound));
            if names.len() == 1 && already_bound.contains(&names[0]) {
                Exp::Assign(names, value)
            } else {
                Exp::LetBind(names, value)
            }
        }
        Exp::Declare(names) => {
            let kept: Vec<String> = names
                .into_iter()
                .filter(|n| !already_bound.contains(n))
                .collect();
            if kept.is_empty() {
                // Empty Seq is dropped by flatten / refinement later.
                Exp::Seq(vec![])
            } else {
                Exp::Declare(kept)
            }
        }
        Exp::Assign(names, value) => {
            let value_sum = expect_one(subs);
            Exp::Assign(names, Box::new(exp(*value, value_sum, already_bound)))
        }
        Exp::Return(items) => {
            let new = exp_list(items, subs, already_bound);
            Exp::Return(new)
        }
        Exp::Call(target, args) => {
            let new = exp_list(args, subs, already_bound);
            Exp::Call(target, new)
        }
        Exp::Abort(value) => {
            let value_sum = expect_one(subs);
            Exp::Abort(Box::new(exp(*value, value_sum, already_bound)))
        }
        Exp::Primitive { op, args } => {
            let new = exp_list(args, subs, already_bound);
            Exp::Primitive { op, args: new }
        }
        Exp::Data { op, args } => {
            let new = exp_list(args, subs, already_bound);
            Exp::Data { op, args: new }
        }
        Exp::Borrow(m, value) => {
            let value_sum = expect_one(subs);
            Exp::Borrow(m, Box::new(exp(*value, value_sum, already_bound)))
        }
        Exp::Unpack(t, fields, value) => {
            let value_sum = expect_one(subs);
            Exp::Unpack(t, fields, Box::new(exp(*value, value_sum, already_bound)))
        }
        Exp::UnpackVariant(k, t, fields, value) => {
            let value_sum = expect_one(subs);
            Exp::UnpackVariant(
                k,
                t,
                fields,
                Box::new(exp(*value, value_sum, already_bound)),
            )
        }
        Exp::VecUnpack(names, value) => {
            let value_sum = expect_one(subs);
            Exp::VecUnpack(names, Box::new(exp(*value, value_sum, already_bound)))
        }
        e @ (Exp::Value(_)
        | Exp::Constant(_)
        | Exp::Variable(_)
        | Exp::Break(_)
        | Exp::Continue(_)
        | Exp::Unstructured(_)) => e,
    }
}

// ---------------------------------------------------------------------------------------------------
// Helpers

fn decl_here(
    child_entries: &[&HashSet<String>],
    already_bound: &HashSet<String>,
) -> HashSet<String> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for entries in child_entries {
        for n in *entries {
            *counts.entry(n.as_str()).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .filter(|(n, c)| *c >= 2 && !already_bound.contains(*n))
        .map(|(n, _)| n.to_owned())
        .collect()
}

fn exp_list(items: Vec<Exp>, subs: Vec<Summary>, already_bound: &HashSet<String>) -> Vec<Exp> {
    assert_eq!(items.len(), subs.len(), "arg-count mismatch");
    items
        .into_iter()
        .zip(subs)
        .map(|(e, s)| exp(e, s, already_bound))
        .collect()
}

fn extend_bound(already_bound: &HashSet<String>, decl: &HashSet<String>) -> HashSet<String> {
    let mut out = already_bound.clone();
    for n in decl {
        out.insert(n.clone());
    }
    out
}

/// Wrap `body` with `Declare(decl)` if `decl` is non-empty. Returns a flat `Seq` that the caller's
/// `seq_append` will splice; lone bodies pass through.
fn make_decls(decl: HashSet<String>, body: Exp) -> Exp {
    if decl.is_empty() {
        body
    } else {
        let mut names: Vec<String> = decl.into_iter().collect();
        names.sort();
        Exp::Seq(vec![Exp::Declare(names), body])
    }
}

fn expect_one(subs: Vec<Summary>) -> Summary {
    let mut iter = subs.into_iter();
    let one = iter.next().expect("expected one sub-summary");
    debug_assert!(iter.next().is_none(), "expected exactly one sub-summary");
    one
}

/// Splice `item` into `out`, flattening if it is itself a `Seq`. This is how the `Declare(...)`
/// that `make_decls` wraps a branching node with lands directly in the parent Seq, adjacent to
/// the assignments that follow.
fn seq_append(out: &mut Vec<Exp>, item: Exp) {
    match item {
        Exp::Seq(items) => {
            for it in items {
                seq_append(out, it);
            }
        }
        other => out.push(other),
    }
}

fn union_entries(subs: &[Summary]) -> HashSet<String> {
    let mut out = HashSet::new();
    for s in subs {
        for n in &s.entries {
            out.insert(n.clone());
        }
    }
    out
}
