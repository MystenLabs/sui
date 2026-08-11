// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Inline `let X = e; next` when `next`'s very first read is `X` and `X` has no other use.
//!
//! For an `Seq([..., LetBind([X], e), next, ...])` we fire when:
//!   - `X` has exactly one defining `LetBind` (this one), zero `Assign`/`Declare`/`Unpack`,
//!     and exactly one `Variable` read across the whole function.
//!   - The unique read sits at `next`'s *head position* - the first child evaluated when
//!     `next` runs. We walk `next` down its leftmost evaluation chain (`Return([head])`,
//!     `Call(_, [head, ..])`, `IfElse(head, _, _)`, etc.) and require to arrive at
//!     `Variable(X)`.
//!
//! When both hold, the binding is removed and the `Variable(X)` slot in `next` is replaced
//! with `e`. Head-position is the safety net: `e` evaluates exactly where the original
//! `let X = e` had it execute (first), so any side effects in `e` happen at the same point.
//!
//! Complements `inline_immutable_alias` (which only handles `let X = Y;` where the RHS is a
//! variable). Here `e` can be any expression - `f(...)`, a struct unpack, a literal, etc.
//! The decompiler emits many `let regN = expr; ...next...` shapes that this collapses.

use crate::{
    ast::Exp,
    refinement::{Refine, liveness::NameCounts},
};

pub fn refine(exp: &mut Exp) -> bool {
    let counts = NameCounts::analyze(exp);
    CollapseLetUsage { counts: &counts }.refine(exp)
}

struct CollapseLetUsage<'a> {
    counts: &'a NameCounts,
}

impl Refine for CollapseLetUsage<'_> {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Seq(items) = exp else {
            return false;
        };
        collapse_in_seq(items, self.counts)
    }
}

// ------------------------------------------------------------------------------------------------
// Per-Seq collapse loop

fn collapse_in_seq(items: &mut Vec<Exp>, counts: &NameCounts) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i + 1 < items.len() {
        let Exp::LetBind(targets, _rhs) = &items[i] else {
            i += 1;
            continue;
        };
        if targets.len() != 1 {
            i += 1;
            continue;
        }
        let x = targets[0].clone();
        if !is_eligible(&x, counts) {
            i += 1;
            continue;
        }
        if !head_var_is(&items[i + 1], &x) {
            i += 1;
            continue;
        }

        // Commit: pull the binding out and substitute its RHS into the next item's head slot.
        let Exp::LetBind(_, rhs) = items.remove(i) else {
            unreachable!()
        };
        let mut payload = Some(*rhs);
        let replaced = replace_head_var(&mut items[i], &x, &mut payload);
        debug_assert!(
            replaced,
            "head_var_is returned true but replace_head_var did not"
        );
        changed = true;
        // Don't advance `i` - the new items[i] might itself be a candidate for further
        // collapsing on the next iteration of this loop.
    }
    changed
}

// ------------------------------------------------------------------------------------------------
// Eligibility

fn is_eligible(x: &str, counts: &NameCounts) -> bool {
    counts.reads(x) == 1
        && counts.letbinds(x) == 1
        && counts.assigns(x) == 0
        && counts.declares(x) == 0
        && counts.unpacks(x) == 0
}

// ------------------------------------------------------------------------------------------------
// Head-position navigation

/// Walk `exp` down its leftmost-evaluated child chain and return `true` if we land on
/// `Variable(name)`. This is the read-side mirror of `replace_head_var`.
fn head_var_is(exp: &Exp, name: &str) -> bool {
    match exp {
        Exp::Variable(n) => n == name,
        _ => head_ref(exp).is_some_and(|h| head_var_is(h, name)),
    }
}

/// Walk `exp` down its leftmost-evaluated child chain and replace the `Variable(name)` leaf
/// with the contents of `replacement` (consumed). Returns `true` iff the leaf was found.
fn replace_head_var(exp: &mut Exp, name: &str, replacement: &mut Option<Exp>) -> bool {
    if let Exp::Variable(n) = exp {
        if n == name {
            *exp = replacement.take().expect("replacement available");
            return true;
        }
        return false;
    }
    head_mut(exp).is_some_and(|h| replace_head_var(h, name, replacement))
}

/// The child of `exp` evaluated first under Move semantics, or `None` if `exp` either has no
/// children or evaluates them in an order we can't safely substitute through (control flow,
/// `Seq` between us and a clean head, `Unstructured`).
fn head_ref(exp: &Exp) -> Option<&Exp> {
    match exp {
        // Single-child wrappers: head of `e` is what flows in first.
        Exp::Abort(e)
        | Exp::Borrow(_, e)
        | Exp::VecUnpack(_, e)
        | Exp::Unpack(_, _, e)
        | Exp::UnpackVariant(_, _, _, e)
        | Exp::Block(_, e) => Some(e),
        Exp::LetBind(_, rhs) | Exp::Assign(_, rhs) => Some(rhs),
        // Multi-arg constructs: leftmost arg.
        Exp::Return(items) | Exp::Call(_, items) => items.first(),
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => args.first(),
        // Control flow: the condition / subject is what's evaluated first.
        Exp::IfElse(cond, _, _) => Some(cond),
        Exp::Switch(subj, _, _) => Some(subj),
        Exp::Match(subj, _, _) => Some(subj),
        Exp::MatchLit(subj, _) => Some(subj),
        // No safe head to substitute through:
        //   - Loop/While bodies execute repeatedly; substituting in would change frequency.
        //   - Seq has multiple items; the head idea doesn't compose cleanly here (a `Seq`
        //     immediately after the LetBind would mean substituting deep into an inner
        //     block past intermediate statements).
        //   - Leaves have no child.
        //   - Unstructured is opaque control flow.
        Exp::Loop(_, _)
        | Exp::While(_, _, _)
        | Exp::Seq(_)
        | Exp::Declare(_)
        | Exp::Value(_)
        | Exp::Variable(_)
        | Exp::Constant(_)
        | Exp::Break(_)
        | Exp::Continue(_)
        | Exp::Unstructured(_) => None,
    }
}

fn head_mut(exp: &mut Exp) -> Option<&mut Exp> {
    match exp {
        Exp::Abort(e)
        | Exp::Borrow(_, e)
        | Exp::VecUnpack(_, e)
        | Exp::Unpack(_, _, e)
        | Exp::UnpackVariant(_, _, _, e)
        | Exp::Block(_, e) => Some(e),
        Exp::LetBind(_, rhs) | Exp::Assign(_, rhs) => Some(rhs),
        Exp::Return(items) | Exp::Call(_, items) => items.first_mut(),
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => args.first_mut(),
        Exp::IfElse(cond, _, _) => Some(cond),
        Exp::Switch(subj, _, _) => Some(subj),
        Exp::Match(subj, _, _) => Some(subj),
        Exp::MatchLit(subj, _) => Some(subj),
        Exp::Loop(_, _)
        | Exp::While(_, _, _)
        | Exp::Seq(_)
        | Exp::Declare(_)
        | Exp::Value(_)
        | Exp::Variable(_)
        | Exp::Constant(_)
        | Exp::Break(_)
        | Exp::Continue(_)
        | Exp::Unstructured(_) => None,
    }
}
