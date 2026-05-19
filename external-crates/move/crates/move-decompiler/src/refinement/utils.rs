// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Helpers shared between refinements.

use crate::ast::{Exp, Label};
use move_stackless_bytecode_2::ast::PrimitiveOp;

/// Look through any `Exp::Block` wrappers to reach the inner expression. Used by refinements
/// whose pattern matching cares about the underlying form, not block delimiters. `Block`
/// carries a block ID for goto cross-referencing; refinements that aren't tracking block
/// boundaries (most of them) want the inner shape.
pub(super) fn peek(exp: &Exp) -> &Exp {
    match exp {
        Exp::Block(_, body) => peek(body),
        _ => exp,
    }
}

pub(super) fn peek_mut(exp: &mut Exp) -> &mut Exp {
    match exp {
        Exp::Block(_, body) => peek_mut(body),
        _ => exp,
    }
}

/// Owned counterpart to `peek`: consume any outer `Block` wrappers and return the inner
/// expression. Used when a refinement needs to destructure (move out of) the value, dropping
/// the block ID (typically because the surrounding control flow is being rewritten).
pub(super) fn unwrap_block(exp: Exp) -> Exp {
    match exp {
        Exp::Block(_, body) => unwrap_block(*body),
        e => e,
    }
}

/// Negate a boolean expression. Strips a single outer `!` if present, otherwise wraps in `!`.
//
// TODO: simplify double negation, De Morgan, etc.
pub(super) fn negate(exp: &mut Exp) {
    use Exp as E;
    match exp {
        E::Primitive { op, args } if *op == PrimitiveOp::Not && args.len() == 1 => {
            *exp = args.pop().unwrap();
        }
        _ => {
            *exp = Exp::Primitive {
                op: PrimitiveOp::Not,
                args: vec![exp.clone()],
            };
        }
    }
}

/// Unify `Exp::Loop(label, body)` and `Exp::While(label, _, body)`: if `exp` is one of them
/// and its body is a `Seq`, return the loop's label and a mutable reference to the body's
/// items. Returns `None` otherwise (including loops whose body has been collapsed to a
/// single non-`Seq` `Exp`).
///
/// `introduce_while` runs before the swap refinements, so by the time they look any
/// already-promoted `While` would be invisible without this helper. We need to match both.
pub(super) fn loop_body_seq_mut(exp: &mut Exp) -> Option<(Option<Label>, &mut Vec<Exp>)> {
    let (label, body) = match exp {
        Exp::Loop(label, body) => (*label, body),
        Exp::While(label, _, body) => (*label, body),
        _ => return None,
    };
    match body.as_mut() {
        Exp::Seq(seq) => Some((label, seq)),
        _ => None,
    }
}

// -------------------------------------------------------------------------------------------------
// IfElse / continue tail-position helpers
//
// Shared across the swap-with-break, swap-with-fallthrough, and hoist-dual-continue
// refinements. Each treats a `Continue` sitting at the trailing position of an arm body as
// the canonical relocation target; these helpers locate it and reshape the surrounding
// structure consistently.

/// True iff `else_b` is missing (`None`) or an empty `Seq` — the shapes the swap-* rules
/// treat as "no else-arm." A non-empty else is the hoist-dual-continue rule's territory
/// (when it also ends in `continue`) or out of scope.
pub(super) fn else_is_empty_or_missing(else_b: Option<&Exp>) -> bool {
    match else_b {
        None => true,
        Some(Exp::Seq(items)) => items.is_empty(),
        Some(_) => false,
    }
}

/// True iff the final statement of `exp` is `Continue(label)` matching `expected`. Walks
/// the last item of `Seq`; doesn't descend into `IfElse`/`Switch`/etc.
pub(super) fn ends_with_continue(exp: &Exp, expected: Option<Label>) -> bool {
    match exp {
        Exp::Continue(l) => *l == expected,
        Exp::Seq(items) => items
            .last()
            .is_some_and(|last| ends_with_continue(last, expected)),
        _ => false,
    }
}

/// If `exp`'s last statement is `Continue(L)`, return `L`. Otherwise return `None`. Used
/// by `hoist_dual_continue` to detect a shared trailing continue and recover its label
/// without committing to which loop encloses us.
pub(super) fn trailing_continue_label(exp: &Exp) -> Option<Option<Label>> {
    match exp {
        Exp::Continue(l) => Some(*l),
        Exp::Seq(items) => items.last().and_then(trailing_continue_label),
        _ => None,
    }
}

/// Consume `exp` (typically an arm body), drop a trailing `Continue`, and return the
/// remaining leading items as a `Vec<Exp>`. Callers splice them back as they wish — extend
/// an enclosing `Seq` directly, or rebuild a single `Exp` via `seq_or_singleton`. The
/// continue's identity is the caller's concern; this helper only handles structure.
pub(super) fn strip_trailing_continue_into_seq(exp: Exp) -> Vec<Exp> {
    match exp {
        Exp::Continue(_) => vec![],
        Exp::Seq(mut items) => {
            // Trim a single trailing `Continue` — preceding refinements have already run
            // `flatten_seq`, so nested `Seq`s aren't expected here.
            if matches!(items.last(), Some(Exp::Continue(_))) {
                items.pop();
            }
            items
        }
        other => vec![other],
    }
}

/// Reshape a `Vec<Exp>` back into a single `Exp` for arm positions: `[]` becomes
/// `Seq([])`, a singleton unwraps to its element, and longer lists stay as a `Seq`.
pub(super) fn seq_or_singleton(mut items: Vec<Exp>) -> Exp {
    match items.len() {
        0 => Exp::Seq(vec![]),
        1 => items.pop().unwrap(),
        _ => Exp::Seq(items),
    }
}
