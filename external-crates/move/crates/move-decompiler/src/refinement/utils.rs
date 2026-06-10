// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Helpers shared between refinements.

use crate::ast::{Exp, Label, UnstructuredNode};
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

/// An expression `always_terminates` if every CFG path through it leaves the surrounding
/// statement context - `abort`/`return`/`break`/`continue`, a `Seq` whose last item does, an
/// `IfElse` whose both arms do, or a `Block` whose body does.
pub(super) fn always_terminates(exp: &Exp) -> bool {
    match exp {
        Exp::Abort(_) | Exp::Return(_) | Exp::Break(_) | Exp::Continue(_) => true,
        Exp::Seq(items) => items.last().is_some_and(always_terminates),
        Exp::IfElse(_, t, alt) => {
            always_terminates(t.as_ref()) && alt.as_ref().as_ref().is_some_and(always_terminates)
        }
        Exp::Block(_, body) => always_terminates(body),
        _ => false,
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

/// True iff `else_b` is missing (`None`) or an empty `Seq` - the shapes the swap-* rules
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
/// remaining leading items as a `Vec<Exp>`. Callers splice them back as they wish - extend
/// an enclosing `Seq` directly, or rebuild a single `Exp` via `seq_or_singleton`. The
/// continue's identity is the caller's concern; this helper only handles structure.
///
/// Recurses through nested `Seq`s the same way [`ends_with_continue`] does - they need to
/// agree on what counts as "trailing." When `flatten_seq` hasn't yet collapsed an
/// intermediate `Seq` (e.g., a single iteration may flatten only the outermost nesting
/// level before `hoist_tail_continue` runs), the detector still recurs to find the
/// continue and the stripper needs to actually remove it. Without this symmetry, the two
/// disagree, and `hoist_tail_continue` pushes a new continue at the loop tail without
/// removing the original - fueling a refinement ping-pong.
pub(super) fn strip_trailing_continue_into_seq(exp: Exp) -> Vec<Exp> {
    match exp {
        Exp::Continue(_) => vec![],
        Exp::Seq(mut items) => match items.pop() {
            None => items,
            Some(Exp::Continue(_)) => items,
            Some(last) => {
                // Recurse into nested sequencing so a continue inside a not-yet-flattened
                // inner Seq (or Block) gets removed, then wrap the recursive result back so
                // the surrounding shape is preserved.
                let stripped = strip_trailing_continue_into_seq(last);
                items.push(seq_or_singleton(stripped));
                items
            }
        },
        Exp::Block(id, body) => {
            let stripped = strip_trailing_continue_into_seq(*body);
            vec![Exp::Block(id, Box::new(seq_or_singleton(stripped)))]
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

// -------------------------------------------------------------------------------------------------
// Tree walkers + goto rewrites
// -------------------------------------------------------------------------------------------------

/// Call `f` on each direct child expression of `exp`. Used by refinements that walk the AST
/// post-order without consuming it. Skips `Unstructured(Goto)` (no body) and visits
/// `Unstructured(Labeled|Statement)` bodies as children.
pub(super) fn walk_children(exp: &Exp, f: &mut impl FnMut(&Exp)) {
    match exp {
        Exp::Loop(_, b)
        | Exp::Block(_, b)
        | Exp::Assign(_, b)
        | Exp::LetBind(_, b)
        | Exp::Abort(b)
        | Exp::Borrow(_, b)
        | Exp::Unpack(_, _, b)
        | Exp::UnpackVariant(_, _, _, b)
        | Exp::VecUnpack(_, b) => f(b),
        Exp::While(_, c, b) => {
            f(c);
            f(b);
        }
        Exp::IfElse(c, t, alt) => {
            f(c);
            f(t);
            if let Some(a) = alt.as_ref().as_ref() {
                f(a);
            }
        }
        Exp::Switch(c, _, arms) => {
            f(c);
            for (_, e) in arms {
                f(e);
            }
        }
        Exp::Match(c, _, arms) => {
            f(c);
            for (_, _, e) in arms {
                f(e);
            }
        }
        Exp::MatchLit(c, arms) => {
            f(c);
            for (_, e) in arms {
                f(e);
            }
        }
        Exp::Seq(es) | Exp::Return(es) | Exp::Call(_, es) => {
            for e in es {
                f(e);
            }
        }
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            for a in args {
                f(a);
            }
        }
        Exp::Unstructured(nodes) => {
            for node in nodes {
                if let UnstructuredNode::Labeled(_, b) | UnstructuredNode::Statement(b) = node {
                    f(b);
                }
            }
        }
        Exp::Break(_)
        | Exp::Continue(_)
        | Exp::Declare(_)
        | Exp::Value(_)
        | Exp::Variable(_)
        | Exp::Constant(_) => {}
    }
}

/// Mutable counterpart to [`walk_children`]. Same traversal shape.
pub(super) fn walk_children_mut(exp: &mut Exp, f: &mut impl FnMut(&mut Exp)) {
    match exp {
        Exp::Loop(_, b)
        | Exp::Block(_, b)
        | Exp::Assign(_, b)
        | Exp::LetBind(_, b)
        | Exp::Abort(b)
        | Exp::Borrow(_, b)
        | Exp::Unpack(_, _, b)
        | Exp::UnpackVariant(_, _, _, b)
        | Exp::VecUnpack(_, b) => f(b),
        Exp::While(_, c, b) => {
            f(c);
            f(b);
        }
        Exp::IfElse(c, t, alt) => {
            f(c);
            f(t);
            if let Some(a) = alt.as_mut().as_mut() {
                f(a);
            }
        }
        Exp::Switch(c, _, arms) => {
            f(c);
            for (_, e) in arms {
                f(e);
            }
        }
        Exp::Match(c, _, arms) => {
            f(c);
            for (_, _, e) in arms {
                f(e);
            }
        }
        Exp::MatchLit(c, arms) => {
            f(c);
            for (_, e) in arms {
                f(e);
            }
        }
        Exp::Seq(es) | Exp::Return(es) | Exp::Call(_, es) => {
            for e in es {
                f(e);
            }
        }
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            for a in args {
                f(a);
            }
        }
        Exp::Unstructured(nodes) => {
            for node in nodes {
                if let UnstructuredNode::Labeled(_, b) | UnstructuredNode::Statement(b) = node {
                    f(b);
                }
            }
        }
        Exp::Break(_)
        | Exp::Continue(_)
        | Exp::Declare(_)
        | Exp::Value(_)
        | Exp::Variable(_)
        | Exp::Constant(_) => {}
    }
}

/// Replace every `Unstructured(Goto(target))` reachable in `exp` with `Break(Some(target))`.
/// A single-goto `Unstructured` collapses to a bare `Break`; a goto among other unstructured
/// nodes becomes an `Unstructured(Statement(Break))` in place.
///
/// Two refinements use this — `goto_to_break` and `hoist_shared_landing` — both at the point
/// where they've decided some `Goto(target)` should redirect into a `Block(target, …)` wrap
/// that now encloses it.
pub(super) fn rewrite_gotos_as_breaks(exp: &mut Exp, target: Label) {
    if let Exp::Unstructured(nodes) = exp {
        for node in nodes.iter_mut() {
            if let UnstructuredNode::Goto(l) = node
                && *l == target
            {
                *node = UnstructuredNode::Statement(Box::new(Exp::Break(Some(target))));
            }
        }
        if nodes.len() == 1
            && let UnstructuredNode::Statement(b) = &nodes[0]
            && matches!(**b, Exp::Break(Some(_)))
        {
            let UnstructuredNode::Statement(b) = nodes.remove(0) else {
                unreachable!()
            };
            *exp = *b;
            return;
        }
        if let Exp::Unstructured(nodes) = exp {
            for node in nodes.iter_mut() {
                if let UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) = node
                {
                    rewrite_gotos_as_breaks(body, target);
                }
            }
        }
        return;
    }
    walk_children_mut(exp, &mut |c| rewrite_gotos_as_breaks(c, target));
}
