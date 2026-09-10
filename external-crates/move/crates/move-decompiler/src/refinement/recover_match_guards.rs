// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Recover Move 2024 match guards (`pattern if (cond) => body`), compiled away as an `if`
//! in the arm body:
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
//! The same-variant by-value re-unpack is the discriminator: a source-level `if` never
//! generates one, a compiled guard must. Else-if chains split into a guarded arm per
//! level; the final else is the unguarded fallback. Alias `let`s staged before the test
//! substitute away. Fieldless patterns, tails without the re-unpack, and reassigned
//! aliases decline, keeping the `if`-in-arm shape.

use crate::{
    ast::{Exp, MatchArm},
    refinement::{
        Refine,
        utils::{assigns_name, first_stmt, negate, peek, substitute_free},
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
        let mut new_arms = Vec::with_capacity(arms.len());
        let mut changed = false;
        for arm in std::mem::take(arms) {
            changed |= split_arm(arm, &mut new_arms);
        }
        *arms = new_arms;
        changed
    }
}

// -------------------------------------------------------------------------------------------------
// Types

/// A recognized guard-shaped arm body, borrowed from the arm: the alias prefix, then the
/// guard tail.
struct Guarded<'a> {
    /// Alias name and the pattern binding it copies, in prefix order.
    aliases: Vec<(&'a String, &'a Exp)>,
    tail: Tail<'a>,
}

/// The guard tail, normalized across its compiled forms ([`guard_tail`]).
struct Tail<'a> {
    guard: &'a Exp,
    /// The compiled `g || e` form carries the guard's negation.
    negated: bool,
    /// The branch taken when the guard passes.
    taken: &'a Exp,
    fallback: Fallback<'a>,
}

/// The guard-fail body: an explicit else, or the literal a boolean collapse elided.
enum Fallback<'a> {
    Body(&'a Exp),
    Elided(bool),
}

impl Tail<'_> {
    /// Any `Assign` to `name` anywhere in the tail.
    fn assigns(&self, name: &str) -> bool {
        assigns_name(self.guard, name)
            || assigns_name(self.taken, name)
            || match self.fallback {
                Fallback::Body(e) => assigns_name(e, name),
                Fallback::Elided(_) => false,
            }
    }
}

// -------------------------------------------------------------------------------------------------
// Recognition

/// Recognize a guard-shaped arm body (see the module doc); `None` declines the arm.
fn recognize(arm: &MatchArm) -> Option<Guarded<'_>> {
    if arm.guard.is_some() || arm.fields.is_empty() {
        return None;
    }
    let mut body = peek(&arm.rhs);
    let mut aliases = Vec::new();
    if let Exp::Seq(items) = body {
        let (last, prefix) = items.split_last()?;
        for stmt in prefix {
            aliases.push(alias_of_binding(peek(stmt), &arm.fields)?);
        }
        body = peek(last);
    }
    let tail = guard_tail(body, arm.variant)?;
    // Splitting deletes the alias `let`s, which would orphan any assignment to an alias
    // name in the tail. Deliberately not shadow-aware: over-declining keeps the safe
    // `if`-in-arm fallback.
    if aliases.iter().any(|(alias, _)| tail.assigns(alias)) {
        return None;
    }
    Some(Guarded { aliases, tail })
}

/// Parse the guard tail in every form it survives refinement; the re-unpacking branch is
/// the guard-taken one:
///
///   * `if (g) { conseq } else { alt }` with a re-unpacking conseq;
///   * `g && e`: collapsed `if (g) { e } else { false }`;
///   * `g || e`: collapsed `if (!g) { true } else { e }`.
fn guard_tail(body: &Exp, variant: Symbol) -> Option<Tail<'_>> {
    match body {
        Exp::IfElse(guard, taken, fallback) => {
            let fallback = fallback.as_ref().as_ref()?;
            if !reunpacks_variant(taken, variant) {
                return None;
            }
            Some(Tail {
                guard,
                negated: false,
                taken,
                fallback: Fallback::Body(fallback),
            })
        }
        Exp::Primitive {
            op: PrimitiveOp::And,
            args,
        } => {
            let [guard, taken] = &args[..] else {
                return None;
            };
            if !reunpacks_variant(taken, variant) {
                return None;
            }
            Some(Tail {
                guard,
                negated: false,
                taken,
                fallback: Fallback::Elided(false),
            })
        }
        Exp::Primitive {
            op: PrimitiveOp::Or,
            args,
        } => {
            let [guard, taken] = &args[..] else {
                return None;
            };
            if !reunpacks_variant(taken, variant) {
                return None;
            }
            Some(Tail {
                guard,
                negated: true,
                taken,
                fallback: Fallback::Elided(true),
            })
        }
        _ => None,
    }
}

/// `let alias = binding;` where `binding` is one of the arm's pattern bindings.
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
        .then_some((alias, rhs.as_ref()))
}

/// Does `branch` start with a by-value re-unpack of `variant`?
fn reunpacks_variant(branch: &Exp, variant: Symbol) -> bool {
    use crate::ast::UnpackKind;
    matches!(
        first_stmt(branch),
        Exp::UnpackVariant(UnpackKind::Value, (_, v), _, _) if *v == variant
    )
}

// -------------------------------------------------------------------------------------------------
// Splitting

/// Split a guard-shaped arm into one guarded arm per `if`/`else if` level plus an unguarded
/// arm for the final else; any other arm passes through untouched. Returns whether it split.
fn split_arm(arm: MatchArm, out: &mut Vec<MatchArm>) -> bool {
    use move_core_types::runtime_value::MoveValue;
    let Some(Guarded { aliases, tail }) = recognize(&arm) else {
        out.push(arm);
        return false;
    };

    let mut guard = tail.guard.clone();
    if tail.negated {
        negate(&mut guard);
    }
    let mut taken = tail.taken.clone();
    let mut fallback = match tail.fallback {
        Fallback::Body(e) => e.clone(),
        Fallback::Elided(b) => Exp::Value(MoveValue::Bool(b)),
    };
    // Later aliases first: a duplicate alias name resolves to its inner binding.
    for (alias, binding) in aliases.iter().rev() {
        substitute_free(&mut guard, alias, binding);
        substitute_free(&mut taken, alias, binding);
        substitute_free(&mut fallback, alias, binding);
    }

    out.push(MatchArm {
        variant: arm.variant,
        fields: arm.fields.clone(),
        guard: Some(guard),
        rhs: taken,
    });
    split_arm(
        MatchArm {
            variant: arm.variant,
            fields: arm.fields.clone(),
            guard: None,
            rhs: fallback,
        },
        out,
    );
    true
}
