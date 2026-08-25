// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// Sink Declares one scope deeper when only a single sibling needs them.
//
// `hoist_declarations` raises `let X;` to the enclosing scope whenever two or more siblings
// touch `X`. Later refinements (notably `simplify_if`'s Rule 5 dropping empty-body `if`s) can
// remove some of those siblings, leaving a hoisted `Declare(X)` whose only remaining user is a
// single nested arm. The hoist is then over-eager: `let X;` and `X = e;` end up in different
// scopes, defeating `fuse_let` and producing
//
//     let __c40;
//     if (l7) {
//         __c40 = option::is_none(&l1);
//         assert!(__c40, C2)
//     }
//
// instead of the source-form
//
//     if (l7) {
//         let __c40 = option::is_none(&l1);
//         assert!(__c40, C2)
//     }
//
// This pass walks each `Seq`'s `Declare`s, finds the names that only one later sibling touches,
// and pushes their declarations into the sub-arm that uses them. After this `fuse_let` can
// collapse the moved `Declare` and the inner `Assign` into a single `LetBind`.
//
// Conservative on shape: a `Declare`'s names are sunk into the one sibling only when each
// name has a single sub-arm use within that sibling (`then`-only or `else`-only of an `IfElse`,
// the body of a `Loop` / `While` / `Block`, or recursively into a nested `Seq`). Anything
// trickier - both arms touch the name, the sibling isn't a recognized scope-bearing
// construct - leaves the name in the outer `Declare`.

use crate::{
    ast::{Exp, UnstructuredNode},
    refinement::{Refine, liveness},
};

use std::collections::BTreeSet;

pub fn refine(exp: &mut Exp) -> bool {
    SinkDeclare.refine(exp)
}

struct SinkDeclare;

impl Refine for SinkDeclare {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Seq(items) = exp else {
            return false;
        };
        sink_seq(items)
    }
}

// -------------------------------------------------------------------------------------------------
// Per-Seq driver

fn sink_seq(items: &mut Vec<Exp>) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < items.len() {
        let Exp::Declare(declared) = &items[i] else {
            i += 1;
            continue;
        };
        let declared = declared.clone();

        // Per name, find which later sibling touches it. Names that have exactly one user
        // and where that user can absorb the `Declare` get split out into a sunk subset; the
        // rest stay in the original `Declare` at position `i`.
        let mut sunk: BTreeSet<String> = BTreeSet::new();
        for name in &declared {
            let users: Vec<usize> = items
                .iter()
                .enumerate()
                .skip(i + 1)
                .filter(|(_, item)| liveness::referenced_names(item).contains(name))
                .map(|(j, _)| j)
                .collect();
            if users.len() != 1 {
                continue;
            }
            let j = users[0];
            if can_sink_into(&items[j], name) {
                sunk.insert(name.clone());
            }
        }
        if sunk.is_empty() {
            i += 1;
            continue;
        }

        // Perform the sinks. Do them name-by-name so each can land in a different inner
        // scope (one name might end up in a `then` arm, another in a loop body).
        for name in &sunk {
            let j = items
                .iter()
                .enumerate()
                .skip(i + 1)
                .find(|(_, item)| liveness::referenced_names(item).contains(name))
                .map(|(j, _)| j)
                .expect("sunk names were collected from existing users");
            let inserted = sink_into(&mut items[j], name);
            debug_assert!(
                inserted,
                "can_sink_into and sink_into must agree on shape ({})",
                name
            );
        }

        // Strip the sunk names from the original `Declare` (or drop it if everything moved).
        let remaining: Vec<String> = declared.into_iter().filter(|n| !sunk.contains(n)).collect();
        if remaining.is_empty() {
            items.remove(i);
        } else {
            items[i] = Exp::Declare(remaining);
            i += 1;
        }
        changed = true;
    }
    changed
}

// -------------------------------------------------------------------------------------------------
// Shape predicates / sink mutators

/// True when `item` is a scope-bearing construct with exactly one inner position that touches
/// `name`. Mirrors the cases [`sink_into`] handles.
fn can_sink_into(item: &Exp, name: &str) -> bool {
    match item {
        Exp::IfElse(_, then_b, else_b) => {
            let then_uses = touches(then_b, name);
            let else_uses = match else_b.as_ref().as_ref() {
                Some(e) => touches(e, name),
                None => false,
            };
            // Exactly one arm uses the name.
            then_uses ^ else_uses
        }
        Exp::Loop(_, body) | Exp::Block(_, body) => touches(body, name),
        Exp::While(_, cond, body) => {
            // `cond` reading the name means the `let X;` would need to live above the
            // `while` so the cond's read sees it. Don't sink in that case.
            !touches(cond, name) && touches(body, name)
        }
        Exp::Seq(inner) => {
            // The Seq itself is a scope; we can recursively sink, provided the sink target
            // inside is identifiable. Match the same single-user constraint.
            let user_count = inner
                .iter()
                .filter(|x| liveness::referenced_names(x).contains(name))
                .count();
            user_count == 1
        }
        _ => false,
    }
}

/// Sink `Declare([name])` into the single sub-position [`can_sink_into`] identified. Returns
/// `true` on success; `false` if the shape no longer matches (defensive - the caller already
/// checked via `can_sink_into`).
fn sink_into(item: &mut Exp, name: &str) -> bool {
    match item {
        Exp::IfElse(_, then_b, else_b) => {
            let then_uses = touches(then_b, name);
            let else_uses = match else_b.as_ref().as_ref() {
                Some(e) => touches(e, name),
                None => false,
            };
            if then_uses && !else_uses {
                prepend_declare(then_b, name);
                return true;
            }
            if !then_uses
                && else_uses
                && let Some(alt) = else_b.as_mut().as_mut()
            {
                prepend_declare(alt, name);
                return true;
            }
            false
        }
        Exp::Loop(_, body) | Exp::Block(_, body) => {
            if touches(body, name) {
                prepend_declare(body, name);
                return true;
            }
            false
        }
        Exp::While(_, cond, body) => {
            if !touches(cond, name) && touches(body, name) {
                prepend_declare(body, name);
                return true;
            }
            false
        }
        Exp::Seq(_) => {
            // Recur into the single user inside this nested Seq.
            let Exp::Seq(inner) = item else {
                unreachable!()
            };
            let users: Vec<usize> = inner
                .iter()
                .enumerate()
                .filter(|(_, x)| liveness::referenced_names(x).contains(name))
                .map(|(j, _)| j)
                .collect();
            if users.len() != 1 {
                return false;
            }
            let j = users[0];
            if can_sink_into(&inner[j], name) {
                sink_into(&mut inner[j], name)
            } else {
                // Single sibling but it's not a recognized scope-bearer - prepend the
                // Declare immediately before it inside this Seq so `fuse_let` can then
                // collapse the pair.
                inner.insert(j, Exp::Declare(vec![name.to_string()]));
                true
            }
        }
        _ => false,
    }
}

/// Prepend `Declare([name])` to `exp`. If `exp` is already a `Seq`, push at front;
/// otherwise wrap into a `Seq` of two items.
fn prepend_declare(exp: &mut Exp, name: &str) {
    let owned = std::mem::replace(exp, Exp::Seq(vec![]));
    let declare = Exp::Declare(vec![name.to_string()]);
    *exp = match owned {
        Exp::Seq(mut items) => {
            items.insert(0, declare);
            Exp::Seq(items)
        }
        other => Exp::Seq(vec![declare, other]),
    };
}

fn touches(exp: &Exp, name: &str) -> bool {
    referenced_names_contains(exp, name)
}

/// Faster than building the full `BTreeSet` for one membership check; bails on first hit.
fn referenced_names_contains(exp: &Exp, target: &str) -> bool {
    match exp {
        Exp::Variable(n) => n == target,
        Exp::Declare(names) => names.iter().any(|n| n == target),
        Exp::Assign(targets, rhs) => {
            targets.iter().any(|n| n == target) || referenced_names_contains(rhs, target)
        }
        Exp::LetBind(targets, rhs) => {
            targets.iter().any(|n| n == target) || referenced_names_contains(rhs, target)
        }
        Exp::Seq(items) => items.iter().any(|e| referenced_names_contains(e, target)),
        Exp::IfElse(c, t, e) => {
            referenced_names_contains(c, target)
                || referenced_names_contains(t, target)
                || e.as_ref()
                    .as_ref()
                    .is_some_and(|alt| referenced_names_contains(alt, target))
        }
        Exp::Loop(_, body) | Exp::Block(_, body) => referenced_names_contains(body, target),
        Exp::While(_, c, body) => {
            referenced_names_contains(c, target) || referenced_names_contains(body, target)
        }
        Exp::Switch(e, _, arms) => {
            referenced_names_contains(e, target)
                || arms
                    .iter()
                    .any(|(_, b)| referenced_names_contains(b, target))
        }
        Exp::Match(e, _, arms) => {
            referenced_names_contains(e, target)
                || arms
                    .iter()
                    .any(|(_, _, b)| referenced_names_contains(b, target))
        }
        Exp::MatchLit(e, arms) => {
            referenced_names_contains(e, target)
                || arms
                    .iter()
                    .any(|(_, b)| referenced_names_contains(b, target))
        }
        Exp::Return(items) => items.iter().any(|e| referenced_names_contains(e, target)),
        Exp::Call(_, args) => args.iter().any(|e| referenced_names_contains(e, target)),
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            args.iter().any(|e| referenced_names_contains(e, target))
        }
        Exp::Borrow(_, body) => referenced_names_contains(body, target),
        Exp::Abort(e) => referenced_names_contains(e, target),
        Exp::VecUnpack(_, e) => referenced_names_contains(e, target),
        Exp::Unpack(_, _, e) => referenced_names_contains(e, target),
        Exp::UnpackVariant(_, _, _, e) => referenced_names_contains(e, target),
        Exp::Unstructured(nodes) => nodes.iter().any(|node| match node {
            UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) => {
                referenced_names_contains(body, target)
            }
            UnstructuredNode::Goto(_) => false,
        }),
        Exp::Break(_) | Exp::Continue(_) | Exp::Value(_) | Exp::Constant(_) => false,
    }
}
