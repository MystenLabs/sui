// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{ast::Exp, refinement::Refine};

use move_symbol_pool::Symbol;

pub fn refine(exp: &mut Exp) -> bool {
    ReconstructMatch.refine(exp)
}

// -------------------------------------------------------------------------------------------------
// Refinement
//
// Promote a `Switch` to a `Match` when at least one arm's body begins with an `UnpackVariant`
// for the arm's variant — that's the source-level pattern bindings showing through, and we want
// them inside the match arm's pattern:
//
//     match (e) {
//         Variant => {
//             let Variant { f: x } = e;
//             ... body ...
//         },
//     }
//
// becomes:
//
//     match (e) {
//         Variant { f: x } => { ... body ... },
//     }
//
// Per arm, only the leading unpack is lifted (later unpacks — e.g. a `&` ref-unpack followed
// by a value unpack of the original scrutinee — stay in the body). An arm whose body doesn't
// start with a matching unpack contributes an empty pattern. We only convert when *some* arm
// has fields to lift; a Switch with nothing to recover stays as `Switch`.

struct ReconstructMatch;

impl Refine for ReconstructMatch {
    fn refine_custom(&mut self, exp: &mut Exp) -> bool {
        let Exp::Switch(_, _, arms) = exp else {
            return false;
        };
        // Only convert when some arm has fields to lift; pure tag-only switches stay as
        // `Switch` so we don't churn the node for nothing.
        if !arms.iter().any(|(v, body)| has_leading_unpack(body, *v)) {
            return false;
        }
        exp.map_mut(|e| {
            let Exp::Switch(scrutinee, enum_, arms) = e else {
                unreachable!()
            };
            let arms = arms
                .into_iter()
                .map(|(variant, mut body)| {
                    let fields = take_leading_unpack(&mut body, variant).unwrap_or_default();
                    (variant, fields, body)
                })
                .collect();
            Exp::Match(scrutinee, enum_, arms)
        });
        true
    }
}

// -------------------------------------------------------------------------------------------------
// Helpers

/// True iff `body` starts (directly, or as the first item of a `Seq`) with an `UnpackVariant`
/// whose variant tag is `arm_variant`.
fn has_leading_unpack(body: &Exp, arm_variant: Symbol) -> bool {
    match body {
        Exp::UnpackVariant(_, (_, v), _, _) => *v == arm_variant,
        Exp::Seq(items) => matches!(
            items.first(),
            Some(Exp::UnpackVariant(_, (_, v), _, _)) if *v == arm_variant
        ),
        _ => false,
    }
}

/// If `body` starts with an `UnpackVariant` of `arm_variant`, remove it and return its field
/// bindings. The body either becomes the empty `Seq` (when the unpack *was* the whole body)
/// or loses just its leading statement (when the unpack sat at the head of a `Seq`).
fn take_leading_unpack(body: &mut Exp, arm_variant: Symbol) -> Option<Vec<(Symbol, String)>> {
    match body {
        Exp::UnpackVariant(_, (_, v), _, _) if *v == arm_variant => {
            let Exp::UnpackVariant(_, _, fields, _) = std::mem::replace(body, Exp::Seq(vec![]))
            else {
                unreachable!()
            };
            Some(fields)
        }
        Exp::Seq(items) if matches!(items.first(), Some(Exp::UnpackVariant(_, (_, v), _, _)) if *v == arm_variant) =>
        {
            let Exp::UnpackVariant(_, _, fields, _) = items.remove(0) else {
                unreachable!()
            };
            Some(fields)
        }
        _ => None,
    }
}
