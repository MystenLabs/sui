// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Recover Move 2024 match guards (`pattern if (cond) => body`) from their compiled shape.
//!
//! A guarded arm is compiled as:
//!
//!   * a variant tag dispatch;
//!   * pattern field binding, by reference;
//!   * guard evaluation, with a fallthrough on failure.
//!
//! After structuring, this reads as an arm whose body is an `if` with by-value unpacking
//! after the test in both branches. This refinement recognizes that pattern and puts the
//! guard back together:
//!
//! ```text
//! Market { qty: reg_4 } => {
//!     if (*reg_4 > 100) { let Order::Market { qty: reg_9 } = l4; ...A... }
//!     else { let Order::Market { qty: reg_13 } = l4; ...B... }
//! }
//! ```
//!
//! becomes:
//!
//! ```text
//! Market { qty: reg_4 } if (*reg_4 > 100) => { let Order::Market { qty: reg_9 } = l4; ...A... },
//! Market { qty: reg_4 } => { let Order::Market { qty: reg_13 } = l4; ...B... }
//! ```
//!
//! The same-variant by-value unpack is our discriminator: a source-level `if` never
//! generates one, a compiled guard must. Else-if chains split into further guarded arms;
//! the final else becomes the unguarded fallback.
//!
//! Translation may alias a binding before the test (`let l5 = reg_6; if (*l5 > 10)`). The
//! alias folds away at split time: a guard is a bare expression, so the `let` cannot
//! survive the split.
//!
//! Recovery declines on fieldless arm patterns (recovery keys on pattern bindings, though
//! compiled fieldless guards do re-unpack, so this is recoverable future work), on
//! by-reference guarded matches whose failure paths abort (no branch re-unpacks), and when
//! the tail reassigns an alias local, whose `let` the split would delete. Declined arms
//! keep the `if`-in-arm shape.

use crate::{
    ast::{Exp, MatchArm},
    refinement::{
        Refine,
        utils::{negate, peek, unwrap_block},
    },
};

use move_stackless_bytecode_2::ast::PrimitiveOp;
use move_symbol_pool::Symbol;

pub fn refine(exp: &mut Exp) -> bool {
    RecoverMatchGuards.refine(exp)
}

struct RecoverMatchGuards;

impl Refine for RecoverMatchGuards {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Match(_, _, arms) = exp else {
            return false;
        };
        if !arms.iter().any(splittable) {
            return false;
        }
        let mut new_arms = Vec::with_capacity(arms.len() + 1);
        for arm in std::mem::take(arms) {
            match vetted_shape(&arm) {
                Some(shape) => split_arm(arm, shape, &mut new_arms),
                None => new_arms.push(arm),
            }
        }
        *arms = new_arms;
        true
    }
}

// -------------------------------------------------------------------------------------------------
// Shape recognition

/// A vetted guard-shaped arm body: `prefix_len` leading alias statements, then the tail.
struct GuardShape {
    prefix_len: usize,
}

fn splittable(arm: &MatchArm) -> bool {
    vetted_shape(arm).is_some()
}

fn vetted_shape(arm: &MatchArm) -> Option<GuardShape> {
    (arm.guard.is_none() && !arm.fields.is_empty())
        .then(|| guard_shape(arm))
        .flatten()
}

/// The arm body is guard-shaped: an optional alias prefix, then a single guard tail
/// (see [`is_guard_tail`]).
fn guard_shape(arm: &MatchArm) -> Option<GuardShape> {
    let mut body = peek(&arm.rhs);
    let mut aliases = Vec::new();
    let mut prefix_len = 0;
    if let Exp::Seq(items) = body {
        while prefix_len < items.len() {
            let Some((alias, _)) = alias_of_binding(peek(&items[prefix_len]), &arm.fields) else {
                break;
            };
            aliases.push(alias);
            prefix_len += 1;
        }
        if prefix_len + 1 != items.len() {
            return None;
        }
        body = peek(&items[prefix_len]);
    }
    if !is_guard_tail(body, arm.variant) {
        return None;
    }
    // Splitting deletes the alias `let`s, which would orphan any assignment to an alias
    // name in the tail. Deliberately not shadow-aware: over-declining keeps the safe
    // `if`-in-arm fallback.
    if aliases.iter().any(|alias| assigns_name(body, alias)) {
        return None;
    }
    Some(GuardShape { prefix_len })
}

/// The guard test in every form it survives refinement; the re-unpacking branch is the
/// guard-taken one:
///
///   * `if (g) { conseq } else { alt }` with a re-unpacking conseq;
///   * `g && e`: collapsed `if (g) { e } else { false }`;
///   * `g || e`: collapsed `if (!g) { true } else { e }`.
fn is_guard_tail(body: &Exp, variant: Symbol) -> bool {
    match body {
        Exp::IfElse(_, conseq, alt) => alt.as_ref().is_some() && reunpacks_variant(conseq, variant),
        Exp::Primitive {
            op: PrimitiveOp::And | PrimitiveOp::Or,
            args,
        } => matches!(&args[..], [_, e] if reunpacks_variant(e, variant)),
        _ => false,
    }
}

/// `let alias = binding;` where `binding` is one of the arm's pattern bindings. Returns
/// `(alias, replacement)` for substitution.
fn alias_of_binding<'a>(
    stmt: &'a Exp,
    fields: &[(Symbol, String)],
) -> Option<(&'a String, &'a Exp)> {
    let Exp::LetBind(lhs, rhs) = stmt else {
        return None;
    };
    let [alias] = &lhs[..] else {
        return None;
    };
    let Exp::Variable(used) = rhs.as_ref() else {
        return None;
    };
    fields
        .iter()
        .any(|(_, binding)| binding == used)
        .then_some((alias, rhs))
}

/// Does `branch` start (through `Block` wrappers and a leading `Seq` position) with a
/// by-value re-unpack of `variant`?
fn reunpacks_variant(branch: &Exp, variant: Symbol) -> bool {
    use crate::ast::UnpackKind;
    match peek(branch) {
        Exp::UnpackVariant(UnpackKind::Value, (_, v), _, _) => *v == variant,
        Exp::Seq(items) => matches!(
            items.first().map(peek),
            Some(Exp::UnpackVariant(UnpackKind::Value, (_, v), _, _)) if *v == variant
        ),
        _ => false,
    }
}

// -------------------------------------------------------------------------------------------------
// Splitting

/// Split into one guarded arm per `if`/`else if` level plus an unguarded arm for the final
/// else. `shape` is the arm's vetted shape from [`vetted_shape`].
fn split_arm(arm: MatchArm, shape: GuardShape, out: &mut Vec<MatchArm>) {
    let MatchArm {
        variant,
        fields,
        guard: _,
        rhs,
    } = arm;
    let mut body = unwrap_block(rhs);

    // Substitute each alias with the binding it copies before splitting.
    body = match body {
        Exp::Seq(mut items) => {
            debug_assert_eq!(items.len(), shape.prefix_len + 1);
            let mut tail = unwrap_block(items.pop().expect("vetted: prefix plus tail"));
            for stmt in items.into_iter().rev() {
                let (alias, replacement) = match unwrap_block(stmt) {
                    Exp::LetBind(mut names, rhs) => {
                        (names.pop().expect("vetted: single-binder alias"), *rhs)
                    }
                    _ => unreachable!("vetted: the prefix is alias `let`s"),
                };
                substitute_free(&mut tail, &alias, &replacement);
            }
            tail
        }
        other => other,
    };

    let (guard, conseq, else_body) = destructure_guard_tail(body);
    out.push(MatchArm {
        variant,
        fields: fields.clone(),
        guard: Some(guard),
        rhs: conseq,
    });

    let rest = MatchArm {
        variant,
        fields,
        guard: None,
        rhs: else_body,
    };
    match vetted_shape(&rest) {
        Some(shape) => split_arm(rest, shape, out),
        None => out.push(rest),
    }
}

/// Take a vetted guard tail apart into `(guard, taken-branch, fallback)`. Boolean-collapsed
/// forms restore the elided literal; `g || e` un-negates the guard.
fn destructure_guard_tail(body: Exp) -> (Exp, Exp, Exp) {
    use move_core_types::runtime_value::MoveValue;
    match body {
        Exp::IfElse(guard, conseq, alt) => {
            let else_body = alt.expect("is_guard_tail checked the else");
            (*guard, *conseq, else_body)
        }
        Exp::Primitive { op, mut args } => {
            let conseq = args.pop().expect("is_guard_tail checked the arity");
            let mut guard = args.pop().expect("is_guard_tail checked the arity");
            let elided = match op {
                PrimitiveOp::And => false,
                PrimitiveOp::Or => {
                    negate(&mut guard);
                    true
                }
                _ => unreachable!("is_guard_tail admits only And/Or"),
            };
            (guard, conseq, Exp::Value(MoveValue::Bool(elided)))
        }
        _ => unreachable!("is_guard_tail admits only IfElse and And/Or"),
    }
}

/// Replace every *free* `Variable(name)` in `exp` with `replacement`: a statement that
/// rebinds `name` shadows the rest of its statement list, and a match arm whose pattern
/// binds `name` shadows its guard and body.
fn substitute_free(exp: &mut Exp, name: &str, replacement: &Exp) {
    struct Subst<'a> {
        name: &'a str,
        replacement: &'a Exp,
    }
    impl Refine for Subst<'_> {
        fn refine_custom(&mut self, exp: &mut Exp) -> bool {
            match exp {
                Exp::Variable(n) if n == self.name => {
                    *exp = self.replacement.clone();
                    true
                }
                Exp::Match(subject, _, arms) => {
                    self.refine(subject);
                    for arm in arms {
                        if arm.fields.iter().any(|(_, binding)| binding == self.name) {
                            continue;
                        }
                        arm.guard.iter_mut().for_each(|g| {
                            self.refine(g);
                        });
                        self.refine(&mut arm.rhs);
                    }
                    true
                }
                _ => false,
            }
        }

        fn refine_seq(&mut self, exps: &mut Vec<Exp>) -> bool {
            for exp in exps.iter_mut() {
                self.refine(exp);
                if rebinds(exp, self.name) {
                    break;
                }
            }
            false
        }
    }
    let _ = Subst { name, replacement }.refine(exp);
}

/// Does `stmt` rebind `name` for the statements after it? `Block` contents are spliced
/// into the enclosing statement list, so a rebinding anywhere in one leaks out; a bare
/// `Seq` renders braced and does not.
fn rebinds(stmt: &Exp, name: &str) -> bool {
    match stmt {
        Exp::Block(_, body) => rebinds_spliced(body, name),
        Exp::LetBind(names, _) | Exp::Declare(names) | Exp::VecUnpack(names, _) => {
            names.iter().any(|n| n == name)
        }
        Exp::Unpack(_, fields, _) | Exp::UnpackVariant(_, _, fields, _) => {
            fields.iter().any(|(_, n)| n == name)
        }
        _ => false,
    }
}

fn rebinds_spliced(exp: &Exp, name: &str) -> bool {
    match exp {
        Exp::Seq(items) => items.iter().any(|item| rebinds(item, name)),
        other => rebinds(other, name),
    }
}

/// Any `Assign` targeting `name` anywhere in `exp`.
fn assigns_name(exp: &Exp, name: &str) -> bool {
    use crate::ast::UnstructuredNode;
    match exp {
        Exp::Assign(targets, rhs) => targets.iter().any(|t| t == name) || assigns_name(rhs, name),
        Exp::Break(_)
        | Exp::Continue(_)
        | Exp::Declare(_)
        | Exp::Value(_)
        | Exp::Variable(_)
        | Exp::Constant(_) => false,
        Exp::Loop(_, e)
        | Exp::LetBind(_, e)
        | Exp::Abort(e)
        | Exp::Borrow(_, e)
        | Exp::VecUnpack(_, e)
        | Exp::Unpack(_, _, e)
        | Exp::UnpackVariant(_, _, _, e)
        | Exp::Block(_, e) => assigns_name(e, name),
        Exp::While(_, cond, body) => assigns_name(cond, name) || assigns_name(body, name),
        Exp::IfElse(cond, conseq, alt) => {
            assigns_name(cond, name)
                || assigns_name(conseq, name)
                || alt.as_ref().as_ref().is_some_and(|a| assigns_name(a, name))
        }
        Exp::Seq(items) | Exp::Return(items) | Exp::Call(_, items) => {
            items.iter().any(|e| assigns_name(e, name))
        }
        Exp::Primitive { args, .. } | Exp::Data { args, .. } => {
            args.iter().any(|e| assigns_name(e, name))
        }
        Exp::Switch(subject, _, arms) => {
            assigns_name(subject, name) || arms.iter().any(|(_, e)| assigns_name(e, name))
        }
        Exp::Match(subject, _, arms) => {
            assigns_name(subject, name)
                || arms.iter().any(|arm| {
                    arm.guard.as_ref().is_some_and(|g| assigns_name(g, name))
                        || assigns_name(&arm.rhs, name)
                })
        }
        Exp::MatchLit(subject, arms) => {
            assigns_name(subject, name) || arms.iter().any(|(_, e)| assigns_name(e, name))
        }
        Exp::Unstructured(nodes) => nodes.iter().any(|node| match node {
            UnstructuredNode::Labeled(_, body) | UnstructuredNode::Statement(body) => {
                assigns_name(body, name)
            }
            UnstructuredNode::Goto(_) => false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{TypeRef, UnpackKind};
    use move_core_types::runtime_value::MoveValue;

    fn var(n: &str) -> Exp {
        Exp::Variable(n.to_string())
    }
    fn letb(name: &str, rhs: Exp) -> Exp {
        Exp::LetBind(vec![name.to_string()], Box::new(rhs))
    }
    fn assign(name: &str, rhs: Exp) -> Exp {
        Exp::Assign(vec![name.to_string()], Box::new(rhs))
    }
    fn num() -> Exp {
        Exp::Value(MoveValue::U64(0))
    }
    fn reunpack(binding: &str) -> Exp {
        Exp::UnpackVariant(
            UnpackKind::Value,
            (
                TypeRef::Aliased(move_symbol_pool::Symbol::from("Order")),
                move_symbol_pool::Symbol::from("Market"),
            ),
            vec![(move_symbol_pool::Symbol::from("qty"), binding.to_string())],
            Box::new(var("l4")),
        )
    }
    fn market_arm(rhs: Exp) -> Exp {
        Exp::Match(
            Box::new(var("l4")),
            TypeRef::Aliased(move_symbol_pool::Symbol::from("Order")),
            vec![MatchArm {
                variant: move_symbol_pool::Symbol::from("Market"),
                fields: vec![(move_symbol_pool::Symbol::from("qty"), "reg_4".to_string())],
                guard: None,
                rhs,
            }],
        )
    }

    /// `let l5 = reg_4; if (l5) { <unpack>; let l5 = other; l5 } else { <unpack> }`:
    /// the guard's `l5` substitutes to `reg_4`; the use after the rebinding must not.
    #[test]
    fn substitution_stops_at_rebinding() {
        let conseq = Exp::Seq(vec![reunpack("reg_9"), letb("l5", var("other")), var("l5")]);
        let body = Exp::Seq(vec![
            letb("l5", var("reg_4")),
            Exp::IfElse(
                Box::new(var("l5")),
                Box::new(conseq),
                Box::new(Some(reunpack("reg_13"))),
            ),
        ]);
        let mut exp = market_arm(body);
        assert!(refine(&mut exp));
        let Exp::Match(_, _, arms) = &exp else {
            panic!("expected a Match");
        };
        assert_eq!(arms.len(), 2);
        assert!(matches!(&arms[0].guard, Some(Exp::Variable(n)) if n == "reg_4"));
        let Exp::Seq(items) = &arms[0].rhs else {
            panic!("expected the taken branch");
        };
        assert!(matches!(&items[2], Exp::Variable(n) if n == "l5"));
    }

    /// `let l5 = reg_4; if (l5) { <unpack>; l5 = e; } else { <unpack> }`: splitting would
    /// delete the alias `let` out from under the assignment, so recovery declines.
    #[test]
    fn reassigned_alias_declines() {
        let conseq = Exp::Seq(vec![reunpack("reg_9"), assign("l5", num())]);
        let body = Exp::Seq(vec![
            letb("l5", var("reg_4")),
            Exp::IfElse(
                Box::new(var("l5")),
                Box::new(conseq),
                Box::new(Some(reunpack("reg_13"))),
            ),
        ]);
        let mut exp = market_arm(body);
        assert!(!refine(&mut exp));
        let Exp::Match(_, _, arms) = &exp else {
            panic!("expected a Match");
        };
        assert_eq!(arms.len(), 1);
        assert!(arms[0].guard.is_none());
    }
}
